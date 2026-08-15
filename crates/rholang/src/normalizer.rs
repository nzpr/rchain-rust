//! Normalizer matchers (port of `interpreter/compiler/normalizer/` + the
//! `ProcNormalizeMatcher` dispatch in `normalize.scala`).
//!
//! These fold the concrete `Proc` AST into the de Bruijn `Par`. Source positions are placeholders
//! until the lexer is ported; the structural processes (`PSend`/`PNew`/`PInput`/…) are deferred.

use num_bigint::BigInt;

use rchain_models::ast::{Connective, ConnectiveBody, Expr, Par, Var, VarRef};
use rchain_models::par_ops::{par_concat, prepend_connective, prepend_expr, single_connective};

use crate::compiler::{
    FreeMap, NameVisitInputs, NameVisitOutputs, ProcVisitInputs, ProcVisitOutputs, VarSort,
};
use crate::errors::{RholangError, SourcePosition};
use crate::proc_ast::{
    BoolLiteral, Ground, Name, NameRemainder, Proc, ProcRemainder, ProcVar, SimpleType, VarRefKind,
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
