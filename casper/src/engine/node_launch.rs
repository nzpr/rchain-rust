//! Node launch (port of `engine/NodeLaunch.scala`).
//!
//! The genesis-from-config helpers (`createGenesisBlock` / `createGenesisBlockFromConfig`) are
//! ported here. The `apply` state machine (mode dispatch → genesis/syncing/running over the packet
//! stream) is deferred pending the node runtime's comm/discovery wiring.

use std::path::Path;

use rchain_models::casper::protocol::casper_message::BlockMessage;

use crate::bonds_parser;
use crate::conf::CasperConf;
use crate::genesis::contracts::{ProofOfStake, Registry, Validator};
use crate::genesis::Genesis;
use crate::runtime_manager::RuntimeManager;
use crate::validator_identity::ValidatorIdentity;
use crate::vault_parser;

/// Create the genesis block from raw config values (port of `NodeLaunch.createGenesisBlock`).
#[allow(clippy::too_many_arguments)]
pub async fn create_genesis_block(
    validator: &ValidatorIdentity,
    shard_id: &str,
    block_number: i64,
    bonds_path: &str,
    autogen_shard_size: i32,
    vaults_path: &str,
    minimum_bond: i64,
    maximum_bond: i64,
    epoch_length: i32,
    quarantine_length: i32,
    number_of_active_validators: i32,
    pos_multi_sig_public_keys: &[String],
    pos_multi_sig_quorum: i32,
    pos_vault_pub_key: &str,
    system_contract_pub_key: &str,
    runtime: &RuntimeManager,
) -> Result<BlockMessage, String> {
    // Initial REV vaults.
    let vaults = vault_parser::parse(Path::new(vaults_path))?;

    // Initial validators.
    let bonds = bonds_parser::parse_or_generate(Path::new(bonds_path), autogen_shard_size);
    let validators: Vec<Validator> = bonds
        .into_iter()
        .map(|(pk, stake)| Validator { pk, stake })
        .collect();

    // Run the genesis deploys and create the block.
    let genesis = Genesis {
        sender: validator.public_key.clone(),
        shard_id: shard_id.to_string(),
        block_number,
        proof_of_stake: ProofOfStake {
            minimum_bond,
            maximum_bond,
            validators,
            epoch_length,
            quarantine_length,
            number_of_active_validators,
            pos_multi_sig_public_keys: pos_multi_sig_public_keys.to_vec(),
            pos_multi_sig_quorum,
            pos_vault_pub_key: pos_vault_pub_key.to_string(),
        },
        registry: Registry {
            system_contract_pub_key: system_contract_pub_key.to_string(),
        },
        vaults,
    };

    crate::genesis::create_genesis_block(validator, &genesis, runtime).await
}

/// Create the genesis block from a [`CasperConf`] (port of
/// `NodeLaunch.createGenesisBlockFromConfig`).
pub async fn create_genesis_block_from_config(
    validator: &ValidatorIdentity,
    conf: &CasperConf,
    runtime: &RuntimeManager,
) -> Result<BlockMessage, String> {
    let gbd = &conf.genesis_block_data;
    create_genesis_block(
        validator,
        &conf.shard_name,
        gbd.genesis_block_number,
        &gbd.bonds_file,
        conf.autogen_shard_size,
        &gbd.wallets_file,
        gbd.bond_minimum,
        gbd.bond_maximum,
        gbd.epoch_length,
        gbd.quarantine_length,
        gbd.number_of_active_validators,
        &gbd.pos_multi_sig_public_keys,
        gbd.pos_multi_sig_quorum,
        &gbd.pos_vault_pub_key,
        &gbd.system_contract_pub_key,
        runtime,
    )
    .await
}
