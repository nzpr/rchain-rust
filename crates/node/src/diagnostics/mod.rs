//! Node diagnostics (port of `coop.rchain.node.diagnostics`).
//!
//! The metrics *export* side: a snapshot data model and the Prometheus/InfluxDB text-format
//! encoders. The kamon-backed registry/tracing (`effects/package.scala`) is deferred.

pub mod influxdb;
pub mod model;
pub mod scrape_data_builder;
