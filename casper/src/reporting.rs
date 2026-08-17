//! Block replay reporting (port of `casper/reporting/ReportingCasper.scala`).
//!
//! The reporting runtime (`ReportingRuntime`/`rhoReporter`), the proto transformer, and the report
//! store are deferred pending the report protos and full runtime wiring.

use async_trait::async_trait;

use rchain_models::ast::Par;
use rchain_models::casper::protocol::casper_message::{
    BlockMessage, Peek, ProcessedDeploy, SystemDeployData,
};
use rchain_models::casper::protocol::report::{
    ReportCommProto, ReportConsumeProto, ReportProduceProto, ReportProto,
};
use rchain_models::runtime::{BindPattern, ListParWithRandom, TaggedContinuation};
use rchain_rspace::reporting_rspace::{
    ReportingComm, ReportingConsume, ReportingEvent, ReportingProduce,
};
use rchain_rspace::reporting_transformer::ReportingTransformer;

/// The concrete reporting-event type.
pub type RhoReportingEvent =
    ReportingEvent<Par, BindPattern, ListParWithRandom, TaggedContinuation>;

/// A user deploy's report result (port of `DeployReportResult`).
#[derive(Clone, Debug)]
pub struct DeployReportResult {
    pub processed_deploy: ProcessedDeploy,
    pub events: Vec<Vec<RhoReportingEvent>>,
}

/// A system deploy's report result (port of `SystemDeployReportResult`).
#[derive(Clone, Debug)]
pub struct SystemDeployReportResult {
    pub processed_system_deploy: SystemDeployData,
    pub events: Vec<Vec<RhoReportingEvent>>,
}

/// The result of replaying a block with reporting (port of `ReplayResult`).
#[derive(Clone, Debug)]
pub struct ReplayResult {
    pub deploy_report_result: Vec<DeployReportResult>,
    pub system_deploy_report_result: Vec<SystemDeployReportResult>,
    pub post_state_hash: Vec<u8>,
}

/// Replays a block and collects a human-readable report (port of `ReportingCasper`).
#[async_trait]
pub trait ReportingCasper: Send + Sync {
    async fn trace(&self, block: BlockMessage) -> Result<ReplayResult, String>;
}

/// A no-op reporter (port of `ReportingCasper.noop`).
pub fn noop() -> impl ReportingCasper {
    NoopReportingCasper
}

struct NoopReportingCasper;

#[async_trait]
impl ReportingCasper for NoopReportingCasper {
    async fn trace(&self, _block: BlockMessage) -> Result<ReplayResult, String> {
        Ok(ReplayResult {
            deploy_report_result: Vec::new(),
            system_deploy_report_result: Vec::new(),
            post_state_hash: b"empty".to_vec(),
        })
    }
}

/// Transforms [`RhoReportingEvent`]s into casper report protos (port of
/// `ReportingProtoTransformer`).
pub struct ReportingProtoTransformer;

impl ReportingTransformer<Par, BindPattern, ListParWithRandom, TaggedContinuation, ReportProto>
    for ReportingProtoTransformer
{
    fn serialize_consume(&self, rc: &ReportingConsume<Par, BindPattern, TaggedContinuation>) -> ReportProto {
        ReportProto::Consume(ReportConsumeProto {
            channels: rc.channels.clone(),
            patterns: rc.patterns.clone(),
            peeks: rc
                .peeks
                .iter()
                .map(|i| Peek {
                    channel_index: *i as i32,
                })
                .collect(),
        })
    }

    fn serialize_produce(&self, rp: &ReportingProduce<Par, ListParWithRandom>) -> ReportProto {
        ReportProto::Produce(ReportProduceProto {
            channel: rp.channel.clone(),
            data: rp.data.clone(),
        })
    }

    fn serialize_comm(
        &self,
        rc: &ReportingComm<Par, BindPattern, ListParWithRandom, TaggedContinuation>,
    ) -> ReportProto {
        let consume = ReportConsumeProto {
            channels: rc.consume.channels.clone(),
            patterns: rc.consume.patterns.clone(),
            peeks: rc
                .consume
                .peeks
                .iter()
                .map(|i| Peek {
                    channel_index: *i as i32,
                })
                .collect(),
        };
        let produces = rc
            .produces
            .iter()
            .map(|p| ReportProduceProto {
                channel: p.channel.clone(),
                data: p.data.clone(),
            })
            .collect();
        ReportProto::Comm(ReportCommProto { consume, produces })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rchain_models::block_hash::BlockHash;
    use rchain_models::casper::protocol::casper_message::RholangState;
    use rchain_models::validator::Validator;
    use std::collections::{BTreeMap, BTreeSet};

    fn block() -> BlockMessage {
        BlockMessage {
            version: 1,
            shard_id: "root".to_string(),
            block_hash: BlockHash::new([0u8; 32]),
            block_number: 0,
            sender: Validator::new([0u8; 65]),
            seq_num: 0,
            pre_state_hash: vec![],
            post_state_hash: vec![],
            justifications: vec![],
            bonds: BTreeMap::new(),
            rejected_deploys: BTreeSet::new(),
            rejected_blocks: BTreeSet::new(),
            rejected_senders: BTreeSet::new(),
            state: RholangState::default(),
            sig_algorithm: "secp256k1".to_string(),
            sig: vec![],
        }
    }

    #[tokio::test]
    async fn noop_returns_empty_result() {
        let reporter = noop();
        let result = reporter.trace(block()).await.unwrap();
        assert!(result.deploy_report_result.is_empty());
        assert!(result.system_deploy_report_result.is_empty());
        assert_eq!(result.post_state_hash, b"empty".to_vec());
    }
}
