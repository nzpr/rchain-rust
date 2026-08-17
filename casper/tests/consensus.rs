//! End-to-end consensus-pipeline integration tests (genesis → block → replay).

mod common;

use rchain_crypto::hash::blake2b512_random::Blake2b512Random;
use rchain_models::casper::protocol::casper_message::{DeployData, ProcessedDeploy, SignedDeployData};
use rchain_rholang::system_processes::BlockData;

use common::build_runtime_manager;

fn fixed_rand() -> Blake2b512Random {
    Blake2b512Random::from_init(&[0u8; 32])
}

/// A minimal signed deploy with the given term (signature verification is deferred to the
/// deploy-acceptance path, so the sig/deployer fields are left empty here).
fn deploy(term: &str) -> SignedDeployData {
    SignedDeployData {
        data: DeployData {
            term: term.to_string(),
            timestamp: 0,
            phlo_price: 1,
            phlo_limit: 90_000,
            valid_after_block_number: 0,
            shard_id: "root".to_string(),
        },
        deployer: vec![0u8; 32],
        sig: Vec::new(),
        sig_algorithm: "secp256k1".to_string(),
    }
}

#[tokio::test]
async fn genesis_deploy_replay_recomputes_state() {
    let rm = build_runtime_manager().await;
    let rand = fixed_rand();
    let (pre, post, results) = rm
        .compute_genesis(&[deploy(r#"@"chan"!(42)"#)], &rand, BlockData::empty())
        .await
        .expect("compute_genesis");
    assert_eq!(results.len(), 1);
    assert!(results[0].eval_result.succeeded(), "deploy should succeed");

    // Law 11: replay recomputes the same post-state hash from the recorded log.
    let processed: Vec<ProcessedDeploy> = results.iter().map(|r| r.deploy.clone()).collect();
    let (replay_post, _) = rm
        .replay_compute_state(&pre, &processed, &[], &rand, BlockData::empty(), false)
        .await
        .expect("replay_compute_state");
    assert_eq!(post, replay_post, "replay must reproduce the play post-state");
}
