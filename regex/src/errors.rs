//! Regex module errors.
//!
//! Hard error type for the `regex` crate. Mirrors the exceptions the Scala oracle throws
//! (`IllegalArgumentException`, `NoSuchElementException`) plus host-regex compilation failures.

use std::fmt;

/// An error produced by the regex module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegexError {
    /// An argument was invalid (mirrors Scala `IllegalArgumentException`).
    InvalidArgument(String),
    /// A symbol was not in an FSM's alphabet (mirrors Scala `NoSuchElementException` in `derive`).
    SymbolOutOfAlphabet(char),
    /// A host regex failed to compile.
    Compile(String),
}

impl fmt::Display for RegexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RegexError::InvalidArgument(m) => write!(f, "invalid argument: {m}"),
            RegexError::SymbolOutOfAlphabet(c) => {
                write!(f, "Symbol '{c}' is not in the source alphabet.")
            }
            RegexError::Compile(m) => write!(f, "regex compilation error: {m}"),
        }
    }
}

impl std::error::Error for RegexError {}
