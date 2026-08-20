//! Node DAG implementations (port of `dag/implementation/{NetworkBlockRequester,RNodeDagManager}.scala`).
//!
//! The Scala oracle leaves both of these as `???`; the Rust port implements them against the
//! in-memory stores, so the `todo!()` stubs are closed.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Mutex;

use rchain_sdk::block::BlockRequester;
use rchain_sdk::dag::data::{DagData, DagManager, DagView};

/// The status of a requested block (port of `BlockStatus`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockStatus<B, BId> {
    Requested(BId),
    Received(BId, B),
}

/// A block requester backed by in-memory status (port of `NetworkBlockRequester`).
pub struct NetworkBlockRequester<B, BId: Ord> {
    st: Mutex<BTreeMap<BId, BlockStatus<B, BId>>>,
}

impl<B, BId: Ord> NetworkBlockRequester<B, BId> {
    pub fn new(st: Mutex<BTreeMap<BId, BlockStatus<B, BId>>>) -> Self {
        NetworkBlockRequester { st }
    }
}

impl<B: Clone, BId: Clone + Ord> BlockRequester<B, BId> for NetworkBlockRequester<B, BId> {
    fn request_block(&self, id: &BId) {
        let mut st = self.st.lock().unwrap_or_else(|p| p.into_inner());
        st.insert(id.clone(), BlockStatus::Requested(id.clone()));
    }

    fn response(&self) -> Vec<B> {
        let st = self.st.lock().unwrap_or_else(|p| p.into_inner());
        st.values()
            .filter_map(|status| match status {
                BlockStatus::Received(_, b) => Some(b.clone()),
                BlockStatus::Requested(_) => None,
            })
            .collect()
    }
}

/// A read-only snapshot view of the node DAG, rooted at a latest message (port of `DagView`).
struct RNodeDagView<M, MId: Ord, S, SId: Ord> {
    seen_by: M,
    messages: Vec<(M, Vec<M>)>,
    message_map: BTreeMap<MId, M>,
    sender_map: BTreeMap<SId, S>,
}

impl<M: Clone + 'static, MId: Clone + Ord + 'static, S: Clone + 'static, SId: Clone + Ord + 'static>
    DagView<M, MId, S, SId> for RNodeDagView<M, MId, S, SId>
{
    fn seen_by(&self) -> M {
        self.seen_by.clone()
    }

    fn messages(&self) -> Vec<(M, Vec<M>)> {
        self.messages.clone()
    }

    fn load_message(&self, mid: &MId) -> M {
        self.message_map
            .get(mid)
            .cloned()
            .unwrap_or_else(|| panic!("message not found in DAG view"))
    }

    fn load_sender(&self, sid: &SId) -> S {
        self.sender_map
            .get(sid)
            .cloned()
            .unwrap_or_else(|| panic!("sender not found in DAG view"))
    }
}

/// The node DAG manager (port of `RNodeDagManager`), backed by an in-memory message map, a sender
/// map, and a `DagData` accessor.
pub struct RNodeDagManager<M, MId: Ord, S, SId: Ord, R: BlockRequester<M, MId>, D: DagData<M, MId, S, SId>> {
    st: Mutex<BTreeMap<MId, M>>,
    senders: Mutex<BTreeMap<SId, S>>,
    requester: R,
    dag_data: D,
}

impl<M, MId: Ord, S, SId: Ord, R: BlockRequester<M, MId>, D: DagData<M, MId, S, SId>>
    RNodeDagManager<M, MId, S, SId, R, D>
{
    pub fn new(
        st: Mutex<BTreeMap<MId, M>>,
        senders: Mutex<BTreeMap<SId, S>>,
        requester: R,
        dag_data: D,
    ) -> Self {
        RNodeDagManager {
            st,
            senders,
            requester,
            dag_data,
        }
    }
}

impl<
        M: Clone + 'static,
        MId: Clone + Ord + 'static,
        S: Clone + 'static,
        SId: Clone + Ord + 'static,
        R: BlockRequester<M, MId>,
        D: DagData<M, MId, S, SId>,
    > DagManager<M, MId, S, SId> for RNodeDagManager<M, MId, S, SId, R, D>
{
    fn get_dag_view(&self, seen_by: &MId) -> Box<dyn DagView<M, MId, S, SId>> {
        let (message_map, sender_map) = {
            let st = self.st.lock().unwrap_or_else(|p| p.into_inner());
            let senders = self.senders.lock().unwrap_or_else(|p| p.into_inner());
            (st.clone(), senders.clone())
        };

        // Breadth-first traversal from `seen_by`; collect `(message, parents)` pairs.
        let mut messages: Vec<(M, Vec<M>)> = Vec::new();
        let mut seen: BTreeSet<MId> = BTreeSet::new();
        let mut queue: VecDeque<MId> = VecDeque::new();
        queue.push_back(seen_by.clone());

        while let Some(mid) = queue.pop_front() {
            if !seen.insert(mid.clone()) {
                continue;
            }
            let msg = message_map
                .get(&mid)
                .cloned()
                .unwrap_or_else(|| panic!("DAG view referenced an unknown message"));
            let parent_ids = self.dag_data.justifications(&msg);
            let mut parents = Vec::with_capacity(parent_ids.len());
            for pid in &parent_ids {
                if let Some(p) = message_map.get(pid).cloned() {
                    parents.push(p);
                    queue.push_back(pid.clone());
                }
            }
            messages.push((msg, parents));
        }

        let seen_by_msg = message_map
            .get(seen_by)
            .cloned()
            .unwrap_or_else(|| panic!("DAG view referenced an unknown message"));

        Box::new(RNodeDagView {
            seen_by: seen_by_msg,
            messages,
            message_map,
            sender_map,
        })
    }

    fn latest_messages(&self) -> Vec<(S, Vec<M>)> {
        let st = self.st.lock().unwrap_or_else(|p| p.into_inner());
        let all: Vec<M> = st.values().cloned().collect();
        // A message is a tip iff no other message justifies it.
        let justified: BTreeSet<MId> = all
            .iter()
            .flat_map(|m| self.dag_data.justifications(m))
            .collect();

        let mut by_sender: BTreeMap<SId, Vec<M>> = BTreeMap::new();
        for m in all {
            let sid = self.dag_data.sender(&m);
            let mid = self.dag_data.mid(&m);
            if !justified.contains(&mid) {
                by_sender.entry(sid).or_default().push(m);
            }
        }

        let senders = self.senders.lock().unwrap_or_else(|p| p.into_inner());
        by_sender
            .into_iter()
            .filter_map(|(sid, msgs)| senders.get(&sid).cloned().map(|s| (s, msgs)))
            .collect()
    }

    fn insert(&self, msg: M, _finalized: Vec<MId>, _provisionally_finalized: bool) {
        let mid = self.dag_data.mid(&msg);
        let mut st = self.st.lock().unwrap_or_else(|p| p.into_inner());
        st.insert(mid, msg);
    }

    fn load_message(&self, mid: &MId) -> M {
        let st = self.st.lock().unwrap_or_else(|p| p.into_inner());
        st.get(mid)
            .cloned()
            .unwrap_or_else(|| panic!("message not found"))
    }

    fn load_sender(&self, sid: &SId) -> S {
        let senders = self.senders.lock().unwrap_or_else(|p| p.into_inner());
        senders
            .get(sid)
            .cloned()
            .unwrap_or_else(|| panic!("sender not found"))
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

    #[derive(Clone, Debug)]
    struct Msg {
        id: i32,
        sender: i32,
        parents: Vec<i32>,
    }

    struct Data;
    impl DagData<Msg, i32, i32, i32> for Data {
        fn mid(&self, m: &Msg) -> i32 {
            m.id
        }
        fn seq_num(&self, m: &Msg) -> i64 {
            m.id as i64
        }
        fn block_num(&self, m: &Msg) -> i64 {
            m.id as i64
        }
        fn justifications(&self, m: &Msg) -> Vec<i32> {
            m.parents.clone()
        }
        fn sender(&self, m: &Msg) -> i32 {
            m.sender
        }
        fn bonds_map(&self, _m: &Msg) -> Vec<(i32, i64)> {
            vec![]
        }
        fn sid(&self, _s: &i32) -> i32 {
            0
        }
    }

    struct Req;
    impl BlockRequester<Msg, i32> for Req {
        fn request_block(&self, _id: &i32) {}
        fn response(&self) -> Vec<Msg> {
            vec![]
        }
    }

    fn manager() -> RNodeDagManager<Msg, i32, i32, i32, Req, Data> {
        RNodeDagManager::new(
            Mutex::new(BTreeMap::new()),
            Mutex::new(BTreeMap::new()),
            Req,
            Data,
        )
    }

    #[test]
    fn insert_and_load_message() {
        let mgr = manager();
        mgr.insert(
            Msg { id: 0, sender: 7, parents: vec![] },
            vec![],
            false,
        );
        let m = mgr.load_message(&0);
        assert_eq!(m.id, 0);
        assert_eq!(m.sender, 7);
    }

    #[test]
    fn dag_view_traverses_justifications() {
        let mgr = manager();
        mgr.insert(
            Msg { id: 0, sender: 1, parents: vec![] },
            vec![],
            false,
        );
        mgr.insert(
            Msg { id: 1, sender: 1, parents: vec![0] },
            vec![],
            false,
        );
        let view = mgr.get_dag_view(&1);
        assert_eq!(view.seen_by().id, 1);
        assert_eq!(view.messages().len(), 2);
    }
}
