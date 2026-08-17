//! The rholang↔rspace runtime bridge (port of `interpreter/storage/`).
//!
//! `RhoHistoryRepository` specializes `rspace::HistoryRepository` to the rholang types;
//! [`ChargingRSpace`] adapts the async rspace `Tuplespace` to the async rholang `reduce::Tuplespace`.

use std::collections::BTreeSet;
use std::sync::Arc;

use async_trait::async_trait;
use rchain_models::ast::{EList, Expr, Par, Var};
use rchain_models::par_ops::from_expr;
use rchain_models::runtime::{BindPattern, ListParWithRandom, TaggedContinuation};
use rchain_rspace::history::history_repository::HistoryRepository;
use rchain_rspace::match_::Match;
use rchain_rspace::tuple_space::{
    ContResult, Result as RSpaceResult, Tuplespace as RSpaceTuplespace,
};

use crate::errors::RholangError;
use crate::matcher::{fold_match, spatial_match, FreeMap};
use crate::reduce::{Application, Tuplespace};

/// The rholang history repository (port of `RhoHistoryRepository`).
pub type RhoHistoryRepository =
    Arc<HistoryRepository<Par, BindPattern, ListParWithRandom, TaggedContinuation>>;

/// The rholang tuplespace (port of `RhoTuplespace`).
pub type RhoTuplespace =
    Arc<dyn RSpaceTuplespace<Par, BindPattern, ListParWithRandom, TaggedContinuation>>;

/// Convert an rspace produce/consume result into the rholang `Application` (port of
/// `unpackOptionWithPeek`).
pub fn to_application(
    r: Option<(
        ContResult<Par, BindPattern, TaggedContinuation>,
        Vec<RSpaceResult<Par, ListParWithRandom>>,
    )>,
) -> Application {
    r.map(|(cont, data)| {
        (
            cont.continuation,
            data.into_iter()
                .map(|d| (d.channel, d.matched_datum, d.removed_datum, d.persistent))
                .collect(),
            cont.peek,
        )
    })
}

/// The spatial matcher instance for `(BindPattern, ListParWithRandom)` (port of `matchListPar`).
#[derive(Clone)]
pub struct RhoMatch;

impl Match<BindPattern, ListParWithRandom> for RhoMatch {
    fn get(&self, pattern: &BindPattern, data: &ListParWithRandom) -> Option<ListParWithRandom> {
        let matches = fold_match(
            &data.pars,
            &pattern.patterns,
            pattern.remainder.as_ref(),
            &FreeMap::new(),
            &spatial_match,
        )
        .ok()?;
        let (caught_rem, free_map) = matches.into_iter().next()?;

        let mut remainder_map = free_map;
        if let Some(Var::FreeVar(level)) = pattern.remainder.as_ref() {
            remainder_map.insert(
                *level,
                from_expr(Expr::EList(EList {
                    ps: caught_rem,
                    ..Default::default()
                })),
            );
        }

        let pars = (0..pattern.free_count)
            .map(|i| remainder_map.get(&i).cloned().unwrap_or_default())
            .collect();
        Some(ListParWithRandom {
            pars,
            random_state: data.random_state.clone(),
        })
    }
}

/// The charging tuplespace bridge: adapts the async rspace to the async rholang `Tuplespace` (port
/// of `ChargingRSpace`). Gas charging for storage/events is deferred; this delegates produce/consume
/// and converts the result.
#[derive(Clone)]
pub struct ChargingRSpace {
    space: RhoTuplespace,
}

impl ChargingRSpace {
    /// Wrap `space`, delegating its async produce/consume directly (no `block_on` bridge).
    pub fn new(space: RhoTuplespace) -> Self {
        ChargingRSpace { space }
    }
}

#[async_trait]
impl Tuplespace for ChargingRSpace {
    async fn produce(
        &self,
        channel: &Par,
        data: ListParWithRandom,
        persist: bool,
    ) -> Result<Application, RholangError> {
        let result = self
            .space
            .produce(channel.clone(), data, persist)
            .await
            .map_err(|e| RholangError::ReduceError(e.to_string()))?;
        Ok(to_application(result))
    }

    async fn consume(
        &self,
        channels: &[Par],
        patterns: &[BindPattern],
        continuation: TaggedContinuation,
        persist: bool,
        peeks: BTreeSet<usize>,
    ) -> Result<Application, RholangError> {
        let result = self
            .space
            .consume(channels, patterns, continuation, persist, peeks)
            .await
            .map_err(|e| RholangError::ReduceError(e.to_string()))?;
        Ok(to_application(result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rchain_models::ast::Par;

    fn par(exprs: Vec<Expr>) -> Par {
        Par {
            exprs,
            ..Par::default()
        }
    }

    #[test]
    fn rho_match_binds_free_vars() {
        let pattern = BindPattern {
            patterns: vec![Par {
                exprs: vec![Expr::EVar(Box::new(Var::FreeVar(0)))],
                connective_used: true,
                ..Par::default()
            }],
            remainder: None,
            free_count: 1,
        };
        let data = ListParWithRandom {
            pars: vec![par(vec![Expr::GInt(42)])],
            random_state: rchain_crypto::hash::blake2b512_random::Blake2b512Random::new_random(128),
        };
        let result = RhoMatch.get(&pattern, &data).unwrap();
        assert_eq!(result.pars, vec![par(vec![Expr::GInt(42)])]);
    }

    #[test]
    fn to_application_converts() {
        let cont = ContResult {
            continuation: TaggedContinuation::Empty,
            persistent: false,
            channels: vec![par(vec![Expr::GInt(1)])],
            patterns: vec![],
            peek: true,
        };
        let data = RSpaceResult {
            channel: par(vec![Expr::GInt(1)]),
            matched_datum: ListParWithRandom {
                pars: vec![],
                random_state: rchain_crypto::hash::blake2b512_random::Blake2b512Random::new_random(128),
            },
            removed_datum: ListParWithRandom {
                pars: vec![],
                random_state: rchain_crypto::hash::blake2b512_random::Blake2b512Random::new_random(128),
            },
            persistent: false,
        };
        let app = to_application(Some((cont, vec![data]))).unwrap();
        assert!(matches!(app.0, TaggedContinuation::Empty));
        assert!(app.2);
        assert_eq!(app.1.len(), 1);
    }
}
