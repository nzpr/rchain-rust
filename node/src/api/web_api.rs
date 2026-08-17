//! Web API interface (port of the `WebApi[F]` trait in `api/WebApi.scala`).

use rchain_models::casper::protocol::deploy_service::{BlockInfo, LightBlockInfo};

use super::dto::{
    ApiStatus, DataAtNameByBlockHashRequest, DataAtNameRequest, DataAtNameResponse,
    DeployExecStatus, DeployRequest, RhoDataResponse,
};
use crate::web::transaction::TransactionResponse;

/// The web API contract (port of `WebApi[F]`; the `F[_]` effect is simplified to synchronous
/// calls).
pub trait WebApi {
    fn status(&self) -> ApiStatus;

    fn deploy(&self, request: &DeployRequest) -> String;

    fn deploy_status(&self, deploy_id: &str) -> DeployExecStatus;

    fn listen_for_data_at_name(&self, request: &DataAtNameRequest) -> DataAtNameResponse;

    fn get_data_at_par(&self, request: &DataAtNameByBlockHashRequest) -> RhoDataResponse;

    fn last_finalized_block(&self) -> BlockInfo;

    fn get_block(&self, hash: &str) -> BlockInfo;

    fn get_blocks(&self, depth: i32) -> Vec<LightBlockInfo>;

    fn find_deploy(&self, deploy_id: &str) -> LightBlockInfo;

    fn exploratory_deploy(
        &self,
        term: &str,
        block_hash: Option<&str>,
        use_pre_state_hash: bool,
    ) -> RhoDataResponse;

    fn get_blocks_by_heights(
        &self,
        start_block_number: i64,
        end_block_number: i64,
    ) -> Vec<LightBlockInfo>;

    fn is_finalized(&self, hash: &str) -> bool;

    fn get_transaction(&self, hash: &str) -> TransactionResponse;
}
