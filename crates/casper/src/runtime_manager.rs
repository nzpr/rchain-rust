//! The runtime manager façade (port of `casper/rholang/RuntimeManager.scala`, read-only surface).

use rchain_crypto::hash::blake2b256_hash::Blake2b256Hash;
use rchain_models::ast::Par;
use rchain_models::block::state_hash::StateHash;
use rchain_models::runtime::BindPattern;
use rchain_rholang::runtime::RhoRuntime;
use rchain_rholang::storage::RhoHistoryRepository;

/// The runtime manager (port of `RuntimeManager`). The deploy-execution, replay, and bond-computation
/// methods are deferred pending the system deploys + replay runtime wiring.
pub struct RuntimeManager {
    runtime: RhoRuntime,
    history_repo: RhoHistoryRepository,
}

impl RuntimeManager {
    pub fn new(runtime: RhoRuntime, history_repo: RhoHistoryRepository) -> Self {
        RuntimeManager {
            runtime,
            history_repo,
        }
    }

    pub fn get_history_repo(&self) -> &RhoHistoryRepository {
        &self.history_repo
    }

    pub fn runtime(&self) -> &RhoRuntime {
        &self.runtime
    }

    /// Read the `Par`s at a channel in the state identified by `hash` (port of `getData`).
    pub async fn get_data(&self, hash: &StateHash, channel: &Par) -> Result<Vec<Par>, String> {
        self.runtime.reset(to_blake(hash)).await.map_err(|e| e)?;
        self.runtime
            .get_data_par(channel)
            .await
            .map_err(|e| e.to_string())
    }

    /// Read the `ParBody` continuations at `channels` in the state identified by `hash` (port of
    /// `getContinuation`).
    pub async fn get_continuation(
        &self,
        hash: &StateHash,
        channels: &[Par],
    ) -> Result<Vec<(Vec<BindPattern>, Par)>, String> {
        self.runtime.reset(to_blake(hash)).await.map_err(|e| e)?;
        self.runtime
            .get_continuation_par(channels)
            .await
            .map_err(|e| e.to_string())
    }
}

fn to_blake(hash: &StateHash) -> Blake2b256Hash {
    Blake2b256Hash::from_byte_array(hash.as_bytes())
}
