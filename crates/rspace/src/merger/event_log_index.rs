//! An index over an event log, classifying produces/consumes (a monoid).
//!
//! Mirrors `rspace/src/main/scala/coop/rchain/rspace/merger/EventLogIndex.scala` (the data type and
//! its `empty`/`combine`; the effectful `apply` constructor is in the engine phase).

use std::collections::{BTreeMap, BTreeSet};

use rchain_crypto::hash::blake2b256_hash::Blake2b256Hash;

use crate::merger::event_log_merging_logic::combine_produces_copied_by_peek;
use crate::trace::event::{Consume, Produce};

/// Numeric-channel difference map (port of `NumberChannelsDiff`).
pub type NumberChannelsDiff = BTreeMap<Blake2b256Hash, i64>;

fn union<T: Ord + Clone>(a: &BTreeSet<T>, b: &BTreeSet<T>) -> BTreeSet<T> {
    a.union(b).cloned().collect()
}

/// An event-log index (port of `EventLogIndex`).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct EventLogIndex {
    pub produces_linear: BTreeSet<Produce>,
    pub produces_persistent: BTreeSet<Produce>,
    pub produces_consumed: BTreeSet<Produce>,
    pub produces_peeked: BTreeSet<Produce>,
    pub produces_copied_by_peek: BTreeSet<Produce>,
    pub produces_touching_base_joins: BTreeSet<Produce>,
    pub consumes_linear_and_peeks: BTreeSet<Consume>,
    pub consumes_persistent: BTreeSet<Consume>,
    pub consumes_produced: BTreeSet<Consume>,
    pub produces_mergeable: BTreeSet<Produce>,
    pub consumes_mergeable: BTreeSet<Consume>,
    pub number_channels_data: NumberChannelsDiff,
}

impl EventLogIndex {
    pub fn empty() -> Self {
        EventLogIndex::default()
    }

    /// Combine two indices (port of `EventLogIndex.combine`).
    pub fn combine(x: &EventLogIndex, y: &EventLogIndex) -> EventLogIndex {
        let mut number_channels = x.number_channels_data.clone();
        for (k, v) in &y.number_channels_data {
            *number_channels.entry(*k).or_insert(0) += *v;
        }
        EventLogIndex {
            produces_linear: union(&x.produces_linear, &y.produces_linear),
            produces_persistent: union(&x.produces_persistent, &y.produces_persistent),
            produces_consumed: union(&x.produces_consumed, &y.produces_consumed),
            produces_peeked: union(&x.produces_peeked, &y.produces_peeked),
            produces_copied_by_peek: combine_produces_copied_by_peek(x, y),
            produces_touching_base_joins: union(
                &x.produces_touching_base_joins,
                &y.produces_touching_base_joins,
            ),
            consumes_linear_and_peeks: union(
                &x.consumes_linear_and_peeks,
                &y.consumes_linear_and_peeks,
            ),
            consumes_persistent: union(&x.consumes_persistent, &y.consumes_persistent),
            consumes_produced: union(&x.consumes_produced, &y.consumes_produced),
            produces_mergeable: union(&x.produces_mergeable, &y.produces_mergeable),
            consumes_mergeable: union(&x.consumes_mergeable, &y.consumes_mergeable),
            number_channels_data: number_channels,
        }
    }
}
