//! Event-log merging (Law 9: merge is a monoid; non-conflicting logs commute).
//!
//! Mirrors `rspace/src/main/scala/coop/rchain/rspace/merger/`.

pub mod channel_change;
pub mod event_log_index;
pub mod event_log_merging_logic;
pub mod state_change;
