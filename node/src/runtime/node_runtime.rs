//! Node runtime assembly (port of `runtime/Setup.scala` + `runtime/NodeRuntime.scala`).
//!
//! Assembles the store manager → RSpace → RhoRuntime → RuntimeManager → BlockApiImpl →
//! GrpcServices/WebApi/AdminWebApi chain and serves it over gRPC + HTTP. The comm/transport/
//! discovery layer, the proposer, the block receiver/processor streams, the NodeLaunch state
//! machines, and the report-store codec are deferred.

use std::sync::Arc;

use prost::Message;

use rchain_block_storage::block_store;
use rchain_block_storage::dag::codecs::{
    Blake2b256HashCodec, BlockHashCodec, BlockMetadataCodec, FringeDataCodec,
    SignedDeployDataCodec,
};
use rchain_block_storage::dag::dag_storage::{BlockDagStorage, DeployId};
use rchain_casper::api::block_api_impl::{BlockApiImpl, NetworkStatus};
use rchain_casper::api::block_report_api::BlockReportApi;
use rchain_casper::block_metadata_store::BlockMetadataStore;
use rchain_casper::dag::BlockDagKeyValueStorage;
use rchain_casper::reporting::noop;
use rchain_casper::runtime_manager::RuntimeManager;
use rchain_casper::storage::rnode_key_value_store_manager;
use rchain_casper::validator_identity::ValidatorIdentity;
use rchain_comm::peer_node::NodeIdentifier;
use rchain_crypto::hash::blake2b256_hash::Blake2b256Hash;
use rchain_models::ast::Par;
use rchain_models::block_hash::BlockHash;
use rchain_models::block_metadata::BlockMetadata;
use rchain_models::casper::protocol::casper_message::SignedDeployData;
use rchain_models::casper::protocol::report::BlockEventInfo;
use rchain_models::fringe_data::FringeData;
use rchain_models::runtime::{BindPattern, ListParWithRandom, TaggedContinuation};
use rchain_rholang::merging::DeployMergeableDataCodec;
use rchain_rholang::runtime::{ReplayRhoRuntime, RhoRuntime};
use rchain_rholang::storage::RhoMatch;
use rchain_rspace::factory::create_history_repository;
use rchain_rspace::hot_store::InMemHotStore;
use rchain_rspace::rspace::RSpace;
use rchain_shared::refined::Port;
use rchain_shared::store_manager::database;
use rchain_shared::typed_store::{BytesCodec, Codec, KeyValueTypedStore};

use crate::api::admin_web_api::AdminWebApi;
use crate::api::admin_web_api_impl::AdminWebApiImpl;
use crate::api::grpc::GrpcServices;
use crate::api::web_api::WebApi;
use crate::api::web_api_impl::WebApiImpl;
use crate::configuration::model::NodeConf;
use crate::diagnostics::NewPrometheusReporter;
use crate::web::http::{acquire_admin_http_server, acquire_http_server};
use crate::web::transaction::{TransactionApi, TransactionInfo};

/// A no-op transaction API (the cache-backed `TransactionAPI` is deferred).
struct NoopTransactionApi;

impl TransactionApi for NoopTransactionApi {
    fn get_transaction(&self, _block_hash: &Blake2b256Hash) -> Vec<TransactionInfo> {
        Vec::new()
    }
}

/// The `BlockEventInfo` report-store codec (prost wire round-trip).
struct BlockEventInfoCodec;

impl Codec<BlockEventInfo> for BlockEventInfoCodec {
    fn encode(&self, value: &BlockEventInfo) -> Vec<u8> {
        crate::api::grpc::tonic::block_event_info_to_wire(value).encode_to_vec()
    }
    fn decode(&self, bytes: &[u8]) -> Result<BlockEventInfo, String> {
        let wire = <rchain_models::proto::casper::BlockEventInfo as prost::Message>::decode(bytes)
            .map_err(|e| e.to_string())?;
        crate::api::grpc::tonic::block_event_info_from_wire(&wire)
    }
}

/// The assembled node program (port of the `setupNodeProgram` result).
pub struct NodeProgram {
    grpc_services: GrpcServices,
    web_api: Arc<dyn WebApi>,
    admin_web_api: Arc<dyn AdminWebApi>,
    block_report_api: Arc<BlockReportApi>,
    reporter: Arc<NewPrometheusReporter>,
    host: String,
    port_http: Port,
    port_admin_http: Port,
    port_grpc_internal: Port,
}

impl NodeProgram {
    /// Serve the gRPC + HTTP servers (port of `NetworkServers.create`; the protocol/discovery
    /// servers are deferred).
    pub async fn serve(self) -> Result<(), String> {
        let grpc_addr: std::net::SocketAddr = format!("{}:{}", self.host, u16::from(self.port_grpc_internal))
            .parse::<std::net::SocketAddr>()
            .map_err(|e| e.to_string())?;

        let grpc = tokio::spawn(self.grpc_services.serve(grpc_addr));

        let host = self.host.clone();
        let reporter = self.reporter.clone();
        let web_api = self.web_api.clone();
        let block_report_api = self.block_report_api.clone();
        let http = tokio::spawn(async move {
            acquire_http_server(
                &host,
                self.port_http,
                reporter,
                web_api,
                block_report_api,
            )
            .await
        });

        let host = self.host.clone();
        let admin_web_api = self.admin_web_api.clone();
        let admin = tokio::spawn(async move {
            acquire_admin_http_server(&host, self.port_admin_http, admin_web_api).await
        });

        let (g, h, a) = tokio::join!(grpc, http, admin);
        g.map_err(|e| e.to_string())??;
        h.map_err(|e| e.to_string())??;
        a.map_err(|e| e.to_string())??;
        Ok(())
    }
}

/// Assemble the node program (port of `Setup.setupNodeProgram`, minus the comm/discovery/proposer/
/// block-stream pieces).
pub async fn setup(conf: &NodeConf, id: &NodeIdentifier) -> Result<NodeProgram, String> {
    let store_manager = rnode_key_value_store_manager(&conf.storage.data_dir);

    // Block store + DAG storage.
    let block_store = block_store::create(&store_manager).await;
    let block_metadata_kv: Arc<dyn KeyValueTypedStore<BlockHash, BlockMetadata>> = Arc::new(
        database(
            &store_manager,
            "block-metadata",
            Arc::new(BlockHashCodec),
            Arc::new(BlockMetadataCodec),
        )
        .await,
    );
    let block_metadata_store = Arc::new(
        BlockMetadataStore::create(block_metadata_kv)
            .await
            .map_err(|e| e.to_string())?,
    );
    let fringe_data_store: Arc<dyn KeyValueTypedStore<Blake2b256Hash, FringeData>> = Arc::new(
        database(
            &store_manager,
            "fringe-data",
            Arc::new(Blake2b256HashCodec),
            Arc::new(FringeDataCodec),
        )
        .await,
    );
    let deploy_index: Arc<dyn KeyValueTypedStore<DeployId, BlockHash>> = Arc::new(
        database(
            &store_manager,
            "deploy-index",
            Arc::new(BytesCodec),
            Arc::new(BlockHashCodec),
        )
        .await,
    );
    let deploy_store: Arc<dyn KeyValueTypedStore<DeployId, SignedDeployData>> = Arc::new(
        database(
            &store_manager,
            "deploy-pool",
            Arc::new(BytesCodec),
            Arc::new(SignedDeployDataCodec),
        )
        .await,
    );
    let block_dag_storage: Arc<dyn BlockDagStorage> = Arc::new(
        BlockDagKeyValueStorage::create(
            block_metadata_store,
            fringe_data_store,
            deploy_index,
            deploy_store,
        )
        .await
        .map_err(|e| e.to_string())?,
    );

    // Runtime manager (play + replay runtimes + mergeable store).
    let history =
        create_history_repository::<Par, BindPattern, ListParWithRandom, TaggedContinuation>(
            &store_manager,
        )
        .await
        .map_err(|e| e.to_string())?;
    let reader = history.get_history_reader(history.root()).await;
    let hot = Arc::new(InMemHotStore::new(reader.base()));
    let (play, replay) = RSpace::create_with_replay(history.clone(), hot, Arc::new(RhoMatch));
    let rho_runtime = RhoRuntime::create(play.clone(), history.clone(), Par::default())
        .await
        .map_err(|e| e.to_string())?;
    let replay_runtime =
        ReplayRhoRuntime::create(Arc::new(replay), history.clone(), Par::default())
            .await
            .map_err(|e| e.to_string())?;
    let mergeable_store = Arc::new(
        database(
            &store_manager,
            "mergeable-channel-cache",
            Arc::new(BytesCodec),
            Arc::new(DeployMergeableDataCodec),
        )
        .await,
    );
    let runtime_manager = Arc::new(RuntimeManager::new(
        rho_runtime,
        replay_runtime,
        history,
        mergeable_store,
    ));

    // Eval runtime for the Repl service (a second RSpace over the same history; the separate
    // eval-* stores are deferred).
    let eval_history =
        create_history_repository::<Par, BindPattern, ListParWithRandom, TaggedContinuation>(
            &store_manager,
        )
        .await
        .map_err(|e| e.to_string())?;
    let eval_reader = eval_history.get_history_reader(eval_history.root()).await;
    let eval_hot = Arc::new(InMemHotStore::new(eval_reader.base()));
    let (eval_play, _) =
        RSpace::create_with_replay(eval_history.clone(), eval_hot, Arc::new(RhoMatch));
    let eval_runtime = Arc::new(
        RhoRuntime::create(eval_play, eval_history, Par::default())
            .await
            .map_err(|e| e.to_string())?,
    );

    // Validator identity (from the PEM-decrypted private key, if set).
    let validator_opt: Option<ValidatorIdentity> = conf
        .casper
        .validator_private_key
        .as_deref()
        .and_then(ValidatorIdentity::from_hex);

    let network_id = conf.protocol_server.network_id.clone();
    let shard_id = conf.casper.shard_name.clone();
    let network_status: Box<dyn Fn() -> NetworkStatus + Send + Sync> = Box::new({
        let id = id.clone();
        move || NetworkStatus {
            address: id.to_string(),
            peers: 0,
            nodes: 0,
        }
    });

    let block_api: Arc<dyn rchain_casper::api::block_api::BlockApi> = Arc::new(BlockApiImpl::new(
        block_dag_storage,
        block_store.clone(),
        runtime_manager,
        validator_opt.clone(),
        network_id,
        shard_id,
        conf.casper.min_phlo_price,
        env!("CARGO_PKG_VERSION").to_string(),
        network_status,
        conf.casper.validator_private_key.is_none(),
        conf.api_server.max_blocks_limit,
        conf.dev_mode,
        None,
        None,
        conf.autopropose,
        std::collections::BTreeSet::new(),
    ));

    let report_store: Arc<dyn KeyValueTypedStore<BlockHash, BlockEventInfo>> = Arc::new(
        database(
            &store_manager,
            "reporting-cache",
            Arc::new(BlockHashCodec),
            Arc::new(BlockEventInfoCodec),
        )
        .await,
    );
    let block_report_api = Arc::new(BlockReportApi::new(
        block_store.clone(),
        Arc::new(noop()),
        report_store,
        validator_opt,
    ));

    let grpc_services = GrpcServices::build(
        block_api.clone(),
        block_report_api.clone(),
        eval_runtime,
    );
    let web_api: Arc<dyn WebApi> = Arc::new(WebApiImpl::new(
        block_api.clone(),
        Arc::new(NoopTransactionApi),
    ));
    let admin_web_api: Arc<dyn AdminWebApi> = Arc::new(AdminWebApiImpl::new(block_api));

    Ok(NodeProgram {
        grpc_services,
        web_api,
        admin_web_api,
        block_report_api,
        reporter: Arc::new(NewPrometheusReporter::new(
            crate::diagnostics::scrape_data_builder::Configuration::default(),
        )),
        host: conf.api_server.host.clone(),
        port_http: Port::try_from(conf.api_server.port_http).map_err(|e| e.to_string())?,
        port_admin_http: Port::try_from(conf.api_server.port_admin_http).map_err(|e| e.to_string())?,
        port_grpc_internal: Port::try_from(conf.api_server.port_grpc_internal)
            .map_err(|e| e.to_string())?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configuration::configuration::parse_defaults;
    use crate::configuration::hocon::node_conf_from_hocon;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rchain-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn setup_assembles_node_program_over_lmdb() {
        let dir = temp_dir("node-runtime");
        let conf = {
            let defaults = parse_defaults(dir.to_str().unwrap()).unwrap();
            let mut conf = node_conf_from_hocon(&defaults).unwrap();
            conf.storage.data_dir = dir.clone();
            conf.api_server.host = "127.0.0.1".to_string();
            conf
        };

        let id = NodeIdentifier::new(vec![1u8]);

        let program = setup(&conf, &id).await.expect("setup should assemble");
        assert_eq!(program.host, "127.0.0.1");
        assert_eq!(
            u16::from(program.port_http),
            u16::try_from(conf.api_server.port_http).unwrap()
        );
        assert_eq!(
            u16::from(program.port_admin_http),
            u16::try_from(conf.api_server.port_admin_http).unwrap()
        );
        assert_eq!(
            u16::from(program.port_grpc_internal),
            u16::try_from(conf.api_server.port_grpc_internal).unwrap()
        );

        drop(program);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
