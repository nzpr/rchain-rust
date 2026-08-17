//! Tonic (gRPC) bindings for the gRPC service adapters.
//!
//! Bridges the tonic-generated `ProposeService`/`Repl` traits (whose message types are the prost
//! wire types in `rchain_models::proto`) to the hand-written adapters in this module. The
//! `DeployService` binding is added incrementally.

use std::pin::Pin;

use tokio_stream::Stream;
use tonic::{Request, Response, Status};

use rchain_models::casper::protocol::casper_message::SignedDeployData;
use rchain_models::casper::protocol::deploy_service::{
    BlockInfo, BlockQuery, BlocksQuery, BlocksQueryByHeight, BondInfo, BondStatusQuery,
    ContinuationAtNameQuery, ContinuationsWithBlockInfo, DataAtNameByBlockQuery, DataAtNameQuery,
    DataWithBlockInfo, DeployExecStatus, DeployInfo, ExploratoryDeployQuery, FindDeployQuery,
    IsFinalizedQuery, LightBlockInfo, MachineVerifyQuery, ReportQuery, ServiceError, Status,
    VersionInfo, VisualizeDagQuery, WaitingContinuationInfo,
};
use rchain_models::casper::protocol::propose_service::{ProposeQuery, ProposeResultQuery};
use rchain_models::casper::protocol::report::{
    BlockEventInfo, DeployInfoWithEventData, ReportCommProto, ReportConsumeProto, ReportProduceProto,
    ReportProto, SingleReport, SystemDeployInfoWithEventData,
};
use rchain_models::proto::casper::deploy_service_server::DeployService;
use rchain_models::proto::casper::propose_service_server::ProposeService;
use rchain_models::proto::casper::{
    propose_response, propose_result_response, ProposeResponse, ProposeResultResponse,
    ServiceError as TonicServiceError,
};
use rchain_models::proto::casper as wire;
use rchain_models::proto::repl::repl_server::Repl;
use rchain_models::proto::repl::ReplResponse as TonicReplResponse;
use rchain_models::wire::{bind_pattern_to_proto, list_par_with_random_to_proto, par_to_proto};

use super::deploy_grpc_service_v1::DeployGrpcServiceV1;
use super::propose_grpc_service_v1::ProposeGrpcServiceV1;
use super::repl_grpc_service::{CmdRequest, EvalRequest, ReplGrpcService};

fn to_tonic_service_error(e: ServiceError) -> TonicServiceError {
    TonicServiceError { messages: e.messages }
}

fn propose_response(r: Result<String, ServiceError>) -> ProposeResponse {
    let message = match r {
        Ok(s) => propose_response::Message::Result(s),
        Err(e) => propose_response::Message::Error(to_tonic_service_error(e)),
    };
    ProposeResponse {
        message: Some(message),
    }
}

fn propose_result_response(r: Result<String, ServiceError>) -> ProposeResultResponse {
    let message = match r {
        Ok(s) => propose_result_response::Message::Result(s),
        Err(e) => propose_result_response::Message::Error(to_tonic_service_error(e)),
    };
    ProposeResultResponse {
        message: Some(message),
    }
}

#[tonic::async_trait]
impl ProposeService for ProposeGrpcServiceV1 {
    async fn propose(
        &self,
        request: Request<rchain_models::proto::casper::ProposeQuery>,
    ) -> Result<Response<ProposeResponse>, Status> {
        let req = request.into_inner();
        let r = self
            .propose(&ProposeQuery {
                is_async: req.is_async,
            })
            .await;
        Ok(Response::new(propose_response(r)))
    }

    async fn propose_result(
        &self,
        _request: Request<rchain_models::proto::casper::ProposeResultQuery>,
    ) -> Result<Response<ProposeResultResponse>, Status> {
        let r = self.propose_result(&ProposeResultQuery).await;
        Ok(Response::new(propose_result_response(r)))
    }
}

#[tonic::async_trait]
impl Repl for ReplGrpcService {
    async fn run(
        &self,
        request: Request<rchain_models::proto::repl::CmdRequest>,
    ) -> Result<Response<TonicReplResponse>, Status> {
        let req = request.into_inner();
        let resp = self.run(&CmdRequest { line: req.line }).await;
        Ok(Response::new(TonicReplResponse {
            output: resp.output,
        }))
    }

    async fn eval(
        &self,
        request: Request<rchain_models::proto::repl::EvalRequest>,
    ) -> Result<Response<TonicReplResponse>, Status> {
        let req = request.into_inner();
        let resp = self
            .eval(&EvalRequest {
                program: req.program,
                print_unmatched_sends_only: req.print_unmatched_sends_only,
            })
            .await;
        Ok(Response::new(TonicReplResponse {
            output: resp.output,
        }))
    }
}
