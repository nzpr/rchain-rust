//! The top-level history repository: checkpoint, reset, and reader construction.
//!
//! Mirrors `HistoryRepository.scala` + `HistoryRepositoryImpl.scala` (exporter/importer deferred).

use std::marker::PhantomData;
use std::sync::Arc;

use rchain_crypto::hash::blake2b256_hash::Blake2b256Hash;
use rchain_shared::serialize::Serialize;

use crate::hashing::stable_hash_provider::{hash_channel, hash_channels};
use crate::history::cold_store::{ColdKeyValueStore, PersistedData};
use crate::history::history::History;
use crate::history::history_action::HistoryAction;
use crate::history::history_reader::HistoryReader;
use crate::history::instances::rspace_history_reader_impl::RSpaceHistoryReaderImpl;
use crate::history::key_segment::KeySegment;
use crate::history::root_repository::RootRepository;
use crate::hot_store_action::HotStoreAction;
use crate::hot_store_trie_action::HotStoreTrieAction;
use crate::serializers::scodec_serialize::{
    encode_continuations, encode_continuations_binary, encode_datums, encode_datums_binary,
    encode_joins, encode_joins_binary,
};

const PREFIX_DATUM: u8 = 0x00;
const PREFIX_KONT: u8 = 0x01;
const PREFIX_JOINS: u8 = 0x02;

/// The history repository (port of `HistoryRepository` / `HistoryRepositoryImpl`).
pub struct HistoryRepository<C, P, A, K> {
    current_history: Arc<dyn History>,
    roots_repository: Arc<RootRepository>,
    leaf_store: ColdKeyValueStore,
    marker: PhantomData<(C, P, A, K)>,
}

/// A cold-store write: leaf hash + leaf content (port of `ColdAction`).
type ColdAction = (Blake2b256Hash, Option<PersistedData>);

fn key_segment(prefix: u8, hash: Blake2b256Hash) -> KeySegment {
    let mut bytes = vec![prefix];
    bytes.extend_from_slice(hash.as_bytes());
    KeySegment::new(bytes)
}

fn calculate_storage_action<C, P, A, K>(
    action: &HotStoreTrieAction<C, P, A, K>,
) -> (ColdAction, HistoryAction)
where
    A: Serialize<A>,
    P: Serialize<P>,
    K: Serialize<K>,
    C: Serialize<C>,
{
    match action {
        HotStoreTrieAction::TrieInsertProduce(hash, data) => {
            let leaf = PersistedData::DataLeaf(encode_datums(data));
            let leaf_hash = Blake2b256Hash::create(&crate::history::cold_store::encode_persisted_data(&leaf));
            (
                (leaf_hash, Some(leaf)),
                HistoryAction::Insert { key: key_segment(PREFIX_DATUM, *hash), hash: leaf_hash },
            )
        }
        HotStoreTrieAction::TrieInsertConsume(hash, conts) => {
            let leaf = PersistedData::ContinuationsLeaf(encode_continuations(conts));
            let leaf_hash = Blake2b256Hash::create(&crate::history::cold_store::encode_persisted_data(&leaf));
            (
                (leaf_hash, Some(leaf)),
                HistoryAction::Insert { key: key_segment(PREFIX_KONT, *hash), hash: leaf_hash },
            )
        }
        HotStoreTrieAction::TrieInsertJoins(hash, joins) => {
            let leaf = PersistedData::JoinsLeaf(encode_joins(joins));
            let leaf_hash = Blake2b256Hash::create(&crate::history::cold_store::encode_persisted_data(&leaf));
            (
                (leaf_hash, Some(leaf)),
                HistoryAction::Insert { key: key_segment(PREFIX_JOINS, *hash), hash: leaf_hash },
            )
        }
        HotStoreTrieAction::TrieInsertBinaryProduce(hash, data) => {
            let leaf = PersistedData::DataLeaf(encode_datums_binary(data));
            let leaf_hash = Blake2b256Hash::create(&crate::history::cold_store::encode_persisted_data(&leaf));
            (
                (leaf_hash, Some(leaf)),
                HistoryAction::Insert { key: key_segment(PREFIX_DATUM, *hash), hash: leaf_hash },
            )
        }
        HotStoreTrieAction::TrieInsertBinaryConsume(hash, conts) => {
            let leaf = PersistedData::ContinuationsLeaf(encode_continuations_binary(conts));
            let leaf_hash = Blake2b256Hash::create(&crate::history::cold_store::encode_persisted_data(&leaf));
            (
                (leaf_hash, Some(leaf)),
                HistoryAction::Insert { key: key_segment(PREFIX_KONT, *hash), hash: leaf_hash },
            )
        }
        HotStoreTrieAction::TrieInsertBinaryJoins(hash, joins) => {
            let leaf = PersistedData::JoinsLeaf(encode_joins_binary(joins));
            let leaf_hash = Blake2b256Hash::create(&crate::history::cold_store::encode_persisted_data(&leaf));
            (
                (leaf_hash, Some(leaf)),
                HistoryAction::Insert { key: key_segment(PREFIX_JOINS, *hash), hash: leaf_hash },
            )
        }
        HotStoreTrieAction::TrieDeleteProduce(hash) => (
            (*hash, None),
            HistoryAction::Delete { key: key_segment(PREFIX_DATUM, *hash) },
        ),
        HotStoreTrieAction::TrieDeleteConsume(hash) => (
            (*hash, None),
            HistoryAction::Delete { key: key_segment(PREFIX_KONT, *hash) },
        ),
        HotStoreTrieAction::TrieDeleteJoins(hash) => (
            (*hash, None),
            HistoryAction::Delete { key: key_segment(PREFIX_JOINS, *hash) },
        ),
    }
}

fn transform<C, P, A, K>(action: &HotStoreAction<C, P, A, K>) -> HotStoreTrieAction<C, P, A, K>
where
    C: Serialize<C> + Clone,
    P: Clone,
    A: Clone,
    K: Clone,
{
    match action {
        HotStoreAction::InsertData(channel, data) => {
            HotStoreTrieAction::TrieInsertProduce(hash_channel(channel), data.to_vec())
        }
        HotStoreAction::InsertContinuations(channels, conts) => {
            HotStoreTrieAction::TrieInsertConsume(hash_channels(channels), conts.to_vec())
        }
        HotStoreAction::InsertJoins(channel, joins) => {
            HotStoreTrieAction::TrieInsertJoins(hash_channel(channel), joins.to_vec())
        }
        HotStoreAction::DeleteData(channel) => {
            HotStoreTrieAction::TrieDeleteProduce(hash_channel(channel))
        }
        HotStoreAction::DeleteContinuations(channels) => {
            HotStoreTrieAction::TrieDeleteConsume(hash_channels(channels))
        }
        HotStoreAction::DeleteJoins(channel) => {
            HotStoreTrieAction::TrieDeleteJoins(hash_channel(channel))
        }
    }
}

impl<C, P, A, K> HistoryRepository<C, P, A, K> {
    pub fn new(
        current_history: Arc<dyn History>,
        roots_repository: Arc<RootRepository>,
        leaf_store: ColdKeyValueStore,
    ) -> Self {
        HistoryRepository {
            current_history,
            roots_repository,
            leaf_store,
            marker: PhantomData,
        }
    }

    pub fn root(&self) -> Blake2b256Hash {
        self.current_history.root()
    }

    pub fn history(&self) -> Arc<dyn History> {
        self.current_history.clone()
    }

    pub async fn checkpoint(&self, actions: &[HotStoreAction<C, P, A, K>]) -> Arc<Self>
    where
        C: Serialize<C> + Clone,
        P: Serialize<P> + Clone,
        A: Serialize<A> + Clone,
        K: Serialize<K> + Clone,
    {
        let trie_actions: Vec<HotStoreTrieAction<C, P, A, K>> =
            actions.iter().map(transform).collect();
        self.do_checkpoint(&trie_actions).await
    }

    pub async fn do_checkpoint(&self, trie_actions: &[HotStoreTrieAction<C, P, A, K>]) -> Arc<Self>
    where
        C: Serialize<C> + Clone,
        P: Serialize<P> + Clone,
        A: Serialize<A> + Clone,
        K: Serialize<K> + Clone,
    {
        let mut cold_actions: Vec<(Blake2b256Hash, PersistedData)> = Vec::new();
        let mut history_actions: Vec<HistoryAction> = Vec::new();
        for action in trie_actions {
            let (cold, history) = calculate_storage_action(action);
            if let Some(leaf) = cold.1 {
                cold_actions.push((cold.0, leaf));
            }
            history_actions.push(history);
        }

        // Write cold leaves (put-if-absent).
        if !cold_actions.is_empty() {
            let keys: Vec<Blake2b256Hash> = cold_actions.iter().map(|(k, _)| *k).collect();
            let present = self.leaf_store.contains(&keys).await;
            let absent: Vec<(Blake2b256Hash, PersistedData)> = cold_actions
                .iter()
                .zip(present.iter())
                .filter(|(_, &p)| !p)
                .map(|((k, v), _)| (*k, v.clone()))
                .collect();
            self.leaf_store.put(&absent).await;
        }

        // Apply radix-history actions and commit the new root.
        let new_history = self.current_history.process(&history_actions).await;
        self.roots_repository.commit(new_history.root()).await;

        Arc::new(HistoryRepository {
            current_history: new_history,
            roots_repository: self.roots_repository.clone(),
            leaf_store: self.leaf_store.clone(),
            marker: PhantomData,
        })
    }

    pub async fn reset(&self, root: Blake2b256Hash) -> Result<Arc<Self>, String>
    where
        C: Serialize<C>,
        P: Serialize<P>,
        A: Serialize<A>,
        K: Serialize<K>,
    {
        self.roots_repository
            .validate_and_set_current_root(root)
            .await?;
        let next = self.current_history.reset(root).await;
        Ok(Arc::new(HistoryRepository {
            current_history: next,
            roots_repository: self.roots_repository.clone(),
            leaf_store: self.leaf_store.clone(),
            marker: PhantomData,
        }))
    }

    pub async fn get_history_reader(&self, state_hash: Blake2b256Hash) -> Arc<dyn HistoryReader<C, P, A, K>>
    where
        C: Serialize<C> + Send + Sync + 'static,
        P: Serialize<P> + Send + Sync + 'static,
        A: Serialize<A> + Send + Sync + 'static,
        K: Serialize<K> + Send + Sync + 'static,
    {
        let history = self.current_history.reset(state_hash).await;
        Arc::new(RSpaceHistoryReaderImpl::new(history, self.leaf_store.clone()))
    }
}
