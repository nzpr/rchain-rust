//! Block proposer (port of `blocks/proposer/Proposer.scala`).
//!
//! The `Proposer.apply` factory (which builds the closures from the DAG/runtime/comm) is deferred
//! pending `CommUtil`; only the `Proposer` class + `ProposerResult` are ported.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use rchain_models::casper::protocol::casper_message::BlockMessage;
use rchain_models::validator::Validator;

use super::propose_result::{BlockCreatorResult, ProposeResult, ProposeStatus};
use crate::block_status::BlockStatus;
use crate::validator_identity::ValidatorIdentity;

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A proposal result signaled to the caller (port of `ProposerResult`).
#[derive(Clone, Debug)]
pub enum ProposerResult {
    Empty,
    Success {
        status: ProposeStatus,
        block: BlockMessage,
    },
    Failure {
        status: ProposeStatus,
        seq_number: i64,
    },
    Started {
        seq_number: i64,
    },
}

/// The block proposer (port of `Proposer`).
pub struct Proposer {
    get_latest_seq_number: Arc<dyn Fn(Validator) -> BoxFuture<'static, i64> + Send + Sync>,
    check_active_validator:
        Arc<dyn Fn(&ValidatorIdentity) -> BoxFuture<'static, bool> + Send + Sync>,
    create_block:
        Arc<dyn Fn(&ValidatorIdentity) -> BoxFuture<'static, BlockCreatorResult> + Send + Sync>,
    validate_block:
        Arc<dyn Fn(&BlockMessage) -> BoxFuture<'static, Result<(), BlockStatus>> + Send + Sync>,
    propose_effect: Arc<dyn Fn(&BlockMessage) -> BoxFuture<'static, ()> + Send + Sync>,
    validator: ValidatorIdentity,
}

impl Proposer {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        get_latest_seq_number: Arc<dyn Fn(Validator) -> BoxFuture<'static, i64> + Send + Sync>,
        check_active_validator:
            Arc<dyn Fn(&ValidatorIdentity) -> BoxFuture<'static, bool> + Send + Sync>,
        create_block:
            Arc<dyn Fn(&ValidatorIdentity) -> BoxFuture<'static, BlockCreatorResult> + Send + Sync>,
        validate_block:
            Arc<dyn Fn(&BlockMessage) -> BoxFuture<'static, Result<(), BlockStatus>> + Send + Sync>,
        propose_effect: Arc<dyn Fn(&BlockMessage) -> BoxFuture<'static, ()> + Send + Sync>,
        validator: ValidatorIdentity,
    ) -> Self {
        Proposer {
            get_latest_seq_number,
            check_active_validator,
            create_block,
            validate_block,
            propose_effect,
            validator,
        }
    }

    /// The core propose logic (port of `doPropose`).
    async fn do_propose(&self) -> (ProposeResult, Option<BlockMessage>) {
        if !(self.check_active_validator)(&self.validator).await {
            return (
                ProposeResult {
                    propose_status: ProposeStatus::NotBonded,
                },
                None,
            );
        }

        match (self.create_block)(&self.validator).await {
            BlockCreatorResult::NoNewDeploys => (
                ProposeResult {
                    propose_status: ProposeStatus::NoNewDeploys,
                },
                None,
            ),
            BlockCreatorResult::Created(block) => match (self.validate_block)(&block).await {
                Ok(()) => {
                    (self.propose_effect)(&block).await;
                    (
                        ProposeResult {
                            propose_status: ProposeStatus::ProposeSuccess,
                        },
                        Some(block),
                    )
                }
                Err(status) => panic!(
                    "Validation of self created block failed with reason: {status:?}, cancelling propose."
                ),
            },
        }
    }

    /// Propose a block and signal the result (port of `propose`).
    pub async fn propose(
        &self,
        is_async: bool,
        propose_id: tokio::sync::oneshot::Sender<ProposerResult>,
    ) -> (ProposeResult, Option<BlockMessage>) {
        let validator = Validator::from_slice(self.validator.public_key.bytes());
        let next_seq = (self.get_latest_seq_number)(validator).await + 1;

        if is_async {
            let _ = propose_id.send(ProposerResult::Started {
                seq_number: next_seq,
            });
            self.do_propose().await
        } else {
            let (result, block_opt) = self.do_propose().await;
            let proposer_result = match &block_opt {
                Some(block) => ProposerResult::Success {
                    status: result.propose_status.clone(),
                    block: block.clone(),
                },
                None => ProposerResult::Failure {
                    status: result.propose_status.clone(),
                    seq_number: next_seq,
                },
            };
            let _ = propose_id.send(proposer_result);
            (result, block_opt)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proposer(create: BlockCreatorResult) -> Proposer {
        let get_seq: Arc<dyn Fn(Validator) -> BoxFuture<'static, i64> + Send + Sync> =
            Arc::new(|_v| Box::pin(async { 0i64 }));
        let check_active: Arc<dyn Fn(&ValidatorIdentity) -> BoxFuture<'static, bool> + Send + Sync> =
            Arc::new(|_v| Box::pin(async { true }));
        let create_block: Arc<
            dyn Fn(&ValidatorIdentity) -> BoxFuture<'static, BlockCreatorResult> + Send + Sync,
        > = Arc::new(move |_v| {
            let create = create.clone();
            Box::pin(async move { create })
        });
        let validate: Arc<
            dyn Fn(&BlockMessage) -> BoxFuture<'static, Result<(), BlockStatus>> + Send + Sync,
        > = Arc::new(|_b| Box::pin(async { Ok(()) }));
        let effect: Arc<dyn Fn(&BlockMessage) -> BoxFuture<'static, ()> + Send + Sync> =
            Arc::new(|_b| Box::pin(async {}));
        let validator = ValidatorIdentity::from_hex(
            "67e56582298859ddae725f972992a07c6c4fb9f62a8fff58ce3ca926a1063530",
        )
        .unwrap();
        Proposer::new(get_seq, check_active, create_block, validate, effect, validator)
    }

    fn block() -> BlockMessage {
        BlockMessage {
            version: 1,
            shard_id: "root".to_string(),
            block_hash: rchain_models::block_hash::BlockHash::new([1u8; 32]),
            block_number: 0,
            sender: Validator::new([0u8; 65]),
            seq_num: 0,
            pre_state_hash: vec![],
            post_state_hash: vec![],
            justifications: vec![],
            bonds: std::collections::BTreeMap::new(),
            rejected_deploys: std::collections::BTreeSet::new(),
            rejected_blocks: std::collections::BTreeSet::new(),
            rejected_senders: std::collections::BTreeSet::new(),
            state: rchain_models::casper::protocol::casper_message::RholangState::default(),
            sig_algorithm: "secp256k1".to_string(),
            sig: vec![],
        }
    }

    #[tokio::test]
    async fn no_new_deploys_returns_failure() {
        let p = proposer(BlockCreatorResult::NoNewDeploys);
        let (tx, rx) = tokio::sync::oneshot::channel();
        let (result, block_opt) = p.propose(false, tx).await;
        assert_eq!(result.propose_status, ProposeStatus::NoNewDeploys);
        assert!(block_opt.is_none());
        assert!(matches!(rx.await.unwrap(), ProposerResult::Failure { .. }));
    }

    #[tokio::test]
    async fn created_block_returns_success() {
        let p = proposer(BlockCreatorResult::Created(block()));
        let (tx, rx) = tokio::sync::oneshot::channel();
        let (result, block_opt) = p.propose(false, tx).await;
        assert_eq!(result.propose_status, ProposeStatus::ProposeSuccess);
        assert!(block_opt.is_some());
        assert!(matches!(rx.await.unwrap(), ProposerResult::Success { .. }));
    }
}
