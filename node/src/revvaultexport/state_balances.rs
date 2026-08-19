//! State-balance reading (port of `node/revvaultexport/StateBalances.scala`).
//!
//! The full `read` (which opens the LMDB store manager) is deferred; only the genesis vault-map
//! extraction is ported.

use rchain_casper::block_random_seed::BlockRandomSeed;
use rchain_models::ast::{ETuple, Expr, Par};
use rchain_rholang::runtime::RhoRuntime;

/// Read the genesis vault-map channel by querying the `extractState` continuation (port of
/// `getGenesisVaultMapPar`).
pub async fn get_genesis_vault_map_par(
    shard_id: &str,
    runtime: &RhoRuntime,
) -> Result<Par, String> {
    let rev_vault_unf = BlockRandomSeed::rev_vault_unforgeable(shard_id);
    let extract_state_string = Par {
        exprs: vec![Expr::GString("extractState".to_string())],
        ..Default::default()
    };
    let e = Par {
        exprs: vec![Expr::ETuple(ETuple {
            ps: vec![rev_vault_unf, extract_state_string],
            ..ETuple::default()
        })],
        ..Default::default()
    };

    let conts = runtime
        .get_continuation_par(&[e])
        .await
        .map_err(|e| e.to_string())?;
    let body = conts
        .first()
        .map(|(_, body)| body)
        .ok_or_else(|| "no extractState continuation".to_string())?;

    let vault_map_key = Par {
        exprs: vec![Expr::GString("vaultMap".to_string())],
        ..Default::default()
    };
    body.sends
        .first()
        .and_then(|s| s.data.first())
        .and_then(|d| d.exprs.first())
        .and_then(|expr| match expr {
            Expr::EMap(map) => map
                .kvs
                .iter()
                .find(|(k, _)| *k == vault_map_key)
                .map(|(_, v)| v.clone()),
            _ => None,
        })
        .ok_or_else(|| "vaultMap key not found".to_string())
}
