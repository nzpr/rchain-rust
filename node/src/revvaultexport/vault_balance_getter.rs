//! Vault balance getter (port of `node/revvaultexport/VaultBalanceGetter.scala`).

use rchain_crypto::hash::blake2b512_random::Blake2b512Random;
use rchain_models::ast::{Expr, GPrivate, GUnforgeable, Par, Send};
use rchain_models::types::Closed;
use rchain_rholang::accounting::Costs;
use rchain_rholang::env::Env;
use rchain_rholang::runtime::RhoRuntime;

use super::rho_trie_traverser::{traverse_trie, vec_par_map_to_map};

fn new_return_name() -> Par {
    let mut rand = Blake2b512Random::default_random();
    Par {
        unforgeables: vec![GUnforgeable::GPrivate(GPrivate { id: rand.next() })],
        ..Default::default()
    }
}

fn get_balance_par(vault_par: &Par, return_channel: &Par) -> Par {
    Par {
        sends: vec![Send {
            chan: Box::new(vault_par.clone().quote()),
            data: vec![
                Par {
                    exprs: vec![Expr::GString("balance".to_string())],
                    ..Default::default()
                }
                .quote(),
                return_channel.clone().quote(),
            ],
            persistent: false,
            locally_free: Default::default(),
            connective_used: false,
        }],
        ..Default::default()
    }
}

/// Read the balance of a single vault (port of `getBalanceFromVaultPar`).
pub async fn get_balance_from_vault_par(
    vault_par: &Par,
    runtime: &RhoRuntime,
) -> Result<Option<i64>, String> {
    runtime.cost().set(Costs::unsafe_max());
    let ret = new_return_name();
    let get_balance_par = get_balance_par(vault_par, &ret);
    let get_balance_par =
        Closed::new(get_balance_par).ok_or_else(|| "balance probe is not closed".to_string())?;
    runtime
        .inj(
            &get_balance_par,
            &Env::new(),
            &Blake2b512Random::default_random(),
        )
        .await
        .map_err(|e| e.to_string())?;
    let data = runtime.get_data(&ret).await.map_err(|e| e.to_string())?;
    Ok(data.first().and_then(|d| {
        let head_par = match d.a.pars.as_slice() {
            [single] => single,
            _ => return None,
        };
        let head_expr = match head_par.exprs.as_slice() {
            [single] => single,
            _ => return None,
        };
        match head_expr {
            Expr::GInt(i) => Some(*i),
            _ => None,
        }
    }))
}

/// Read all vault balances by traversing the vault TreeHashMap (port of `getAllVaultBalance`).
pub async fn get_all_vault_balance(
    vault_tree_hash_map_depth: i32,
    vault_channel: &Par,
    store_token_unf: &Par,
    runtime: &RhoRuntime,
) -> Result<Vec<(Vec<u8>, i64)>, String> {
    let vault_map = traverse_trie(vault_tree_hash_map_depth, vault_channel, store_token_unf, runtime)
        .await
        .map_err(|e| e.to_string())?;
    let extracted = vec_par_map_to_map(
        &vault_map,
        |p| match p.exprs.first() {
            Some(Expr::GByteArray(bs)) => bs.clone(),
            _ => Vec::new(),
        },
        |p| p.clone(),
    );

    let mut result = Vec::new();
    for (key, vault_par) in extracted {
        let balance = get_balance_from_vault_par(&vault_par, runtime).await?;
        result.push((key, balance.unwrap_or(0)));
    }
    Ok(result)
}
