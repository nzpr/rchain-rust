//! Determinism regression (spec: `docs/src/formal/determinism.md`).
//!
//! Play (block creation) and replay (validation) of a deploy that binds `rho:rchain:deployerId`
//! must produce the same post-state hash (sub-invariants S1/S3). This pins the replay-normalizer-env
//! and refund-amount fixes so future drift fails here instead of in consensus.

mod common;

use std::collections::BTreeMap;

use rchain_casper::genesis::contracts::Vault;
use rchain_crypto::hash::blake2b512_random::Blake2b512Random;
use rchain_crypto::public_key::PublicKey;
use rchain_models::casper::protocol::casper_message::{
    DeployData, ProcessedDeploy, ProcessedSystemDeploy, SignedDeployData,
};
use rchain_rholang::system_processes::BlockData;
use rchain_rholang::util::rev_address::RevAddress;
use rchain_shared::refined::NonNegI64;

fn deploy(term: &str) -> SignedDeployData {
    SignedDeployData {
        data: DeployData {
            term: term.to_string(),
            timestamp: 0,
            phlo_price: 1,
            phlo_limit: 500_000,
            valid_after_block_number: 0,
            shard_id: "root".to_string(),
        },
        // 65-byte public key so RevAddress derivation succeeds (matches the seeded vault).
        deployer: vec![0u8; 65],
        sig: Vec::new(),
        sig_algorithm: "secp256k1".to_string(),
    }
}

fn seeded_vault() -> Vault {
    Vault {
        rev_address: RevAddress::from_public_key(&PublicKey::new(vec![0u8; 65]))
            .expect("valid rev address"),
        initial_balance: NonNegI64::try_from(1_000_000_000).unwrap(),
    }
}

#[tokio::test]
async fn play_and_replay_agree_for_deployer_id_binding_deploy() {
    let rm = common::build_runtime_manager().await;
    let rand = Blake2b512Random::from_init(&[0u8; 32]);

    let (_pre, post, _) = rm
        .compute_genesis(
            &[],
            &rand,
            BlockData::empty(),
            &BTreeMap::new(),
            &[seeded_vault()],
        )
        .await
        .expect("compute_genesis");

    // Binds `rho:rchain:deployerId` (the REV-transfer idiom). Replay must normalize with the SAME
    // env, else `add_urn` fails with `BugFoundError` (S1) — before the fix this returned
    // `InvalidStateHash`.
    let term = r#"new deployerId(`rho:rchain:deployerId`) in { @"marker"!(true) }"#;

    let (post_state, user_results, sys_results) = rm
        .compute_state(&post, &[deploy(term)], &[], &rand, BlockData::empty())
        .await
        .expect("play compute_state");
    assert!(
        user_results[0].eval_result.succeeded(),
        "play deploy must succeed: {:?}",
        user_results[0].eval_result.errors
    );

    let processed: Vec<ProcessedDeploy> = user_results.into_iter().map(|r| r.deploy).collect();
    let processed_sys: Vec<ProcessedSystemDeploy> =
        sys_results.into_iter().map(|r| r.deploy).collect();

    let (replay_state, _) = rm
        .replay_compute_state(
            &post,
            &processed,
            &processed_sys,
            &rand,
            BlockData::empty(),
            true,
            &BTreeMap::new(),
            &[],
        )
        .await
        .expect("replay compute_state");

    assert_eq!(
        post_state, replay_state,
        "play and replay post-state hashes must agree (S1/S3)"
    );
}
