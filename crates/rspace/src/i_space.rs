//! The RSpace interface.
//!
//! Mirrors `rspace/src/main/scala/coop/rchain/rspace/ISpace.scala`.

use std::collections::BTreeMap;

use async_trait::async_trait;
use rchain_crypto::hash::blake2b256_hash::Blake2b256Hash;

use crate::checkpoint::{Checkpoint, SoftCheckpoint};
use crate::internal::{Datum, Row, WaitingContinuation};
use crate::tuple_space::Tuplespace;

/// The RSpace interface (port of `ISpace[F]`).
#[async_trait]
pub trait ISpace<C, P, A, K>: Tuplespace<C, P, A, K> {
    async fn create_checkpoint(&self) -> Checkpoint;

    async fn reset(&self, root: Blake2b256Hash);

    async fn get_data(&self, channel: &C) -> Vec<Datum<A>>;

    async fn get_waiting_continuations(&self, channels: &[C]) -> Vec<WaitingContinuation<P, K>>;

    async fn get_joins(&self, channel: &C) -> Vec<Vec<C>>;

    async fn clear(&self);

    async fn to_map(&self) -> BTreeMap<Vec<C>, Row<P, A, K>>;

    async fn create_soft_checkpoint(&self) -> SoftCheckpoint<C, P, A, K>;

    async fn revert_to_soft_checkpoint(&self, checkpoint: SoftCheckpoint<C, P, A, K>);
}
