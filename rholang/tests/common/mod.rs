//! Shared harness for the rholang execution-pipeline integration tests.
//!
//! The rholang runtimes are `!Send` and drive the async rspace through an internal `block_on`
//! (`ChargingRSpace`'s sync-over-async bridge). Async work must therefore be entered only
//! transiently — never nested — so [`block_on`] builds a fresh single-threaded runtime per call,
//! and the synchronous `RhoRuntime`/`ReplayRhoRuntime` constructors + `inj`/`evaluate` run strictly
//! between those entered regions.

use std::sync::Arc;

use rchain_models::ast::Par;
use rchain_models::runtime::{BindPattern, ListParWithRandom, TaggedContinuation};
use rchain_rholang::runtime::{RhoRuntime, ReplayRhoRuntime};
use rchain_rholang::storage::RhoMatch;
use rchain_rspace::factory::create_history_repository;
use rchain_rspace::hot_store::InMemHotStore;
use rchain_rspace::rspace::RSpace;
use rchain_shared::store_manager::InMemoryStoreManager;

/// Drive `f` to completion on a fresh single-threaded runtime.
pub fn block_on<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build current_thread runtime")
        .block_on(f)
}

/// Assemble a play + replay runtime pair over a fresh in-memory store.
pub fn build_runtime_pair() -> (RhoRuntime, ReplayRhoRuntime) {
    let (history, play, replay) = block_on(async {
        let manager = InMemoryStoreManager::default();
        let history =
            create_history_repository::<Par, BindPattern, ListParWithRandom, TaggedContinuation>(
                &manager,
            )
            .await
            .expect("history repository");
        let reader = history.get_history_reader(history.root()).await;
        let hot = Arc::new(InMemHotStore::new(reader.base()));
        let (play, replay) = RSpace::create_with_replay(history.clone(), hot, Arc::new(RhoMatch));
        (history, play, replay)
    });
    let rho = RhoRuntime::create(play, history.clone(), Par::default()).expect("rho runtime");
    let replay = ReplayRhoRuntime::create(Arc::new(replay), history, Par::default())
        .expect("replay runtime");
    (rho, replay)
}

/// Look up a committed golden hex vector for `case` in `testdata/differential/<target>.tsv`.
///
/// Returns `None` when the fixture is absent or the case is not yet recorded (used to bootstrap
/// the vectors on first run).
pub fn load_golden(case: &str, target: &str) -> Option<String> {
    let path = format!(
        "{}/testdata/differential/{target}.tsv",
        env!("CARGO_MANIFEST_DIR")
    );
    let contents = std::fs::read_to_string(path).ok()?;
    contents.lines().find_map(|line| {
        let (id, hex) = line.split_once('\t')?;
        (id == case).then(|| hex.to_string())
    })
}
