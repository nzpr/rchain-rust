//! The pure expression-evaluation surface of the reducer (Law 4).
//!
//! Mirrors `Reduce.scala` (`evalExpr` / `evalExprToExpr` / `evalSingleExpr` / `evalToBool` and the
//! arithmetic/comparison/boolean/string/method helpers). The effectful term dispatch
//! (`eval(Send/Receive/New/Match/Bundle)`, `produce`/`consume`, `new` allocation) and the
//! collection methods (`union`/`diff`/`add`/`delete`/`contains`/`slice`/`keys`) are deferred.

use std::collections::BTreeSet;

use num_bigint::BigInt;
use rchain_models::ast::{AlwaysEqual, EList, ETuple, Expr, Par, ParMap, Var};
use rchain_models::par_ops::{from_expr, par_concat, single_expr, typ};
use rchain_models::sorter::{par_map, par_set};

use crate::accounting::{CostAccounting, Costs};
use crate::env::Env;
use crate::errors::RholangError;
use crate::matcher::spatial_match_result;
use crate::substitute::substitute_par;

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
}
