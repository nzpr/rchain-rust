//! LMDB-backed key-value store (port of `store/LmdbKeyValueStore.scala` +
//! `store/LmdbStoreManager.scala`).
//!
//! The `KeyValueStore` trait is synchronous, so LMDB transactions run inline; the surrounding
//! `tokio::sync::Mutex` in `SharedStore` serializes access to a store. `LmdbStoreManager` opens a
//! single LMDB environment (file) whose named databases are the `KeyValueStore`s.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use lmdb::{Cursor, Database, DatabaseFlags, Environment, Error as LmdbError, Transaction, WriteFlags};

use crate::store::KeyValueStore;
use crate::store_manager::KeyValueStoreManager;
use crate::typed_store::SharedStore;

/// An LMDB-backed key-value store (port of `LmdbKeyValueStore`).
pub struct LmdbKeyValueStore {
    env: Arc<Environment>,
    db: Database,
}

impl LmdbKeyValueStore {
    pub fn new(env: Arc<Environment>, db: Database) -> Self {
        LmdbKeyValueStore { env, db }
    }
}

impl KeyValueStore for LmdbKeyValueStore {
    fn get(&self, keys: &[Vec<u8>]) -> Vec<Option<Vec<u8>>> {
        let txn = self.env.begin_ro_txn().expect("LMDB read transaction");
        let result = keys
            .iter()
            .map(|k| match txn.get(self.db, k) {
                Ok(v) => Some(v.to_vec()),
                Err(LmdbError::NotFound) => None,
                Err(e) => panic!("LMDB get failed: {e}"),
            })
            .collect();
        txn.commit().expect("LMDB commit");
        result
    }

    fn put(&mut self, pairs: Vec<(Vec<u8>, Vec<u8>)>) {
        let mut txn = self.env.begin_rw_txn().expect("LMDB write transaction");
        for (k, v) in &pairs {
            txn.put(self.db, k, v, WriteFlags::empty())
                .expect("LMDB put");
        }
        txn.commit().expect("LMDB commit");
    }

    fn delete(&mut self, keys: &[Vec<u8>]) -> usize {
        let mut txn = self.env.begin_rw_txn().expect("LMDB write transaction");
        let mut removed = 0;
        for k in keys {
            match txn.del(self.db, k, None) {
                Ok(()) => removed += 1,
                Err(LmdbError::NotFound) => {}
                Err(e) => panic!("LMDB delete failed: {e}"),
            }
        }
        txn.commit().expect("LMDB commit");
        removed
    }

    fn entries(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
        let txn = self.env.begin_ro_txn().expect("LMDB read transaction");
        let mut out = Vec::new();
        {
            let mut cursor = txn.open_ro_cursor(self.db).expect("LMDB cursor");
            for item in cursor.iter() {
                let (k, v) = item.expect("LMDB iterate");
                out.push((k.to_vec(), v.to_vec()));
            }
        }
        txn.commit().expect("LMDB commit");
        out
    }
}

/// A store manager over a single LMDB environment (port of `LmdbStoreManager`).
pub struct LmdbStoreManager {
    env: Arc<Environment>,
}

impl LmdbStoreManager {
    /// Open (creating if absent) an LMDB environment at `dir_path` with the given max size (port of
    /// `LmdbStoreManager.apply`).
    pub fn new(dir_path: &Path, max_env_size: usize) -> Result<Self, String> {
        std::fs::create_dir_all(dir_path).map_err(|e| e.to_string())?;
        let mut builder = Environment::new();
        builder.set_map_size(max_env_size);
        builder.set_max_dbs(20);
        builder.set_max_readers(2048);
        let env = builder.open(dir_path).map_err(|e| e.to_string())?;
        Ok(LmdbStoreManager {
            env: Arc::new(env),
        })
    }
}

#[async_trait]
impl KeyValueStoreManager for LmdbStoreManager {
    async fn store(&self, name: &str) -> SharedStore {
        let db = self
            .env
            .create_db(Some(name), DatabaseFlags::empty())
            .expect("LMDB create_db");
        Arc::new(tokio::sync::Mutex::new(Box::new(LmdbKeyValueStore::new(
            self.env.clone(),
            db,
        )) as Box<dyn KeyValueStore + Send + Sync>))
    }

    async fn shutdown(&self) {
        // The environment is closed when the last `Arc` handle is dropped (lmdb-rkv `Drop`).
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "rchain-lmdb-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[tokio::test]
    async fn lmdb_store_round_trips() {
        let dir = temp_dir();
        let manager = LmdbStoreManager::new(&dir, 10 * 1024 * 1024).unwrap();
        let store = manager.store("db").await;

        {
            let mut kv = store.lock().await;
            kv.put(vec![
                (b"k1".to_vec(), b"v1".to_vec()),
                (b"k2".to_vec(), b"v2".to_vec()),
            ]);
        }
        {
            let kv = store.lock().await;
            assert_eq!(kv.get(&[b"k1".to_vec()]), vec![Some(b"v1".to_vec())]);
            assert_eq!(kv.get(&[b"missing".to_vec()]), vec![None]);
            assert_eq!(kv.entries().len(), 2);
        }
        {
            let mut kv = store.lock().await;
            assert_eq!(kv.delete(&[b"k1".to_vec()]), 1);
            assert_eq!(kv.entries().len(), 1);
        }

        manager.shutdown().await;
        drop(manager);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
