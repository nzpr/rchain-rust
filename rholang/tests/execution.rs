//! End-to-end execution-pipeline integration tests (parse → normalize → reduce → rspace → hash).
//!
//! These assemble a real `RhoRuntime` + `ReplayRhoRuntime` over an in-memory store and drive a
//! deploy through the full stack, pinning post-state hashes against committed golden vectors and
//! asserting the Scala-independent `replay == play` determinism invariant (Law 11).

mod common;

use rchain_crypto::hash::blake2b512_random::Blake2b512Random;
use rchain_models::ast::{Expr, Par};
use rchain_models::par_ops::from_expr;
use rchain_rholang::env::Env;
use rchain_rholang::registry::registry_bootstrap_ast;
use rchain_rspace::history::history::empty_root_hash_value;

use common::{block_on, build_runtime_pair, load_golden};

/// A fixed, deterministic random seed so post-state hashes are reproducible.
fn fixed_rand() -> Blake2b512Random {
    Blake2b512Random::from_init(&[0u8; 32])
}

fn chan(name: &str) -> Par {
    from_expr(Expr::GString(name.to_string()))
}

/// Assert `hash` matches the committed golden vector for `case`.
fn assert_state_hash(case: &str, hash: &[u8]) {
    let want = load_golden(case, "execution").unwrap_or_else(|| panic!("missing golden case {case}"));
    assert_eq!(rchain_shared::base16::encode(hash), want, "golden mismatch for {case}");
}

#[test]
fn execute_deploy_produces_datum() {
    let (rt, _replay) = build_runtime_pair();
    let res = rt.evaluate(r#"@"chan"!(42)"#, &fixed_rand()).expect("evaluate");
    assert!(res.succeeded(), "unexpected errors: {:?}", res.errors);
    let data = block_on(rt.get_data_par(&chan("chan"))).expect("get_data_par");
    assert_eq!(data, vec![from_expr(Expr::GInt(42))]);
}

#[test]
fn execute_deploy_state_hash_is_deterministic() {
    let (a, _) = build_runtime_pair();
    let (b, _) = build_runtime_pair();
    a.evaluate(r#"@"chan"!(42)"#, &fixed_rand()).unwrap();
    b.evaluate(r#"@"chan"!(42)"#, &fixed_rand()).unwrap();
    let ha = block_on(a.create_checkpoint()).unwrap().root;
    let hb = block_on(b.create_checkpoint()).unwrap().root;
    assert_eq!(ha, hb);
    assert_state_hash("exec_deploy_42", ha.as_bytes());
}

#[test]
fn replay_matches_play() {
    let (rt, rrt) = build_runtime_pair();
    let rand = fixed_rand();
    rt.evaluate(r#"@"chan"!(42)"#, &rand).unwrap();
    let cp = block_on(rt.create_checkpoint()).unwrap();

    // Replay from the empty pre-state against the recorded play log.
    block_on(rrt.reset(empty_root_hash_value())).unwrap();
    block_on(rrt.rig(cp.log.clone()));
    rrt.evaluate(r#"@"chan"!(42)"#, &rand).unwrap();
    block_on(rrt.check_replay_data()).expect("replay data consistent");

    let replay_cp = block_on(rrt.create_checkpoint()).unwrap();
    assert_eq!(cp.root, replay_cp.root);
    assert_state_hash("replay_deploy_42", replay_cp.root.as_bytes());
}

#[test]
fn empty_state_bootstrap_is_deterministic() {
    let (rt, _replay) = build_runtime_pair();
    block_on(rt.reset(empty_root_hash_value())).unwrap();
    rt.inj(&registry_bootstrap_ast(), &Env::new(), &fixed_rand())
        .unwrap();
    let root = block_on(rt.create_checkpoint()).unwrap().root;
    assert_state_hash("empty_state", root.as_bytes());
}

#[test]
fn failing_deploy_is_captured_not_propagated() {
    let (rt, _replay) = build_runtime_pair();
    // `1 + "a"` is a well-formed term whose reduction is a type error (Int + String).
    let res = rt.evaluate(r#"@"chan"!(1 + "a")"#, &fixed_rand()).expect("evaluate returns Ok");
    assert!(res.failed(), "expected a captured failure");
    assert!(!res.errors.is_empty());
    // The post-state is still checkpointable.
    block_on(rt.create_checkpoint()).expect("checkpoint after failed deploy");
}
