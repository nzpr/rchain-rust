//! Normalizer matchers (port of `interpreter/compiler/normalizer/` + the
//! `ProcNormalizeMatcher` dispatch in `normalize.scala`).
//!
//! These fold the concrete `Proc` AST into the de Bruijn `Par`. Source positions are placeholders
//! until the lexer is ported; the structural processes (`PSend`/`PNew`/`PInput`/…) are deferred.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};

use num_bigint::BigInt;

use rchain_models::ast::{
    AlwaysEqual, Bundle, Connective, ConnectiveBody, EList, ETuple, Expr, Match, MatchCase, New,
    Par, ParMap, ParSet, Receive, ReceiveBind, Send, Var, VarRef,
};
use rchain_models::par_ops::{
    from_expr, par_concat, prepend_bundle, prepend_connective, prepend_expr, prepend_match,
    prepend_new, prepend_receive, prepend_send, single_bundle, single_connective,
};

use crate::compiler::{
    CollectVisitInputs, CollectVisitOutputs, FreeMap, NameVisitInputs, NameVisitOutputs,
    ProcVisitInputs, ProcVisitOutputs, VarSort,
};
use crate::errors::{RholangError, SourcePosition};
use crate::proc_ast::{
    BoolLiteral, Bundle as BundleKind, Case, Collection, Ground, KeyValuePair, LinearBind, Name,
    NameDecl, NameRemainder, NameSource, Proc, ProcRemainder, ProcVar, Receipt, ReceiptLinearImpl,
    Send as SendKind, SimpleType, SynchSendCont, Tuple, VarRefKind,
};

fn pos() -> SourcePosition {
    SourcePosition { row: 0, column: 0 }
}

fn defer(name: &str) -> RholangError {
    RholangError::UnrecognizedNormalizerError(format!(
        "Compilation of construct not yet supported: {name}"
    ))
}

fn with_connective_used(mut par: Par) -> Par {
    par.connective_used = true;
    par
}

/// Normalize a bool literal (port of `BoolNormalizeMatcher.normalizeMatch`).
pub fn normalize_bool(b: &BoolLiteral) -> Expr {
    match b {
        BoolLiteral::BoolTrue => Expr::GBool(true),
        BoolLiteral::BoolFalse => Expr::GBool(false),
    }
}

/// Normalize a ground term (port of `GroundNormalizeMatcher.normalizeMatch`).
pub fn normalize_ground(g: &Ground) -> Result<Expr, RholangError> {
    match g {
        Ground::GroundBool(b) => Ok(normalize_bool(b)),
        Ground::GroundInt(s) => s
            .parse::<i64>()
            .map(Expr::GInt)
            .map_err(|e| RholangError::NormalizerError(e.to_string())),
        Ground::GroundBigInt(s) => s
            .parse::<BigInt>()
            .map(Expr::GBigInt)
            .map_err(|e| RholangError::NormalizerError(e.to_string())),
        Ground::GroundString(s) => Ok(Expr::GString(strip_string(s))),
        Ground::GroundUri(s) => Ok(Expr::GUri(strip_uri(s))),
    }
}

fn strip_string(raw: &str) -> String {
    raw[1..raw.len() - 1].to_string()
}

fn strip_uri(raw: &str) -> String {
    raw[1..raw.len() - 1].to_string()
}

/// Normalize a name (port of `NameNormalizeMatcher.normalizeMatch`).
pub fn normalize_name(n: &Name, input: NameVisitInputs) -> Result<NameVisitOutputs, RholangError> {
    match n {
        Name::NameWildcard => Ok(NameVisitOutputs {
            par: prepend_expr(
                &Par::default(),
                Expr::EVar(Box::new(Var::Wildcard)),
                0,
            ),
            free_map: input.free_map.add_wildcard(pos()),
        }),
        Name::NameVar(var) => match input.bound_map_chain.get(var) {
            Some(bc) => match bc.typ {
                VarSort::NameSort => Ok(NameVisitOutputs {
                    par: prepend_expr(
                        &Par::default(),
                        Expr::EVar(Box::new(Var::BoundVar(bc.index))),
                        0,
                    ),
                    free_map: input.free_map,
                }),
                VarSort::ProcSort => Err(RholangError::UnexpectedNameContext {
                    var_name: var.clone(),
                    proc_var_source_position: bc.source_position,
                    name_source_position: pos(),
                }),
            },
            None => match input.free_map.get(var) {
                None => {
                    let free_map =
                        input
                            .free_map
                            .put(&(var.clone(), VarSort::NameSort, pos()));
                    Ok(NameVisitOutputs {
                        par: prepend_expr(
                            &Par::default(),
                            Expr::EVar(Box::new(Var::FreeVar(input.free_map.next_level()))),
                            0,
                        ),
                        free_map,
                    })
                }
                Some(fc) => Err(RholangError::UnexpectedReuseOfNameContextFree {
                    var_name: var.clone(),
                    first_use: fc.source_position,
                    second_use: pos(),
                }),
            },
        },
        Name::NameQuote(sub) => {
            let result = normalize_proc(
                sub,
                ProcVisitInputs {
                    par: Par::default(),
                    bound_map_chain: input.bound_map_chain,
                    free_map: input.free_map,
                },
            )?;
            Ok(NameVisitOutputs {
                par: result.par,
                free_map: result.free_map,
            })
        }
    }
}

/// Normalize a process (port of `ProcNormalizeMatcher.normalizeMatch`).
pub fn normalize_proc(p: &Proc, input: ProcVisitInputs) -> Result<ProcVisitOutputs, RholangError> {
    match p {
        Proc::PGround(g) => {
            let expr = normalize_ground(g)?;
            Ok(ProcVisitOutputs {
                par: prepend_expr(&input.par, expr, input.bound_map_chain.depth()),
                free_map: input.free_map,
            })
        }
        Proc::PNil => Ok(ProcVisitOutputs {
            par: input.par,
            free_map: input.free_map,
        }),
        Proc::PExprs(sub) => normalize_proc(sub, input),
        Proc::PVar(pv) => normalize_pvar(pv, input),
        Proc::PVarRef(kind, var) => normalize_pvar_ref(kind, var, input),
        Proc::PEval(name) => {
            let ProcVisitInputs {
                par,
                bound_map_chain,
                free_map,
            } = input;
            let name_result =
                normalize_name(name, NameVisitInputs { bound_map_chain, free_map })?;
            Ok(ProcVisitOutputs {
                par: par_concat(&par, &name_result.par),
                free_map: name_result.free_map,
            })
        }
        Proc::PPar(l, r) => {
            let bound_map_chain = input.bound_map_chain.clone();
            let result = normalize_proc(l, input)?;
            let chained = ProcVisitInputs {
                par: result.par,
                bound_map_chain,
                free_map: result.free_map,
            };
            normalize_proc(r, chained)
        }
        Proc::PNot(sub) => unary_exp(sub, input, |par| Expr::ENot(Box::new(par))),
        Proc::PNeg(sub) => unary_exp(sub, input, |par| Expr::ENeg(Box::new(par))),
        Proc::PMult(l, r) => binary_exp(l, r, input, Expr::EMult),
        Proc::PDiv(l, r) => binary_exp(l, r, input, Expr::EDiv),
        Proc::PMod(l, r) => binary_exp(l, r, input, Expr::EMod),
        Proc::PPercentPercent(l, r) => binary_exp(l, r, input, Expr::EPercentPercent),
        Proc::PAdd(l, r) => binary_exp(l, r, input, Expr::EPlus),
        Proc::PMinus(l, r) => binary_exp(l, r, input, Expr::EMinus),
        Proc::PPlusPlus(l, r) => binary_exp(l, r, input, Expr::EPlusPlus),
        Proc::PMinusMinus(l, r) => binary_exp(l, r, input, Expr::EMinusMinus),
        Proc::PLt(l, r) => binary_exp(l, r, input, Expr::ELt),
        Proc::PLte(l, r) => binary_exp(l, r, input, Expr::ELte),
        Proc::PGt(l, r) => binary_exp(l, r, input, Expr::EGt),
        Proc::PGte(l, r) => binary_exp(l, r, input, Expr::EGte),
        Proc::PMatches(l, r) => normalize_pmatches(l, r, input),
        Proc::PEq(l, r) => binary_exp(l, r, input, Expr::EEq),
        Proc::PNeq(l, r) => binary_exp(l, r, input, Expr::ENeq),
        Proc::PAnd(l, r) => binary_exp(l, r, input, Expr::EAnd),
        Proc::PShortAnd(l, r) => binary_exp(l, r, input, Expr::EShortAnd),
        Proc::POr(l, r) => binary_exp(l, r, input, Expr::EOr),
        Proc::PShortOr(l, r) => binary_exp(l, r, input, Expr::EShortOr),
        Proc::PNegation(sub) => normalize_negation(sub, input),
        Proc::PConjunction(l, r) => normalize_conjunction(l, r, input),
        Proc::PDisjunction(l, r) => normalize_disjunction(l, r, input),
        Proc::PSimpleType(t) => normalize_simple_type(t, input),
        Proc::PSend(name, send, data) => normalize_send(name, send, data, input),
        Proc::PMethod(target, method, args) => normalize_method(target, method, args, input),
        Proc::PIf(cond, body) => normalize_if(cond, body, &Proc::PNil, input),
        Proc::PIfElse(cond, t, f) => normalize_if(cond, t, f, input),
        Proc::PNew(decls, body) => normalize_new(decls, body, input),
        Proc::PMatch(target, cases) => normalize_match(target, cases, input),
        Proc::PContr(name, formals, remainder, body) => {
            normalize_contr(name, formals, remainder, body, input)
        }
        Proc::PBundle(kind, body) => normalize_bundle(kind, body, input),
        Proc::PSendSynch(name, data, cont) => normalize_send_synch(name, data, cont, input),
        Proc::PCollect(c) => {
            let collect_result = normalize_collection(
                c,
                CollectVisitInputs {
                    bound_map_chain: input.bound_map_chain.clone(),
                    free_map: input.free_map.clone(),
                },
            )?;
            Ok(ProcVisitOutputs {
                par: prepend_expr(&input.par, collect_result.expr, input.bound_map_chain.depth()),
                free_map: collect_result.free_map,
            })
        }
        _ => Err(defer("process")),
    }
}

fn normalize_pvar(pv: &ProcVar, input: ProcVisitInputs) -> Result<ProcVisitOutputs, RholangError> {
    match pv {
        ProcVar::ProcVarVar(var) => match input.bound_map_chain.get(var) {
            Some(bc) => match bc.typ {
                VarSort::ProcSort => Ok(ProcVisitOutputs {
                    par: prepend_expr(
                        &input.par,
                        Expr::EVar(Box::new(Var::BoundVar(bc.index))),
                        input.bound_map_chain.depth(),
                    ),
                    free_map: input.free_map,
                }),
                VarSort::NameSort => Err(RholangError::UnexpectedProcContext {
                    var_name: var.clone(),
                    name_var_source_position: bc.source_position,
                    process_source_position: pos(),
                }),
            },
            None => match input.free_map.get(var) {
                None => {
                    let free_map =
                        input
                            .free_map
                            .put(&(var.clone(), VarSort::ProcSort, pos()));
                    Ok(ProcVisitOutputs {
                        par: with_connective_used(prepend_expr(
                            &input.par,
                            Expr::EVar(Box::new(Var::FreeVar(input.free_map.next_level()))),
                            input.bound_map_chain.depth(),
                        )),
                        free_map,
                    })
                }
                Some(fc) => Err(RholangError::UnexpectedReuseOfProcContextFree {
                    var_name: var.clone(),
                    first_use: fc.source_position,
                    second_use: pos(),
                }),
            },
        },
        ProcVar::ProcVarWildcard => Ok(ProcVisitOutputs {
            par: with_connective_used(prepend_expr(
                &input.par,
                Expr::EVar(Box::new(Var::Wildcard)),
                input.bound_map_chain.depth(),
            )),
            free_map: input.free_map.add_wildcard(pos()),
        }),
    }
}

fn normalize_pvar_ref(
    kind: &VarRefKind,
    var: &str,
    input: ProcVisitInputs,
) -> Result<ProcVisitOutputs, RholangError> {
    let (bc, depth) = match input.bound_map_chain.find(var) {
        Some(found) => found,
        None => {
            return Err(RholangError::UnboundVariableRef {
                var_name: var.to_string(),
                line: 0,
                col: 0,
            })
        }
    };
    let connective = Connective::VarRef(VarRef {
        index: bc.index,
        depth,
    });
    match bc.typ {
        VarSort::ProcSort => match kind {
            VarRefKind::VarRefKindProc => Ok(ProcVisitOutputs {
                par: prepend_connective(&input.par, connective, input.bound_map_chain.depth()),
                free_map: input.free_map,
            }),
            _ => Err(RholangError::UnexpectedProcContext {
                var_name: var.to_string(),
                name_var_source_position: bc.source_position,
                process_source_position: pos(),
            }),
        },
        VarSort::NameSort => match kind {
            VarRefKind::VarRefKindName => Ok(ProcVisitOutputs {
                par: prepend_connective(&input.par, connective, input.bound_map_chain.depth()),
                free_map: input.free_map,
            }),
            _ => Err(RholangError::UnexpectedNameContext {
                var_name: var.to_string(),
                proc_var_source_position: bc.source_position,
                name_source_position: pos(),
            }),
        },
    }
}

fn normalize_negation(
    sub: &Proc,
    input: ProcVisitInputs,
) -> Result<ProcVisitOutputs, RholangError> {
    let body = normalize_proc(
        sub,
        ProcVisitInputs {
            par: Par::default(),
            bound_map_chain: input.bound_map_chain.clone(),
            free_map: FreeMap::empty(),
        },
    )?;
    let connective = Connective::ConnNot(Box::new(body.par.clone()));
    Ok(ProcVisitOutputs {
        par: prepend_connective(&input.par, connective.clone(), input.bound_map_chain.depth()),
        free_map: input.free_map.add_connective(connective, pos()),
    })
}

fn normalize_conjunction(
    l: &Proc,
    r: &Proc,
    input: ProcVisitInputs,
) -> Result<ProcVisitOutputs, RholangError> {
    let left = normalize_proc(
        l,
        ProcVisitInputs {
            par: Par::default(),
            bound_map_chain: input.bound_map_chain.clone(),
            free_map: input.free_map.clone(),
        },
    )?;
    let right = normalize_proc(
        r,
        ProcVisitInputs {
            par: Par::default(),
            bound_map_chain: input.bound_map_chain.clone(),
            free_map: left.free_map.clone(),
        },
    )?;
    let connective = match single_connective(&left.par) {
        Some(Connective::ConnAnd(body)) => {
            let mut ps = body.ps.clone();
            ps.push(right.par.clone());
            Connective::ConnAnd(ConnectiveBody { ps })
        }
        _ => Connective::ConnAnd(ConnectiveBody {
            ps: vec![left.par.clone(), right.par.clone()],
        }),
    };
    Ok(ProcVisitOutputs {
        par: prepend_connective(&input.par, connective.clone(), input.bound_map_chain.depth()),
        free_map: right.free_map.add_connective(connective, pos()),
    })
}

fn normalize_disjunction(
    l: &Proc,
    r: &Proc,
    input: ProcVisitInputs,
) -> Result<ProcVisitOutputs, RholangError> {
    let left = normalize_proc(
        l,
        ProcVisitInputs {
            par: Par::default(),
            bound_map_chain: input.bound_map_chain.clone(),
            free_map: FreeMap::empty(),
        },
    )?;
    let right = normalize_proc(
        r,
        ProcVisitInputs {
            par: Par::default(),
            bound_map_chain: input.bound_map_chain.clone(),
            free_map: FreeMap::empty(),
        },
    )?;
    let connective = match single_connective(&left.par) {
        Some(Connective::ConnOr(body)) => {
            let mut ps = body.ps.clone();
            ps.push(right.par.clone());
            Connective::ConnOr(ConnectiveBody { ps })
        }
        _ => Connective::ConnOr(ConnectiveBody {
            ps: vec![left.par.clone(), right.par.clone()],
        }),
    };
    Ok(ProcVisitOutputs {
        par: prepend_connective(&input.par, connective.clone(), input.bound_map_chain.depth()),
        free_map: input.free_map.add_connective(connective, pos()),
    })
}

fn normalize_simple_type(
    t: &SimpleType,
    input: ProcVisitInputs,
) -> Result<ProcVisitOutputs, RholangError> {
    let connective = match t {
        SimpleType::SimpleTypeBool => Connective::ConnBool(true),
        SimpleType::SimpleTypeInt => Connective::ConnInt(true),
        SimpleType::SimpleTypeBigInt => Connective::ConnBigInt(true),
        SimpleType::SimpleTypeString => Connective::ConnString(true),
        SimpleType::SimpleTypeUri => Connective::ConnUri(true),
        SimpleType::SimpleTypeByteArray => Connective::ConnByteArray(true),
    };
    Ok(ProcVisitOutputs {
        par: with_connective_used(prepend_connective(
            &input.par,
            connective,
            input.bound_map_chain.depth(),
        )),
        free_map: input.free_map,
    })
}

fn unary_exp(
    sub: &Proc,
    input: ProcVisitInputs,
    constructor: impl FnOnce(Par) -> Expr,
) -> Result<ProcVisitOutputs, RholangError> {
    let sub_result = normalize_proc(
        sub,
        ProcVisitInputs {
            par: Par::default(),
            bound_map_chain: input.bound_map_chain.clone(),
            free_map: input.free_map.clone(),
        },
    )?;
    Ok(ProcVisitOutputs {
        par: prepend_expr(
            &input.par,
            constructor(sub_result.par),
            input.bound_map_chain.depth(),
        ),
        free_map: sub_result.free_map,
    })
}

fn binary_exp(
    l: &Proc,
    r: &Proc,
    input: ProcVisitInputs,
    constructor: fn(Box<Par>, Box<Par>) -> Expr,
) -> Result<ProcVisitOutputs, RholangError> {
    let left = normalize_proc(
        l,
        ProcVisitInputs {
            par: Par::default(),
            bound_map_chain: input.bound_map_chain.clone(),
            free_map: input.free_map.clone(),
        },
    )?;
    let right = normalize_proc(
        r,
        ProcVisitInputs {
            par: Par::default(),
            bound_map_chain: input.bound_map_chain.clone(),
            free_map: left.free_map.clone(),
        },
    )?;
    Ok(ProcVisitOutputs {
        par: prepend_expr(
            &input.par,
            constructor(Box::new(left.par), Box::new(right.par)),
            input.bound_map_chain.depth(),
        ),
        free_map: right.free_map,
    })
}

/// Normalize a `matches` expression: the pattern's free variables are discarded (port of
/// `PMatchesNormalizer`).
fn normalize_pmatches(
    l: &Proc,
    r: &Proc,
    input: ProcVisitInputs,
) -> Result<ProcVisitOutputs, RholangError> {
    let bound_map_chain = input.bound_map_chain.clone();
    let left = normalize_proc(
        l,
        ProcVisitInputs {
            par: Par::default(),
            bound_map_chain: bound_map_chain.clone(),
            free_map: input.free_map.clone(),
        },
    )?;
    let right = normalize_proc(
        r,
        ProcVisitInputs {
            par: Par::default(),
            bound_map_chain: bound_map_chain.push(),
            free_map: FreeMap::empty(),
        },
    )?;
    Ok(ProcVisitOutputs {
        par: prepend_expr(
            &input.par,
            Expr::EMatches(Box::new(left.par), Box::new(right.par)),
            input.bound_map_chain.depth(),
        ),
        free_map: left.free_map,
    })
}

/// Handle a remainder proc-var (port of `RemainderNormalizeMatcher.handleProcVar`).
fn handle_proc_var(
    pv: &ProcVar,
    known_free: FreeMap<VarSort>,
) -> Result<(Option<Var>, FreeMap<VarSort>), RholangError> {
    match pv {
        ProcVar::ProcVarWildcard => Ok((Some(Var::Wildcard), known_free.add_wildcard(pos()))),
        ProcVar::ProcVarVar(var) => match known_free.get(var) {
            None => {
                let free_map = known_free.put(&(var.clone(), VarSort::ProcSort, pos()));
                Ok((Some(Var::FreeVar(known_free.next_level())), free_map))
            }
            Some(fc) => Err(RholangError::UnexpectedReuseOfProcContextFree {
                var_name: var.clone(),
                first_use: fc.source_position,
                second_use: pos(),
            }),
        },
    }
}

/// Normalize a proc remainder (port of `RemainderNormalizeMatcher.normalizeMatchProc`).
pub fn normalize_remainder_proc(
    r: &ProcRemainder,
    known_free: FreeMap<VarSort>,
) -> Result<(Option<Var>, FreeMap<VarSort>), RholangError> {
    match r {
        ProcRemainder::ProcRemainderEmpty => Ok((None, known_free)),
        ProcRemainder::ProcRemainderVar(pv) => handle_proc_var(pv, known_free),
    }
}

/// Normalize a name remainder (port of `RemainderNormalizeMatcher.normalizeMatchName`).
pub fn normalize_remainder_name(
    r: &NameRemainder,
    known_free: FreeMap<VarSort>,
) -> Result<(Option<Var>, FreeMap<VarSort>), RholangError> {
    match r {
        NameRemainder::NameRemainderEmpty => Ok((None, known_free)),
        NameRemainder::NameRemainderVar(pv) => handle_proc_var(pv, known_free),
    }
}

fn union_free(a: &[i32], b: &[i32]) -> Vec<i32> {
    let mut set: BTreeSet<i32> = a.iter().copied().collect();
    set.extend(b.iter().copied());
    set.into_iter().collect()
}

/// Keep levels `>= n` and shift them down by `n` (the Scala `BitSet.from(n).map(x => x - n)`).
fn from_free(b: &[i32], n: i32) -> Vec<i32> {
    b.iter().copied().filter(|&x| x >= n).map(|x| x - n).collect()
}

/// Normalize a `new` (port of `PNewNormalizer.normalize`).
fn normalize_new(
    decls: &[NameDecl],
    body: &Proc,
    input: ProcVisitInputs,
) -> Result<ProcVisitOutputs, RholangError> {
    let mut tagged: Vec<(Option<String>, String, SourcePosition)> = Vec::new();
    for d in decls {
        match d {
            NameDecl::NameDeclSimpl(var) => tagged.push((None, var.clone(), pos())),
            NameDecl::NameDeclUrn(var, uri) => {
                tagged.push((Some(strip_uri(uri)), var.clone(), pos()))
            }
        }
    }
    // None first, then uris lexicographically (matches the Scala sort).
    tagged.sort_by_key(|row| row.0.clone());
    let new_bindings: Vec<(String, VarSort, SourcePosition)> = tagged
        .iter()
        .map(|(_, var, p)| (var.clone(), VarSort::NameSort, p.clone()))
        .collect();
    let uris: Vec<String> = tagged.iter().filter_map(|(uri, _, _)| uri.clone()).collect();

    let new_env = input.bound_map_chain.put_all(&new_bindings);
    let new_count = new_env.count() - input.bound_map_chain.count();
    let body_result = normalize_proc(
        body,
        ProcVisitInputs {
            par: Par::default(),
            bound_map_chain: new_env,
            free_map: input.free_map.clone(),
        },
    )?;

    let n = New {
        bind_count: new_count,
        p: Box::new(body_result.par.clone()),
        uri: uris,
        injections: BTreeMap::new(),
        locally_free: AlwaysEqual(from_free(&body_result.par.locally_free.0, new_count)),
    };
    Ok(ProcVisitOutputs {
        par: prepend_new(&input.par, n),
        free_map: body_result.free_map,
    })
}

/// Normalize a `match` (port of `PMatchNormalizer.normalize`).
fn normalize_match(
    target: &Proc,
    cases: &[Case],
    input: ProcVisitInputs,
) -> Result<ProcVisitOutputs, RholangError> {
    let bound_map_chain = input.bound_map_chain.clone();
    let target_result = normalize_proc(
        target,
        ProcVisitInputs {
            par: Par::default(),
            bound_map_chain: bound_map_chain.clone(),
            free_map: input.free_map.clone(),
        },
    )?;

    let mut match_cases: Vec<MatchCase> = Vec::new();
    let mut locally_free: Vec<i32> = Vec::new();
    let mut connective_used = false;
    let mut free_map = target_result.free_map.clone();
    for case in cases {
        let (pattern, case_body) = (case.0.as_ref(), case.1.as_ref());
        let pattern_result = normalize_proc(
            pattern,
            ProcVisitInputs {
                par: Par::default(),
                bound_map_chain: input.bound_map_chain.push(),
                free_map: FreeMap::empty(),
            },
        )?;
        let case_env = input.bound_map_chain.absorb_free(&pattern_result.free_map);
        let bound_count = pattern_result.free_map.count_no_wildcards();
        let case_body_result = normalize_proc(
            case_body,
            ProcVisitInputs {
                par: Par::default(),
                bound_map_chain: case_env,
                free_map,
            },
        )?;
        match_cases.insert(
            0,
            MatchCase {
                pattern: Box::new(pattern_result.par.clone()),
                source: Box::new(case_body_result.par.clone()),
                free_count: bound_count,
            },
        );
        locally_free = union_free(&locally_free, &pattern_result.par.locally_free.0);
        locally_free = union_free(
            &locally_free,
            &from_free(&case_body_result.par.locally_free.0, bound_count),
        );
        connective_used = connective_used || case_body_result.par.connective_used;
        free_map = case_body_result.free_map;
    }

    let m = Match {
        target: Box::new(target_result.par.clone()),
        cases: match_cases,
        locally_free: AlwaysEqual(union_free(&locally_free, &target_result.par.locally_free.0)),
        connective_used: connective_used || target_result.par.connective_used,
    };
    Ok(ProcVisitOutputs {
        par: prepend_match(&input.par, m),
        free_map,
    })
}

fn fail_on_invalid_connective(
    input: &ProcVisitInputs,
    name_res: &NameVisitOutputs,
) -> Result<(), RholangError> {
    if input.bound_map_chain.depth() == 0 {
        for (conn, sp) in &name_res.free_map.connectives {
            match conn {
                Connective::ConnOr(_) => {
                    return Err(RholangError::PatternReceiveError(format!(
                        "\\/ (disjunction) at {sp}"
                    )))
                }
                Connective::ConnNot(_) => {
                    return Err(RholangError::PatternReceiveError(format!(
                        "~ (negation) at {sp}"
                    )))
                }
                _ => {}
            }
        }
    }
    Ok(())
}

/// Normalize a contract (port of `PContrNormalizer.normalize`).
fn normalize_contr(
    name: &Name,
    formals: &[Name],
    remainder: &NameRemainder,
    body: &Proc,
    input: ProcVisitInputs,
) -> Result<ProcVisitOutputs, RholangError> {
    let name_result = normalize_name(
        name,
        NameVisitInputs {
            bound_map_chain: input.bound_map_chain.clone(),
            free_map: input.free_map.clone(),
        },
    )?;

    let mut formal_pars: Vec<Par> = Vec::new();
    let mut formal_locally_free: Vec<i32> = Vec::new();
    let mut free_map = FreeMap::<VarSort>::empty();
    for n in formals {
        let res = normalize_name(
            n,
            NameVisitInputs {
                bound_map_chain: input.bound_map_chain.push(),
                free_map,
            },
        )?;
        fail_on_invalid_connective(&input, &res)?;
        formal_pars.insert(0, res.par.clone());
        formal_locally_free = union_free(&formal_locally_free, &res.par.locally_free.0);
        free_map = res.free_map;
    }

    let (remainder_var, remainder_free_map) = normalize_remainder_name(remainder, free_map)?;
    let new_env = input.bound_map_chain.absorb_free(&remainder_free_map);
    let bound_count = remainder_free_map.count_no_wildcards();
    let body_result = normalize_proc(
        body,
        ProcVisitInputs {
            par: Par::default(),
            bound_map_chain: new_env,
            free_map: name_result.free_map.clone(),
        },
    )?;

    let receive = Receive {
        binds: vec![ReceiveBind {
            patterns: formal_pars,
            source: Box::new(name_result.par.clone()),
            remainder: remainder_var.map(Box::new),
            free_count: bound_count,
        }],
        body: Box::new(body_result.par.clone()),
        persistent: true,
        peek: false,
        bind_count: bound_count,
        locally_free: AlwaysEqual(union_free(
            &union_free(&name_result.par.locally_free.0, &formal_locally_free),
            &from_free(&body_result.par.locally_free.0, bound_count),
        )),
        connective_used: name_result.par.connective_used || body_result.par.connective_used,
    };
    Ok(ProcVisitOutputs {
        par: prepend_receive(&input.par, receive),
        free_map: body_result.free_map,
    })
}

/// Normalize a send (port of `PSendNormalizer.normalize`).
fn normalize_send(
    name: &Name,
    send: &SendKind,
    data: &[Proc],
    input: ProcVisitInputs,
) -> Result<ProcVisitOutputs, RholangError> {
    let name_result = normalize_name(
        name,
        NameVisitInputs {
            bound_map_chain: input.bound_map_chain.clone(),
            free_map: input.free_map.clone(),
        },
    )?;

    let mut data_pars: Vec<Par> = Vec::new();
    let mut data_locally_free: Vec<i32> = Vec::new();
    let mut data_connective_used = false;
    let mut free_map = name_result.free_map.clone();
    for e in data.iter().rev() {
        let result = normalize_proc(
            e,
            ProcVisitInputs {
                par: Par::default(),
                bound_map_chain: input.bound_map_chain.clone(),
                free_map,
            },
        )?;
        data_pars.insert(0, result.par.clone());
        data_locally_free = union_free(&data_locally_free, &result.par.locally_free.0);
        data_connective_used = data_connective_used || result.par.connective_used;
        free_map = result.free_map;
    }

    let persistent = matches!(send, SendKind::SendMultiple);
    let s = Send {
        chan: Box::new(name_result.par.clone()),
        data: data_pars,
        persistent,
        locally_free: AlwaysEqual(union_free(
            &name_result.par.locally_free.0,
            &data_locally_free,
        )),
        connective_used: name_result.par.connective_used || data_connective_used,
    };
    Ok(ProcVisitOutputs {
        par: prepend_send(&input.par, s),
        free_map,
    })
}

/// Normalize a method call (port of `PMethodNormalizer.normalize`).
fn normalize_method(
    target_proc: &Proc,
    method: &str,
    args: &[Proc],
    input: ProcVisitInputs,
) -> Result<ProcVisitOutputs, RholangError> {
    let target_result = normalize_proc(
        target_proc,
        ProcVisitInputs {
            par: Par::default(),
            bound_map_chain: input.bound_map_chain.clone(),
            free_map: input.free_map.clone(),
        },
    )?;
    let target = target_result.par.clone();

    let mut arg_pars: Vec<Par> = Vec::new();
    let mut arg_locally_free: Vec<i32> = Vec::new();
    let mut arg_connective_used = false;
    let mut free_map = target_result.free_map.clone();
    for e in args.iter().rev() {
        let result = normalize_proc(
            e,
            ProcVisitInputs {
                par: Par::default(),
                bound_map_chain: input.bound_map_chain.clone(),
                free_map,
            },
        )?;
        arg_pars.insert(0, result.par.clone());
        arg_locally_free = union_free(&arg_locally_free, &result.par.locally_free.0);
        arg_connective_used = arg_connective_used || result.par.connective_used;
        free_map = result.free_map;
    }

    let expr = Expr::EMethod(rchain_models::ast::EMethod {
        method_name: method.to_string(),
        target: Box::new(target.clone()),
        arguments: arg_pars,
        locally_free: AlwaysEqual(union_free(&target.locally_free.0, &arg_locally_free)),
        connective_used: target.connective_used || arg_connective_used,
    });
    Ok(ProcVisitOutputs {
        par: prepend_expr(&input.par, expr, input.bound_map_chain.depth()),
        free_map,
    })
}

/// Normalize an `if`/`if-else` by desugaring to a `match` (port of `PIfNormalizer.normalize`).
fn normalize_if(
    value: &Proc,
    true_body: &Proc,
    false_body: &Proc,
    input: ProcVisitInputs,
) -> Result<ProcVisitOutputs, RholangError> {
    let input_par = input.par.clone();
    let bound_map_chain = input.bound_map_chain.clone();
    let target = normalize_proc(value, input)?;
    let true_result = normalize_proc(
        true_body,
        ProcVisitInputs {
            par: Par::default(),
            bound_map_chain: bound_map_chain.clone(),
            free_map: target.free_map.clone(),
        },
    )?;
    let false_result = normalize_proc(
        false_body,
        ProcVisitInputs {
            par: Par::default(),
            bound_map_chain,
            free_map: true_result.free_map.clone(),
        },
    )?;

    let m = Match {
        target: Box::new(target.par.clone()),
        cases: vec![
            MatchCase {
                pattern: Box::new(from_expr(Expr::GBool(true))),
                source: Box::new(true_result.par.clone()),
                free_count: 0,
            },
            MatchCase {
                pattern: Box::new(from_expr(Expr::GBool(false))),
                source: Box::new(false_result.par.clone()),
                free_count: 0,
            },
        ],
        locally_free: AlwaysEqual(union_free(
            &union_free(&target.par.locally_free.0, &true_result.par.locally_free.0),
            &false_result.par.locally_free.0,
        )),
        connective_used: target.par.connective_used
            || true_result.par.connective_used
            || false_result.par.connective_used,
    };
    Ok(ProcVisitOutputs {
        par: prepend_match(&input_par, m),
        free_map: false_result.free_map,
    })
}

/// Normalize a bundle (port of `PBundleNormalizer.normalize`).
fn normalize_bundle(
    kind: &BundleKind,
    body: &Proc,
    input: ProcVisitInputs,
) -> Result<ProcVisitOutputs, RholangError> {
    let bound_map_chain = input.bound_map_chain.clone();
    let target = normalize_proc(
        body,
        ProcVisitInputs {
            par: Par::default(),
            bound_map_chain,
            free_map: input.free_map.clone(),
        },
    )?;

    let (write_flag, read_flag) = match kind {
        BundleKind::BundleReadWrite => (true, true),
        BundleKind::BundleRead => (false, true),
        BundleKind::BundleWrite => (true, false),
        BundleKind::BundleEquiv => (false, false),
    };
    let outermost = Bundle {
        body: Box::new(target.par.clone()),
        write_flag,
        read_flag,
    };

    if !target.par.connectives.is_empty() {
        return Err(RholangError::UnexpectedBundleContent(
            "Illegal top level connective in bundle.".to_string(),
        ));
    }
    if !target.free_map.wildcards.is_empty() || !target.free_map.level_bindings.is_empty() {
        return Err(RholangError::UnexpectedBundleContent(
            "Bundle's content must not have free variables or wildcards.".to_string(),
        ));
    }
    let new_bundle = match single_bundle(&target.par) {
        Some(single) => outermost.merge(single),
        None => outermost,
    };
    Ok(ProcVisitOutputs {
        par: prepend_bundle(&input.par, new_bundle),
        free_map: input.free_map,
    })
}

static FRESH_COUNTER: AtomicU64 = AtomicU64::new(0);

fn fresh_identifier() -> String {
    let n = FRESH_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("$synch{n}")
}

/// Normalize a synchronous send by desugaring to `new` + `PPar` of a `PSend` and a `PInput` (port
/// of `PSendSynchNormalizer.normalize`).
fn normalize_send_synch(
    name: &Name,
    data: &[Proc],
    cont: &SynchSendCont,
    input: ProcVisitInputs,
) -> Result<ProcVisitOutputs, RholangError> {
    let identifier = fresh_identifier();

    let mut send_data = vec![Proc::PEval(Name::NameVar(identifier.clone()))];
    send_data.extend(data.iter().cloned());
    let send = Proc::PSend(name.clone(), SendKind::SendSingle, send_data);

    let linear_bind = LinearBind(
        vec![Name::NameWildcard],
        NameRemainder::NameRemainderEmpty,
        NameSource::SimpleSource(Name::NameVar(identifier.clone())),
    );
    let receipt = Receipt::ReceiptLinear(ReceiptLinearImpl::LinearSimple(vec![linear_bind]));
    let continuation = match cont {
        SynchSendCont::EmptyCont => Proc::PNil,
        SynchSendCont::NonEmptyCont(p) => (**p).clone(),
    };
    let receive = Proc::PInput(vec![receipt], Box::new(continuation));

    let ppar = Proc::PPar(Box::new(send), Box::new(receive));
    let pnew = Proc::PNew(
        vec![NameDecl::NameDeclSimpl(identifier)],
        Box::new(ppar),
    );
    normalize_proc(&pnew, input)
}

fn fold_collection(
    procs: &[Proc],
    known_free: FreeMap<VarSort>,
    input: &CollectVisitInputs,
    constructor: impl Fn(Vec<Par>, Vec<i32>, bool) -> Expr,
) -> Result<CollectVisitOutputs, RholangError> {
    let mut pars: Vec<Par> = Vec::new();
    let mut locally_free: Vec<i32> = Vec::new();
    let mut connective_used = false;
    let mut free_map = known_free;
    for proc in procs {
        let result = normalize_proc(
            proc,
            ProcVisitInputs {
                par: Par::default(),
                bound_map_chain: input.bound_map_chain.clone(),
                free_map,
            },
        )?;
        pars.push(result.par.clone());
        locally_free = union_free(&locally_free, &result.par.locally_free.0);
        connective_used = connective_used || result.par.connective_used;
        free_map = result.free_map;
    }
    Ok(CollectVisitOutputs {
        expr: constructor(pars, locally_free, connective_used),
        free_map,
    })
}

fn fold_collection_map(
    kvps: &[KeyValuePair],
    known_free: FreeMap<VarSort>,
    remainder: Option<Var>,
    input: &CollectVisitInputs,
) -> Result<CollectVisitOutputs, RholangError> {
    let mut pairs: Vec<(Par, Par)> = Vec::new();
    let mut locally_free: Vec<i32> = Vec::new();
    let mut connective_used = false;
    let mut free_map = known_free;
    for kv in kvps {
        let key_result = normalize_proc(
            &kv.0,
            ProcVisitInputs {
                par: Par::default(),
                bound_map_chain: input.bound_map_chain.clone(),
                free_map,
            },
        )?;
        let val_result = normalize_proc(
            &kv.1,
            ProcVisitInputs {
                par: Par::default(),
                bound_map_chain: input.bound_map_chain.clone(),
                free_map: key_result.free_map.clone(),
            },
        )?;
        pairs.push((key_result.par.clone(), val_result.par.clone()));
        locally_free = union_free(&locally_free, &key_result.par.locally_free.0);
        locally_free = union_free(&locally_free, &val_result.par.locally_free.0);
        connective_used = connective_used
            || key_result.par.connective_used
            || val_result.par.connective_used;
        free_map = val_result.free_map;
    }
    Ok(CollectVisitOutputs {
        expr: Expr::EMap(ParMap {
            kvs: pairs,
            connective_used,
            locally_free: AlwaysEqual(locally_free),
            remainder: remainder.map(Box::new),
        }),
        free_map,
    })
}

/// Normalize a collection (port of `CollectionNormalizeMatcher.normalizeMatch`).
fn normalize_collection(
    c: &Collection,
    input: CollectVisitInputs,
) -> Result<CollectVisitOutputs, RholangError> {
    match c {
        Collection::CollectList(procs, remainder) => {
            let (opt_rem, known_free) =
                normalize_remainder_proc(remainder, input.free_map.clone())?;
            let has_rem = opt_rem.is_some();
            let rem = opt_rem;
            fold_collection(procs, known_free, &input, move |ps, lf, cu| {
                Expr::EList(EList {
                    ps,
                    locally_free: AlwaysEqual(lf),
                    connective_used: cu || has_rem,
                    remainder: rem.clone().map(Box::new),
                })
            })
        }
        Collection::CollectTuple(tuple) => {
            let procs: Vec<Proc> = match tuple {
                Tuple::TupleSingle(p) => vec![(**p).clone()],
                Tuple::TupleMultiple(p, rest) => {
                    let mut v = vec![(**p).clone()];
                    v.extend(rest.iter().cloned());
                    v
                }
            };
            fold_collection(&procs, input.free_map.clone(), &input, |ps, lf, cu| {
                Expr::ETuple(ETuple {
                    ps,
                    locally_free: AlwaysEqual(lf),
                    connective_used: cu,
                })
            })
        }
        Collection::CollectSet(procs, remainder) => {
            let (opt_rem, known_free) =
                normalize_remainder_proc(remainder, input.free_map.clone())?;
            let has_rem = opt_rem.is_some();
            let rem = opt_rem;
            fold_collection(procs, known_free, &input, move |ps, lf, cu| {
                Expr::ESet(ParSet {
                    ps,
                    connective_used: cu || has_rem,
                    locally_free: AlwaysEqual(lf),
                    remainder: rem.clone().map(Box::new),
                })
            })
        }
        Collection::CollectMap(kvps, remainder) => {
            let (opt_rem, known_free) =
                normalize_remainder_proc(remainder, input.free_map.clone())?;
            fold_collection_map(kvps, known_free, opt_rem, &input)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bool_literals() {
        assert_eq!(normalize_bool(&BoolLiteral::BoolTrue), Expr::GBool(true));
        assert_eq!(normalize_bool(&BoolLiteral::BoolFalse), Expr::GBool(false));
    }

    #[test]
    fn int_ground() {
        assert_eq!(
            normalize_ground(&Ground::GroundInt("42".to_string())).unwrap(),
            Expr::GInt(42)
        );
    }

    #[test]
    fn bigint_ground() {
        assert_eq!(
            normalize_ground(&Ground::GroundBigInt("123".to_string())).unwrap(),
            Expr::GBigInt(BigInt::from(123))
        );
    }

    #[test]
    fn string_ground_strips_quotes() {
        assert_eq!(
            normalize_ground(&Ground::GroundString("\"hello\"".to_string())).unwrap(),
            Expr::GString("hello".to_string())
        );
    }

    #[test]
    fn uri_ground_strips_backticks() {
        assert_eq!(
            normalize_ground(&Ground::GroundUri("`rho:io:stdout`".to_string())).unwrap(),
            Expr::GUri("rho:io:stdout".to_string())
        );
    }

    #[test]
    fn invalid_int_is_normalizer_error() {
        assert!(normalize_ground(&Ground::GroundInt("not-a-number".to_string())).is_err());
    }

    #[test]
    fn ground_proc_normalizes() {
        let p = Proc::PGround(Ground::GroundInt("42".to_string()));
        let out = normalize_proc(&p, ProcVisitInputs {
            par: Par::default(),
            bound_map_chain: crate::compiler::BoundMapChain::empty(),
            free_map: FreeMap::empty(),
        })
        .unwrap();
        assert_eq!(out.par.exprs, vec![Expr::GInt(42)]);
    }

    #[test]
    fn binary_arith_normalizes() {
        let p = Proc::PAdd(
            Box::new(Proc::PGround(Ground::GroundInt("1".to_string()))),
            Box::new(Proc::PGround(Ground::GroundInt("2".to_string()))),
        );
        let out = normalize_proc(&p, ProcVisitInputs {
            par: Par::default(),
            bound_map_chain: crate::compiler::BoundMapChain::empty(),
            free_map: FreeMap::empty(),
        })
        .unwrap();
        assert_eq!(
            out.par.exprs,
            vec![Expr::EPlus(
                Box::new(Par { exprs: vec![Expr::GInt(1)], ..Par::default() }),
                Box::new(Par { exprs: vec![Expr::GInt(2)], ..Par::default() }),
            )]
        );
    }
}
