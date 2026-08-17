//! gRPC service adapters (port of `api/DeployGrpcServiceV1.scala`,
//! `api/ProposeGrpcServiceV1.scala`, `api/ReplGrpcService.scala`, and `runtime/GrpcServices.scala`).
//!
//! The monix-gRPC transport is mapped to plain async service structs over `BlockApi`/`RhoRuntime`;
//! the protobuf `oneof` responses collapse to `Result<_, ServiceError>` (the Scala `Either`). The
//! tonic (gRPC) bindings for these adapters live in [`tonic`].

mod deploy_grpc_service_v1;
mod propose_grpc_service_v1;
mod repl_grpc_service;
pub mod tonic;

pub use deploy_grpc_service_v1::DeployGrpcServiceV1;
pub use propose_grpc_service_v1::ProposeGrpcServiceV1;
pub use repl_grpc_service::{CmdRequest, EvalRequest, ReplGrpcService, ReplResponse};

use std::sync::Arc;

use rchain_casper::api::block_api::BlockApi;
use rchain_casper::api::block_report_api::BlockReportApi;
use rchain_rholang::runtime::RhoRuntime;

/// The trio of node gRPC services (port of `GrpcServices`).
pub struct GrpcServices {
    pub deploy: DeployGrpcServiceV1,
    pub propose: ProposeGrpcServiceV1,
    pub repl: ReplGrpcService,
}

impl GrpcServices {
    /// Build the service trio from the block APIs + runtime (port of `GrpcServices.build`).
    pub fn build(
        block_api: Arc<dyn BlockApi>,
        block_report_api: Arc<BlockReportApi>,
        runtime: Arc<RhoRuntime>,
    ) -> GrpcServices {
        let repl = ReplGrpcService::new(runtime);
        let deploy = DeployGrpcServiceV1::new(block_api.clone(), block_report_api);
        let propose = ProposeGrpcServiceV1::new(block_api);
        GrpcServices {
            deploy,
            propose,
            repl,
        }
    }
}
