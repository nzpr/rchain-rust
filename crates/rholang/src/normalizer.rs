//! Normalizer matchers (port of `interpreter/compiler/normalizer/`).
//!
//! These fold the concrete `Proc` AST into the de Bruijn `Par`. The ground-term leaf matchers are
//! ported first; the process/name/collection matchers and the `ProcNormalizeMatcher` dispatch
//! follow.

use num_bigint::BigInt;

use rchain_models::ast::Expr;

use crate::errors::RholangError;
use crate::proc_ast::{BoolLiteral, Ground};

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

/// Strip the surrounding quotes from a string literal.
fn strip_string(raw: &str) -> String {
    raw[1..raw.len() - 1].to_string()
}

/// Strip the surrounding backticks from a URI literal.
fn strip_uri(raw: &str) -> String {
    raw[1..raw.len() - 1].to_string()
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
}
