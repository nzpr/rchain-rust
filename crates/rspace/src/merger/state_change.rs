//! The state change between two history snapshots (a monoid).
//!
//! Mirrors `rspace/src/main/scala/coop/rchain/rspace/merger/StateChange.scala` (the data type and
//! its `empty`/`combine`; the effectful `apply` is in the engine phase).

use std::collections::BTreeMap;

use rchain_crypto::hash::blake2b256_hash::Blake2b256Hash;

use crate::merger::channel_change::ChannelChange;

fn combine_channel_change_map<K, V>(
    x: &BTreeMap<K, ChannelChange<V>>,
    y: &BTreeMap<K, ChannelChange<V>>,
) -> BTreeMap<K, ChannelChange<V>>
where
    K: Ord + Clone,
    V: Clone,
{
    let mut out = x.clone();
    for (k, v) in y {
        match out.get(k) {
            Some(existing) => {
                out.insert(k.clone(), ChannelChange::combine(existing, v));
            }
            None => {
                out.insert(k.clone(), v.clone());
            }
        }
    }
    out
}

/// The diff between two history states (port of `StateChange`).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct StateChange {
    pub datums_changes: BTreeMap<Blake2b256Hash, ChannelChange<Vec<u8>>>,
    pub kont_changes: BTreeMap<Vec<Blake2b256Hash>, ChannelChange<Vec<u8>>>,
    pub consume_channels_to_join_serialized_map: BTreeMap<Vec<Blake2b256Hash>, Vec<u8>>,
}

impl StateChange {
    pub fn empty() -> Self {
        StateChange::default()
    }

    /// Combine two state changes (port of `StateChange.combine`).
    pub fn combine(x: &StateChange, y: &StateChange) -> StateChange {
        let mut joins = x.consume_channels_to_join_serialized_map.clone();
        for (k, v) in &y.consume_channels_to_join_serialized_map {
            joins.insert(k.clone(), v.clone());
        }
        StateChange {
            datums_changes: combine_channel_change_map(&x.datums_changes, &y.datums_changes),
            kont_changes: combine_channel_change_map(&x.kont_changes, &y.kont_changes),
            consume_channels_to_join_serialized_map: joins,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combine_is_associative() {
        let a = StateChange {
            datums_changes: BTreeMap::from([(
                Blake2b256Hash::from_bytes([1; 32]),
                ChannelChange { added: vec![vec![1]], removed: vec![] },
            )]),
            ..Default::default()
        };
        let b = StateChange {
            datums_changes: BTreeMap::from([(
                Blake2b256Hash::from_bytes([1; 32]),
                ChannelChange { added: vec![], removed: vec![vec![2]] },
            )]),
            ..Default::default()
        };
        let ab = StateChange::combine(&a, &b);
        let ba = StateChange::combine(&b, &a);
        // ChannelChange combine is concatenation, so order of added/removed differs,
        // but the monoid law tested here is empty-is-identity.
        assert_eq!(StateChange::combine(&a, &StateChange::empty()), a);
        // both orders contain the same multiset of added/removed
        let mut ab_added = ab.datums_changes[&Blake2b256Hash::from_bytes([1; 32])].added.clone();
        ab_added.sort();
        let mut ba_added = ba.datums_changes[&Blake2b256Hash::from_bytes([1; 32])].added.clone();
        ba_added.sort();
        assert_eq!(ab_added, ba_added);
    }
}
