//! Node state setup (port of `dag/RNodeStateSetup.scala`).

use std::collections::BTreeMap;
use std::sync::Mutex;

use super::implementation::{BlockStatus, NetworkBlockRequester, RNodeDagManager};

/// Wire the node DAG state (port of `RNodeStateSetup.setupRNodeState`).
pub fn setup_rnode_state<M, MId: Ord, S, SId>(
) -> RNodeDagManager<M, MId, S, SId, NetworkBlockRequester<M, MId>> {
    let block_req_st: Mutex<BTreeMap<MId, BlockStatus<M, MId>>> = Mutex::new(BTreeMap::new());
    let dag_mngr_st: Mutex<BTreeMap<MId, M>> = Mutex::new(BTreeMap::new());

    let requester = NetworkBlockRequester::new(block_req_st);
    RNodeDagManager::new(dag_mngr_st, requester)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_wires_the_dag_manager() {
        let _mgr = setup_rnode_state::<i32, i32, i32, i32>();
    }
}
