//! Block creation (port of `blocks/proposer/BlockCreator.scala`).

use std::collections::BTreeSet;

use rchain_block_storage::dag::dag_storage::{BlockDagStorage, DeployId};
use rchain_crypto::hash::blake2b256_hash::Blake2b256Hash;
use rchain_models::block_hash::BlockHash;
use rchain_models::block_version::CURRENT;
use rchain_models::casper::protocol::casper_message::{
    ProcessedDeploy, ProcessedSystemDeploy, RholangState, SignedDeployData,
};
use rchain_models::validator::Validator;
use rchain_rholang::system_processes::BlockData;

use crate::block_random_seed::BlockRandomSeed;
use crate::interpreter_util::compute_deploys_checkpoint;
use crate::merging::ParentsMergedState;
use crate::proto_util::unsigned_block_proto;
use crate::rholang::{SystemDeployRuntimeResult, UserDeployRuntimeResult};
use crate::runtime_manager::RuntimeManager;
use crate::system_deploy::SystemDeploy;
use crate::validator_identity::ValidatorIdentity;

use super::propose_result::BlockCreatorResult;

/// A block creator: validator identity + shard (port of `BlockCreator`).
#[derive(Clone, Debug)]
pub struct BlockCreator {
    pub id: ValidatorIdentity,
    pub shard_id: String,
}

type StateTransitionResult = (
    Blake2b256Hash,
    Vec<UserDeployRuntimeResult>,
    Vec<SystemDeployRuntimeResult>,
);

impl BlockCreator {
    /// Create (and sign) a block from the merged pre-state + a set of pooled deploys (port of
    /// `BlockCreator.create`).
    #[allow(clippy::too_many_arguments)]
    pub async fn create(
        &self,
        runtime: &RuntimeManager,
        dag: &dyn BlockDagStorage,
        pre_state: &ParentsMergedState,
        deploys: &[DeployId],
        to_slash: &BTreeSet<Validator>,
        change_epoch: bool,
        suppress_attestation: bool,
    ) -> Result<BlockCreatorResult, String> {
        let pre_state_hash = pre_state.pre_state_hash;
        let parents: Vec<BlockHash> =
            pre_state.justifications.iter().map(|m| m.block_hash).collect();
        let bonds_map = pre_state.fringe_bonds_map.clone();
        let block_num = pre_state
            .justifications
            .iter()
            .map(|m| m.block_num)
            .max()
            .unwrap_or(-1)
            + 1;
        let creators_pk = self.id.public_key.clone();
        let creators_validator = Validator::from_slice(creators_pk.bytes());
        let seq_num = pre_state
            .justifications
            .iter()
            .find(|m| m.sender == creators_validator)
            .map(|m| m.seq_num + 1)
            .unwrap_or(0);
        let block_data = BlockData {
            block_number: block_num,
            sender: creators_pk.clone(),
            seq_num,
        };
        let should_propose = !deploys.is_empty() || !to_slash.is_empty() || change_epoch;
        let finalization = pre_state.fringe_rejected_deploys.clone();

        let post_state: Option<StateTransitionResult> = if should_propose {
            let rand = BlockRandomSeed::random_generator_from(
                &self.shard_id,
                block_num,
                creators_pk.clone(),
                pre_state_hash,
            );

            // Slash + close-block system deploys.
            let mut system_deploys: Vec<SystemDeploy> = Vec::new();
            let mut sorted_to_slash: Vec<&Validator> = to_slash.iter().collect();
            sorted_to_slash.sort();
            for (i, v) in sorted_to_slash.into_iter().enumerate() {
                let seed = rand.split_byte((deploys.len() + i) as u8);
                system_deploys.push(SystemDeploy::slash(v, seed));
            }
            let close_seed = rand.split_byte((deploys.len() + to_slash.len()) as u8);
            system_deploys.push(SystemDeploy::close_block(close_seed));

            // Pooled deploys filtered to the selected ids.
            let pooled = dag.pooled_deploys().await?;
            let deploy_set: BTreeSet<&DeployId> = deploys.iter().collect();
            let selected: Vec<SignedDeployData> = pooled
                .into_iter()
                .filter(|(id, _)| deploy_set.contains(id))
                .map(|(_, d)| d)
                .collect();

            Some(
                compute_deploys_checkpoint(
                    runtime,
                    &selected,
                    &system_deploys,
                    &rand,
                    block_data,
                    &pre_state_hash,
                )
                .await?,
            )
        } else if !suppress_attestation {
            // Attestation: empty state transition over the pre-state.
            Some((pre_state_hash, Vec::new(), Vec::new()))
        } else {
            None
        };

        match post_state {
            None => Ok(BlockCreatorResult::NoNewDeploys),
            Some((post_state_hash, user_results, sys_results)) => {
                let processed_deploys: Vec<ProcessedDeploy> =
                    user_results.into_iter().map(|r| r.deploy).collect();
                let processed_system_deploys: Vec<ProcessedSystemDeploy> =
                    sys_results.into_iter().map(|r| r.deploy).collect();
                let state = RholangState {
                    deploys: processed_deploys,
                    system_deploys: processed_system_deploys,
                };
                let unsigned_block = unsigned_block_proto(
                    CURRENT,
                    self.shard_id.clone(),
                    block_num,
                    creators_validator,
                    seq_num,
                    pre_state_hash.as_bytes().to_vec(),
                    post_state_hash.as_bytes().to_vec(),
                    parents,
                    bonds_map,
                    finalization,
                    state,
                );
                Ok(BlockCreatorResult::Created(self.id.sign_block(&unsigned_block)))
            }
        }
    }
}
