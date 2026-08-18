//! Block receiver (port of `blocks/BlockReceiver.scala`).
//!
//! The pure `BlockReceiverState` state machine (begin/end storing + finished) and the
//! `not_validated` helper are fully ported. The fs2 `BlockReceiver.apply` stream wiring (incoming
//! + validated block streams, validation queue) is deferred pending the comm/stream layer.

use std::collections::{BTreeMap, BTreeSet};

use rchain_block_storage::block_store::BlockStore;
use rchain_block_storage::dag::dag_storage::BlockDagStorage;
use rchain_models::block_hash::BlockHash;

/// Block-receive status (port of `RecvStatus`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecvStatus {
    /// Begin checking and storing block.
    BeginStoreBlock,
    /// Block stored in the block store, waiting for validation and DAG insertion.
    EndStoreBlock,
    /// Block sent to validation.
    PendingValidation,
    /// Requested missing dependencies.
    Requested,
}

/// Block receiver state (port of `BlockReceiverState`).
///
/// It consists of three events: two to store blocks (begin and end) to prevent a race when storing
/// blocks, and `finished` when a block is validated and added to the DAG (end of processing).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockReceiverState<MId: Ord + Clone + std::fmt::Debug> {
    /// Blocks received and stored in BlockStore (not validated) with parent relations.
    blocks_st: BTreeMap<MId, BTreeSet<MId>>,
    /// Blocks receiving status.
    receive_st: BTreeMap<MId, RecvStatus>,
    /// Blocks mapping with children relations.
    child_relations: BTreeMap<MId, BTreeSet<MId>>,
}

impl<MId: Ord + Clone + std::fmt::Debug> BlockReceiverState<MId> {
    /// Create an empty receiver state (port of `BlockReceiverState.apply`).
    pub fn new() -> Self {
        BlockReceiverState {
            blocks_st: BTreeMap::new(),
            receive_st: BTreeMap::new(),
            child_relations: BTreeMap::new(),
        }
    }

    /// Begin storing a block, marking it to prevent duplicate threads storing the same block. The
    /// returned flag is `true` when storing should proceed (port of `beginStored`).
    pub fn begin_stored(&self, id: MId) -> (Self, bool) {
        // If state is not known or pending request, it's expected, so continue with receiving.
        let expected_receive = match self.receive_st.get(&id) {
            Some(RecvStatus::Requested) => true,
            Some(_) => false,
            None => true,
        };
        if expected_receive {
            let mut receive_st = self.receive_st.clone();
            receive_st.insert(id, RecvStatus::BeginStoreBlock);
            (
                BlockReceiverState {
                    receive_st,
                    ..self.clone()
                },
                true,
            )
        } else {
            (self.clone(), false)
        }
    }

    /// Storing of the block is done, waiting validation. Returns the updated state and the unseen
    /// parent dependencies (port of `endStored`).
    pub fn end_stored(&self, id: MId, parents: Vec<(MId, bool)>) -> (Self, BTreeSet<MId>) {
        let cur_state_opt = self.receive_st.get(&id);
        assert_eq!(
            cur_state_opt,
            Some(&RecvStatus::BeginStoreBlock),
            "Received should be called only in begin received state, actual: {:?}, hash: {:?}",
            cur_state_opt,
            id
        );
        match cur_state_opt {
            Some(RecvStatus::BeginStoreBlock) => {
                // Update blocks state, keep unseen parents only.
                let parents_not_stored: BTreeSet<MId> = parents
                    .iter()
                    .filter(|(_, not_stored)| *not_stored)
                    .map(|(parent, _)| parent.clone())
                    .collect();
                let mut unseen_parents = parents_not_stored;
                unseen_parents.retain(|parent| {
                    !self.blocks_st.contains_key(parent)
                        && !self.receive_st.contains_key(parent)
                        && parent != &id
                });
                let mut new_blocks_st = self.blocks_st.clone();
                new_blocks_st.insert(id.clone(), unseen_parents.clone());

                // Update block status to received and set unseen parents to Requested.
                let mut new_receive_st = self.receive_st.clone();
                new_receive_st.insert(id.clone(), RecvStatus::EndStoreBlock);
                for parent in &unseen_parents {
                    new_receive_st.insert(parent.clone(), RecvStatus::Requested);
                }

                // Update children relations of the received block.
                let mut new_child_relations = self.child_relations.clone();
                for (parent, _) in &parents {
                    new_child_relations
                        .entry(parent.clone())
                        .or_default()
                        .insert(id.clone());
                }

                let new_state = BlockReceiverState {
                    blocks_st: new_blocks_st,
                    receive_st: new_receive_st,
                    child_relations: new_child_relations,
                };
                (new_state, unseen_parents)
            }
            // TODO: this should never happen, protected by assert
            //  (maybe we need helper function to wrap the whole pattern or return error to caller).
            _ => (self.clone(), BTreeSet::new()),
        }
    }

    /// Finish block validation, updating state and returning the next blocks with validated
    /// dependencies (port of `finished`).
    pub fn finished(&self, id: MId, parents: BTreeSet<MId>) -> (Self, BTreeSet<MId>) {
        let parents_in_state = self.blocks_st.contains_key(&id);
        let is_received = matches!(
            self.receive_st.get(&id),
            Some(RecvStatus::EndStoreBlock) | Some(RecvStatus::PendingValidation)
        );
        // To finish a block it must be present in the state (parents relations and at least stored).
        assert!(
            parents_in_state && is_received,
            "Calling finished on unexpected block hash {:?}.",
            id
        );

        // Remove the finished block from its children's dependencies and from the blocks state.
        let childs: Vec<MId> = self
            .child_relations
            .get(&id)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default();
        let updated_blocks: BTreeMap<MId, BTreeSet<MId>> = childs
            .iter()
            .map(|child| {
                let mut deps = self.blocks_st.get(child).cloned().unwrap_or_default();
                deps.remove(&id);
                (child.clone(), deps)
            })
            .collect();
        let mut new_blocks_st = self.blocks_st.clone();
        for (child, deps) in &updated_blocks {
            new_blocks_st.insert(child.clone(), deps.clone());
        }
        new_blocks_st.remove(&id);

        // Next blocks with all dependencies validated and not already in pending validation state.
        let deps_validated: BTreeSet<MId> = updated_blocks
            .iter()
            .filter(|(bid, parents)| {
                let pending = matches!(self.receive_st.get(*bid), Some(RecvStatus::PendingValidation));
                parents.is_empty() && !pending
            })
            .map(|(bid, _)| bid.clone())
            .collect();

        // Set next blocks to pending validation and remove the finished block.
        let mut new_receive_st = self.receive_st.clone();
        for dep in &deps_validated {
            new_receive_st.insert(dep.clone(), RecvStatus::PendingValidation);
        }
        new_receive_st.remove(&id);

        // Remove the finished block from children relations.
        let mut new_child_relations: BTreeMap<MId, BTreeSet<MId>> = BTreeMap::new();
        for (parent, childs) in &self.child_relations {
            if parents.contains(parent) {
                let mut childs = childs.clone();
                childs.remove(&id);
                if !childs.is_empty() {
                    new_child_relations.insert(parent.clone(), childs);
                }
            } else {
                new_child_relations.insert(parent.clone(), childs.clone());
            }
        }

        let new_state = BlockReceiverState {
            blocks_st: new_blocks_st,
            receive_st: new_receive_st,
            child_relations: new_child_relations,
        };
        (new_state, deps_validated)
    }
}

impl<MId: Ord + Clone + std::fmt::Debug> Default for BlockReceiverState<MId> {
    fn default() -> Self {
        Self::new()
    }
}

/// Check whether a block is stored but not yet validated into the DAG (port of
/// `BlockReceiver.notValidated`).
pub async fn not_validated(
    block_store: &BlockStore,
    dag: &dyn BlockDagStorage,
    hash: &BlockHash,
) -> bool {
    let in_store = block_store.contains(&[*hash]).await.first().copied().unwrap_or(false);
    if !in_store {
        return false;
    }
    let repr = dag.get_representation().await;
    !repr.contains(hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    type MId = String;

    fn parents(items: &[(&str, bool)]) -> Vec<(MId, bool)> {
        items
            .iter()
            .map(|(id, stored)| (id.to_string(), *stored))
            .collect()
    }

    #[test]
    fn begin_stored_true_if_unknown() {
        let (st, is_receiving) = BlockReceiverState::<MId>::new().begin_stored("A1".to_string());
        assert_eq!(st.receive_st, BTreeMap::from([("A1".to_string(), RecvStatus::BeginStoreBlock)]));
        assert!(is_receiving);
    }

    #[test]
    fn begin_stored_false_if_not_requested() {
        let (st, _) = BlockReceiverState::<MId>::new().begin_stored("A1".to_string());
        let (new_st, is_receiving) = st.begin_stored("A1".to_string());
        assert_eq!(new_st.receive_st, BTreeMap::from([("A1".to_string(), RecvStatus::BeginStoreBlock)]));
        assert!(!is_receiving);
    }

    #[test]
    fn begin_stored_true_if_requested() {
        let (st, _) = BlockReceiverState::<MId>::new().begin_stored("A2".to_string());
        let (new_st, _) = st.end_stored("A2".to_string(), parents(&[("A1", true)]));
        // Unseen parent A1 now has Requested status.
        let (_, is_receiving) = new_st.begin_stored("A1".to_string());
        assert!(is_receiving);
    }

    #[test]
    #[should_panic]
    fn end_stored_panics_if_not_begin_store_block() {
        let (st, _) = BlockReceiverState::<MId>::new().begin_stored("A1".to_string());
        let (new_st, _) = st.end_stored("A1".to_string(), Vec::new());
        // A1 is now EndStoreBlock but should be BeginStoreBlock.
        let _ = new_st.end_stored("A1".to_string(), Vec::new());
    }

    #[test]
    fn end_stored_updates_state_and_child_relations() {
        let (st, _) = BlockReceiverState::<MId>::new().begin_stored("A2".to_string());
        let (new_st, unseen_parents) = st.end_stored("A2".to_string(), parents(&[("A1", true)]));

        assert_eq!(st.receive_st.get("A2"), Some(&RecvStatus::BeginStoreBlock));
        assert_eq!(new_st.receive_st.get("A2"), Some(&RecvStatus::EndStoreBlock));

        assert!(!st.receive_st.contains_key("A1"));
        assert_eq!(new_st.receive_st.get("A1"), Some(&RecvStatus::Requested));
        assert_eq!(unseen_parents, BTreeSet::from(["A1".to_string()]));

        assert_eq!(
            new_st.blocks_st,
            BTreeMap::from([("A2".to_string(), BTreeSet::from(["A1".to_string()]))])
        );
        assert_eq!(
            new_st.child_relations,
            BTreeMap::from([("A1".to_string(), BTreeSet::from(["A2".to_string()]))])
        );
    }

    #[test]
    #[should_panic]
    fn finished_panics_if_block_not_in_state() {
        let _ = BlockReceiverState::<MId>::new().finished("A1".to_string(), BTreeSet::new());
    }

    #[test]
    #[should_panic]
    fn finished_panics_if_block_not_received() {
        let (st, _) = BlockReceiverState::<MId>::new().begin_stored("A1".to_string());
        let _ = st.finished("A1".to_string(), BTreeSet::new());
    }

    #[test]
    fn finished_returns_empty_state_if_all_processed() {
        let (st1, _) = BlockReceiverState::<MId>::new().begin_stored("A1".to_string());
        let (st2, _) = st1.end_stored("A1".to_string(), Vec::new());
        // A1 has no dependencies; finishing it removes it from the state.
        let (st3, _) = st2.finished("A1".to_string(), BTreeSet::new());
        assert!(st3.blocks_st.is_empty());
        assert!(st3.receive_st.is_empty());
        assert!(st3.child_relations.is_empty());
    }

    #[test]
    fn finished_removes_resolved_deps_and_returns_next() {
        let (st1, _) = BlockReceiverState::<MId>::new().begin_stored("A2".to_string());
        assert_eq!(st1.receive_st.get("A2"), Some(&RecvStatus::BeginStoreBlock));

        let (st2, a2_unseen) = st1.end_stored("A2".to_string(), parents(&[("A1", true)]));
        assert_eq!(st2.blocks_st.get("A2"), Some(&BTreeSet::from(["A1".to_string()])));
        assert_eq!(st2.receive_st.get("A2"), Some(&RecvStatus::EndStoreBlock));
        assert_eq!(st2.receive_st.get("A1"), Some(&RecvStatus::Requested));
        assert_eq!(st2.child_relations.get("A1"), Some(&BTreeSet::from(["A2".to_string()])));
        assert_eq!(a2_unseen, BTreeSet::from(["A1".to_string()]));

        let (st3, _) = st2.begin_stored("A1".to_string());
        assert_eq!(st3.receive_st.get("A1"), Some(&RecvStatus::BeginStoreBlock));

        let (st4, a1_unseen) = st3.end_stored("A1".to_string(), Vec::new());
        assert_eq!(st4.blocks_st.get("A1"), Some(&BTreeSet::new()));
        assert_eq!(st4.receive_st.get("A1"), Some(&RecvStatus::EndStoreBlock));
        assert!(a1_unseen.is_empty());

        // Finishing A1 removes it from receive state; child A2 becomes PendingValidation.
        let (st5, deps_validated) = st4.finished("A1".to_string(), BTreeSet::new());
        assert!(!st5.receive_st.contains_key("A1"));
        assert_eq!(st5.receive_st.get("A2"), Some(&RecvStatus::PendingValidation));
        assert_eq!(deps_validated, BTreeSet::from(["A2".to_string()]));
    }
}
