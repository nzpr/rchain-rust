//! Web API protobuf conversion functions (port of the conversion fns in `api/WebApi.scala`).

use rchain_models::ast::Par;
use rchain_models::casper::protocol::deploy_service::{
    DataWithBlockInfo, DeployExecStatus as CasperDeployExecStatus, LightBlockInfo, Status,
};

use super::dto::{
    ApiStatus, DataAtNameResponse, DeployExecStatus as ApiDeployExecStatus, RhoDataResponse,
    RhoExprWithBlock, VersionInfo,
};
use super::rho_expr::{expr_from_par, RhoExpr};

/// Map a casper `Status` to an `ApiStatus` (port of `toApiStatus`).
pub fn to_api_status(status: &Status) -> ApiStatus {
    ApiStatus {
        version: VersionInfo {
            api: status.version.api.clone(),
            node: status.version.node.clone(),
        },
        address: status.address.clone(),
        network_id: status.network_id.clone(),
        shard_id: status.shard_id.clone(),
        peers: status.peers,
        nodes: status.nodes,
        min_phlo_price: status.min_phlo_price,
        latest_block_number: status.latest_block_number,
    }
}

/// Map a casper `DeployExecStatus` to the API `DeployExecStatus` (port of `toDeployExecStatus`).
pub fn to_deploy_exec_status(status: &CasperDeployExecStatus) -> Option<ApiDeployExecStatus> {
    match status {
        CasperDeployExecStatus::ProcessedWithSuccess {
            deploy_result,
            block,
        } => {
            let result: Vec<RhoExpr> = deploy_result.iter().filter_map(expr_from_par).collect();
            Some(ApiDeployExecStatus::ProcessedWithSuccess {
                deploy_result: result,
                block: block.clone(),
            })
        }
        CasperDeployExecStatus::ProcessedWithError {
            deploy_error,
            block,
        } => Some(ApiDeployExecStatus::ProcessedWithError {
            deploy_error: deploy_error.clone(),
            block: block.clone(),
        }),
        CasperDeployExecStatus::NotProcessed { status } => {
            Some(ApiDeployExecStatus::NotProcessed {
                status: status.clone(),
            })
        }
    }
}

/// Map post-block `Par`s plus a block to a `RhoDataResponse` (port of `toRhoDataResponse`).
pub fn to_rho_data_response(pars: &[Par], block: &LightBlockInfo) -> RhoDataResponse {
    RhoDataResponse {
        expr: pars.iter().filter_map(expr_from_par).collect(),
        block: block.clone(),
    }
}

/// Map post-block data plus a length to a `DataAtNameResponse` (port of `toDataAtNameResponse`).
pub fn to_data_at_name_response(dbs: &[DataWithBlockInfo], length: i32) -> DataAtNameResponse {
    let mut exprs_with_block = Vec::new();
    for data in dbs {
        let exprs: Vec<RhoExpr> = data
            .post_block_data
            .iter()
            .filter_map(expr_from_par)
            .collect();
        // Implements the semantic of Par with Unit: P | Nil ==> P.
        let expr = if let [single] = exprs.as_slice() {
            single.clone()
        } else {
            RhoExpr::ExprPar(exprs)
        };
        exprs_with_block.insert(
            0,
            RhoExprWithBlock {
                expr,
                block: data.block.clone(),
            },
        );
    }
    DataAtNameResponse {
        exprs: exprs_with_block,
        length,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rchain_models::ast::Expr;
    use rchain_models::casper::protocol::deploy_service::{
        BondInfo, Status, VersionInfo as CasperVersionInfo,
    };

    fn light_block_info() -> LightBlockInfo {
        LightBlockInfo {
            version: 1,
            shard_id: "root".to_string(),
            block_hash: "h".to_string(),
            block_number: 1,
            sender: "s".to_string(),
            seq_num: 1,
            pre_state_hash: "pre".to_string(),
            post_state_hash: "post".to_string(),
            justifications: vec![],
            bonds: vec![BondInfo {
                validator: "v".to_string(),
                stake: 100,
            }],
            sig_algorithm: "secp256k1".to_string(),
            sig: "sig".to_string(),
            block_size: "0".to_string(),
            deploy_count: 0,
            rejected_deploys: vec![],
        }
    }

    fn par_int(n: i64) -> Par {
        Par {
            exprs: vec![Expr::GInt(n)],
            ..Par::default()
        }
    }

    #[test]
    fn to_api_status_maps_fields() {
        let status = Status {
            version: CasperVersionInfo {
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
        };
        let api = to_api_status(&status);
        assert_eq!(api.version.api, "1.0");
        assert_eq!(api.address, "addr");
        assert_eq!(api.min_phlo_price, 3);
    }

    #[test]
    fn to_deploy_exec_status_maps_oneof() {
        let not_processed = to_deploy_exec_status(&CasperDeployExecStatus::NotProcessed {
            status: "pending".to_string(),
        })
        .unwrap();
        assert_eq!(
            not_processed,
            ApiDeployExecStatus::NotProcessed {
                status: "pending".to_string()
            }
        );

        let err = to_deploy_exec_status(&CasperDeployExecStatus::ProcessedWithError {
            deploy_error: "boom".to_string(),
            block: light_block_info(),
        })
        .unwrap();
        assert!(matches!(err, ApiDeployExecStatus::ProcessedWithError { .. }));
    }

    #[test]
    fn to_rho_data_response_maps_pars() {
        let resp = to_rho_data_response(&[par_int(42)], &light_block_info());
        assert_eq!(resp.expr, vec![RhoExpr::ExprInt(42)]);
        assert_eq!(resp.block.block_hash, "h");
    }

    #[test]
    fn to_data_at_name_response_maps_data() {
        let db = DataWithBlockInfo {
            post_block_data: vec![par_int(1)],
            block: light_block_info(),
        };
        let resp = to_data_at_name_response(&[db], 5);
        assert_eq!(resp.length, 5);
        assert_eq!(resp.exprs.len(), 1);
        assert_eq!(resp.exprs[0].expr, RhoExpr::ExprInt(1));
    }
}
