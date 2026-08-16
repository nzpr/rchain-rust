//! Genesis block creation (port of `casper/genesis/Genesis.scala`).
//!
//! `defaultBlessedTerms` and `createGenesisBlock` are deferred pending the standard-deploy
//! template resources (`StandardDeploys`).

pub mod contracts;

use std::collections::{BTreeMap, BTreeSet};

use rchain_crypto::public_key::PublicKey;
use rchain_models::block_version::CURRENT;
use rchain_models::casper::protocol::casper_message::{BlockMessage, ProcessedDeploy, RholangState};
use rchain_models::validator::Validator as ModelsValidator;

use crate::genesis::contracts::{ProofOfStake, Registry, Vault};
use crate::proto_util::unsigned_block_proto;

/// Genesis parameters (port of `Genesis`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Genesis {
    pub sender: PublicKey,
    pub shard_id: String,
    pub block_number: i64,
    pub proof_of_stake: ProofOfStake,
    pub registry: Registry,
    pub vaults: Vec<Vault>,
}

/// Build the bonds map (validator pubkey → stake) from the PoS validators (port of `buildBondsMap`).
#[allow(dead_code)] // used by the deferred `createGenesisBlock`
fn build_bonds_map(proof_of_stake: &ProofOfStake) -> BTreeMap<ModelsValidator, i64> {
    proof_of_stake
        .validators
        .iter()
        .map(|v| (ModelsValidator::from_slice(v.pk.bytes()), v.stake))
        .collect()
}

/// Build the unsigned genesis block from processed deploys (port of
/// `createBlockWithProcessedDeploys`).
#[allow(dead_code)] // used by the deferred `createGenesisBlock`
fn create_block_with_processed_deploys(
    genesis: &Genesis,
    pre_state_hash: Vec<u8>,
    post_state_hash: Vec<u8>,
    processed_deploys: Vec<ProcessedDeploy>,
) -> BlockMessage {
    assert!(
        processed_deploys.iter().all(|d| !d.is_failed),
        "Genesis block contains failed deploys."
    );
    let state = RholangState {
        deploys: processed_deploys,
        system_deploys: Vec::new(),
    };
    unsigned_block_proto(
        CURRENT,
        genesis.shard_id.clone(),
        genesis.block_number,
        ModelsValidator::from_slice(genesis.sender.bytes()),
        0,
        pre_state_hash,
        post_state_hash,
        Vec::new(),
        build_bonds_map(&genesis.proof_of_stake),
        BTreeSet::new(),
        state,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genesis::contracts::Validator;

    fn pos() -> ProofOfStake {
        ProofOfStake {
            minimum_bond: 1,
            maximum_bond: 100,
            validators: vec![
                Validator {
                    pk: PublicKey::new(vec![1; 65]),
                    stake: 10,
                },
                Validator {
                    pk: PublicKey::new(vec![2; 65]),
                    stake: 20,
                },
            ],
            epoch_length: 0,
            quarantine_length: 0,
            number_of_active_validators: 0,
            pos_multi_sig_public_keys: vec![],
            pos_multi_sig_quorum: 0,
            pos_vault_pub_key: String::new(),
        }
    }

    #[test]
    fn build_bonds_map_extracts_stakes() {
        let bonds = build_bonds_map(&pos());
        assert_eq!(bonds.len(), 2);
        assert_eq!(bonds[&ModelsValidator::from_slice(&[1; 65])], 10);
        assert_eq!(bonds[&ModelsValidator::from_slice(&[2; 65])], 20);
    }
}
