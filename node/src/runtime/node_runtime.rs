//! Node runtime assembly (port of `runtime/Setup.scala` + `runtime/NodeRuntime.scala`).
//!
//! Assembles the store manager → RSpace → RhoRuntime → RuntimeManager → BlockApiImpl →
//! GrpcServices/WebApi/AdminWebApi chain and serves it over gRPC + HTTP. The comm/transport/
//! discovery layer, the proposer, the block receiver/processor streams, the NodeLaunch state
//! machines, and the report-store codec are deferred.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use prost::Message;
use tokio::sync::mpsc;

use rchain_block_storage::approved_store::{self, ApprovedStore};
use rchain_block_storage::block_store::{self, BlockStore};
use rchain_block_storage::dag::codecs::{
    Blake2b256HashCodec, BlockHashCodec, BlockMetadataCodec, FringeDataCodec,
    SignedDeployDataCodec,
};
use rchain_block_storage::dag::dag_storage::{BlockDagStorage, DeployId};
use rchain_block_storage::syntax::put_block;
use rchain_casper::api::block_api_impl::{BlockApiImpl, NetworkStatus, ProposeFunction};
use rchain_casper::api::block_report_api::BlockReportApi;
use rchain_casper::block_metadata_store::BlockMetadataStore;
use rchain_casper::blocks::block_receiver::{self, BlockReceiverState};
use rchain_casper::blocks::block_processor;
use rchain_casper::blocks::block_retriever::BlockRetriever;
use rchain_casper::blocks::proposer::proposer::{Proposer, ProposerResult};
use rchain_casper::merging::BlockIndex;
use rchain_casper::dag::BlockDagKeyValueStorage;
use rchain_casper::engine::node_launch::{self, PeerMessage};
use rchain_casper::protocol::comm_util::{CommUtil, ConnectionsCell};
use rchain_casper::reporting::noop;
use rchain_casper::runtime_manager::RuntimeManager;
use rchain_casper::state::ProposerState;
use rchain_casper::storage::rnode_key_value_store_manager;
use rchain_casper::validator_identity::ValidatorIdentity;
use rchain_comm::peer_node::{NodeIdentifier, PeerNode};
use rchain_comm::rp::handle_messages::{self, RoutingMessage};
use rchain_comm::rp::rp_conf::{ClearConnectionsConf, RPConf};
use rchain_comm::transport::chunker::Blob;
use rchain_comm::transport::communication_response::CommunicationResponse;
use rchain_comm::transport::grpc_transport_client::GrpcTransportClient;
use rchain_comm::transport::grpc_transport_receiver::BoxFuture;
use rchain_comm::transport::grpc_transport_server::TransportLayerServer;
use rchain_comm::transport::transport_layer::TransportLayer;
use rchain_comm::who_am_i;
use rchain_crypto::hash::blake2b256_hash::Blake2b256Hash;
use rchain_models::ast::Par;
use rchain_models::block_hash::BlockHash;
use rchain_models::block_metadata::BlockMetadata;
use rchain_models::casper::protocol::casper_message::{BlockMessage, CasperMessage, SignedDeployData};
use rchain_models::casper::protocol::casper_message_protocol::to_casper_message_proto;
use rchain_models::casper::protocol::report::BlockEventInfo;
use rchain_models::comm::protocol::Protocol;
use rchain_models::fringe_data::FringeData;
use rchain_models::runtime::{BindPattern, ListParWithRandom, TaggedContinuation};
use rchain_rholang::merging::DeployMergeableDataCodec;
use rchain_rholang::runtime::{ReplayRhoRuntime, RhoRuntime};
use rchain_rholang::storage::RhoMatch;
use rchain_rspace::factory::create_history_repository;
use rchain_rspace::hot_store::InMemHotStore;
use rchain_rspace::rspace::RSpace;
use rchain_rspace::state::instances::RSpaceImporterStore;
use rchain_shared::lmdb::LmdbDirStoreManager;
use rchain_shared::log::{Log, LogSource};
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
use crate::instances::proposer_instance;
use crate::web::http::{acquire_admin_http_server, acquire_http_server, StatusProvider};
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

/// The assembled comm/discovery state (port of the `NodeRuntime` transport/comm-state setup; the
/// Kademlia discovery service is deferred).
pub struct CommState {
    pub transport: Arc<dyn TransportLayer>,
    pub connections: ConnectionsCell,
    pub rp_conf: RPConf,
    pub comm_util: Arc<CommUtil>,
    pub block_retriever: Arc<BlockRetriever>,
    pub local_peer: PeerNode,
}

/// Create the comm/discovery state (port of `NodeRuntime.main`'s transport + comm-state setup).
pub async fn create_comm_state(
    conf: &NodeConf,
    id: &NodeIdentifier,
    log: Arc<dyn Log>,
) -> Result<CommState, String> {
    let source = LogSource::new("coop.rchain.node.runtime.NodeRuntime");

    // Fetch the local peer node (blocking external-IP/UPnP discovery at startup).
    let protocol_port = Port::try_from(conf.protocol_server.port).map_err(|e| e.to_string())?;
    let discovery_port = Port::try_from(conf.peers_discovery.port).map_err(|e| e.to_string())?;
    let mut log_buffer = Vec::new();
    let local_peer = who_am_i::fetch_local_peer_node(
        conf.protocol_server.host.clone(),
        protocol_port,
        discovery_port,
        conf.protocol_server.no_upnp,
        id.clone(),
        &mut |msg| log_buffer.push(msg),
    );
    for msg in log_buffer {
        log.info(source, &msg);
    }

    // Transport client (mutual TLS).
    let cert = std::fs::read_to_string(&conf.tls.certificate_path).map_err(|e| e.to_string())?;
    let key = std::fs::read_to_string(&conf.tls.key_path).map_err(|e| e.to_string())?;
    let transport: Arc<dyn TransportLayer> = Arc::new(GrpcTransportClient::new(
        conf.protocol_client.network_id.clone(),
        &cert,
        &key,
        usize::try_from(conf.protocol_client.grpc_max_recv_message_size)
            .map_err(|e| e.to_string())?,
        usize::try_from(conf.protocol_client.grpc_stream_chunk_size)
            .map_err(|e| e.to_string())?,
        100,
    )?);

    // Comm state (connections cell + RPConf).
    let connections: ConnectionsCell = Arc::new(tokio::sync::RwLock::new(Vec::new()));
    let bootstrap = if conf.standalone {
        None
    } else {
        Some(conf.protocol_client.bootstrap.clone())
    };
    let rp_conf = RPConf {
        local: local_peer.clone(),
        network_id: conf.protocol_client.network_id.clone(),
        bootstrap,
        default_timeout: conf.protocol_client.network_timeout,
        max_num_of_connections: usize::try_from(conf.protocol_client.batch_max_connections)
            .map_err(|e| e.to_string())?,
        clear_connections: ClearConnectionsConf {
            num_of_connections_pinged: usize::try_from(
                conf.peers_discovery.heartbeat_batch_size,
            )
            .map_err(|e| e.to_string())?,
        },
    };

    let comm_util = Arc::new(CommUtil::new(
        transport.clone(),
        rp_conf.clone(),
        connections.clone(),
        log.clone(),
    ));
    let block_retriever = Arc::new(BlockRetriever::new(comm_util.clone(), log.clone()));

    Ok(CommState {
        transport,
        connections,
        rp_conf,
        comm_util,
        block_retriever,
        local_peer,
    })
}

/// The transport (protocol) server and its inbound-message dispatch closures (port of
/// `NetworkServers.protocolServer`).
pub struct ProtocolServer {
    server: TransportLayerServer,
    dispatch: Box<dyn Fn(Protocol) -> BoxFuture<CommunicationResponse> + Send + Sync>,
    handle_streamed: Box<dyn Fn(Blob) -> BoxFuture<()> + Send + Sync>,
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
    protocol_server: Option<ProtocolServer>,
    status_provider: Option<StatusProvider>,
}

impl NodeProgram {
    /// Serve the gRPC + HTTP + protocol servers (port of `NetworkServers.create`).
    pub async fn serve(self) -> Result<(), String> {
        let NodeProgram {
            grpc_services,
            web_api,
            admin_web_api,
            block_report_api,
            reporter,
            host,
            port_http,
            port_admin_http,
            port_grpc_internal,
            protocol_server,
            status_provider,
        } = self;

        let grpc_addr: std::net::SocketAddr = format!("{}:{}", host, u16::from(port_grpc_internal))
            .parse::<std::net::SocketAddr>()
            .map_err(|e| e.to_string())?;

        let grpc = tokio::spawn(grpc_services.serve(grpc_addr));

        let http = tokio::spawn({
            let host = host.clone();
            async move {
                acquire_http_server(
                    &host,
                    port_http,
                    reporter,
                    web_api,
                    block_report_api,
                    status_provider,
                )
                .await
            }
        });

        let admin = tokio::spawn({
            let host = host.clone();
            async move { acquire_admin_http_server(&host, port_admin_http, admin_web_api).await }
        });

        if let Some(protocol) = protocol_server {
            let protocol = tokio::spawn(async move {
                protocol
                    .server
                    .serve(protocol.dispatch, protocol.handle_streamed)
                    .await
            });
            let (g, h, a, p) = tokio::join!(grpc, http, admin, protocol);
            g.map_err(|e| e.to_string())??;
            h.map_err(|e| e.to_string())??;
            a.map_err(|e| e.to_string())??;
            p.map_err(|e| e.to_string())??;
        } else {
            let (g, h, a) = tokio::join!(grpc, http, admin);
            g.map_err(|e| e.to_string())??;
            h.map_err(|e| e.to_string())??;
            a.map_err(|e| e.to_string())??;
        }
        Ok(())
    }
}

/// The store/runtime handles extracted from [`setup`], so the block-processing streams can be wired
/// in a separate step.
pub struct SetupParts {
    pub block_store: BlockStore,
    pub dag: Arc<dyn BlockDagStorage>,
    pub runtime_manager: Arc<RuntimeManager>,
    pub approved_store: ApprovedStore,
    pub store_manager: LmdbDirStoreManager,
    pub validator_identity_opt: Option<ValidatorIdentity>,
    pub proposer: Option<ProposerParts>,
}

/// The proposer request queue + shared state (port of the `proposerQueue`/`proposerStateRefOpt` in
/// `Setup.setupNodeProgram`). Built in [`setup`] (so `BlockApiImpl` can get the trigger + state);
/// consumed in [`setup_node_program`] to drive the proposer stream.
pub struct ProposerParts {
    pub queue_tx: mpsc::Sender<(bool, tokio::sync::oneshot::Sender<ProposerResult>)>,
    pub queue_rx: mpsc::Receiver<(bool, tokio::sync::oneshot::Sender<ProposerResult>)>,
    pub state: Arc<tokio::sync::Mutex<ProposerState>>,
}

/// Wire the block receiver + processor streams (port of the `BlockReceiver`/`BlockProcessor` part of
/// `Setup.setupNodeProgram`). Returns the `(incoming_blocks, validated_blocks)` channel senders so the
/// transport and proposer can plug into the pipeline. `NodeLaunch.apply` and the proposer are wired
/// separately.
pub fn wire_block_processing(
    comm_state: &CommState,
    parts: &SetupParts,
    shard_id: &str,
    min_phlo_price: i64,
    log: Arc<dyn Log>,
) -> (
    mpsc::UnboundedSender<BlockMessage>,
    mpsc::UnboundedSender<BlockMessage>,
) {
    let (incoming_blocks_tx, incoming_blocks_rx) = mpsc::unbounded_channel();
    let (validated_blocks_tx, validated_blocks_rx) = mpsc::unbounded_channel();

    // Block receiver: incoming + validated blocks → a queue of dependency-free block hashes.
    let receiver_state =
        Arc::new(tokio::sync::Mutex::new(BlockReceiverState::<BlockHash>::new()));
    let put_to_incoming_queue: Arc<dyn Fn(BlockMessage) + Send + Sync> = Arc::new({
        let incoming_blocks_tx = incoming_blocks_tx.clone();
        move |block| {
            let _ = incoming_blocks_tx.send(block);
        }
    });
    let validation_rx = block_receiver::apply(
        receiver_state,
        incoming_blocks_rx,
        validated_blocks_rx,
        shard_id.to_string(),
        parts.block_store.clone(),
        parts.dag.clone(),
        comm_state.block_retriever.clone(),
        put_to_incoming_queue,
        log.clone(),
    );

    // Load each validated hash's block from the store and feed the processor (port of
    // `blockReceiverStream.evalMap(blockStore.getUnsafe)`).
    let (processor_input_tx, processor_input_rx) = mpsc::unbounded_channel();
    let load_blocks = {
        let block_store = parts.block_store.clone();
        let mut validation_rx = validation_rx;
        async move {
            while let Some(hash) = validation_rx.recv().await {
                if let Some(block) = block_store
                    .get(&[hash])
                    .await
                    .ok()
                    .and_then(|mut v| v.pop())
                    .flatten()
                {
                    let _ = processor_input_tx.send(block);
                }
            }
        }
    };
    tokio::spawn(load_blocks);

    // Block processor: validate + insert into the DAG, then notify the validated queue.
    let block_index = {
        let runtime = parts.runtime_manager.clone();
        let block_store = parts.block_store.clone();
        move |hash: BlockHash| {
            let runtime = runtime.clone();
            let block_store = block_store.clone();
            async move { BlockIndex::get_block_index(&runtime, &block_store, hash).await }
        }
    };
    let processor = block_processor::apply(
        processor_input_rx,
        validated_blocks_tx.clone(),
        shard_id.to_string(),
        min_phlo_price,
        parts.dag.clone(),
        parts.block_store.clone(),
        parts.runtime_manager.clone(),
        comm_state.comm_util.clone(),
        block_index,
        log,
    );
    tokio::spawn(processor);

    (incoming_blocks_tx, validated_blocks_tx)
}

/// Build the RSpace importer over the on-chain state stores (port of
/// `HistoryRepository.lmdbRepository`'s `RSpaceImporterStore(history, cold, roots)`).
async fn create_rspace_importer(
    store_manager: &LmdbDirStoreManager,
) -> Result<RSpaceImporterStore, String> {
    let history_store = store_manager.store_sync("rspace-history").await?;
    let value_store = store_manager.store_sync("rspace-cold").await?;
    let roots_store = store_manager.store_sync("rspace-roots").await?;
    Ok(RSpaceImporterStore::new(
        history_store,
        value_store,
        roots_store,
    ))
}

/// Parse routing messages into peer messages for `NodeLaunch.apply` (port of the
/// `peerMessageStream` in `Setup.setupNodeProgram`).
fn spawn_peer_message_stream(
    mut routing_rx: mpsc::Receiver<RoutingMessage>,
    peer_message_tx: mpsc::Sender<PeerMessage>,
    log: Arc<dyn Log>,
) {
    let source = LogSource::new("coop.rchain.node.runtime.Setup");
    tokio::spawn(async move {
        while let Some(rm) = routing_rx.recv().await {
            let peer = rm.peer.clone();
            match to_casper_message_proto(&rm.packet).and_then(|proto| CasperMessage::from_proto(&proto)) {
                Ok(message) => {
                    let _ = peer_message_tx.send(PeerMessage { peer, message }).await;
                }
                Err(err) => {
                    log.warn(
                        source,
                        &format!("Could not extract casper message from packet sent by {peer}: {err}"),
                    );
                }
            }
        }
    });
}

/// Build the transport (protocol) server and its inbound-message dispatch closures (port of
/// `NetworkServers.protocolServer`).
fn build_protocol_server(
    conf: &NodeConf,
    comm_state: &CommState,
    routing_tx: mpsc::Sender<RoutingMessage>,
) -> Result<ProtocolServer, String> {
    let cert = std::fs::read_to_string(&conf.tls.certificate_path).map_err(|e| e.to_string())?;
    let key = std::fs::read_to_string(&conf.tls.key_path).map_err(|e| e.to_string())?;

    let server = TransportLayerServer::new(
        comm_state.local_peer.clone(),
        conf.protocol_server.network_id.clone(),
        u16::try_from(conf.protocol_server.port).map_err(|e| e.to_string())?,
        &cert,
        &key,
        conf.protocol_server.grpc_max_recv_stream_message_size,
    )?;

    let dispatch: Box<dyn Fn(Protocol) -> BoxFuture<CommunicationResponse> + Send + Sync> = {
        let transport = comm_state.transport.clone();
        let rp_conf = comm_state.rp_conf.clone();
        let connections = comm_state.connections.clone();
        let routing_tx = routing_tx.clone();
        Box::new(move |proto: Protocol| {
            let transport = transport.clone();
            let rp_conf = rp_conf.clone();
            let connections = connections.clone();
            let routing_tx = routing_tx.clone();
            Box::pin(async move {
                let mut conns = connections.write().await;
                handle_messages::handle(proto, &rp_conf, transport.as_ref(), &mut *conns, &routing_tx)
                    .await
            })
        })
    };

    let handle_streamed: Box<dyn Fn(Blob) -> BoxFuture<()> + Send + Sync> = {
        let routing_tx = routing_tx.clone();
        Box::new(move |blob: Blob| {
            let routing_tx = routing_tx.clone();
            Box::pin(async move {
                let _ = routing_tx
                    .send(RoutingMessage {
                        peer: blob.sender,
                        packet: blob.packet,
                    })
                    .await;
            })
        })
    };

    Ok(ProtocolServer {
        server,
        dispatch,
        handle_streamed,
    })
}

/// Assemble the full running node program (port of `Setup.setupNodeProgram` +
/// `NodeRuntime.main`): comm/discovery state, block receiver/processor, peer-message stream,
/// transport server, `NodeLaunch.apply`, the proposer stream, and the request-missing-dependencies
/// loop.
pub async fn setup_node_program(
    conf: &NodeConf,
    id: &NodeIdentifier,
    log: Arc<dyn Log>,
) -> Result<NodeProgram, String> {
    let (mut program, mut parts) = setup(conf, id).await?;
    let comm_state = create_comm_state(conf, id, log.clone()).await?;
    let importer = create_rspace_importer(&parts.store_manager).await?;

    let shard_id = conf.casper.shard_name.clone();
    let min_phlo_price = conf.casper.min_phlo_price;

    // Block receiver + processor streams (spawned internally).
    let (incoming_blocks_tx, _validated_blocks_tx) =
        wire_block_processing(&comm_state, &parts, &shard_id, min_phlo_price, log.clone());

    // Routing queue → peer-message stream → NodeLaunch.
    let (routing_tx, routing_rx) = mpsc::channel::<RoutingMessage>(50);
    let (peer_message_tx, peer_message_rx) = mpsc::channel::<PeerMessage>(50);
    spawn_peer_message_stream(routing_rx, peer_message_tx, log.clone());

    // Transport (protocol) server.
    program.protocol_server = Some(build_protocol_server(conf, &comm_state, routing_tx)?);

    // Comm state for the `/status` HTTP route.
    program.status_provider = Some(StatusProvider {
        connections: comm_state.connections.clone(),
        rp_conf: comm_state.rp_conf.clone(),
    });

    // Node launch mode dispatch (genesis → syncing → running over the peer-message stream).
    let node_launch = node_launch::apply(
        peer_message_rx,
        incoming_blocks_tx,
        conf.casper.clone(),
        !conf.protocol_client.disable_lfs,
        conf.protocol_server.disable_state_exporter,
        parts.validator_identity_opt.clone(),
        conf.standalone,
        comm_state.transport.clone(),
        comm_state.comm_util.clone(),
        comm_state.block_retriever.clone(),
        comm_state.connections.clone(),
        comm_state.rp_conf.clone(),
        parts.runtime_manager.clone(),
        parts.block_store.clone(),
        parts.approved_store.clone(),
        parts.dag.clone(),
        importer,
        log.clone(),
    );
    tokio::spawn(node_launch);

    // Request-missing-dependencies loop (port of `requestDependencies` in `Setup.setupNodeProgram`).
    let request_deps = {
        let block_retriever = comm_state.block_retriever.clone();
        let timeout = conf.casper.requested_blocks_timeout;
        let interval = conf.casper.casper_loop_interval;
        async move {
            loop {
                block_retriever.request_all(timeout).await;
                tokio::time::sleep(interval).await;
            }
        }
    };
    tokio::spawn(request_deps);

    // Proposer stream (port of `proposerStream` in `Setup.setupNodeProgram`). Runs only when a
    // validator identity is configured; the propose trigger + state were wired into `BlockApiImpl`
    // by [`setup`].
    let proposer_parts = parts.proposer.take();
    let validator = parts.validator_identity_opt.clone();
    if let (Some(proposer_parts), Some(validator)) = (proposer_parts, validator) {
        let block_index = {
            let runtime = parts.runtime_manager.clone();
            let block_store = parts.block_store.clone();
            move |hash: BlockHash| {
                let runtime = runtime.clone();
                let block_store = block_store.clone();
                async move { BlockIndex::get_block_index(&runtime, &block_store, hash).await }
            }
        };
        let propose_effect: Arc<
            dyn Fn(&BlockMessage) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> + Send + Sync,
        > = {
            let block_store = parts.block_store.clone();
            let comm_util = comm_state.comm_util.clone();
            Arc::new(move |block: &BlockMessage| {
                let block_store = block_store.clone();
                let comm_util = comm_util.clone();
                let block = block.clone();
                Box::pin(async move {
                    put_block(&block_store, block.clone()).await;
                    comm_util
                        .send_block_hash(&block.block_hash, block.sender.as_bytes())
                        .await;
                })
            })
        };
        let proposer = Proposer::apply(
            validator,
            conf.casper.shard_name.clone(),
            conf.casper.min_phlo_price,
            conf.casper.genesis_block_data.epoch_length,
            parts.dag.clone(),
            parts.block_store.clone(),
            parts.runtime_manager.clone(),
            block_index,
            propose_effect,
        );
        let proposer_stream = proposer_instance::create(
            proposer_parts.queue_rx,
            proposer_parts.queue_tx,
            proposer,
            proposer_parts.state,
        );
        tokio::spawn(async move {
            use futures_util::StreamExt;
            let mut stream = Box::pin(proposer_stream);
            while stream.next().await.is_some() {}
        });
    }

    Ok(program)
}

/// Assemble the node program (port of `Setup.setupNodeProgram`, minus the comm/discovery/proposer/
/// block-stream pieces).
pub async fn setup(conf: &NodeConf, id: &NodeIdentifier) -> Result<(NodeProgram, SetupParts), String> {
    let store_manager = rnode_key_value_store_manager(&conf.storage.data_dir);

    // Block store + DAG storage.
    let block_store = block_store::create(&store_manager).await?;
    let approved_store = approved_store::create(&store_manager).await?;
    let block_metadata_kv: Arc<dyn KeyValueTypedStore<BlockHash, BlockMetadata>> = Arc::new(
        database(
            &store_manager,
            "block-metadata",
            Arc::new(BlockHashCodec),
            Arc::new(BlockMetadataCodec),
        )
        .await?,
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
        .await?,
    );
    let deploy_index: Arc<dyn KeyValueTypedStore<DeployId, BlockHash>> = Arc::new(
        database(
            &store_manager,
            "deploy-index",
            Arc::new(BytesCodec),
            Arc::new(BlockHashCodec),
        )
        .await?,
    );
    let deploy_store: Arc<dyn KeyValueTypedStore<DeployId, SignedDeployData>> = Arc::new(
        database(
            &store_manager,
            "deploy-pool",
            Arc::new(BytesCodec),
            Arc::new(SignedDeployDataCodec),
        )
        .await?,
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
        .await?,
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

    // Proposer queue + trigger + state (port of the `proposerQueue`/`triggerProposeFOpt`/
    // `proposerStateRefOpt` in `Setup.setupNodeProgram`). The proposer stream itself is driven in
    // `setup_node_program`, but the trigger + state must be available to `BlockApiImpl` here.
    let (proposer_queue_tx, proposer_queue_rx) =
        mpsc::channel::<(bool, tokio::sync::oneshot::Sender<ProposerResult>)>(100);
    let proposer_state: Option<Arc<tokio::sync::Mutex<ProposerState>>> = validator_opt
        .as_ref()
        .map(|_| Arc::new(tokio::sync::Mutex::new(ProposerState::default())));
    let trigger_propose: Option<ProposeFunction> = validator_opt.as_ref().map(|_| {
        let tx = proposer_queue_tx.clone();
        Box::new(move |is_async: bool| {
            let tx = tx.clone();
            Box::pin(async move {
                let (otx, orx) = tokio::sync::oneshot::channel();
                let _ = tx.send((is_async, otx)).await;
                orx.await.unwrap_or(ProposerResult::Empty)
            })
        })
    });
    let proposer_parts: Option<ProposerParts> =
        proposer_state.as_ref().map(|state| ProposerParts {
            queue_tx: proposer_queue_tx.clone(),
            queue_rx: proposer_queue_rx,
            state: state.clone(),
        });

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
        block_dag_storage.clone(),
        block_store.clone(),
        runtime_manager.clone(),
        validator_opt.clone(),
        network_id,
        shard_id,
        conf.casper.min_phlo_price,
        env!("CARGO_PKG_VERSION").to_string(),
        network_status,
        conf.casper.validator_private_key.is_none(),
        conf.api_server.max_blocks_limit,
        conf.dev_mode,
        trigger_propose,
        proposer_state.clone(),
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
        .await?,
    );
    let block_report_api = Arc::new(BlockReportApi::new(
        block_store.clone(),
        Arc::new(noop()),
        report_store,
        validator_opt.clone(),
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

    Ok((
        NodeProgram {
            grpc_services,
            web_api,
            admin_web_api,
            block_report_api,
            reporter: Arc::new(NewPrometheusReporter::new(
                crate::diagnostics::scrape_data_builder::Configuration::default(),
            )),
            host: conf.api_server.host.clone(),
            port_http: Port::try_from(conf.api_server.port_http).map_err(|e| e.to_string())?,
            port_admin_http: Port::try_from(conf.api_server.port_admin_http)
                .map_err(|e| e.to_string())?,
            port_grpc_internal: Port::try_from(conf.api_server.port_grpc_internal)
                .map_err(|e| e.to_string())?,
            protocol_server: None,
            status_provider: None,
        },
        SetupParts {
            block_store,
            dag: block_dag_storage,
            runtime_manager,
            approved_store,
            store_manager,
            validator_identity_opt: validator_opt,
            proposer: proposer_parts,
        },
    ))
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

        let (program, _parts) = setup(&conf, &id).await.expect("setup should assemble");
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

    #[tokio::test]
    async fn create_rspace_importer_round_trips_over_lmdb() {
        use rchain_rspace::state::RSpaceImporter;
        use rchain_shared::state::TrieImporter;

        let dir = temp_dir("rspace-importer");
        let manager = rnode_key_value_store_manager(&dir);
        let mut importer = create_rspace_importer(&manager)
            .await
            .expect("importer should build");

        let hash = Blake2b256Hash::from_bytes([0x33; 32]);
        let value = vec![1u8, 2, 3];
        importer.set_history_items(&[(hash, value.clone())], |v: &Vec<u8>| v.clone());
        importer.set_root(hash);

        assert_eq!(importer.get_history_item(hash), Some(value));

        drop(importer);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
