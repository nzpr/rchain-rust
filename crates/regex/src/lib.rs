//! Faithful Rust port of the RChain `regex` module.
//!
//! Mirrors `regex/src/main/scala/coop/rchain/regex/*.scala`: a pure finite-state-machine engine,
//! a regex AST + parser, repetition-bound arithmetic, and a path-to-regex tokenizer. There are no
//! inter-module RChain dependencies; the only external dependency is `fancy-regex` (for the
//! lookahead/inline-flag regexes the Scala code delegates to `java.util.regex`).

pub mod errors;
pub mod fsm;
pub mod multiplier;
pub mod path_regex;
pub mod regex_pattern;

pub use errors::RegexError;
pub use fsm::{Fsm, ANYTHING_ELSE};
pub use multiplier::Multiplier;
pub use path_regex::{PathRegex, PathRegexOptions, PathToken};
pub use regex_pattern::{AltPattern, CharClassPattern, ConcPattern, MultPattern, RegexPattern};
