//! HTTP server routes (port of the http4s `service` methods in the `web` package and
//! `NewPrometheusReporter.service`).
//!
//! The `/api` and `/api/v1` routes (which need `WebApi`) and the `/status` route (which needs the
//! comm status builder) are deferred.

use std::sync::Arc;

use axum::extract::State;
use axum::routing::get;
use axum::Router;

use crate::diagnostics::NewPrometheusReporter;
use crate::web::version_info;

/// `GET /version` (port of `VersionInfo.service`): the node version string.
pub async fn version() -> String {
    version_info::get(env!("CARGO_PKG_VERSION"), None)
}

/// `GET /metrics` (port of `NewPrometheusReporter.service`): the Prometheus scrape data.
pub async fn metrics(State(reporter): State<Arc<NewPrometheusReporter>>) -> String {
    reporter.scrape_data()
}

/// Build the base routes (port of the `baseRoutes` map in `web/acquireHttpServer`).
pub fn router(reporter: Arc<NewPrometheusReporter>) -> Router {
    Router::new()
        .route("/version", get(version))
        .route("/metrics", get(metrics))
        .with_state(reporter)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::scrape_data_builder::Configuration;

    #[tokio::test]
    async fn version_returns_node_version() {
        assert!(version().await.starts_with("RChain Node "));
    }

    #[tokio::test]
    async fn metrics_returns_scrape_data() {
        let reporter = Arc::new(NewPrometheusReporter::new(Configuration::default()));
        let out = metrics(State(reporter)).await;
        assert_eq!(
            out,
            "# The kamon-prometheus module didn't receive any data just yet.\n"
        );
    }
}
