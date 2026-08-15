//! The reporting replay space (port of `ReportingRspace.scala`).
//!
//! Mirrors `ReportingRspace`: a replay space that also accumulates a human-readable report of the
//! produce/consume/COMM events. The report is a `Seq[Seq[ReportingEvent]]` separated by soft
//! checkpoint (system deploy segments). The Scala overrides `logComm`/`logConsume`/`logProduce`
//! (hooks on `RSpaceOps`) to collect events; the Rust port instead exposes `record_*` methods that
//! the caller invokes alongside replay, since the replay space does not expose those hooks.

use std::collections::BTreeSet;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use rchain_crypto::hash::blake2b256_hash::Blake2b256Hash;
use rchain_shared::serialize::Serialize;

use crate::checkpoint::SoftCheckpoint;
use crate::i_replay_space::IReplaySpace;
use crate::i_space::ISpace;
use crate::internal::{Datum, Row, WaitingContinuation};
use crate::replay_rspace::ReplayRSpace;
use crate::trace::Log;
use crate::tuple_space::{ContResult, Result, Tuplespace};
use crate::util::ReplayException;

/// A report entry (port of `ReportingRspace.ReportingEvent`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReportingEvent<C, P, A, K> {
    Produce(ReportingProduce<C, A>),
    Consume(ReportingConsume<C, P, K>),
    Comm(ReportingComm<C, P, A, K>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportingProduce<C, A> {
    pub channel: C,
    pub data: A,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportingConsume<C, P, K> {
    pub channels: Vec<C>,
    pub patterns: Vec<P>,
    pub continuation: K,
    pub peeks: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportingComm<C, P, A, K> {
    pub consume: ReportingConsume<C, P, K>,
    pub produces: Vec<ReportingProduce<C, A>>,
}

/// The reporting replay space (port of `ReportingRspace`).
pub struct ReportingRspace<C, P, A, K> {
    replay: Arc<ReplayRSpace<C, P, A, K>>,
    report: RwLock<Vec<Vec<ReportingEvent<C, P, A, K>>>>,
    soft_report: RwLock<Vec<ReportingEvent<C, P, A, K>>>,
}

impl<C, P, A, K> ReportingRspace<C, P, A, K>
where
    C: Ord + Clone + Serialize<C> + Send + Sync + 'static,
    P: Clone + Serialize<P> + Send + Sync + 'static,
    A: Clone + Serialize<A> + Send + Sync + 'static,
    K: Clone + Serialize<K> + Send + Sync + 'static,
{
    pub fn new(replay: Arc<ReplayRSpace<C, P, A, K>>) -> Self {
        ReportingRspace {
            replay,
            report: RwLock::new(Vec::new()),
            soft_report: RwLock::new(Vec::new()),
        }
    }

    pub fn record_produce(&self, channel: C, data: A) {
        crate::lock::wlock(&self.soft_report)
            .push(ReportingEvent::Produce(ReportingProduce { channel, data }));
    }

    pub fn record_consume(&self, channels: Vec<C>, patterns: Vec<P>, continuation: K, peeks: Vec<usize>) {
        crate::lock::wlock(&self.soft_report)
            .push(ReportingEvent::Consume(ReportingConsume {
                channels,
                patterns,
                continuation,
                peeks,
            }));
    }

    pub fn record_comm(&self, consume: ReportingConsume<C, P, K>, produces: Vec<ReportingProduce<C, A>>) {
        crate::lock::wlock(&self.soft_report)
            .push(ReportingEvent::Comm(ReportingComm { consume, produces }));
    }

    /// Move the soft report into the report history (port of `collectReport`).
    pub fn collect_report(&self) {
        let mut soft = std::mem::take(&mut *crate::lock::wlock(&self.soft_report));
        if !soft.is_empty() {
            crate::lock::wlock(&self.report).push(std::mem::take(&mut soft));
        }
    }

    /// Drain and return the report (port of `getReport`).
    pub fn get_report(&self) -> Vec<Vec<ReportingEvent<C, P, A, K>>> {
        self.collect_report();
        std::mem::take(&mut *crate::lock::wlock(&self.report))
    }
}

#[async_trait]
impl<C, P, A, K> Tuplespace<C, P, A, K> for ReportingRspace<C, P, A, K>
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
    ) -> Option<(ContResult<C, P, K>, Vec<Result<C, A>>)> {
        self.replay
            .consume(channels, patterns, continuation, persist, peeks)
            .await
    }

    async fn produce(
        &self,
        channel: C,
        data: A,
        persist: bool,
    ) -> Option<(ContResult<C, P, K>, Vec<Result<C, A>>)> {
        self.replay.produce(channel, data, persist).await
    }

    async fn install(&self, channels: &[C], patterns: &[P], continuation: K) -> Option<(K, Vec<A>)> {
        self.replay.install(channels, patterns, continuation).await
    }
}

#[async_trait]
impl<C, P, A, K> ISpace<C, P, A, K> for ReportingRspace<C, P, A, K>
where
    C: Ord + Clone + Serialize<C> + Send + Sync + 'static,
    P: Clone + Serialize<P> + Send + Sync + 'static,
    A: Clone + Serialize<A> + Send + Sync + 'static,
    K: Clone + Serialize<K> + Send + Sync + 'static,
{
    async fn create_checkpoint(&self) -> crate::checkpoint::Checkpoint {
        let checkpoint = self.replay.create_checkpoint().await;
        crate::lock::wlock(&self.soft_report).clear();
        crate::lock::wlock(&self.report).clear();
        checkpoint
    }

    async fn reset(&self, root: Blake2b256Hash) {
        self.replay.reset(root).await;
    }

    async fn get_data(&self, channel: &C) -> Vec<Datum<A>> {
        self.replay.get_data(channel).await
    }

    async fn get_waiting_continuations(&self, channels: &[C]) -> Vec<WaitingContinuation<P, K>> {
        self.replay.get_waiting_continuations(channels).await
    }

    async fn get_joins(&self, channel: &C) -> Vec<Vec<C>> {
        self.replay.get_joins(channel).await
    }

    async fn clear(&self) {
        self.replay.clear().await;
    }

    async fn to_map(&self) -> std::collections::BTreeMap<Vec<C>, Row<P, A, K>> {
        self.replay.to_map().await
    }

    async fn create_soft_checkpoint(&self) -> SoftCheckpoint<C, P, A, K> {
        self.collect_report();
        self.replay.create_soft_checkpoint().await
    }

    async fn revert_to_soft_checkpoint(&self, checkpoint: SoftCheckpoint<C, P, A, K>) {
        self.replay.revert_to_soft_checkpoint(checkpoint).await;
    }
}

#[async_trait]
impl<C, P, A, K> IReplaySpace<C, P, A, K> for ReportingRspace<C, P, A, K>
where
    C: Ord + Clone + Serialize<C> + Send + Sync + 'static,
    P: Clone + Serialize<P> + Send + Sync + 'static,
    A: Clone + Serialize<A> + Send + Sync + 'static,
    K: Clone + Serialize<K> + Send + Sync + 'static,
{
    async fn rig(&self, log: Log) {
        self.replay.rig(log).await;
    }

    async fn rig_and_reset(&self, start_root: Blake2b256Hash, log: Log) {
        self.replay.rig_and_reset(start_root, log).await;
    }

    async fn check_replay_data(&self) -> std::result::Result<(), ReplayException> {
        self.replay.check_replay_data().await
    }
}
