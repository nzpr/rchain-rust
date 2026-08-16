//! Casper merge index data structures (Law 17: merge determinism).
//!
//! Ports the pure data types and conflict/dependency relations from `casper/.../merging/`. The
//! effectful constructors (`DeployChainIndex.apply`, `BlockIndex.apply`, `MergeScope.merge`) are
//! deferred pending the runtime/history wiring.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};

use rchain_block_storage::dag::finalizer::Message;
use rchain_block_storage::dag::message_map;
use rchain_crypto::hash::blake2b256_hash::Blake2b256Hash;
use rchain_models::block_hash::BlockHash;
use rchain_models::block_metadata::BlockMetadata;
use rchain_models::casper::protocol::casper_message::{
    Event, ProcessedDeploy, ProcessedSystemDeploy, SystemDeployData,
};
use rchain_models::validator::Validator;
use rchain_rspace::history::history_repository::HistoryRepository;
use rchain_rspace::merger::event_log_index::{EventLogIndex, NumberChannelsDiff};
use rchain_rspace::merger::event_log_merging_logic::{are_conflicting, depends};
use rchain_rspace::merger::state_change::StateChange;
use rchain_rspace::trace::event::{Event as REvent, Produce};
use rchain_sdk::dag::merging::{compute_dependency_map, compute_greedy_non_intersecting_branches};
use rchain_shared::serialize::Serialize;

use crate::event_converter::to_rspace_event;

/// A deploy id paired with its execution cost (port of `DeployIdWithCost`).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeployIdWithCost {
    pub id: Vec<u8>,
    pub cost: i64,
}

/// The index of a single deploy (port of `DeployIndex`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeployIndex {
    pub deploy_id: Vec<u8>,
    pub cost: i64,
    pub event_log_index: EventLogIndex,
}

impl Ord for DeployIndex {
    fn cmp(&self, other: &Self) -> Ordering {
        self.deploy_id.cmp(&other.deploy_id)
    }
}

impl PartialOrd for DeployIndex {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl DeployIndex {
    pub const SYS_SLASH_DEPLOY_COST: i64 = 0;
    pub const SYS_CLOSE_BLOCK_DEPLOY_COST: i64 = 0;
    pub const SYS_EMPTY_DEPLOY_COST: i64 = 0;

    pub fn sys_slash_deploy_id() -> Vec<u8> {
        vec![1]
    }
    pub fn sys_close_block_deploy_id() -> Vec<u8> {
        vec![2]
    }
    pub fn sys_empty_deploy_id() -> Vec<u8> {
        vec![3]
    }
}

/// The merged state seen by a block's parents (port of `ParentsMergedState`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParentsMergedState {
    pub justifications: Vec<BlockMetadata>,
    pub max_block_num: i64,
    pub max_seq_nums: BTreeMap<Validator, i64>,
    pub fringe: BTreeSet<BlockHash>,
    pub fringe_state: Blake2b256Hash,
    pub fringe_bonds_map: BTreeMap<Validator, i64>,
    pub fringe_rejected_deploys: BTreeSet<Vec<u8>>,
    pub pre_state_hash: Blake2b256Hash,
    pub rejected_deploys: BTreeSet<Vec<u8>>,
}

/// The index of deploys depending on each other within a single block (port of `DeployChainIndex`).
#[derive(Clone, Debug)]
pub struct DeployChainIndex {
    pub host_block: Blake2b256Hash,
    pub deploys_with_cost: BTreeSet<DeployIdWithCost>,
    pub pre_state_hash: Blake2b256Hash,
    pub post_state_hash: Blake2b256Hash,
    pub event_log_index: EventLogIndex,
    pub state_changes: StateChange,
}

// Equality/hash are over `deploysWithCost` only (the Scala override), to speed up rejection-option
// computation.
impl PartialEq for DeployChainIndex {
    fn eq(&self, other: &Self) -> bool {
        self.deploys_with_cost == other.deploys_with_cost
    }
}

impl Eq for DeployChainIndex {}

impl Hash for DeployChainIndex {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.deploys_with_cost.hash(state);
    }
}

impl DeployChainIndex {
    /// The total cost of the deploy chain (port of `deployChainCost`).
    pub fn deploy_chain_cost(r: &DeployChainIndex) -> i64 {
        r.deploys_with_cost.iter().map(|d| d.cost).sum()
    }

    /// Whether `a` depends on `b` (port of `depends`).
    pub fn depends(a: &DeployChainIndex, b: &DeployChainIndex) -> bool {
        depends(&a.event_log_index, &b.event_log_index)
    }

    /// Whether two sets of deploy chains conflict (port of `branchesAreConflicting`).
    pub fn branches_are_conflicting(
        a: &BTreeSet<DeployChainIndex>,
        b: &BTreeSet<DeployChainIndex>,
    ) -> bool {
        let a_ids: BTreeSet<&Vec<u8>> = a
            .iter()
            .flat_map(|d| d.deploys_with_cost.iter().map(|x| &x.id))
            .collect();
        let b_ids: BTreeSet<&Vec<u8>> = b
            .iter()
            .flat_map(|d| d.deploys_with_cost.iter().map(|x| &x.id))
            .collect();
        let a_event = a
            .iter()
            .fold(EventLogIndex::empty(), |acc, d| EventLogIndex::combine(&acc, &d.event_log_index));
        let b_event = b
            .iter()
            .fold(EventLogIndex::empty(), |acc, d| EventLogIndex::combine(&acc, &d.event_log_index));
        !a_ids.is_disjoint(&b_ids) || are_conflicting(&a_event, &b_event)
    }

    /// Whether two deploy chains conflict (port of `deploysAreConflicting`).
    pub fn deploys_are_conflicting(a: &DeployChainIndex, b: &DeployChainIndex) -> bool {
        let a_ids: BTreeSet<&Vec<u8>> = a.deploys_with_cost.iter().map(|x| &x.id).collect();
        let b_ids: BTreeSet<&Vec<u8>> = b.deploys_with_cost.iter().map(|x| &x.id).collect();
        !a_ids.is_disjoint(&b_ids) || are_conflicting(&a.event_log_index, &b.event_log_index)
    }

    /// Build a deploy chain from its member deploys + pre/post state (port of
    /// `DeployChainIndex.apply`).
    pub async fn apply<C, P, A, K>(
        host_block: Blake2b256Hash,
        deploys: &BTreeSet<DeployIndex>,
        pre_state_hash: Blake2b256Hash,
        post_state_hash: Blake2b256Hash,
        history_repository: &HistoryRepository<C, P, A, K>,
    ) -> Result<DeployChainIndex, String>
    where
        C: Serialize<C> + Send + Sync + 'static,
        P: Serialize<P> + Send + Sync + 'static,
        A: Serialize<A> + Send + Sync + 'static,
        K: Serialize<K> + Send + Sync + 'static,
    {
        let deploys_with_cost: BTreeSet<DeployIdWithCost> = deploys
            .iter()
            .map(|d| DeployIdWithCost {
                id: d.deploy_id.clone(),
                cost: d.cost,
            })
            .collect();
        let event_log_index = deploys.iter().fold(EventLogIndex::empty(), |acc, d| {
            EventLogIndex::combine(&acc, &d.event_log_index)
        });

        let pre_reader = history_repository.get_history_reader(pre_state_hash).await;
        let pre_binary = pre_reader.reader_binary();
        let post_reader = history_repository.get_history_reader(post_state_hash).await;
        let post_binary = post_reader.reader_binary();

        let state_changes = StateChange::apply(
            pre_binary.as_ref(),
            post_binary.as_ref(),
            &event_log_index,
        )
        .await?;

        Ok(DeployChainIndex {
            host_block,
            deploys_with_cost,
            pre_state_hash,
            post_state_hash,
            event_log_index,
            state_changes,
        })
    }
}

/// The index of a block: its deploy chains (port of `BlockIndex`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockIndex {
    pub block_hash: BlockHash,
    pub deploy_chains: Vec<DeployChainIndex>,
}

impl BlockIndex {
    /// Build an `EventLogIndex` from a deploy's events against the pre-state (port of
    /// `BlockIndex.createEventLogIndex`).
    pub async fn create_event_log_index<C, P, A, K>(
        events: &[Event],
        history_repository: &HistoryRepository<C, P, A, K>,
        pre_state_hash: Blake2b256Hash,
        mergeable_chs: NumberChannelsDiff,
    ) -> Result<EventLogIndex, String>
    where
        C: Serialize<C> + Send + Sync + 'static,
        P: Serialize<P> + Send + Sync + 'static,
        A: Serialize<A> + Send + Sync + 'static,
        K: Serialize<K> + Send + Sync + 'static,
    {
        let pre_reader = history_repository.get_history_reader(pre_state_hash).await;
        let rspace_events: Vec<REvent> = events.iter().map(to_rspace_event).collect();

        // Collect the distinct produces referenced by the trace to resolve the two pre-state
        // predicates (`produceExistsInPreState` and `produceTouchesPreStateJoin`).
        let mut produces: BTreeSet<Produce> = BTreeSet::new();
        for e in &rspace_events {
            match e {
                REvent::Produce(p) => {
                    produces.insert(p.clone());
                }
                REvent::Comm(c) => {
                    for p in &c.produces {
                        produces.insert(p.clone());
                    }
                }
                REvent::Consume(_) => {}
            }
        }

        let mut exists_in_pre_state: BTreeSet<Produce> = BTreeSet::new();
        let mut touches_pre_state_join: BTreeSet<Produce> = BTreeSet::new();
        for p in &produces {
            let data = pre_reader
                .get_data(p.channels_hash)
                .await
                .map_err(|e| e.to_string())?;
            if data.iter().any(|d| d.source == *p) {
                exists_in_pre_state.insert(p.clone());
            }
            let joins = pre_reader
                .get_joins(p.channels_hash)
                .await
                .map_err(|e| e.to_string())?;
            if joins.iter().any(|j| j.len() > 1) {
                touches_pre_state_join.insert(p.clone());
            }
        }

        Ok(EventLogIndex::apply(
            &rspace_events,
            |p| exists_in_pre_state.contains(p),
            |p| touches_pre_state_join.contains(p),
            mergeable_chs,
        ))
    }

    /// Build a `BlockIndex` from the processed deploys + mergeable-channel data (port of
    /// `BlockIndex.apply`).
    #[allow(clippy::too_many_arguments)]
    pub async fn apply<C, P, A, K>(
        block_hash: BlockHash,
        usr_processed_deploys: &[ProcessedDeploy],
        sys_processed_deploys: &[ProcessedSystemDeploy],
        pre_state_hash: Blake2b256Hash,
        post_state_hash: Blake2b256Hash,
        history_repository: &HistoryRepository<C, P, A, K>,
        mergeable_chan_data: &[NumberChannelsDiff],
    ) -> Result<BlockIndex, String>
    where
        C: Serialize<C> + Send + Sync + 'static,
        P: Serialize<P> + Send + Sync + 'static,
        A: Serialize<A> + Send + Sync + 'static,
        K: Serialize<K> + Send + Sync + 'static,
    {
        let usr_count = usr_processed_deploys.len();
        let deploy_count = usr_count + sys_processed_deploys.len();
        let mrg_count = mergeable_chan_data.len();
        assert_eq!(
            deploy_count, mrg_count,
            "Cache of mergeable channels ({mrg_count}) doesn't match deploys count ({deploy_count})."
        );

        let (usr_mergeable, sys_mergeable) = mergeable_chan_data.split_at(usr_count);

        let mut deploy_indices: BTreeSet<DeployIndex> = BTreeSet::new();

        // User deploy indices (failed deploys are skipped).
        for (d, merge_chs) in usr_processed_deploys.iter().zip(usr_mergeable.iter()) {
            if d.is_failed {
                continue;
            }
            let event_log_index = Self::create_event_log_index(
                &d.deploy_log,
                history_repository,
                pre_state_hash,
                merge_chs.clone(),
            )
            .await?;
            deploy_indices.insert(DeployIndex {
                deploy_id: d.deploy.sig.clone(),
                cost: d.cost.cost as i64,
                event_log_index,
            });
        }

        // System deploy indices (only `Succeeded` blocks contribute).
        for (sd, merge_chs) in sys_processed_deploys.iter().zip(sys_mergeable.iter()) {
            let (id, log) = match sd {
                ProcessedSystemDeploy::Succeeded {
                    event_list,
                    system_deploy: SystemDeployData::Slash(_),
                } => (sys_deploy_id(&block_hash, 1), event_list),
                ProcessedSystemDeploy::Succeeded {
                    event_list,
                    system_deploy: SystemDeployData::CloseBlock,
                } => (sys_deploy_id(&block_hash, 2), event_list),
                ProcessedSystemDeploy::Succeeded {
                    event_list,
                    system_deploy: SystemDeployData::Empty,
                } => (sys_deploy_id(&block_hash, 3), event_list),
                ProcessedSystemDeploy::Failed { .. } => continue,
            };
            let event_log_index = Self::create_event_log_index(
                log,
                history_repository,
                pre_state_hash,
                merge_chs.clone(),
            )
            .await?;
            deploy_indices.insert(DeployIndex {
                deploy_id: id,
                cost: 0,
                event_log_index,
            });
        }

        // Deploys in a block execute sequentially, so there are only dependencies (no conflicts).
        let dependency_map =
            compute_dependency_map(&deploy_indices, &deploy_indices, |l, r| {
                depends(&l.event_log_index, &r.event_log_index)
            });
        let deploy_chains =
            compute_greedy_non_intersecting_branches(&deploy_indices, &dependency_map);

        let host_block = Blake2b256Hash::from_bytes(*block_hash.as_bytes());
        let mut chains = Vec::new();
        for chain in &deploy_chains {
            chains.push(
                DeployChainIndex::apply(
                    host_block,
                    chain,
                    pre_state_hash,
                    post_state_hash,
                    history_repository,
                )
                .await?,
            );
        }

        Ok(BlockIndex {
            block_hash,
            deploy_chains: chains,
        })
    }
}

/// Build the system-deploy id as `blockHash ++ prefix` (port of `blockHash.concat(SYS_*_DEPLOY_ID)`).
fn sys_deploy_id(block_hash: &BlockHash, prefix: u8) -> Vec<u8> {
    let mut id = block_hash.as_bytes().to_vec();
    id.push(prefix);
    id
}

/// The scope of a merge: final (immutable) and conflict (alterable) blocks (port of `MergeScope`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MergeScope {
    pub final_scope: BTreeSet<BlockHash>,
    pub conflict_scope: BTreeSet<BlockHash>,
}

impl MergeScope {
    /// Create a merge scope from the DAG (port of `MergeScope.fromDag`).
    pub fn from_dag(
        merge_fringe: &BTreeSet<BlockHash>,
        final_fringe: &BTreeSet<BlockHash>,
        child_map: &BTreeMap<BlockHash, BTreeSet<BlockHash>>,
        dag_data: &BTreeMap<BlockHash, Message<BlockHash, Validator>>,
    ) -> (MergeScope, Option<BlockHash>) {
        let prune_fringe: BTreeSet<BlockHash> =
            message_map::prune_fringe(dag_data, final_fringe, child_map)
                .iter()
                .map(|m| m.id)
                .collect();
        MergeScope::from_fringes(merge_fringe, final_fringe, &prune_fringe, dag_data)
    }

    /// Create a merge scope from explicit fringes (port of `MergeScope.fromFringes`).
    pub fn from_fringes(
        merge_fringe: &BTreeSet<BlockHash>,
        final_fringe: &BTreeSet<BlockHash>,
        prune_fringe: &BTreeSet<BlockHash>,
        dag_data: &BTreeMap<BlockHash, Message<BlockHash, Validator>>,
    ) -> (MergeScope, Option<BlockHash>) {
        let merge_msgs: BTreeSet<Message<BlockHash, Validator>> = merge_fringe
            .iter()
            .map(|h| dag_data.get(h).expect("merge fringe not in dag").clone())
            .collect();
        let final_msgs: BTreeSet<Message<BlockHash, Validator>> = final_fringe
            .iter()
            .map(|h| dag_data.get(h).expect("final fringe not in dag").clone())
            .collect();
        let prune_msgs: BTreeSet<Message<BlockHash, Validator>> = prune_fringe
            .iter()
            .map(|h| dag_data.get(h).expect("prune fringe not in dag").clone())
            .collect();

        let c_scope = message_map::between(dag_data, &merge_msgs, &final_msgs);
        let f_scope = message_map::between(dag_data, &final_msgs, &prune_msgs);

        let f_scope_ids: BTreeSet<BlockHash> = f_scope.iter().map(|m| m.id).collect();
        let base_msg = if f_scope.is_empty() {
            let genesis = message_map::find_with_empty_parents(dag_data)
                .expect("Final scope is empty but no genesis found.");
            Some(genesis.id)
        } else {
            None
        };
        let c_scope_ids: BTreeSet<BlockHash> = c_scope.iter().map(|m| m.id).collect();
        let conflict_scope: BTreeSet<BlockHash> = c_scope_ids
            .difference(&base_msg.into_iter().collect())
            .copied()
            .collect();

        (
            MergeScope {
                final_scope: f_scope_ids,
                conflict_scope,
            },
            base_msg,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain(id: u8, cost: i64) -> DeployChainIndex {
        DeployChainIndex {
            host_block: Blake2b256Hash::from_bytes([id; 32]),
            deploys_with_cost: BTreeSet::from([DeployIdWithCost {
                id: vec![id],
                cost,
            }]),
            pre_state_hash: Blake2b256Hash::from_bytes([0u8; 32]),
            post_state_hash: Blake2b256Hash::from_bytes([0u8; 32]),
            event_log_index: EventLogIndex::empty(),
            state_changes: StateChange::empty(),
        }
    }

    #[test]
    fn deploy_chain_cost_sums() {
        let a = chain(1, 5);
        let b = chain(2, 7);
        assert_eq!(DeployChainIndex::deploy_chain_cost(&a), 5);
        assert_eq!(DeployChainIndex::deploy_chain_cost(&b), 7);
    }

    #[test]
    fn equality_is_by_deploys_with_cost() {
        let a = chain(1, 5);
        let mut b = chain(1, 5);
        // Different host block — equality is over deploysWithCost only.
        b.host_block = Blake2b256Hash::from_bytes([9; 32]);
        assert_eq!(a, b);
        // Different cost — not equal.
        let c = chain(1, 6);
        assert_ne!(a, c);
    }

    #[test]
    fn deploys_are_conflicting_by_shared_id() {
        let a = chain(1, 5);
        let mut b = chain(1, 5);
        // Same deploy id -> conflicting (shared id), regardless of event logs.
        b.host_block = Blake2b256Hash::from_bytes([9; 32]);
        assert!(DeployChainIndex::deploys_are_conflicting(&a, &b));
        // Different ids, empty event logs -> not conflicting.
        assert!(!DeployChainIndex::deploys_are_conflicting(&chain(1, 5), &chain(2, 5)));
    }

    fn msg(id: u8, parents: &[u8], seen: &[u8]) -> Message<BlockHash, Validator> {
        let hash = |b: u8| BlockHash::new([b; 32]);
        Message {
            id: hash(id),
            height: 0,
            sender: Validator::new([id; 65]),
            sender_seq: 0,
            bonds_map: BTreeMap::new(),
            parents: parents.iter().map(|&b| hash(b)).collect(),
            fringe: BTreeSet::new(),
            seen: seen.iter().map(|&b| hash(b)).collect(),
        }
    }

    #[test]
    fn merge_scope_genesis_returns_genesis_as_base() {
        let g = msg(1, &[], &[1]);
        let c = msg(2, &[1], &[1, 2]);
        let dag = BTreeMap::from([(g.id, g.clone()), (c.id, c.clone())]);

        let (scope, base) = MergeScope::from_fringes(
            &[c.id].into_iter().collect(),
            &BTreeSet::new(),
            &BTreeSet::new(),
            &dag,
        );
        // Final scope empty -> genesis is the base; conflict scope excludes it.
        assert!(scope.final_scope.is_empty());
        assert_eq!(scope.conflict_scope, [c.id].into_iter().collect());
        assert_eq!(base, Some(g.id));
    }
}
