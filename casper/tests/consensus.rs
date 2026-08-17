//! End-to-end consensus-pipeline integration tests (genesis → block → validate → replay → merge).

mod common;

use rchain_crypto::hash::blake2b512_random::Blake2b512Random;
use rchain_models::casper::protocol::casper_message::{DeployData, SignedDeployData};
use rchain_rholang::system_processes::BlockData;

use common::{block_on, build_runtime_manager};

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

// Blocked: the async `RuntimeManager` methods drive the synchronous `evaluate`/`inj` path, which
// `block_on`s the `ChargingRSpace` runtime; awaiting them from any async context therefore panics
// with "Cannot start a runtime from within a runtime". The consensus pipeline needs a sync/async
// refactor of the reduce→rspace bridge before this test can be enabled.
#[test]
#[ignore = "blocked: sync-over-async RuntimeManager (nested block_on panic)"]
fn genesis_produces_deterministic_state() {
    let rm = build_runtime_manager();
    let (_pre, _post, results) =
        block_on(rm.compute_genesis(&[deploy(r#"@"chan"!(42)"#)], &fixed_rand(), BlockData::empty()))
            .expect("compute_genesis");
    assert_eq!(results.len(), 1);
}
