//! RNode key-value store layout (port of `storage/RNodeKeyValueStoreManager.scala`).
//!
//! The `apply` constructor (which opens the LMDB environments via `LmdbDirStoreManager`) is
//! deferred pending the LMDB FFI store manager; only the DB → environment mapping is ported.

use rchain_shared::store_manager::{Db, LmdbEnvConfig, GB, TB};

/// The RNode DB → LMDB environment mapping (port of `rnodeDbMapping`).
///
/// Keys with the same environment name share one LMDB file.
pub fn rnode_db_mapping() -> Vec<(Db, LmdbEnvConfig)> {
    vec![
        // Block storage
        (Db::new("blocks"), LmdbEnvConfig::new("blockstorage", 1 * TB)),
        // Block metadata storage
        (
            Db::new("block-metadata"),
            LmdbEnvConfig::new("dagstorage", 100 * GB),
        ),
        (Db::new("fringe-data"), LmdbEnvConfig::new("dagstorage", 100 * GB)),
        (
            Db::new("finalized-store"),
            LmdbEnvConfig::new("dagstorage", 100 * GB),
        ),
        // Deploys from blocks
        (Db::new("deploy-index"), LmdbEnvConfig::new("dagstorage", 100 * GB)),
        // Runtime mergeable store (cache of mergeable channels for block-merge)
        (
            Db::new("mergeable-channel-cache"),
            LmdbEnvConfig::new("dagstorage", 100 * GB),
        ),
        // Deploys waiting to be added
        (
            Db::new("deploy-pool"),
            LmdbEnvConfig::new("deploypoolstorage", 1 * GB),
        ),
        // Reporting (trace) cache
        (
            Db::new("reporting-cache"),
            LmdbEnvConfig::new("reporting", 10 * TB),
        ),
        // On-chain RSpace (Rholang state); history and roots share one environment.
        (
            Db::new("rspace-history"),
            LmdbEnvConfig::new("rspace/history", 1 * TB),
        ),
        (
            Db::new("rspace-roots"),
            LmdbEnvConfig::new("rspace/history", 1 * TB),
        ),
        (Db::new("rspace-cold"), LmdbEnvConfig::new("rspace/cold", 1 * TB)),
        // Transaction store
        (Db::new("transaction"), LmdbEnvConfig::new("transaction", 1 * GB)),
        // Evaluator RSpace (Rholang state)
        (
            Db::new("eval-history"),
            LmdbEnvConfig::new("eval/history", 1 * TB),
        ),
        (
            Db::new("eval-roots"),
            LmdbEnvConfig::new("eval/history", 1 * TB),
        ),
        (Db::new("eval-cold"), LmdbEnvConfig::new("eval/cold", 1 * TB)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping_has_expected_databases() {
        let mapping = rnode_db_mapping();
        let ids: Vec<&str> = mapping.iter().map(|(db, _)| db.id.as_str()).collect();
        assert!(ids.contains(&"blocks"));
        assert!(ids.contains(&"rspace-history"));
        assert!(ids.contains(&"mergeable-channel-cache"));
        // History and roots share an environment name.
        let history = mapping
            .iter()
            .find(|(db, _)| db.id == "rspace-history")
            .map(|(_, c)| c.name.clone());
        let roots = mapping
            .iter()
            .find(|(db, _)| db.id == "rspace-roots")
            .map(|(_, c)| c.name.clone());
        assert_eq!(history, roots);
    }
}
