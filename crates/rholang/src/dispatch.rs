//! Continuation dispatch (port of `dispatch.scala`).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use rchain_crypto::hash::blake2b512_random::Blake2b512Random;
use rchain_models::ast::Par;
use rchain_models::runtime::{ListParWithRandom, TaggedContinuation};

use crate::env::Env;
use crate::errors::RholangError;
use crate::reduce::Dispatch;

/// A built-in (Scala-side) continuation handler (port of the dispatch-table function).
pub type ScalaBodyFn = Box<dyn Fn(&[ListParWithRandom]) -> Result<(), RholangError>>;

/// The `ParBody` continuation evaluator: evals a body in the env built from the matched data with
/// the merged random state.
pub type EvalBodyFn = Box<dyn Fn(&Par, &Env<Par>, &Blake2b512Random) -> Result<(), RholangError>>;

/// Build an environment from the data captured by a match (port of `Dispatch.buildEnv`).
pub fn build_env(data_list: &[ListParWithRandom]) -> Env<Par> {
    Env::make_env(data_list.iter().flat_map(|d| d.pars.iter().cloned()))
}

/// Dispatches a continuation: eval `ParBody`, invoke the built-in handler for `ScalaBodyRef`, or
/// no-op for `Empty` (port of `RholangAndScalaDispatcher`).
///
/// `eval` is set after construction to break the reducer↔dispatcher cycle.
pub struct RholangAndScalaDispatcher {
    dispatch_table: RefCell<BTreeMap<i64, ScalaBodyFn>>,
    eval: RefCell<Option<EvalBodyFn>>,
}

impl RholangAndScalaDispatcher {
    pub fn new(dispatch_table: BTreeMap<i64, ScalaBodyFn>) -> Self {
        RholangAndScalaDispatcher {
            dispatch_table: RefCell::new(dispatch_table),
            eval: RefCell::new(None),
        }
    }

    pub fn set_eval(&self, eval: EvalBodyFn) {
        *self.eval.borrow_mut() = Some(eval);
    }

    pub fn set_dispatch_table(&self, table: BTreeMap<i64, ScalaBodyFn>) {
        *self.dispatch_table.borrow_mut() = table;
    }
}

impl Dispatch for RholangAndScalaDispatcher {
    fn dispatch(
        &self,
        continuation: &TaggedContinuation,
        data_list: &[ListParWithRandom],
    ) -> Result<(), RholangError> {
        match continuation {
            TaggedContinuation::ParBody(pwr) => {
                let env = build_env(data_list);
                let mut randoms: Vec<Blake2b512Random> = vec![pwr.random_state.clone()];
                randoms.extend(data_list.iter().map(|d| d.random_state.clone()));
                let merged = Blake2b512Random::merge(&randoms);
                let eval = self.eval.borrow();
                let f = eval.as_ref().expect("dispatcher eval not set");
                f(&pwr.body, &env, &merged)
            }
            TaggedContinuation::ScalaBodyRef(r) => {
                let table = self.dispatch_table.borrow();
                match table.get(r) {
                    Some(f) => f(data_list),
                    None => Err(RholangError::ReduceError(format!(
                        "dispatch: no function for {r}"
                    ))),
                }
            }
            TaggedContinuation::Empty => Ok(()),
        }
    }
}

impl Dispatch for Rc<RholangAndScalaDispatcher> {
    fn dispatch(
        &self,
        continuation: &TaggedContinuation,
        data_list: &[ListParWithRandom],
    ) -> Result<(), RholangError> {
        self.as_ref().dispatch(continuation, data_list)
    }
}
