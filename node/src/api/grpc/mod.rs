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

    /// Serve the three gRPC services on `addr` (the tonic transport binding). `max_message_size`
    /// bounds each inbound message (the `grpc-max-recv-message-size` config, applied to all three
    /// services so an oversized deploy/repl request is rejected before decoding).
    pub async fn serve(self, addr: std::net::SocketAddr, max_message_size: usize) -> Result<(), String> {
        use rchain_models::proto::casper::deploy_service_server::DeployServiceServer;
        use rchain_models::proto::casper::propose_service_server::ProposeServiceServer;
        use rchain_models::proto::repl::repl_server::ReplServer;

        ::tonic::transport::Server::builder()
            .add_service(DeployServiceServer::new(self.deploy).max_decoding_message_size(max_message_size))
            .add_service(ProposeServiceServer::new(self.propose).max_decoding_message_size(max_message_size))
            .add_service(ReplServer::new(self.repl).max_decoding_message_size(max_message_size))
            .serve(addr)
            .await
            .map_err(|e| e.to_string())
    }
}
