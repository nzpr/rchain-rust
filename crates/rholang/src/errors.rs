//! Interpreter error types. Mirrors `rholang/src/main/scala/coop/rchain/rholang/interpreter/errors.scala`.

use std::fmt;

/// The rholang interpreter error ADT (mirrors the Scala `InterpreterError` hierarchy).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RholangError {
    /// A reduction step failed (mirrors `ReduceError`).
    ReduceError(String),
    /// Gas/phlogiston exhausted (mirrors `OutOfPhlogistonsError`).
    OutOfPhlogistonsError,
    /// An illegal substitution (mirrors `SubstituteError`).
    SubstituteError(String),
    /// A normalization error (mirrors `NormalizerError`; reserved for the deferred normalizer).
    NormalizerError(String),
}

impl fmt::Display for RholangError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RholangError::ReduceError(m) => write!(f, "Reduce error: {m}"),
            RholangError::OutOfPhlogistonsError => write!(f, "Out of phlogistons"),
            RholangError::SubstituteError(m) => write!(f, "Substitute error: {m}"),
            RholangError::NormalizerError(m) => write!(f, "Normalizer error: {m}"),
        }
    }
}

impl std::error::Error for RholangError {}

/// Convenience result alias.
pub type Result<A> = std::result::Result<A, RholangError>;
