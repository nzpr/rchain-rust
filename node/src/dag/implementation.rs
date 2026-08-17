//! Node DAG implementations (port of `dag/implementation/{NetworkBlockRequester,RNodeDagManager}.scala`).

use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::sync::Mutex;

use rchain_sdk::block::BlockRequester;
use rchain_sdk::dag::data::{DagManager, DagView};

/// The status of a requested block (port of `BlockStatus`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockStatus<B, BId> {
    Requested(BId),
    Received(BId, B),
}

/// A block requester backed by in-memory status (port of `NetworkBlockRequester`; the body is a
/// TODO stub in the Scala oracle).
#[allow(dead_code)] // `st` is stored for the (unimplemented) stub methods
pub struct NetworkBlockRequester<B, BId: Ord> {
    st: Mutex<BTreeMap<BId, BlockStatus<B, BId>>>,
}

impl<B, BId: Ord> NetworkBlockRequester<B, BId> {
    pub fn new(st: Mutex<BTreeMap<BId, BlockStatus<B, BId>>>) -> Self {
        NetworkBlockRequester { st }
    }
}

impl<B, BId: Ord> BlockRequester<B, BId> for NetworkBlockRequester<B, BId> {
    fn request_block(&self, _id: &BId) {
        todo!()
    }

    fn response(&self) -> Vec<B> {
        todo!()
    }
}

/// The node DAG manager (port of `RNodeDagManager`; the body is a TODO stub in the Scala oracle).
#[allow(dead_code)] // `st`/`requester` are stored for the (unimplemented) stub methods
pub struct RNodeDagManager<M, MId: Ord, S, SId, R: BlockRequester<M, MId>> {
    st: Mutex<BTreeMap<MId, M>>,
    requester: R,
    _marker: PhantomData<(S, SId)>,
}

impl<M, MId: Ord, S, SId, R: BlockRequester<M, MId>> RNodeDagManager<M, MId, S, SId, R> {
    pub fn new(st: Mutex<BTreeMap<MId, M>>, requester: R) -> Self {
        RNodeDagManager {
            st,
            requester,
            _marker: PhantomData,
        }
    }
}

impl<M, MId: Ord, S, SId, R: BlockRequester<M, MId>> DagManager<M, MId, S, SId>
    for RNodeDagManager<M, MId, S, SId, R>
{
    fn get_dag_view(&self, _seen_by: &MId) -> Box<dyn DagView<M, MId, S, SId>> {
        todo!()
    }

    fn latest_messages(&self) -> Vec<(S, Vec<M>)> {
        todo!()
    }

    fn insert(&self, _msg: M, _finalized: Vec<MId>, _provisionally_finalized: bool) {
        todo!()
    }

    fn load_message(&self, _mid: &MId) -> M {
        todo!()
    }

    fn load_sender(&self, _sid: &SId) -> S {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_status_variants() {
        assert_eq!(
            BlockStatus::<(), u32>::Requested(1),
            BlockStatus::<(), u32>::Requested(1)
        );
        assert_eq!(
            BlockStatus::Received(1u32, "b"),
            BlockStatus::<&str, u32>::Received(1, "b")
        );
        assert_ne!(
            BlockStatus::<u32, u32>::Requested(1),
            BlockStatus::<u32, u32>::Received(1, 2)
        );
    }
}
