//! Property tests for RSpace Laws 7–11 (A7).
//!
//! Randomized (`proptest`) invariants: join commutativity (Law 7), deterministic COMM (Law 8),
//! and Merkle determinism under insertion reordering (Law 10).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use proptest::prelude::*;

use rchain_crypto::hash::blake2b256_hash::Blake2b256Hash;
use rchain_shared::store::{InMemoryKeyValueStore, KeyValueStore};
use rchain_shared::typed_store::{BytesCodec, KeyValueTypedStoreCodec};

use crate::hashing::stable_hash_provider::hash_channels;
use crate::history::codecs::Blake2b256HashCodec;
use crate::history::history_action::HistoryAction;
use crate::history::key_segment::KeySegment;
use crate::history::radix_tree::{empty_node, RadixTreeImpl};
use crate::internal::{ConsumeCandidate, Datum};
use crate::trace::event::{Comm, Consume, Produce};

fn in_memory_tree() -> RadixTreeImpl {
    let shared: rchain_shared::typed_store::SharedStore = Arc::new(tokio::sync::Mutex::new(
        Box::new(InMemoryKeyValueStore::default()) as Box<dyn KeyValueStore + Send + Sync>,
    ));
    let typed = Arc::new(KeyValueTypedStoreCodec::new(
        shared,
        Arc::new(Blake2b256HashCodec),
        Arc::new(BytesCodec),
    ));
    RadixTreeImpl::new(typed)
}

proptest! {
    /// Law 7: a join's hash is independent of channel order.
    #[test]
    fn law7_join_hash_commutes(channels in prop::collection::vec(".*", 0..16)) {
        let mut shuffled = channels.clone();
        shuffled.reverse();
        prop_assert_eq!(hash_channels(&channels), hash_channels(&shuffled));
    }

    /// Law 8: `Comm::apply` produces a deterministic (sorted) produce order.
    #[test]
    fn law8_comm_sorts_produces(
        triples in prop::collection::vec(
            (any::<[u8; 32]>(), any::<[u8; 32]>(), any::<bool>()),
            1..32,
        )
    ) {
        let candidates: Vec<ConsumeCandidate<String, String>> = triples
            .iter()
            .map(|(ch, h, persist)| {
                let source = Produce::from_hash(
                    Blake2b256Hash::from_bytes(*ch),
                    Blake2b256Hash::from_bytes(*h),
                    *persist,
                );
                ConsumeCandidate {
                    channel: "c".to_string(),
                    datum: Datum {
                        a: "a".to_string(),
                        persist: *persist,
                        source,
                    },
                    removed_datum: "a".to_string(),
                    datum_index: 0,
                }
            })
            .collect();
        let consume = Consume::from_hash(vec![], Blake2b256Hash::from_bytes([0u8; 32]), false);
        let comm = Comm::apply(&candidates, consume, BTreeSet::new(), |_| BTreeMap::new());
        let sorted = comm.produces.windows(2).all(|w| {
            (w[0].channels_hash, w[0].hash, w[0].persistent)
                <= (w[1].channels_hash, w[1].hash, w[1].persistent)
        });
        prop_assert!(sorted);
    }

    /// Law 10: the radix-tree root hash is independent of insertion order. Keys are fixed-length
    /// (the Scala asserts every prefix in a subtree has equal length, so variable-length keys are
    /// out of contract).
    #[test]
    fn law10_merkle_root_is_insertion_order_independent(
        pairs in prop::collection::btree_map(
            prop::collection::vec(any::<u8>(), 4),
            any::<[u8; 32]>(),
            1..32,
        )
    ) {
        let actions: Vec<HistoryAction> = pairs
            .iter()
            .map(|(k, v)| HistoryAction::Insert {
                key: KeySegment::new(k.clone()),
                hash: Blake2b256Hash::from_bytes(*v),
            })
            .collect();
        let mut reversed = actions.clone();
        reversed.reverse();

        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let (h1, h2) = rt.block_on(async {
            let root = empty_node();
            let (_, h1) = in_memory_tree()
                .save_and_commit(&root, &actions)
                .await
                .unwrap()
                .unwrap();
            let (_, h2) = in_memory_tree()
                .save_and_commit(&root, &reversed)
                .await
                .unwrap()
                .unwrap();
            (h1, h2)
        });
        prop_assert_eq!(h1, h2);
    }
}
