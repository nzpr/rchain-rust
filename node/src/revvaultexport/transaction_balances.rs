//! Transaction-balance reporting data model (port of the pure half of
//! `revvaultexport/reporting/TransactionBalances.scala`).
//!
//! The effectful entry points (`main`, `getPerValidatorVaults`, `getGenesisVaultMap`,
//! `getBlockHashByHeight`) need the node runtime/RSpace/block-store wiring and are deferred; the
//! account model, the pure transfer folding, and the wallet/bond loading are ported here.

use std::collections::BTreeMap;
use std::path::Path;

use rchain_casper::bonds_parser;
use rchain_casper::vault_parser;
use rchain_crypto::public_key::PublicKey;
use rchain_crypto::signatures::secp256k1::Secp256k1;
use rchain_rholang::util::rev_address::RevAddress;
use rchain_shared::base16;

use crate::revvaultexport::rho_trie_traverser::keccak_par_string;
use crate::web::transaction::{Transaction, TransactionInfo};

/// The PoS generator private key (port of `StandardDeploys.poSGeneratorPk`).
const POS_GENERATOR_PK: &str = "a9585a0687761139ab3587a4938fb5ab9fcba675c79fefba889859674046d4a5";

/// The coop multi-sig vault address (port of `TransactionBalances.CoopVaultAddr`).
pub const COOP_VAULT_ADDR: &str = "11112q61nMYJKnJhQmqz7xKBNupyosG4Cy9rVupBPmpwcyT6s2SAoF";

/// The kind of a REV account (port of `AccountType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountType {
    NormalVault,
    PerValidatorVault,
    PosStakingVault,
    CoopPosMultiSigVault,
}

/// A REV account with its balance and kind (port of `RevAccount`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevAccount {
    pub address: RevAddress,
    pub amount: i64,
    pub account_type: AccountType,
}

impl RevAccount {
    /// Add `receive_amount` to the balance (port of `receiveRev`).
    pub fn receive_rev(&self, receive_amount: i64) -> RevAccount {
        RevAccount {
            address: self.address.clone(),
            amount: self.amount + receive_amount,
            account_type: self.account_type,
        }
    }

    /// Subtract `send_amount` from the balance (port of `sendRev`).
    pub fn send_rev(&self, send_amount: i64) -> RevAccount {
        RevAccount {
            address: self.address.clone(),
            amount: self.amount - send_amount,
            account_type: self.account_type,
        }
    }

    /// The keccak-hashed address as hex, dropping the leading 2 bytes (port of `keccakHashedAddress`).
    pub fn keccak_hashed_address(&self) -> String {
        base16::encode(&keccak_par_string(&self.address.to_base58())[2..])
    }

    /// The account-kind name (port of `typeString`).
    pub fn type_string(&self) -> &'static str {
        match self.account_type {
            AccountType::NormalVault => "NormalVault",
            AccountType::PerValidatorVault => "PerValidatorVault",
            AccountType::PosStakingVault => "PosStakingVault",
            AccountType::CoopPosMultiSigVault => "CoopPosMultiSigVault",
        }
    }
}

/// The initial PoS staking vault (port of `initialPosStakingVault`).
pub fn initial_pos_staking_vault() -> Result<RevAccount, String> {
    let secret = base16::unsafe_decode(POS_GENERATOR_PK);
    let public_bytes = Secp256k1::to_public_bytes(&secret)
        .map_err(|e| format!("invalid PoS generator secret key: {e}"))?;
    let address = RevAddress::from_public_key(&PublicKey::new(public_bytes))
        .ok_or_else(|| "invalid PoS generator public key".to_string())?;
    Ok(RevAccount {
        address,
        amount: 0,
        account_type: AccountType::PosStakingVault,
    })
}

/// The global vault summary (port of `GlobalVaultsInfo`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlobalVaultsInfo {
    pub vault_maps: BTreeMap<String, RevAccount>,
    pub pos_vault_address: String,
    pub coop_pos_multi_sig_vault: String,
    pub per_validator_vaults: Vec<String>,
}

/// A transaction annotated with its block number and finalization (port of `TransactionBlockInfo`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransactionBlockInfo {
    pub transaction: TransactionInfo,
    pub block_number: i64,
    pub is_finalized: bool,
}

impl TransactionBlockInfo {
    /// Whether the transaction succeeded (port of `isSucceed`).
    pub fn is_succeed(&self) -> bool {
        self.transaction.transaction.fail_reason.is_none()
    }
}

/// Apply finalized, successful transfers to the genesis vault map (port of
/// `updateGenesisFromTransfer`).
pub fn update_genesis_from_transfer(
    genesis_vault: GlobalVaultsInfo,
    transfers: &[TransactionBlockInfo],
) -> Result<GlobalVaultsInfo, String> {
    let mut result_map = genesis_vault.vault_maps.clone();
    for transfer in transfers {
        if transfer.is_finalized && transfer.is_succeed() {
            let tx: &Transaction = &transfer.transaction.transaction;
            let from_vault = match result_map.get(&tx.from_addr).cloned() {
                Some(v) => v,
                None => normal_vault(&tx.from_addr)?,
            };
            result_map.insert(tx.from_addr.clone(), from_vault.send_rev(tx.amount));
            let to_vault = match result_map.get(&tx.to_addr).cloned() {
                Some(v) => v,
                None => normal_vault(&tx.to_addr)?,
            };
            result_map.insert(tx.to_addr.clone(), to_vault.receive_rev(tx.amount));
        }
    }
    Ok(GlobalVaultsInfo {
        vault_maps: result_map,
        ..genesis_vault
    })
}

fn normal_vault(addr: &str) -> Result<RevAccount, String> {
    Ok(RevAccount {
        address: RevAddress::parse(addr).map_err(|e| format!("invalid REV address: {e}"))?,
        amount: 0,
        account_type: AccountType::NormalVault,
    })
}

/// Build the initial REV account map from a wallets file and a bonds file (port of
/// `generateRevAccountFromWalletAndBond`).
pub fn generate_rev_account_from_wallet_and_bond(
    wallet_path: &Path,
    bonds_path: &Path,
) -> Result<BTreeMap<String, RevAccount>, String> {
    let bonds = bonds_parser::parse(bonds_path)?;
    let vaults = vault_parser::parse(wallet_path)?;

    let mut account_map: BTreeMap<String, RevAccount> = vaults
        .iter()
        .map(|v| {
            (
                v.rev_address.to_base58(),
                RevAccount {
                    address: v.rev_address.clone(),
                    amount: v.initial_balance,
                    account_type: AccountType::NormalVault,
                },
            )
        })
        .collect();

    let pos_vault = initial_pos_staking_vault()?;
    let pos_addr = pos_vault.address.to_base58();
    for (_, bond_amount) in &bonds {
        let current = account_map
            .get(&pos_addr)
            .cloned()
            .unwrap_or_else(|| pos_vault.clone());
        account_map.insert(pos_addr.clone(), current.receive_rev(*bond_amount));
    }
    Ok(account_map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::web::transaction::TransactionType;

    fn valid_address(byte: u8) -> RevAddress {
        let mut key = vec![byte; 65];
        key[0] = 0x04;
        RevAddress::from_public_key(&PublicKey::new(key)).unwrap()
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("rchain_txbal_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn rev_account_adjusts_balance() {
        let account = RevAccount {
            address: valid_address(1),
            amount: 100,
            account_type: AccountType::NormalVault,
        };
        assert_eq!(account.receive_rev(30).amount, 130);
        assert_eq!(account.send_rev(30).amount, 70);
        assert_eq!(account.type_string(), "NormalVault");
    }

    #[test]
    fn keccak_hashed_address_drops_two_bytes() {
        let account = RevAccount {
            address: valid_address(1),
            amount: 0,
            account_type: AccountType::PosStakingVault,
        };
        // 32-byte keccak, drop 2 → 30 bytes → 60 hex chars.
        assert_eq!(account.keccak_hashed_address().len(), 60);
    }

    #[test]
    fn update_genesis_transfers_between_accounts() {
        let addr_a = valid_address(1);
        let addr_b = valid_address(2);
        let mut vault_maps = BTreeMap::new();
        vault_maps.insert(
            addr_a.to_base58(),
            RevAccount {
                address: addr_a.clone(),
                amount: 100,
                account_type: AccountType::NormalVault,
            },
        );
        let genesis = GlobalVaultsInfo {
            vault_maps,
            pos_vault_address: String::new(),
            coop_pos_multi_sig_vault: String::new(),
            per_validator_vaults: vec![],
        };
        let transfer = TransactionBlockInfo {
            transaction: TransactionInfo {
                transaction: Transaction {
                    from_addr: addr_a.to_base58(),
                    to_addr: addr_b.to_base58(),
                    amount: 30,
                    ret_unforgeable: Default::default(),
                    fail_reason: None,
                },
                transaction_type: TransactionType::UserDeploy {
                    deploy_id: "d".to_string(),
                },
            },
            block_number: 1,
            is_finalized: true,
        };
        let updated = update_genesis_from_transfer(genesis, &[transfer]).unwrap();
        assert_eq!(updated.vault_maps[&addr_a.to_base58()].amount, 70);
        assert_eq!(updated.vault_maps[&addr_b.to_base58()].amount, 30);
    }

    #[test]
    fn generate_accounts_from_wallet_and_bond() {
        let addr = valid_address(1);
        let dir = temp_dir("wallet_bond");
        let wallet = dir.join("wallets.txt");
        let bonds = dir.join("bonds.txt");
        std::fs::write(&wallet, format!("{},500\n", addr.to_base58())).unwrap();
        std::fs::write(&bonds, format!("{} 100\n", "04".repeat(65))).unwrap();

        let map = generate_rev_account_from_wallet_and_bond(&wallet, &bonds).unwrap();
        assert_eq!(map[&addr.to_base58()].amount, 500);
        let pos_addr = initial_pos_staking_vault().unwrap().address.to_base58();
        assert_eq!(map[&pos_addr].amount, 100);
        assert_eq!(map[&pos_addr].account_type, AccountType::PosStakingVault);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
