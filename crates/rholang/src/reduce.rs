//! The pure expression-evaluation surface of the reducer (Law 4).
//!
//! Mirrors `Reduce.scala` (`evalExpr` / `evalExprToExpr` / `evalSingleExpr` / `evalToBool` and the
//! arithmetic/comparison/boolean/string/method helpers). The effectful term dispatch
//! (`eval(Send/Receive/New/Match/Bundle)`, `produce`/`consume`, `new` allocation) and the
//! collection methods (`union`/`diff`/`add`/`delete`/`contains`/`slice`/`keys`) are deferred.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use num_bigint::BigInt;
use rchain_crypto::hash::blake2b512_random::Blake2b512Random;
use rchain_models::ast::{
    AlwaysEqual, Bundle, EList, ETuple, Expr, GPrivate, GUnforgeable, Match, MatchCase, New, Par,
    ParMap, Receive, ReceiveBind, Send, Var,
};
use rchain_models::par_ops::{from_expr, par_concat, single_bundle, single_expr, typ};
use rchain_models::runtime::{BindPattern, ListParWithRandom, ParWithRandom, TaggedContinuation};
use rchain_models::sorter::{par_map, par_set};

use crate::accounting::{CostAccounting, Costs};
use crate::env::Env;
use crate::errors::RholangError;
use crate::matcher::spatial_match_result;
use crate::substitute::{substitute_par, substitute_par_no_sort};

fn union_free(a: Vec<i32>, b: Vec<i32>) -> Vec<i32> {
    let mut set: BTreeSet<i32> = a.into_iter().collect();
    set.extend(b);
    set.into_iter().collect()
}

/// Recompute a `Par`'s cached `locallyFree` from its sub-terms (port of `updateLocallyFree`).
pub fn update_locally_free(par: &Par) -> Par {
    let mut free = Vec::new();
    for s in &par.sends {
        free = union_free(free, s.locally_free.0.clone());
    }
    for r in &par.receives {
        free = union_free(free, r.locally_free.0.clone());
    }
    for n in &par.news {
        free = union_free(free, n.locally_free.0.clone());
    }
    for e in &par.exprs {
        free = union_free(free, rchain_models::par_ops::locally_free_of_expr(e, 0));
    }
    for m in &par.matches {
        free = union_free(free, m.locally_free.0.clone());
    }
    for b in &par.bundles {
        free = union_free(free, b.body.locally_free.0.clone());
    }
    Par {
        locally_free: AlwaysEqual(free),
        ..par.clone()
    }
}

fn eval_var(v: &Var, env: &Env<Par>, cost: &CostAccounting) -> Result<Par, RholangError> {
    cost.charge(Costs::var_eval_cost())?;
    match v {
        Var::BoundVar(level) => env.get(*level).ok_or_else(|| {
            RholangError::ReduceError(format!("Unbound variable: {level}"))
        }),
        Var::Wildcard | Var::FreeVar(_) => Err(RholangError::ReduceError(
            "Unbound variable: attempting to evaluate a pattern".to_string(),
        )),
        Var::Empty => Err(RholangError::ReduceError(
            "Impossible var instance EMPTY".to_string(),
        )),
    }
}

fn eval_to_bool(par: &Par, env: &Env<Par>, cost: &CostAccounting) -> Result<bool, RholangError> {
    match eval_single_expr(par, env, cost)? {
        Expr::GBool(b) => Ok(b),
        other => Err(RholangError::ReduceError(format!(
            "Error: expected Bool, got {}",
            typ(&other)
        ))),
    }
}

fn eval_to_long(par: &Par, env: &Env<Par>, cost: &CostAccounting) -> Result<i64, RholangError> {
    match eval_single_expr(par, env, cost)? {
        Expr::GInt(v) => Ok(v),
        other => Err(RholangError::ReduceError(format!(
            "Error: expected Int, got {}",
            typ(&other)
        ))),
    }
}

fn restrict_to_int(n: i64) -> Result<usize, RholangError> {
    if n > i32::MAX as i64 || n < i32::MIN as i64 {
        Err(RholangError::ReduceError(format!(
            "Error: value out of range: {n}"
        )))
    } else {
        Ok(n as usize)
    }
}

pub fn eval_single_expr(
    par: &Par,
    env: &Env<Par>,
    cost: &CostAccounting,
) -> Result<Expr, RholangError> {
    match single_expr(par) {
        Some(expr) => eval_expr_to_expr(expr, env, cost),
        None => Err(RholangError::ReduceError(
            "Expected a single expression".to_string(),
        )),
    }
}

fn relop(
    p1: &Par,
    p2: &Par,
    relopb: fn(bool, bool) -> bool,
    relopi: fn(i64, i64) -> bool,
    relopbi: fn(&BigInt, &BigInt) -> bool,
    relops: fn(&str, &str) -> bool,
    env: &Env<Par>,
    cost: &CostAccounting,
) -> Result<Expr, RholangError> {
    let v1 = eval_single_expr(p1, env, cost)?;
    let v2 = eval_single_expr(p2, env, cost)?;
    Ok(match (&v1, &v2) {
        (Expr::GBool(b1), Expr::GBool(b2)) => {
            cost.charge(Costs::comparison_cost())?;
            Expr::GBool(relopb(*b1, *b2))
        }
        (Expr::GInt(i1), Expr::GInt(i2)) => {
            cost.charge(Costs::comparison_cost())?;
            Expr::GBool(relopi(*i1, *i2))
        }
        (Expr::GBigInt(b1), Expr::GBigInt(b2)) => {
            cost.charge(Costs::big_int_comparison(b1, b2))?;
            Expr::GBool(relopbi(b1, b2))
        }
        (Expr::GString(s1), Expr::GString(s2)) => {
            cost.charge(Costs::comparison_cost())?;
            Expr::GBool(relops(s1, s2))
        }
        _ => {
            return Err(RholangError::ReduceError(format!(
                "Unexpected compare: {v1:?} vs. {v2:?}"
            )))
        }
    })
}

fn eval_to_string_pair(
    key: &Expr,
    value: &Expr,
) -> Result<(String, String), RholangError> {
    match (key, value) {
        (Expr::GString(k), Expr::GString(v)) => Ok((k.clone(), v.clone())),
        (Expr::GString(k), Expr::GInt(v)) => Ok((k.clone(), v.to_string())),
        (Expr::GString(k), Expr::GBigInt(v)) => Ok((k.clone(), v.to_string())),
        (Expr::GString(k), Expr::GBool(v)) => Ok((k.clone(), v.to_string())),
        (Expr::GString(k), Expr::GUri(v)) => Ok((k.clone(), v.clone())),
        (Expr::GString(_), value) => Err(RholangError::ReduceError(format!(
            "Error: interpolation doesn't support {}",
            typ(value)
        ))),
        _ => Err(RholangError::ReduceError(
            "Error: interpolation Map should only contain String keys".to_string(),
        )),
    }
}

fn interpolate(string: &str, pairs: &[(String, String)]) -> String {
    let mut result = String::new();
    let mut current = string;
    while !current.is_empty() {
        match pairs.iter().find(|(k, _)| current.starts_with(&format!("${{{k}}}"))) {
            Some((k, v)) => {
                result.push_str(v);
                current = &current[k.len() + 3..];
            }
            None => {
                let mut chars = current.chars();
                result.push(chars.next().unwrap());
                current = chars.as_str();
            }
        }
    }
    result
}

fn eval_expr_to_par(expr: &Expr, env: &Env<Par>, cost: &CostAccounting) -> Result<Par, RholangError> {
    match expr {
        Expr::EVar(v) => {
            let p = eval_var(v, env, cost)?;
            eval_expr(&p, env, cost)
        }
        Expr::EMethod(em) => {
            cost.charge(Costs::method_call_cost())?;
            let evaled_target = eval_expr(&em.target, env, cost)?;
            let evaled_args: Vec<Par> = em
                .arguments
                .iter()
                .map(|a| eval_expr(a, env, cost))
                .collect::<Result<_, _>>()?;
            eval_method(&em.method_name, &evaled_target, &evaled_args, env, cost)
        }
        _ => Ok(from_expr(eval_expr_to_expr(expr, env, cost)?)),
    }
}

fn eval_expr_to_expr(expr: &Expr, env: &Env<Par>, cost: &CostAccounting) -> Result<Expr, RholangError> {
    match expr {
        Expr::GBool(_)
        | Expr::GInt(_)
        | Expr::GBigInt(_)
        | Expr::GString(_)
        | Expr::GUri(_)
        | Expr::GByteArray(_) => Ok(expr.clone()),
        Expr::ENot(p) => Ok(Expr::GBool(!eval_to_bool(p, env, cost)?)),
        Expr::ENeg(p) => {
            let v = eval_single_expr(p, env, cost)?;
            match v {
                Expr::GInt(hs) => Ok(Expr::GInt(-hs)),
                Expr::GBigInt(hs) => {
                    let r = -hs;
                    cost.charge(Costs::big_int_negation(&r))?;
                    Ok(Expr::GBigInt(r))
                }
                other => Err(RholangError::OperatorNotDefined {
                    op: "Negation".to_string(),
                    other_type: typ(&other).to_string(),
                }),
            }
        }
        Expr::EMult(p1, p2) => {
            let v1 = eval_single_expr(p1, env, cost)?;
            let v2 = eval_single_expr(p2, env, cost)?;
            match (&v1, &v2) {
                (Expr::GInt(l), Expr::GInt(r)) => {
                    cost.charge(Costs::multiplication_cost())?;
                    Ok(Expr::GInt(l * r))
                }
                (Expr::GBigInt(l), Expr::GBigInt(r)) => {
                    cost.charge(Costs::big_int_multiplication(l, r))?;
                    Ok(Expr::GBigInt(l * r))
                }
                (Expr::GInt(_), o) => Err(RholangError::OperatorExpectedError {
                    op: "*".to_string(),
                    expected: "Int".to_string(),
                    other_type: typ(o).to_string(),
                }),
                (Expr::GBigInt(_), o) => Err(RholangError::OperatorExpectedError {
                    op: "*".to_string(),
                    expected: "BigInt".to_string(),
                    other_type: typ(o).to_string(),
                }),
                (o, _) => Err(RholangError::OperatorNotDefined {
                    op: "*".to_string(),
                    other_type: typ(o).to_string(),
                }),
            }
        }
        Expr::EDiv(p1, p2) => {
            let v1 = eval_single_expr(p1, env, cost)?;
            let v2 = eval_single_expr(p2, env, cost)?;
            match (&v1, &v2) {
                (Expr::GInt(l), Expr::GInt(r)) => {
                    cost.charge(Costs::division_cost())?;
                    Ok(Expr::GInt(l / r))
                }
                (Expr::GBigInt(l), Expr::GBigInt(r)) => {
                    cost.charge(Costs::big_int_division(l, r))?;
                    Ok(Expr::GBigInt(l / r))
                }
                (Expr::GInt(_), o) => Err(RholangError::OperatorExpectedError {
                    op: "/".to_string(),
                    expected: "Int".to_string(),
                    other_type: typ(o).to_string(),
                }),
                (Expr::GBigInt(_), o) => Err(RholangError::OperatorExpectedError {
                    op: "/".to_string(),
                    expected: "BigInt".to_string(),
                    other_type: typ(o).to_string(),
                }),
                (o, _) => Err(RholangError::OperatorNotDefined {
                    op: "/".to_string(),
                    other_type: typ(o).to_string(),
                }),
            }
        }
        Expr::EMod(p1, p2) => {
            let v1 = eval_single_expr(p1, env, cost)?;
            let v2 = eval_single_expr(p2, env, cost)?;
            match (&v1, &v2) {
                (Expr::GInt(l), Expr::GInt(r)) => {
                    cost.charge(Costs::modulo_cost())?;
                    Ok(Expr::GInt(l % r))
                }
                (Expr::GBigInt(l), Expr::GBigInt(r)) => {
                    cost.charge(Costs::big_int_modulo(l, r))?;
                    Ok(Expr::GBigInt(l % r))
                }
                (Expr::GInt(_), o) => Err(RholangError::OperatorExpectedError {
                    op: "%".to_string(),
                    expected: "Int".to_string(),
                    other_type: typ(o).to_string(),
                }),
                (Expr::GBigInt(_), o) => Err(RholangError::OperatorExpectedError {
                    op: "%".to_string(),
                    expected: "BigInt".to_string(),
                    other_type: typ(o).to_string(),
                }),
                (o, _) => Err(RholangError::OperatorNotDefined {
                    op: "%".to_string(),
                    other_type: typ(o).to_string(),
                }),
            }
        }
        Expr::EPlus(p1, p2) => {
            let v1 = eval_single_expr(p1, env, cost)?;
            let v2 = eval_single_expr(p2, env, cost)?;
            match (&v1, &v2) {
                (Expr::GInt(l), Expr::GInt(r)) => {
                    cost.charge(Costs::sum_cost())?;
                    Ok(Expr::GInt(l + r))
                }
                (Expr::GBigInt(l), Expr::GBigInt(r)) => {
                    cost.charge(Costs::big_int_sum(l, r))?;
                    Ok(Expr::GBigInt(l + r))
                }
                (Expr::GInt(_), o) => Err(RholangError::OperatorExpectedError {
                    op: "+".to_string(),
                    expected: "Int".to_string(),
                    other_type: typ(o).to_string(),
                }),
                (Expr::GBigInt(_), o) => Err(RholangError::OperatorExpectedError {
                    op: "+".to_string(),
                    expected: "BigInt".to_string(),
                    other_type: typ(o).to_string(),
                }),
                (o, _) => Err(RholangError::OperatorNotDefined {
                    op: "+".to_string(),
                    other_type: typ(o).to_string(),
                }),
            }
        }
        Expr::EMinus(p1, p2) => {
            let v1 = eval_single_expr(p1, env, cost)?;
            let v2 = eval_single_expr(p2, env, cost)?;
            match (&v1, &v2) {
                (Expr::GInt(l), Expr::GInt(r)) => {
                    cost.charge(Costs::subtraction_cost())?;
                    Ok(Expr::GInt(l - r))
                }
                (Expr::GBigInt(l), Expr::GBigInt(r)) => {
                    cost.charge(Costs::big_int_subtraction(l, r))?;
                    Ok(Expr::GBigInt(l - r))
                }
                (Expr::GInt(_), o) => Err(RholangError::OperatorExpectedError {
                    op: "-".to_string(),
                    expected: "Int".to_string(),
                    other_type: typ(o).to_string(),
                }),
                (Expr::GBigInt(_), o) => Err(RholangError::OperatorExpectedError {
                    op: "-".to_string(),
                    expected: "BigInt".to_string(),
                    other_type: typ(o).to_string(),
                }),
                (o, _) => Err(RholangError::OperatorNotDefined {
                    op: "-".to_string(),
                    other_type: typ(o).to_string(),
                }),
            }
        }
        Expr::ELt(p1, p2) => relop(p1, p2, |a, b| a < b, |a, b| a < b, |a, b| a < b, |a, b| a < b, env, cost),
        Expr::ELte(p1, p2) => {
            relop(p1, p2, |a, b| a <= b, |a, b| a <= b, |a, b| a <= b, |a, b| a <= b, env, cost)
        }
        Expr::EGt(p1, p2) => relop(p1, p2, |a, b| a > b, |a, b| a > b, |a, b| a > b, |a, b| a > b, env, cost),
        Expr::EGte(p1, p2) => {
            relop(p1, p2, |a, b| a >= b, |a, b| a >= b, |a, b| a >= b, |a, b| a >= b, env, cost)
        }
        Expr::EEq(p1, p2) => {
            let v1 = eval_expr(p1, env, cost)?;
            let v2 = eval_expr(p2, env, cost)?;
            let sv1 = substitute_par(&v1, 0, env)?;
            let sv2 = substitute_par(&v2, 0, env)?;
            Ok(Expr::GBool(sv1 == sv2))
        }
        Expr::ENeq(p1, p2) => {
            let v1 = eval_expr(p1, env, cost)?;
            let v2 = eval_expr(p2, env, cost)?;
            let sv1 = substitute_par(&v1, 0, env)?;
            let sv2 = substitute_par(&v2, 0, env)?;
            Ok(Expr::GBool(sv1 != sv2))
        }
        Expr::EAnd(p1, p2) => {
            let b1 = eval_to_bool(p1, env, cost)?;
            let b2 = eval_to_bool(p2, env, cost)?;
            cost.charge(Costs::boolean_and_cost())?;
            Ok(Expr::GBool(b1 && b2))
        }
        Expr::EOr(p1, p2) => {
            let b1 = eval_to_bool(p1, env, cost)?;
            let b2 = eval_to_bool(p2, env, cost)?;
            cost.charge(Costs::boolean_or_cost())?;
            Ok(Expr::GBool(b1 || b2))
        }
        Expr::EShortAnd(p1, p2) => {
            let b1 = eval_to_bool(p1, env, cost)?;
            let b2 = if b1 { eval_to_bool(p2, env, cost)? } else { false };
            cost.charge(Costs::boolean_and_cost())?;
            Ok(Expr::GBool(b1 && b2))
        }
        Expr::EShortOr(p1, p2) => {
            let b1 = eval_to_bool(p1, env, cost)?;
            let b2 = if b1 { true } else { eval_to_bool(p2, env, cost)? };
            cost.charge(Costs::boolean_or_cost())?;
            Ok(Expr::GBool(b1 || b2))
        }
        Expr::EMatches(target, pattern) => {
            let evaled_target = eval_expr(target, env, cost)?;
            let subst_target = substitute_par(&evaled_target, 0, env)?;
            let subst_pattern = substitute_par(pattern, 1, env)?;
            let m = spatial_match_result(&subst_target, &subst_pattern)?;
            Ok(Expr::GBool(m.is_some()))
        }
        Expr::EPercentPercent(p1, p2) => {
            cost.charge(Costs::op_call_cost())?;
            let v1 = eval_single_expr(p1, env, cost)?;
            let v2 = eval_single_expr(p2, env, cost)?;
            match (&v1, &v2) {
                (Expr::GString(lhs), Expr::EMap(ParMap { kvs, .. })) => {
                    if lhs.is_empty() && kvs.is_empty() {
                        Ok(Expr::GString(lhs.clone()))
                    } else {
                        let mut pairs = Vec::new();
                        for (k, v) in kvs {
                            let key_expr = eval_single_expr(k, env, cost)?;
                            let value_expr = eval_single_expr(v, env, cost)?;
                            pairs.push(eval_to_string_pair(&key_expr, &value_expr)?);
                        }
                        cost.charge(Costs::interpolate_cost(lhs.len() as i64, kvs.len() as i64))?;
                        Ok(Expr::GString(interpolate(lhs, &pairs)))
                    }
                }
                (Expr::GString(_), o) => Err(RholangError::OperatorExpectedError {
                    op: "%%".to_string(),
                    expected: "Map".to_string(),
                    other_type: typ(o).to_string(),
                }),
                (o, _) => Err(RholangError::OperatorNotDefined {
                    op: "%%".to_string(),
                    other_type: typ(o).to_string(),
                }),
            }
        }
        Expr::EPlusPlus(p1, p2) => {
            cost.charge(Costs::op_call_cost())?;
            let v1 = eval_single_expr(p1, env, cost)?;
            let v2 = eval_single_expr(p2, env, cost)?;
            match (&v1, &v2) {
                (Expr::GString(l), Expr::GString(r)) => {
                    cost.charge(Costs::string_append_cost(l.len() as i64, r.len() as i64))?;
                    Ok(Expr::GString(format!("{l}{r}")))
                }
                (Expr::GByteArray(l), Expr::GByteArray(r)) => {
                    cost.charge(Costs::string_append_cost(l.len() as i64, r.len() as i64))?;
                    let mut out = l.clone();
                    out.extend(r);
                    Ok(Expr::GByteArray(out))
                }
                (Expr::EList(l), Expr::EList(r)) => {
                    let mut ps = l.ps.clone();
                    ps.extend(r.ps.clone());
                    Ok(Expr::EList(EList {
                        ps,
                        locally_free: AlwaysEqual(union_free(
                            l.locally_free.0.clone(),
                            r.locally_free.0.clone(),
                        )),
                        connective_used: l.connective_used || r.connective_used,
                        ..Default::default()
                    }))
                }
                (Expr::GString(_), o) => Err(RholangError::OperatorExpectedError {
                    op: "++".to_string(),
                    expected: "String".to_string(),
                    other_type: typ(o).to_string(),
                }),
                (Expr::EList(_), o) => Err(RholangError::OperatorExpectedError {
                    op: "++".to_string(),
                    expected: "List".to_string(),
                    other_type: typ(o).to_string(),
                }),
                (o, _) => Err(RholangError::OperatorNotDefined {
                    op: "++".to_string(),
                    other_type: typ(o).to_string(),
                }),
            }
        }
        Expr::EMinusMinus(p1, p2) => {
            cost.charge(Costs::op_call_cost())?;
            let v1 = eval_single_expr(p1, env, cost)?;
            let v2 = eval_single_expr(p2, env, cost)?;
            match (&v1, &v2) {
                (Expr::ESet(_), Expr::ESet(_)) => Err(RholangError::ReduceError(
                    "set difference (--) is not yet ported".to_string(),
                )),
                (Expr::ESet(_), o) => Err(RholangError::OperatorExpectedError {
                    op: "--".to_string(),
                    expected: "Set".to_string(),
                    other_type: typ(o).to_string(),
                }),
                (o, _) => Err(RholangError::OperatorNotDefined {
                    op: "--".to_string(),
                    other_type: typ(o).to_string(),
                }),
            }
        }
        Expr::EVar(v) => {
            let p = eval_var(v, env, cost)?;
            eval_single_expr(&p, env, cost)
        }
        Expr::EList(el) => {
            let evaled: Vec<Par> = el
                .ps
                .iter()
                .map(|p| eval_expr(p, env, cost).map(|p| update_locally_free(&p)))
                .collect::<Result<_, _>>()?;
            Ok(Expr::EList(EList {
                ps: evaled,
                locally_free: el.locally_free.clone(),
                connective_used: el.connective_used,
                ..Default::default()
            }))
        }
        Expr::ETuple(el) => {
            let evaled: Vec<Par> = el
                .ps
                .iter()
                .map(|p| eval_expr(p, env, cost).map(|p| update_locally_free(&p)))
                .collect::<Result<_, _>>()?;
            Ok(Expr::ETuple(ETuple {
                ps: evaled,
                locally_free: el.locally_free.clone(),
                connective_used: el.connective_used,
            }))
        }
        Expr::ESet(set) => {
            let evaled: Vec<Par> = set
                .ps
                .iter()
                .map(|p| eval_expr(p, env, cost).map(|p| update_locally_free(&p)))
                .collect::<Result<_, _>>()?;
            let mut s = par_set(evaled);
            s.connective_used = set.connective_used;
            s.locally_free = set.locally_free.clone();
            s.remainder = set.remainder.clone();
            Ok(Expr::ESet(s))
        }
        Expr::EMap(map) => {
            let evaled: Vec<(Par, Par)> = map
                .kvs
                .iter()
                .map(|(k, v)| {
                    Ok((
                        update_locally_free(&eval_expr(k, env, cost)?),
                        update_locally_free(&eval_expr(v, env, cost)?),
                    ))
                })
                .collect::<Result<_, RholangError>>()?;
            let mut m = par_map(evaled);
            m.connective_used = map.connective_used;
            m.locally_free = map.locally_free.clone();
            m.remainder = map.remainder.clone();
            Ok(Expr::EMap(m))
        }
        Expr::EMethod(em) => {
            cost.charge(Costs::method_call_cost())?;
            let evaled_target = eval_expr(&em.target, env, cost)?;
            let evaled_args: Vec<Par> = em
                .arguments
                .iter()
                .map(|a| eval_expr(a, env, cost))
                .collect::<Result<_, _>>()?;
            let result_par = eval_method(&em.method_name, &evaled_target, &evaled_args, env, cost)?;
            eval_single_expr(&result_par, env, cost)
        }
    }
}

/// Evaluate the top-level expressions of a `Par` (port of `evalExpr`).
pub fn eval_expr(par: &Par, env: &Env<Par>, cost: &CostAccounting) -> Result<Par, RholangError> {
    let mut result = Par {
        exprs: Vec::new(),
        ..par.clone()
    };
    for e in &par.exprs {
        let evaled = eval_expr_to_par(e, env, cost)?;
        result = par_concat(&result, &evaled);
    }
    Ok(result)
}

fn eval_method(
    method: &str,
    target: &Par,
    args: &[Par],
    env: &Env<Par>,
    cost: &CostAccounting,
) -> Result<Par, RholangError> {
    match method {
        "nth" => {
            if args.len() != 1 {
                return Err(RholangError::MethodArgumentNumberMismatch {
                    method: "nth".to_string(),
                    expected: 1,
                    actual: args.len() as i32,
                });
            }
            cost.charge(Costs::nth_method_call_cost())?;
            let nth_raw = eval_to_long(&args[0], env, cost)?;
            let nth = restrict_to_int(nth_raw)?;
            let v = eval_single_expr(target, env, cost)?;
            match v {
                Expr::EList(EList { ps, .. }) | Expr::ETuple(ETuple { ps, .. }) => {
                    ps.get(nth).cloned().ok_or_else(|| {
                        RholangError::ReduceError(format!("Error: index out of bound: {nth}"))
                    })
                }
                Expr::GByteArray(bs) => {
                    if nth < bs.len() {
                        Ok(from_expr(Expr::GInt(bs[nth] as i64)))
                    } else {
                        Err(RholangError::ReduceError(format!(
                            "Error: index out of bound: {nth}"
                        )))
                    }
                }
                _ => Err(RholangError::ReduceError(
                    "Error: nth applied to something that wasn't a list or tuple.".to_string(),
                )),
            }
        }
        "toInt" => {
            if !args.is_empty() {
                return Err(RholangError::MethodArgumentNumberMismatch {
                    method: "toInt".to_string(),
                    expected: 0,
                    actual: args.len() as i32,
                });
            }
            let base = eval_single_expr(target, env, cost)?;
            match base {
                Expr::GInt(v) => Ok(from_expr(Expr::GInt(v))),
                Expr::GBigInt(bi) => {
                    cost.charge(Costs::to_int_cost_bigint(&bi))?;
                    let v = bi.to_string().parse::<i64>().map_err(|_| {
                        RholangError::ReduceError(format!(
                            "Method toInt(): input BigInt value {bi} out of range"
                        ))
                    })?;
                    Ok(from_expr(Expr::GInt(v)))
                }
                Expr::GString(s) => {
                    cost.charge(Costs::to_int_cost_string(&s))?;
                    let v = s.parse::<i64>().map_err(|_| {
                        RholangError::ReduceError(format!(
                            "Method toInt(): input string \"{s}\" cannot be converted to Int"
                        ))
                    })?;
                    Ok(from_expr(Expr::GInt(v)))
                }
                other => Err(RholangError::MethodNotDefined {
                    method: "toInt".to_string(),
                    other_type: typ(&other).to_string(),
                }),
            }
        }
        "toBigInt" => {
            if !args.is_empty() {
                return Err(RholangError::MethodArgumentNumberMismatch {
                    method: "toBigInt".to_string(),
                    expected: 0,
                    actual: args.len() as i32,
                });
            }
            let base = eval_single_expr(target, env, cost)?;
            match base {
                Expr::GBigInt(v) => Ok(from_expr(Expr::GBigInt(v))),
                Expr::GInt(num) => {
                    cost.charge(Costs::int_to_bigint_cost())?;
                    Ok(from_expr(Expr::GBigInt(BigInt::from(num))))
                }
                Expr::GString(s) => {
                    cost.charge(Costs::to_bigint_cost(&s))?;
                    let v = s.parse::<BigInt>().map_err(|_| {
                        RholangError::ReduceError(format!(
                            "Method toBigInt(): input string \"{s}\" cannot be converted to BigInt"
                        ))
                    })?;
                    Ok(from_expr(Expr::GBigInt(v)))
                }
                other => Err(RholangError::MethodNotDefined {
                    method: "toBigInt".to_string(),
                    other_type: typ(&other).to_string(),
                }),
            }
        }
        _ => Err(RholangError::ReduceError(format!(
            "Unimplemented method: {method}"
        ))),
    }
}

/// The result of a tuplespace produce/consume: the matched continuation, the list of
/// (channel, matched data, removed data, persistent), and whether it was a peek.
pub type Application =
    Option<(TaggedContinuation, Vec<(Par, ListParWithRandom, ListParWithRandom, bool)>, bool)>;

/// The tuplespace interface the evaluator produces/consumes against (port of `RhoTuplespace`).
pub trait Tuplespace {
    fn produce(
        &self,
        channel: &Par,
        data: ListParWithRandom,
        persist: bool,
    ) -> Result<Application, RholangError>;

    fn consume(
        &self,
        channels: &[Par],
        patterns: &[BindPattern],
        continuation: TaggedContinuation,
        persist: bool,
        peeks: &BTreeSet<usize>,
    ) -> Result<Application, RholangError>;
}

/// Dispatches a continuation with its matched data (port of `Dispatch`).
pub trait Dispatch {
    fn dispatch(
        &self,
        continuation: &TaggedContinuation,
        data_list: &[ListParWithRandom],
    ) -> Result<(), RholangError>;
}

enum Term<'a> {
    Send(&'a Send),
    Receive(&'a Receive),
    New(&'a New),
    Match(&'a Match),
    Bundle(&'a Bundle),
    Expr(&'a Expr),
}

/// The reducer (port of `DebruijnInterpreter`).
pub struct DebruijnInterpreter<T: Tuplespace, D: Dispatch> {
    space: T,
    dispatcher: D,
    urn_map: BTreeMap<String, Par>,
    merge_chs: RefCell<Vec<Par>>,
    mergeable_tag_name: Par,
}

impl<T: Tuplespace, D: Dispatch> DebruijnInterpreter<T, D> {
    pub fn new(
        space: T,
        dispatcher: D,
        urn_map: BTreeMap<String, Par>,
        mergeable_tag_name: Par,
    ) -> Self {
        DebruijnInterpreter {
            space,
            dispatcher,
            urn_map,
            merge_chs: RefCell::new(Vec::new()),
            mergeable_tag_name,
        }
    }

    /// Evaluate a top-level `Par` (port of `Reduce.eval(par)`).
    pub fn eval(
        &self,
        par: &Par,
        env: &Env<Par>,
        rand: &Blake2b512Random,
        cost: &CostAccounting,
    ) -> Result<(), RholangError> {
        let mut terms: Vec<Term> = Vec::new();
        for s in &par.sends {
            terms.push(Term::Send(s));
        }
        for r in &par.receives {
            terms.push(Term::Receive(r));
        }
        for n in &par.news {
            terms.push(Term::New(n));
        }
        for m in &par.matches {
            terms.push(Term::Match(m));
        }
        for b in &par.bundles {
            terms.push(Term::Bundle(b));
        }
        for e in &par.exprs {
            if matches!(e, Expr::EVar(_) | Expr::EMethod(_)) {
                terms.push(Term::Expr(e));
            }
        }
        if terms.len() > i16::MAX as usize {
            return Err(RholangError::ReduceError(format!(
                "The number of terms in the Par is {}, which exceeds the limit of {}.",
                terms.len(),
                i16::MAX
            )));
        }
        for (i, term) in terms.iter().enumerate() {
            let r = if terms.len() == 1 {
                rand.clone()
            } else if terms.len() > 256 {
                rand.split_short(i as u16)
            } else {
                rand.split_byte(i as u8)
            };
            self.eval_term(term, env, &r, cost)?;
        }
        Ok(())
    }

    fn eval_term(
        &self,
        term: &Term,
        env: &Env<Par>,
        rand: &Blake2b512Random,
        cost: &CostAccounting,
    ) -> Result<(), RholangError> {
        match term {
            Term::Send(s) => self.eval_send(s, env, rand, cost),
            Term::Receive(r) => self.eval_receive(r, env, rand, cost),
            Term::New(n) => self.eval_new(n, env, rand, cost),
            Term::Match(m) => self.eval_match(m, env, rand, cost),
            Term::Bundle(b) => self.eval_bundle(b, env, rand, cost),
            Term::Expr(e) => match e {
                Expr::EVar(v) => {
                    let p = eval_var(v, env, cost)?;
                    self.eval(&p, env, rand, cost)
                }
                Expr::EMethod(_) => {
                    let p = eval_expr_to_par(e, env, cost)?;
                    self.eval(&p, env, rand, cost)
                }
                _ => Err(RholangError::BugFoundError(format!(
                    "Undefined term: {e:?}"
                ))),
            },
        }
    }

    fn eval_send(
        &self,
        send: &Send,
        env: &Env<Par>,
        rand: &Blake2b512Random,
        cost: &CostAccounting,
    ) -> Result<(), RholangError> {
        cost.charge(Costs::send_eval_cost())?;
        let eval_chan = eval_expr(&send.chan, env, cost)?;
        let sub_chan = substitute_par(&eval_chan, 0, env)?;
        let unbundled = match single_bundle(&sub_chan) {
            Some(value) => {
                if !value.write_flag {
                    return Err(RholangError::ReduceError(
                        "Trying to send on non-writeable channel.".to_string(),
                    ));
                }
                (*value.body).clone()
            }
            None => sub_chan,
        };
        let data: Vec<Par> = send
            .data
            .iter()
            .map(|d| eval_expr(d, env, cost))
            .collect::<Result<_, _>>()?;
        let subst_data: Vec<Par> = data
            .iter()
            .map(|d| substitute_par(d, 0, env))
            .collect::<Result<_, _>>()?;
        self.produce(
            &unbundled,
            ListParWithRandom {
                pars: subst_data,
                random_state: rand.clone(),
            },
            send.persistent,
            cost,
        )
    }

    fn eval_receive(
        &self,
        receive: &Receive,
        env: &Env<Par>,
        rand: &Blake2b512Random,
        cost: &CostAccounting,
    ) -> Result<(), RholangError> {
        cost.charge(Costs::receive_eval_cost())?;
        let mut binds: Vec<(BindPattern, Par)> = Vec::new();
        for rb in &receive.binds {
            let q = self.unbundle_receive(rb, env, cost)?;
            let subst_patterns: Vec<Par> = rb
                .patterns
                .iter()
                .map(|p| substitute_par(p, 1, env))
                .collect::<Result<_, _>>()?;
            binds.push((
                BindPattern {
                    patterns: subst_patterns,
                    remainder: rb.remainder.as_deref().cloned(),
                    free_count: rb.free_count,
                },
                q,
            ));
        }
        let subst_body = substitute_par_no_sort(&receive.body, 0, &env.shift(receive.bind_count))?;
        self.consume(
            &binds,
            ParWithRandom {
                body: subst_body,
                random_state: rand.clone(),
            },
            receive.persistent,
            receive.peek,
            cost,
        )
    }

    fn unbundle_receive(
        &self,
        rb: &ReceiveBind,
        env: &Env<Par>,
        cost: &CostAccounting,
    ) -> Result<Par, RholangError> {
        let eval_src = eval_expr(&rb.source, env, cost)?;
        let subst = substitute_par(&eval_src, 0, env)?;
        match single_bundle(&subst) {
            Some(value) => {
                if !value.read_flag {
                    Err(RholangError::ReduceError(
                        "Trying to read from non-readable channel.".to_string(),
                    ))
                } else {
                    Ok((*value.body).clone())
                }
            }
            None => Ok(subst),
        }
    }

    fn eval_new(
        &self,
        new: &New,
        env: &Env<Par>,
        rand: &Blake2b512Random,
        cost: &CostAccounting,
    ) -> Result<(), RholangError> {
        cost.charge(Costs::new_bindings_cost(new.bind_count as i64))?;
        let mut r = rand.clone();
        let new_env = self.alloc(new.bind_count, &new.uri, &new.injections, env, &mut r)?;
        self.eval(&new.p, &new_env, rand, cost)
    }

    fn alloc(
        &self,
        count: i32,
        urns: &[String],
        injections: &BTreeMap<String, Par>,
        env: &Env<Par>,
        rand: &mut Blake2b512Random,
    ) -> Result<Env<Par>, RholangError> {
        let mut new_env = env.clone();
        for _ in 0..(count - urns.len() as i32) {
            let bytes = rand.next();
            let addr = Par {
                unforgeables: vec![GUnforgeable::GPrivate(GPrivate { id: bytes })],
                ..Par::default()
            };
            new_env = new_env.put(addr);
        }
        for urn in urns {
            new_env = self.add_urn(new_env, urn, injections)?;
        }
        Ok(new_env)
    }

    fn add_urn(
        &self,
        env: Env<Par>,
        urn: &str,
        injections: &BTreeMap<String, Par>,
    ) -> Result<Env<Par>, RholangError> {
        if let Some(p) = self.urn_map.get(urn) {
            Ok(env.put(p.clone()))
        } else if let Some(p) = injections.get(urn) {
            Ok(env.put(p.clone()))
        } else {
            Err(RholangError::BugFoundError(format!(
                "No value set for `{urn}`. This is a bug in the normalizer or on the path from it."
            )))
        }
    }

    fn eval_match(
        &self,
        m: &Match,
        env: &Env<Par>,
        rand: &Blake2b512Random,
        cost: &CostAccounting,
    ) -> Result<(), RholangError> {
        cost.charge(Costs::match_eval_cost())?;
        let evaled_target = eval_expr(&m.target, env, cost)?;
        let subst_target = substitute_par(&evaled_target, 0, env)?;
        self.first_match(&subst_target, &m.cases, env, rand, cost)
    }

    fn first_match(
        &self,
        target: &Par,
        cases: &[MatchCase],
        env: &Env<Par>,
        rand: &Blake2b512Random,
        cost: &CostAccounting,
    ) -> Result<(), RholangError> {
        for case in cases {
            let pattern = substitute_par(&case.pattern, 1, env)?;
            if let Some(free_map) = spatial_match_result(target, &pattern)? {
                let mut new_env = env.clone();
                for e in 0..case.free_count {
                    new_env = new_env.put(free_map.get(&e).cloned().unwrap_or_default());
                }
                return self.eval(&case.source, &new_env, rand, cost);
            }
        }
        Ok(())
    }

    fn eval_bundle(
        &self,
        bundle: &Bundle,
        env: &Env<Par>,
        rand: &Blake2b512Random,
        cost: &CostAccounting,
    ) -> Result<(), RholangError> {
        self.eval(&bundle.body, env, rand, cost)
    }

    fn produce(
        &self,
        chan: &Par,
        data: ListParWithRandom,
        persistent: bool,
        cost: &CostAccounting,
    ) -> Result<(), RholangError> {
        self.update_mergeable_channels(chan);
        let result = self.space.produce(chan, data.clone(), persistent)?;
        match result {
            Some((continuation, data_list, peek)) => {
                self.dispatch(&continuation, &data_list)?;
                if persistent {
                    self.produce(chan, data, persistent, cost)?;
                } else if peek {
                    self.produce_peeks(&data_list, cost)?;
                }
            }
            None => {}
        }
        Ok(())
    }

    fn consume(
        &self,
        binds: &[(BindPattern, Par)],
        body: ParWithRandom,
        persistent: bool,
        peek: bool,
        cost: &CostAccounting,
    ) -> Result<(), RholangError> {
        let patterns: Vec<BindPattern> = binds.iter().map(|(p, _)| p.clone()).collect();
        let sources: Vec<Par> = binds.iter().map(|(_, s)| s.clone()).collect();
        for s in &sources {
            self.update_mergeable_channels(s);
        }
        let peeks: BTreeSet<usize> = if peek {
            (0..sources.len()).collect()
        } else {
            BTreeSet::new()
        };
        let result = self.space.consume(
            &sources,
            &patterns,
            TaggedContinuation::ParBody(body.clone()),
            persistent,
            &peeks,
        )?;
        match result {
            Some((continuation, data_list, p)) => {
                self.dispatch(&continuation, &data_list)?;
                if persistent {
                    self.consume(binds, body, persistent, peek, cost)?;
                } else if p {
                    self.produce_peeks(&data_list, cost)?;
                }
            }
            None => {}
        }
        Ok(())
    }

    fn produce_peeks(
        &self,
        data_list: &[(Par, ListParWithRandom, ListParWithRandom, bool)],
        cost: &CostAccounting,
    ) -> Result<(), RholangError> {
        for (chan, _, removed_data, persist) in data_list {
            if !persist {
                self.produce(chan, removed_data.clone(), false, cost)?;
            }
        }
        Ok(())
    }

    fn dispatch(
        &self,
        continuation: &TaggedContinuation,
        data_list: &[(Par, ListParWithRandom, ListParWithRandom, bool)],
    ) -> Result<(), RholangError> {
        let data: Vec<ListParWithRandom> = data_list.iter().map(|(_, d, _, _)| d.clone()).collect();
        self.dispatcher.dispatch(continuation, &data)
    }

    fn update_mergeable_channels(&self, chan: &Par) {
        if self.is_mergeable_channel(chan) {
            let mut chs = self.merge_chs.borrow_mut();
            if !chs.contains(chan) {
                chs.push(chan.clone());
            }
        }
    }

    fn is_mergeable_channel(&self, chan: &Par) -> bool {
        chan.exprs
            .iter()
            .find_map(|e| match e {
                Expr::ETuple(ETuple { ps, .. }) => ps.first(),
                _ => None,
            })
            .map_or(false, |head| head == &self.mergeable_tag_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_arithmetic() {
        let cost = CostAccounting::from_initial(Costs::unsafe_max());
        let e = Env::new();
        let p = from_expr(Expr::EPlus(
            Box::new(from_expr(Expr::GInt(2))),
            Box::new(from_expr(Expr::GInt(3))),
        ));
        assert_eq!(
            eval_single_expr(&p, &e, &cost).unwrap(),
            Expr::GInt(5)
        );
    }

    #[test]
    fn evaluates_boolean_and() {
        let cost = CostAccounting::from_initial(Costs::unsafe_max());
        let e = Env::new();
        let p = from_expr(Expr::EAnd(
            Box::new(from_expr(Expr::GBool(true))),
            Box::new(from_expr(Expr::GBool(false))),
        ));
        assert_eq!(eval_single_expr(&p, &e, &cost).unwrap(), Expr::GBool(false));
    }

    #[test]
    fn string_append() {
        let cost = CostAccounting::from_initial(Costs::unsafe_max());
        let e = Env::new();
        let p = from_expr(Expr::EPlusPlus(
            Box::new(from_expr(Expr::GString("a".to_string()))),
            Box::new(from_expr(Expr::GString("b".to_string()))),
        ));
        assert_eq!(
            eval_single_expr(&p, &e, &cost).unwrap(),
            Expr::GString("ab".to_string())
        );
    }

    struct MockSpace {
        produced: RefCell<Vec<(Par, ListParWithRandom, bool)>>,
    }
    impl Tuplespace for MockSpace {
        fn produce(
            &self,
            channel: &Par,
            data: ListParWithRandom,
            persist: bool,
        ) -> Result<Application, RholangError> {
            self.produced
                .borrow_mut()
                .push((channel.clone(), data, persist));
            Ok(None)
        }
        fn consume(
            &self,
            _channels: &[Par],
            _patterns: &[BindPattern],
            _continuation: TaggedContinuation,
            _persist: bool,
            _peeks: &BTreeSet<usize>,
        ) -> Result<Application, RholangError> {
            Ok(None)
        }
    }
    struct MockDispatch;
    impl Dispatch for MockDispatch {
        fn dispatch(
            &self,
            _continuation: &TaggedContinuation,
            _data_list: &[ListParWithRandom],
        ) -> Result<(), RholangError> {
            Ok(())
        }
    }

    #[test]
    fn eval_send_produces_on_evaluated_channel() {
        let space = MockSpace {
            produced: RefCell::new(Vec::new()),
        };
        let interp = DebruijnInterpreter::new(
            space,
            MockDispatch,
            BTreeMap::new(),
            Par::default(),
        );
        let cost = CostAccounting::from_initial(Costs::unsafe_max());
        let env = Env::new();
        let rand = Blake2b512Random::new_random(128);

        let send = Send {
            chan: Box::new(from_expr(Expr::GInt(1))),
            data: vec![from_expr(Expr::GInt(2))],
            persistent: false,
            locally_free: AlwaysEqual(vec![]),
            connective_used: false,
        };
        let par = Par {
            sends: vec![send],
            ..Par::default()
        };
        interp.eval(&par, &env, &rand, &cost).unwrap();

        let produced = interp.space.produced.borrow();
        assert_eq!(produced.len(), 1);
        assert_eq!(produced[0].0.exprs, vec![Expr::GInt(1)]);
        assert_eq!(produced[0].1.pars, vec![from_expr(Expr::GInt(2))]);
        assert!(!produced[0].2);
    }
}
