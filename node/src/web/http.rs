//! HTTP server routes (port of the http4s `service` methods in the `web` package and
//! `NewPrometheusReporter.service`).
//!
//! The public server mounts `/version`, `/metrics`, and the `/api` JSON routes (port of
//! `WebApiRoutes`). The admin server mounts the `/api` admin routes (port of `AdminWebApiRoutes`).
//! The `/status` route (which needs the comm status builder), the `/api/v1` OpenAPI routes, and the
//! reporting routes are deferred.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use rchain_casper::api::block_report_api::BlockReportApi;
use rchain_models::block_hash::BlockHash;

use crate::api::admin_web_api::AdminWebApi;
use crate::api::dto::{
    BlockApiException, DataAtNameByBlockHashRequest, DataAtNameRequest, DeployRequest,
    ExploreDeployRequest,
};
use crate::api::web_api::WebApi;
use crate::diagnostics::NewPrometheusReporter;
use crate::web::reporting::transform_result;
use crate::web::version_info;

/// State shared by the public HTTP server (port of the `webApi` + `prometheusReporter` +
/// `blockReportAPI` arguments of `acquireHttpServer`).
#[derive(Clone)]
pub struct HttpState {
    pub reporter: Arc<NewPrometheusReporter>,
    pub web_api: Arc<dyn WebApi>,
    pub block_report_api: Arc<BlockReportApi>,
}

/// State shared by the admin HTTP server (port of the `adminWebApiRoutes` argument of
/// `acquireAdminHttpServer`).
#[derive(Clone)]
pub struct AdminState {
    pub admin_web_api: Arc<dyn AdminWebApi>,
}

/// `GET /version` (port of `VersionInfo.service`): the node version string.
pub async fn version() -> String {
    version_info::get(env!("CARGO_PKG_VERSION"), None)
}

/// `GET /metrics` (port of `NewPrometheusReporter.service`): the Prometheus scrape data.
pub async fn metrics(State(state): State<HttpState>) -> String {
    state.reporter.scrape_data()
}

/// Map a `WebApi`/`AdminWebApi` result to an HTTP response: `200` with a JSON body on success,
/// `400` with a JSON error string on `BlockApiException` (port of the `handleResponseError`
/// handler in `WebApiRoutes`).
fn json_result<T: Serialize>(result: Result<T, BlockApiException>) -> Response {
    match result {
        Ok(value) => (StatusCode::OK, Json(value)).into_response(),
        Err(err) => (StatusCode::BAD_REQUEST, Json(err.0)).into_response(),
    }
}

// --- Web API routes (port of `WebApiRoutes.service`) ---

async fn api_status(State(state): State<HttpState>) -> Response {
    json_result(state.web_api.status().await)
}

async fn api_deploy(State(state): State<HttpState>, Json(req): Json<DeployRequest>) -> Response {
    json_result(state.web_api.deploy(&req).await)
}

async fn api_explore_deploy(State(state): State<HttpState>, Json(term): Json<String>) -> Response {
    json_result(state.web_api.exploratory_deploy(&term, None, false).await)
}

async fn api_explore_deploy_by_block_hash(
    State(state): State<HttpState>,
    Json(req): Json<ExploreDeployRequest>,
) -> Response {
    let block_hash = if req.block_hash.is_empty() {
        None
    } else {
        Some(req.block_hash.as_str())
    };
    json_result(
        state
            .web_api
            .exploratory_deploy(&req.term, block_hash, req.use_pre_state_hash)
            .await,
    )
}

async fn api_data_at_name(
    State(state): State<HttpState>,
    Json(req): Json<DataAtNameRequest>,
) -> Response {
    json_result(state.web_api.listen_for_data_at_name(&req).await)
}

async fn api_data_at_name_by_block_hash(
    State(state): State<HttpState>,
    Json(req): Json<DataAtNameByBlockHashRequest>,
) -> Response {
    json_result(state.web_api.get_data_at_par(&req).await)
}

async fn api_last_finalized_block(State(state): State<HttpState>) -> Response {
    json_result(state.web_api.last_finalized_block().await)
}

async fn api_get_block(State(state): State<HttpState>, Path(hash): Path<String>) -> Response {
    json_result(state.web_api.get_block(&hash).await)
}

async fn api_get_blocks(State(state): State<HttpState>) -> Response {
    json_result(state.web_api.get_blocks(1).await)
}

async fn api_get_blocks_by_heights(
    State(state): State<HttpState>,
    Path((start, end)): Path<(i64, i64)>,
) -> Response {
    json_result(state.web_api.get_blocks_by_heights(start, end).await)
}

async fn api_get_blocks_by_depth(
    State(state): State<HttpState>,
    Path(depth): Path<i32>,
) -> Response {
    json_result(state.web_api.get_blocks(depth).await)
}

async fn api_find_deploy(
    State(state): State<HttpState>,
    Path(deploy_id): Path<String>,
) -> Response {
    json_result(state.web_api.find_deploy(&deploy_id).await)
}

async fn api_is_finalized(State(state): State<HttpState>, Path(hash): Path<String>) -> Response {
    json_result(state.web_api.is_finalized(&hash).await)
}

async fn api_get_transaction(
    State(state): State<HttpState>,
    Path(hash): Path<String>,
) -> Response {
    json_result(state.web_api.get_transaction(&hash).await)
}

// --- Reporting routes (port of `ReportingRoutes.service`) ---

/// `GET /reporting/trace` query params (`blockHash` + optional `forceReplay`).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReportingQuery {
    block_hash: String,
    force_replay: Option<bool>,
}

async fn reporting_trace(
    State(state): State<HttpState>,
    Query(query): Query<ReportingQuery>,
) -> Response {
    let hash = BlockHash::from_hex(&query.block_hash);
    let result = state
        .block_report_api
        .block_report(&hash, query.force_replay.unwrap_or(false))
        .await;
    (StatusCode::OK, Json(transform_result(result))).into_response()
}

// --- Admin Web API routes (port of `AdminWebApiRoutes.service`) ---

async fn admin_propose(State(state): State<AdminState>) -> Response {
    json_result(state.admin_web_api.propose().await)
}

/// Build the public HTTP routes (port of `acquireHttpServer`'s route map: `/version`, `/metrics`,
/// the `/api` JSON routes, and `/reporting`). The `/status` and `/api/v1` routes are deferred.
pub fn router(state: HttpState) -> Router {
    Router::new()
        .route("/version", get(version))
        .route("/metrics", get(metrics))
        .route("/reporting/trace", get(reporting_trace))
        .route("/api/status", get(api_status))
        .route("/api/deploy", post(api_deploy))
        .route("/api/explore-deploy", post(api_explore_deploy))
        .route(
            "/api/explore-deploy-by-block-hash",
            post(api_explore_deploy_by_block_hash),
        )
        .route("/api/data-at-name", post(api_data_at_name))
        .route(
            "/api/data-at-name-by-block-hash",
            post(api_data_at_name_by_block_hash),
        )
        .route("/api/last-finalized-block", get(api_last_finalized_block))
        .route("/api/block/:hash", get(api_get_block))
        .route("/api/blocks", get(api_get_blocks))
        .route("/api/blocks/:start/:end", get(api_get_blocks_by_heights))
        .route("/api/blocks/:depth", get(api_get_blocks_by_depth))
        .route("/api/deploy/:deploy_id", get(api_find_deploy))
        .route("/api/is-finalized/:hash", get(api_is_finalized))
        .route("/api/transactions/:hash", get(api_get_transaction))
        .with_state(state)
}

/// Build the admin HTTP routes (port of `acquireAdminHttpServer`'s `/api` admin routes).
pub fn admin_router(state: AdminState) -> Router {
    Router::new()
        .route("/api/propose", post(admin_propose))
        .with_state(state)
}

/// Bind and serve the public HTTP routes (port of `web/acquireHttpServer`; the `/status`,
/// `/api/v1`, and the CORS/connection-timeout configuration are deferred).
pub async fn acquire_http_server(
    host: &str,
    port: u16,
    reporter: Arc<NewPrometheusReporter>,
    web_api: Arc<dyn WebApi>,
    block_report_api: Arc<BlockReportApi>,
) -> Result<(), String> {
    let addr: SocketAddr = format!("{host}:{port}")
        .parse()
        .map_err(|e| format!("invalid bind address {host}:{port}: {e}"))?;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| e.to_string())?;
    axum::serve(
        listener,
        router(HttpState {
            reporter,
            web_api,
            block_report_api,
        }),
    )
    .await
    .map_err(|e| e.to_string())
}

/// Bind and serve the admin HTTP routes (port of `web/acquireAdminHttpServer`).
pub async fn acquire_admin_http_server(
    host: &str,
    port: u16,
    admin_web_api: Arc<dyn AdminWebApi>,
) -> Result<(), String> {
    let addr: SocketAddr = format!("{host}:{port}")
        .parse()
        .map_err(|e| format!("invalid bind address {host}:{port}: {e}"))?;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| e.to_string())?;
    axum::serve(listener, admin_router(AdminState { admin_web_api }))
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::dto::{
        ApiStatus, DataAtNameResponse, DeployExecStatus, RhoDataResponse, VersionInfo,
    };
    use crate::diagnostics::scrape_data_builder::Configuration;
    use crate::web::transaction::TransactionResponse;
    use async_trait::async_trait;
    use axum::body::to_bytes;
    use rchain_block_storage::dag::codecs::{BlockHashCodec, BlockMessageCodec};
    use rchain_casper::reporting::noop;
    use rchain_models::casper::protocol::deploy_service::{BlockInfo, LightBlockInfo};
    use rchain_models::casper::protocol::report::BlockEventInfo;
    use rchain_shared::store::InMemoryKeyValueStore;
    use rchain_shared::typed_store::{Codec, KeyValueTypedStoreCodec, SharedStore};
    use std::marker::PhantomData;

    struct JsonCodec<T>(PhantomData<T>);

    impl<T: Serialize + serde::de::DeserializeOwned + Send + Sync> Codec<T> for JsonCodec<T> {
        fn encode(&self, value: &T) -> Vec<u8> {
            serde_json::to_vec(value).expect("json encode")
        }

        fn decode(&self, bytes: &[u8]) -> Result<T, String> {
            serde_json::from_slice(bytes).map_err(|e| e.to_string())
        }
    }

    fn test_block_report_api() -> Arc<BlockReportApi> {
        let store: SharedStore = Arc::new(tokio::sync::Mutex::new(
            Box::new(InMemoryKeyValueStore::default()),
        ));
        let block_store = Arc::new(KeyValueTypedStoreCodec::new(
            store.clone(),
            Arc::new(BlockHashCodec),
            Arc::new(BlockMessageCodec),
        ));
        let report_store = Arc::new(KeyValueTypedStoreCodec::new(
            store,
            Arc::new(BlockHashCodec),
            Arc::new(JsonCodec::<BlockEventInfo>(PhantomData)),
        ));
        Arc::new(BlockReportApi::new(
            block_store,
            Arc::new(noop()),
            report_store,
            None,
        ))
    }

    fn test_status() -> ApiStatus {
        ApiStatus {
            version: VersionInfo {
                api: "1.0".to_string(),
                node: "2.0".to_string(),
            },
            address: "addr".to_string(),
            network_id: "testnet".to_string(),
            shard_id: "root".to_string(),
            peers: 1,
            nodes: 2,
            min_phlo_price: 3,
            latest_block_number: 4,
        }
    }

    struct MockWebApi {
        status: ApiStatus,
    }

    #[async_trait]
    impl WebApi for MockWebApi {
        async fn status(&self) -> Result<ApiStatus, BlockApiException> {
            Ok(self.status.clone())
        }

        async fn deploy(&self, _: &DeployRequest) -> Result<String, BlockApiException> {
            unimplemented!()
        }

        async fn deploy_status(&self, _: &str) -> Result<DeployExecStatus, BlockApiException> {
            unimplemented!()
        }

        async fn listen_for_data_at_name(
            &self,
            _: &DataAtNameRequest,
        ) -> Result<DataAtNameResponse, BlockApiException> {
            unimplemented!()
        }

        async fn get_data_at_par(
            &self,
            _: &DataAtNameByBlockHashRequest,
        ) -> Result<RhoDataResponse, BlockApiException> {
            unimplemented!()
        }

        async fn last_finalized_block(&self) -> Result<BlockInfo, BlockApiException> {
            unimplemented!()
        }

        async fn get_block(&self, _: &str) -> Result<BlockInfo, BlockApiException> {
            unimplemented!()
        }

        async fn get_blocks(&self, _: i32) -> Result<Vec<LightBlockInfo>, BlockApiException> {
            unimplemented!()
        }

        async fn find_deploy(&self, _: &str) -> Result<LightBlockInfo, BlockApiException> {
            unimplemented!()
        }

        async fn exploratory_deploy(
            &self,
            _: &str,
            _: Option<&str>,
            _: bool,
        ) -> Result<RhoDataResponse, BlockApiException> {
            unimplemented!()
        }

        async fn get_blocks_by_heights(
            &self,
            _: i64,
            _: i64,
        ) -> Result<Vec<LightBlockInfo>, BlockApiException> {
            unimplemented!()
        }

        async fn is_finalized(&self, _: &str) -> Result<bool, BlockApiException> {
            unimplemented!()
        }

        async fn get_transaction(&self, _: &str) -> Result<TransactionResponse, BlockApiException> {
            unimplemented!()
        }
    }

    fn state() -> HttpState {
        HttpState {
            reporter: Arc::new(NewPrometheusReporter::new(Configuration::default())),
            web_api: Arc::new(MockWebApi {
                status: test_status(),
            }),
            block_report_api: test_block_report_api(),
        }
    }

    #[tokio::test]
    async fn version_returns_node_version() {
        assert!(version().await.starts_with("RChain Node "));
    }

    #[tokio::test]
    async fn metrics_returns_scrape_data() {
        let out = metrics(State(state())).await;
        assert_eq!(
            out,
            "# The kamon-prometheus module didn't receive any data just yet.\n"
        );
    }

    #[tokio::test]
    async fn api_status_returns_json() {
        let response = api_status(State(state())).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["address"], "addr");
        assert_eq!(json["version"]["api"], "1.0");
    }
}
