//! Node state setup (port of `dag/RNodeStateSetup.scala`).

use std::collections::BTreeMap;
use std::sync::Mutex;

use rchain_sdk::dag::data::DagData;

use super::implementation::{BlockStatus, NetworkBlockRequester, RNodeDagManager};

/// Wire the node DAG state (port of `RNodeStateSetup.setupRNodeState`).
pub fn setup_rnode_state<M: Clone, MId: Clone + Ord, S, SId: Ord, D: DagData<M, MId, S, SId>>(
    senders: Mutex<BTreeMap<SId, S>>,
    dag_data: D,
) -> RNodeDagManager<M, MId, S, SId, NetworkBlockRequester<M, MId>, D> {
    let block_req_st: Mutex<BTreeMap<MId, BlockStatus<M, MId>>> = Mutex::new(BTreeMap::new());
    let dag_mngr_st: Mutex<BTreeMap<MId, M>> = Mutex::new(BTreeMap::new());

    let requester = NetworkBlockRequester::new(block_req_st);
    RNodeDagManager::new(dag_mngr_st, senders, requester, dag_data)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn setup_wires_the_dag_manager() {
        let _mgr = setup_rnode_state::<Msg, i32, i32, i32, Data>(
            Mutex::new(BTreeMap::new()),
            Data,
        );
    }
}
