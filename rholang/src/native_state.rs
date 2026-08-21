//! Native system-contract state (registry / PoS / vault) layered over the rspace native store.
//!
//! The blessed rholang contracts (`Registry.rho`, `Pos.rhox`, `RevVault.rho`, …) re-implemented a
//! `TreeHashMap` trie *in interpreted rholang*; any interpreter bug made the registry silently empty.
//! This module replaces that fragile bootstrap with native Rust state: typed maps over the
//! content-addressed `InMemNativeStore`, exposed through the same `rho:*` protocol by native system
//! processes. The Scala contracts remain a *checklist* of required behavior only.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use rchain_crypto::hash::blake2b256_hash::Blake2b256Hash;
use rchain_crypto::public_key::PublicKey;
use rchain_models::ast::Par;
use rchain_models::validator::Validator;
use rchain_shared::refined::NonNegI64;
use rchain_shared::serialize::Serialize;

use rchain_rspace::native_store::{InMemNativeStore, PREFIX_POS, PREFIX_REGISTRY, PREFIX_VAULT};

use crate::util::rev_address::RevAddress;

/// The size of a serialized `(Validator, NonNegI64)` bond entry (65-byte key + 8-byte stake).
const BOND_ENTRY_LEN: usize = 65 + 8;

/// Leaf key for the PoS bonds map.
pub fn pos_bonds_key() -> Blake2b256Hash {
    Blake2b256Hash::create(b"pos:bonds")
}

/// Leaf key for a registry URI (the URI string, hashed).
fn registry_key(uri: &str) -> Blake2b256Hash {
    Blake2b256Hash::create(uri.as_bytes())
}

/// Leaf key for a vault balance (the REV address base58 string, hashed).
fn vault_key(address: &str) -> Blake2b256Hash {
    Blake2b256Hash::create(address.as_bytes())
}

/// Canonically encode a bonds map (sorted by `Validator`, 65-byte key + little-endian stake).
pub fn encode_bonds(bonds: &BTreeMap<Validator, NonNegI64>) -> Vec<u8> {
    let mut out = Vec::with_capacity(bonds.len() * BOND_ENTRY_LEN);
    for (v, stake) in bonds {
        out.extend_from_slice(v.as_bytes());
        out.extend_from_slice(&i64::from(*stake).to_le_bytes());
    }
    out
}

/// Decode a bonds map (inverse of [`encode_bonds`]).
pub fn decode_bonds(bytes: &[u8]) -> Result<BTreeMap<Validator, NonNegI64>, String> {
    if bytes.len() % BOND_ENTRY_LEN != 0 {
        return Err(format!(
            "bonds encoding has {} bytes, not a multiple of {BOND_ENTRY_LEN}",
            bytes.len()
        ));
    }
    let mut out = BTreeMap::new();
    for chunk in bytes.chunks_exact(BOND_ENTRY_LEN) {
        let validator = Validator::from_slice(&chunk[..65]);
        let stake_bytes: [u8; 8] = chunk[65..73]
            .try_into()
            .map_err(|_| "bonds encoding: invalid stake length".to_string())?;
        let stake = i64::from_le_bytes(stake_bytes);
        let stake = NonNegI64::try_from(stake).map_err(|_| format!("negative bond stake {stake}"))?;
        out.insert(validator, stake);
    }
    Ok(out)
}

/// The typed native system state, wrapping the shared byte-oriented [`InMemNativeStore`].
#[derive(Clone)]
pub struct NativeSystemState {
    store: Arc<InMemNativeStore>,
}

impl NativeSystemState {
    pub fn new(store: Arc<InMemNativeStore>) -> Self {
        NativeSystemState { store }
    }

    // --- PoS ------------------------------------------------------------

    /// Read the current bonds map.
    pub async fn bonds(&self) -> Result<BTreeMap<Validator, NonNegI64>, String> {
        let key = pos_bonds_key();
        match self.store.get(PREFIX_POS, &key).await? {
            Some(bytes) => decode_bonds(&bytes),
            None => Ok(BTreeMap::new()),
        }
    }

    /// Write the bonds map (appends a native `Put` action).
    pub fn set_bonds(&self, bonds: &BTreeMap<Validator, NonNegI64>) {
        self.store.put(PREFIX_POS, pos_bonds_key(), encode_bonds(bonds));
    }

    /// The active-validator set (derived: every bonded validator is active; the top-N-by-stake limit
    /// is a later refinement).
    pub async fn active_validators(&self) -> Result<BTreeSet<Validator>, String> {
        Ok(self.bonds().await?.into_keys().collect())
    }

    // --- Vault ------------------------------------------------------------

    /// Read a vault balance (address is the REV base58 string).
    pub async fn vault_balance(&self, address: &str) -> Result<Option<NonNegI64>, String> {
        match self.store.get(PREFIX_VAULT, &vault_key(address)).await? {
            Some(bytes) => {
                let arr: [u8; 8] = bytes
                    .as_slice()
                    .try_into()
                    .map_err(|_| format!("vault balance for {address} is {} bytes", bytes.len()))?;
                NonNegI64::try_from(i64::from_le_bytes(arr)).map(Some).map_err(|_| {
                    format!("vault balance for {address} is negative")
                })
            }
            None => Ok(None),
        }
    }

    /// Write a vault balance.
    pub fn set_vault_balance(&self, address: &str, balance: NonNegI64) {
        self.store.put(
            PREFIX_VAULT,
            vault_key(address),
            i64::from(balance).to_le_bytes().to_vec(),
        );
    }

    // --- Registry ------------------------------------------------------------

    /// Look up a registered `Par` by URI.
    pub async fn registry_lookup(&self, uri: &str) -> Result<Option<Par>, String> {
        match self.store.get(PREFIX_REGISTRY, &registry_key(uri)).await? {
            Some(bytes) => Ok(Some(<Par as Serialize<Par>>::decode(&bytes)?)),
            None => Ok(None),
        }
    }

    /// Register a `Par` under `uri`.
    pub fn registry_insert(&self, uri: &str, value: &Par) {
        self.store.put(
            PREFIX_REGISTRY,
            registry_key(uri),
            <Par as Serialize<Par>>::encode(value),
        );
    }

    // --- System-deploy operations (native) -------------------------------

    /// Charge `amount` to the deployer's REV vault (port of the PoS `chargeDeploy` behavior). The
    /// outer `Result` is a platform failure; the inner is the `(Bool, Either)` user result.
    pub async fn pre_charge(
        &self,
        deployer: &PublicKey,
        amount: i64,
    ) -> Result<Result<(), String>, String> {
        if amount == 0 {
            return Ok(Ok(()));
        }
        let address = RevAddress::from_public_key(deployer)
            .ok_or_else(|| "preCharge: invalid deployer public key".to_string())?
            .to_base58();
        let balance = match self.vault_balance(&address).await? {
            Some(b) => b,
            None => NonNegI64::try_from(0).map_err(|e| e.to_string())?,
        };
        if i64::from(balance) < amount {
            return Ok(Err(format!(
                "preCharge: insufficient funds ({} < {amount})",
                i64::from(balance)
            )));
        }
        let new_balance = NonNegI64::try_from(i64::from(balance) - amount)
            .map_err(|e| format!("preCharge: {e}"))?;
        self.set_vault_balance(&address, new_balance);
        Ok(Ok(()))
    }

    /// Refund `amount` (port of the PoS `refundDeploy` behavior). The refund vault is not yet
    /// modeled, so this is a successful no-op for now.
    pub async fn refund(&self, _amount: i64) -> Result<Result<(), String>, String> {
        Ok(Ok(()))
    }

    /// Advance the epoch / close the block (port of the PoS `closeBlock` behavior). Epoch bookkeeping
    /// is not yet modeled, so this is a successful no-op for now.
    pub async fn close_block(&self) -> Result<Result<(), String>, String> {
        Ok(Ok(()))
    }

    /// Remove a validator from the bond map (port of the PoS `slash` behavior).
    pub async fn slash(&self, validator: &Validator) -> Result<Result<(), String>, String> {
        let mut bonds = self.bonds().await?;
        bonds.remove(validator);
        self.set_bonds(&bonds);
        Ok(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn validator(byte: u8) -> Validator {
        Validator::from_slice(&[byte; 65])
    }

    #[test]
    fn bonds_round_trip() {
        let mut bonds = BTreeMap::new();
        bonds.insert(validator(2), NonNegI64::try_from(20).unwrap());
        bonds.insert(validator(1), NonNegI64::try_from(10).unwrap());
        let encoded = encode_bonds(&bonds);
        assert_eq!(decode_bonds(&encoded).unwrap(), bonds);
    }

    #[test]
    fn decode_bonds_rejects_trailing_bytes() {
        assert!(decode_bonds(&[0u8; 3]).is_err());
    }
}
