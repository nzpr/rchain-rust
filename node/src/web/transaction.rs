//! Transaction reporting data model (port of the DTOs + interface in `web/Transaction.scala`).

use rchain_crypto::hash::blake2b256_hash::Blake2b256Hash;
use rchain_models::ast::Par;
use serde::{Deserialize, Serialize};

/// A REV transaction (port of `Transaction`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transaction {
    pub from_addr: String,
    pub to_addr: String,
    pub amount: i64,
    pub ret_unforgeable: Par,
    pub fail_reason: Option<String>,
}

/// The kind of a transaction (port of `TransactionType`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all_fields = "camelCase")]
pub enum TransactionType {
    PreCharge {
        deploy_id: String,
    },
    UserDeploy {
        deploy_id: String,
    },
    Refund {
        deploy_id: String,
    },
    CloseBlock {
        block_hash: String,
    },
    SlashingDeploy {
        block_hash: String,
    },
}

/// A transaction plus its type (port of `TransactionInfo`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionInfo {
    pub transaction: Transaction,
    pub transaction_type: TransactionType,
}

/// A list of transactions (port of `TransactionResponse`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionResponse {
    pub data: Vec<TransactionInfo>,
}

/// Transaction reporting interface (port of `TransactionAPI[F]`, effect simplified to synchronous).
pub trait TransactionApi: Send + Sync {
    fn get_transaction(&self, block_hash: &Blake2b256Hash) -> Vec<TransactionInfo>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transaction_type_variants() {
        assert_eq!(
            TransactionType::PreCharge {
                deploy_id: "d1".to_string()
            },
            TransactionType::PreCharge {
                deploy_id: "d1".to_string()
            }
        );
        assert_eq!(
            TransactionType::UserDeploy {
                deploy_id: "d1".to_string()
            },
            TransactionType::UserDeploy {
                deploy_id: "d1".to_string()
            }
        );
        assert_eq!(
            TransactionType::CloseBlock {
                block_hash: "b1".to_string()
            },
            TransactionType::CloseBlock {
                block_hash: "b1".to_string()
            }
        );
        assert_ne!(
            TransactionType::Refund {
                deploy_id: "d1".to_string()
            },
            TransactionType::SlashingDeploy {
                block_hash: "d1".to_string()
            }
        );
    }

    #[test]
    fn transaction_response_composes() {
        let tx = Transaction {
            from_addr: "a".to_string(),
            to_addr: "b".to_string(),
            amount: 100,
            ret_unforgeable: Par::default(),
            fail_reason: None,
        };
        let info = TransactionInfo {
            transaction: tx.clone(),
            transaction_type: TransactionType::UserDeploy {
                deploy_id: "d".to_string(),
            },
        };
        let response = TransactionResponse { data: vec![info] };
        assert_eq!(response.data.len(), 1);
        assert_eq!(response.data[0].transaction, tx);
        assert_eq!(response.data[0].transaction.amount, 100);
    }
}
