//! Listen-at-name client types (port of the `Name` ADT in `ListenAtName.scala`).
//!
//! The gRPC transport services (`DeployService`/`ProposeService` + `Grpc*`) and the CLI programs
//! (`DeployRuntime`) are deferred pending a gRPC client library.

use std::collections::BTreeMap;

use rchain_models::ast::Par;
use rchain_models::rholang::RhoType::RhoName;
use rchain_rholang::errors::RholangError;

/// A name to listen at (port of `ListenAtName.Name`).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Name {
    PrivName(String),
    PubName(String),
}

/// Build a `Par` from a name (port of `ListenAtName.buildParId`).
pub fn build_par(name: &Name) -> Result<Par, RholangError> {
    match name {
        Name::PubName(content) => {
            rchain_rholang::normalizer::source_to_adt_with_env(content, &BTreeMap::new())
        }
        Name::PrivName(content) => Ok(RhoName::apply_bytes(content.as_bytes().to_vec())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priv_name_builds_unforgeable() {
        let par = build_par(&Name::PrivName("abc".to_string())).unwrap();
        assert!(!par.unforgeables.is_empty());
        assert!(par.exprs.is_empty());
    }

    #[test]
    fn pub_name_normalizes_source() {
        let par = build_par(&Name::PubName("Nil".to_string())).unwrap();
        assert!(par.unforgeables.is_empty());
    }
}
