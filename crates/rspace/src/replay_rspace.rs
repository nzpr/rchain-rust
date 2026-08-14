//! The replay tuple space (Law 11: replay determinism).
//!
//! Mirrors `rspace/src/main/scala/coop/rchain/rspace/ReplayRSpace.scala`. The full re-execution
//! matcher (re-running the recorded COMM) is simplified to delegation onto the play space; the
//! recorded-trace bookkeeping (`rig` / `checkReplayData`) is preserved.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use rchain_crypto::hash::blake2b256_hash::Blake2b256Hash;
use rchain_shared::serialize::Serialize;

use crate::checkpoint::SoftCheckpoint;
use crate::i_replay_space::IReplaySpace;
use crate::i_space::ISpace;
use crate::internal::{Datum, Row, WaitingContinuation};
use crate::rspace::RSpace;
use crate::trace::event::{Comm, Consume, Event, Produce};
use crate::trace::Log;
use crate::tuple_space::Tuplespace;

/// The recorded replay trace: IO events keyed to the COMMs that reference them (port of
/// `ReplayData`).
#[derive(Clone, Debug, Default)]
pub struct ReplayData {
    by_consume: BTreeMap<Consume, Vec<Comm>>,
    by_produce: BTreeMap<Produce, Vec<Comm>>,
}

/// The replay space (port of `ReplayRSpace`).
pub struct ReplayRSpace<C, P, A, K> {
    space: Arc<RSpace<C, P, A, K>>,
    replay_data: RwLock<ReplayData>,
    log: RwLock<Log>,
}

impl<C, P, A, K> ReplayRSpace<C, P, A, K>
where
    C: Ord + Clone + Serialize<C> + Send + Sync + 'static,
    P: Clone + Serialize<P> + Send + Sync + 'static,
    A: Clone + Serialize<A> + Send + Sync + 'static,
    K: Clone + Serialize<K> + Send + Sync + 'static,
{
    pub fn new(space: Arc<RSpace<C, P, A, K>>) -> Self {
        ReplayRSpace {
            space,
            replay_data: RwLock::new(ReplayData::default()),
            log: RwLock::new(Vec::new()),
        }
    }

    fn build_replay_data(log: &Log) -> ReplayData {
        let mut data = ReplayData::default();
        for event in log {
            if let Event::Comm(comm) = event {
                data.by_consume
                    .entry(comm.consume.clone())
                    .or_default()
                    .push(comm.clone());
                for produce in &comm.produces {
                    data.by_produce
                        .entry(produce.clone())
                        .or_default()
                        .push(comm.clone());
                }
            }
        }
        data
    }
}

#[async_trait]
impl<C, P, A, K> Tuplespace<C, P, A, K> for ReplayRSpace<C, P, A, K>
where
    C: Ord + Clone + Serialize<C> + Send + Sync + 'static,
    P: Clone + Serialize<P> + Send + Sync + 'static,
    A: Clone + Serialize<A> + Send + Sync + 'static,
    K: Clone + Serialize<K> + Send + Sync + 'static,
{
    async fn consume(
        &self,
        channels: &[C],
        patterns: &[P],
        continuation: K,
        persist: bool,
        peeks: BTreeSet<usize>,
    ) -> Option<(crate::tuple_space::ContResult<C, P, K>, Vec<crate::tuple_space::Result<C, A>>)> {
        self.space
            .consume(channels, patterns, continuation, persist, peeks)
            .await
    }

    async fn produce(
        &self,
        channel: C,
        data: A,
        persist: bool,
    ) -> Option<(crate::tuple_space::ContResult<C, P, K>, Vec<crate::tuple_space::Result<C, A>>)> {
        self.space.produce(channel, data, persist).await
    }

    async fn install(&self, channels: &[C], patterns: &[P], continuation: K) -> Option<(K, Vec<A>)> {
        self.space.install(channels, patterns, continuation).await
    }
}

#[async_trait]
impl<C, P, A, K> ISpace<C, P, A, K> for ReplayRSpace<C, P, A, K>
where
    C: Ord + Clone + Serialize<C> + Send + Sync + 'static,
    P: Clone + Serialize<P> + Send + Sync + 'static,
    A: Clone + Serialize<A> + Send + Sync + 'static,
    K: Clone + Serialize<K> + Send + Sync + 'static,
{
    async fn create_checkpoint(&self) -> crate::checkpoint::Checkpoint {
        self.space.create_checkpoint().await
    }

    async fn reset(&self, root: Blake2b256Hash) {
        self.space.reset(root).await;
    }

    async fn get_data(&self, channel: &C) -> Vec<Datum<A>> {
        self.space.get_data(channel).await
    }

    async fn get_waiting_continuations(&self, channels: &[C]) -> Vec<WaitingContinuation<P, K>> {
        self.space.get_waiting_continuations(channels).await
    }

    async fn get_joins(&self, channel: &C) -> Vec<Vec<C>> {
        self.space.get_joins(channel).await
    }

    async fn clear(&self) {
        self.space.clear().await;
    }

    async fn to_map(&self) -> BTreeMap<Vec<C>, Row<P, A, K>> {
        self.space.to_map().await
    }

    async fn create_soft_checkpoint(&self) -> SoftCheckpoint<C, P, A, K> {
        self.space.create_soft_checkpoint().await
    }

    async fn revert_to_soft_checkpoint(&self, checkpoint: SoftCheckpoint<C, P, A, K>) {
        self.space.revert_to_soft_checkpoint(checkpoint).await;
    }
}

#[async_trait]
impl<C, P, A, K> IReplaySpace<C, P, A, K> for ReplayRSpace<C, P, A, K>
where
    C: Ord + Clone + Serialize<C> + Send + Sync + 'static,
    P: Clone + Serialize<P> + Send + Sync + 'static,
    A: Clone + Serialize<A> + Send + Sync + 'static,
    K: Clone + Serialize<K> + Send + Sync + 'static,
{
    async fn rig(&self, log: Log) {
        *self.replay_data.write().unwrap() = Self::build_replay_data(&log);
        *self.log.write().unwrap() = log;
    }

    async fn rig_and_reset(&self, start_root: Blake2b256Hash, log: Log) {
        self.reset(start_root).await;
        self.rig(log).await;
    }

    async fn check_replay_data(&self) {
        let data = self.replay_data.read().unwrap();
        assert!(
            data.by_consume.is_empty() && data.by_produce.is_empty(),
            "unused COMM event in replay data"
        );
    }
}
