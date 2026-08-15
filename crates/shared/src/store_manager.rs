//! Key-value store manager.
//!
//! Mirrors `shared/src/main/scala/coop/rchain/store/{KeyValueStoreManager,InMemoryStoreManager,
//! KeyValueStoreManagerSyntax}.scala`. The Scala `TrieMap[String, InMemoryKeyValueStore[F]]` becomes
//! a `tokio::sync::Mutex<BTreeMap<..>>` (deterministic iteration, matching the crate's BTreeMap
//! convention), and `F[_]` becomes `async_trait`.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;

use crate::store::{InMemoryKeyValueStore, KeyValueStore};
use crate::typed_store::{Codec, KeyValueTypedStoreCodec, SharedStore};

/// A key-value store manager (port of `KeyValueStoreManager[F]`).
#[async_trait]
pub trait KeyValueStoreManager: Send + Sync {
    /// Get (creating if necessary) the named byte store.
    async fn store(&self, name: &str) -> SharedStore;
    async fn shutdown(&self);
}

/// In-memory store manager (port of `InMemoryStoreManager[F]`).
#[derive(Default)]
pub struct InMemoryStoreManager {
    state: tokio::sync::Mutex<BTreeMap<String, SharedStore>>,
}

#[async_trait]
impl KeyValueStoreManager for InMemoryStoreManager {
    async fn store(&self, name: &str) -> SharedStore {
        let mut state = self.state.lock().await;
        state
            .entry(name.to_string())
            .or_insert_with(|| {
                Arc::new(tokio::sync::Mutex::new(Box::new(
                    InMemoryKeyValueStore::default(),
                ) as Box<dyn KeyValueStore + Send + Sync>))
            })
            .clone()
    }

    async fn shutdown(&self) {}
}

/// Open a typed store from a manager (port of `KeyValueStoreManagerSyntax.database`).
pub async fn database<K, V>(
    manager: &dyn KeyValueStoreManager,
    name: &str,
    k_codec: Arc<dyn Codec<K>>,
    v_codec: Arc<dyn Codec<V>>,
) -> KeyValueTypedStoreCodec<K, V> {
    let store = manager.store(name).await;
    KeyValueTypedStoreCodec::new(store, k_codec, v_codec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::typed_store::{KeyValueTypedStore, StringCodec};

    #[tokio::test]
    async fn store_returns_same_named_store() {
        let manager = InMemoryStoreManager::default();
        let a = manager.store("db").await;
        let b = manager.store("db").await;
        assert!(Arc::ptr_eq(&a, &b));
        let other = manager.store("other").await;
        assert!(!Arc::ptr_eq(&a, &other));
    }

    #[tokio::test]
    async fn database_round_trips() {
        let manager = InMemoryStoreManager::default();
        let db = database(
            &manager,
            "strings",
            Arc::new(StringCodec),
            Arc::new(StringCodec),
        )
        .await;
        db.put(&[("k".to_string(), "v".to_string())]).await;
        assert_eq!(db.get(&["k".to_string()]).await.unwrap(), vec![Some("v".to_string())]);
    }
}
