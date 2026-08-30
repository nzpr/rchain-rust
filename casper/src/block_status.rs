//! Block validation status (port of `BlockStatus.scala`).

/// The outcome of validating a block (port of `BlockStatus`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlockStatus {
    Valid,
    InvalidBlockNumber,
    InvalidRepeatDeploy,
    InvalidSequenceNumber,
    InvalidDeployShardId,
    InvalidDeploySignature,
    JustificationRegression,
    NeglectedInvalidBlock,
    InvalidStateHash,
    InvalidBondsCache,
    InvalidRejectedDeploy,
    ContainsExpiredDeploy,
    ContainsFutureDeploy,
    ContainsLowCostDeploy,
}

impl BlockStatus {
    pub fn is_valid(&self) -> bool {
        matches!(self, BlockStatus::Valid)
    }
}

impl std::fmt::Display for BlockStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            BlockStatus::Valid => "valid",
            BlockStatus::InvalidBlockNumber => "invalid block number",
            BlockStatus::InvalidRepeatDeploy => "a deploy was repeated across blocks",
            BlockStatus::InvalidSequenceNumber => "invalid sender sequence number",
            BlockStatus::InvalidDeployShardId => "deploy shard id does not match the block's shard",
            BlockStatus::InvalidDeploySignature => "a deploy signature is invalid",
            BlockStatus::JustificationRegression => "a justification regressed from its parent",
            BlockStatus::NeglectedInvalidBlock => "an invalid block was used as a justification",
            BlockStatus::InvalidStateHash => {
                "the block's declared post-state hash does not match the state recomputed by \
                 replaying its deploys — a node state-accounting inconsistency, not an error in your \
                 deploy or API call"
            }
            BlockStatus::InvalidBondsCache => "invalid bonds cache",
            BlockStatus::InvalidRejectedDeploy => "the block's rejected-deploy set does not match its parents",
            BlockStatus::ContainsExpiredDeploy => "a deploy has expired",
            BlockStatus::ContainsFutureDeploy => "a deploy has a future validity window",
            BlockStatus::ContainsLowCostDeploy => "a deploy's phlo price is below the minimum",
        };
        write!(f, "{s}")
    }
}
