//! The concrete `HistoryReader` over a target history + cold store.
//!
//! Mirrors `rspace/src/main/scala/coop/rchain/rspace/history/instances/RSpaceHistoryReaderImpl.scala`.

use std::sync::Arc;

use async_trait::async_trait;
use rchain_crypto::hash::blake2b256_hash::Blake2b256Hash;
use rchain_shared::serialize::Serialize;

use crate::hashing::stable_hash_provider::{hash_channel, hash_channels};
use crate::history::cold_store::{ColdKeyValueStore, PersistedData};
use crate::history::history::History;
use crate::history::history_reader::{HistoryReader, HistoryReaderBase};
use crate::history::key_segment::KeySegment;
use crate::internal::{Datum, WaitingContinuation};
use crate::serializers::scodec_serialize::{
    decode_continuations, decode_datums, decode_joins,
};

const PREFIX_DATUM: u8 = 0x00;
const PREFIX_KONT: u8 = 0x01;
const PREFIX_JOINS: u8 = 0x02;

/// The history reader implementation (port of `RSpaceHistoryReaderImpl`).
pub struct RSpaceHistoryReaderImpl<C, P, A, K> {
    target_history: Arc<dyn History>,
    leaf_store: ColdKeyValueStore,
    marker: std::marker::PhantomData<(C, P, A, K)>,
}

impl<C, P, A, K> RSpaceHistoryReaderImpl<C, P, A, K>
where
    C: Serialize<C> + Send + Sync + 'static,
    P: Serialize<P> + Send + Sync + 'static,
    A: Serialize<A> + Send + Sync + 'static,
    K: Serialize<K> + Send + Sync + 'static,
{
    pub fn new(target_history: Arc<dyn History>, leaf_store: ColdKeyValueStore) -> Self {
        RSpaceHistoryReaderImpl {
            target_history,
            leaf_store,
            marker: std::marker::PhantomData,
        }
    }

    async fn fetch_data(&self, prefix: u8, key: Blake2b256Hash) -> Option<PersistedData> {
        let mut seg = vec![prefix];
        seg.extend_from_slice(key.as_bytes());
        match self.target_history.read(&KeySegment::new(seg)).await {
            Some(leaf_hash) => self
                .leaf_store
                .get(&[leaf_hash])
                .await
                .into_iter()
                .next()
                .flatten(),
            None => None,
        }
    }
}

#[async_trait]
impl<C, P, A, K> HistoryReader<C, P, A, K> for RSpaceHistoryReaderImpl<C, P, A, K>
where
    C: Serialize<C> + Send + Sync + 'static,
    P: Serialize<P> + Send + Sync + 'static,
    A: Serialize<A> + Send + Sync + 'static,
    K: Serialize<K> + Send + Sync + 'static,
{
    fn root(&self) -> Blake2b256Hash {
        self.target_history.root()
    }

    async fn get_data(&self, key: Blake2b256Hash) -> Vec<Datum<A>> {
        match self.fetch_data(PREFIX_DATUM, key).await {
            Some(PersistedData::DataLeaf(bytes)) => decode_datums(&bytes),
            Some(_) => panic!("unexpected leaf while looking for data at key {key:?}"),
            None => Vec::new(),
        }
    }

    async fn get_continuations(&self, key: Blake2b256Hash) -> Vec<WaitingContinuation<P, K>> {
        match self.fetch_data(PREFIX_KONT, key).await {
            Some(PersistedData::ContinuationsLeaf(bytes)) => decode_continuations(&bytes),
            Some(_) => panic!("unexpected leaf while looking for continuations at key {key:?}"),
            None => Vec::new(),
        }
    }

    async fn get_joins(&self, key: Blake2b256Hash) -> Vec<Vec<C>> {
        match self.fetch_data(PREFIX_JOINS, key).await {
            Some(PersistedData::JoinsLeaf(bytes)) => decode_joins(&bytes),
            Some(_) => panic!("unexpected leaf while looking for joins at key {key:?}"),
            None => Vec::new(),
        }
    }

    fn base(&self) -> Arc<dyn HistoryReaderBase<C, P, A, K>> {
        Arc::new(BaseReader {
            reader: Arc::new(RSpaceHistoryReaderImpl {
                target_history: self.target_history.clone(),
                leaf_store: self.leaf_store.clone(),
                marker: std::marker::PhantomData,
            }),
        })
    }
}

struct BaseReader<C, P, A, K> {
    reader: Arc<RSpaceHistoryReaderImpl<C, P, A, K>>,
}

#[async_trait]
impl<C, P, A, K> HistoryReaderBase<C, P, A, K> for BaseReader<C, P, A, K>
where
    C: Serialize<C> + Send + Sync + 'static,
    P: Serialize<P> + Send + Sync + 'static,
    A: Serialize<A> + Send + Sync + 'static,
    K: Serialize<K> + Send + Sync + 'static,
{
    async fn get_data(&self, key: &C) -> Vec<Datum<A>> {
        self.reader.get_data(hash_channel(key)).await
    }

    async fn get_continuations(&self, key: &[C]) -> Vec<WaitingContinuation<P, K>> {
        self.reader.get_continuations(hash_channels(key)).await
    }

    async fn get_joins(&self, key: &C) -> Vec<Vec<C>> {
        self.reader.get_joins(hash_channel(key)).await
    }
}
