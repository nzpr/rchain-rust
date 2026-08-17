//! Key-value store abstractions.
//!
//! Mirrors `shared/src/main/scala/coop/rchain/store/{KeyValueStore,InMemoryKeyValueStore,
//! NoOpKeyValueStore}.scala`. The Scala `F[_]` effect and `ByteBuffer` zero-copy handling are
//! simplified to synchronous `Vec<u8>` operations; the async/effect model is reintroduced when
//! tokio lands. `iterate` becomes `entries` (eager).

use std::collections::BTreeMap;

/// A byte-oriented key-value store.
pub trait KeyValueStore {
    fn get(&self, keys: &[Vec<u8>]) -> Vec<Option<Vec<u8>>>;
    fn put(&mut self, pairs: Vec<(Vec<u8>, Vec<u8>)>);
    /// Delete the given keys, returning the number of keys that were actually present.
    fn delete(&mut self, keys: &[Vec<u8>]) -> usize;
    fn entries(&self) -> Vec<(Vec<u8>, Vec<u8>)>;
}

/// In-memory implementation (port of `InMemoryKeyValueStore`, using a `BTreeMap` for determinism).
#[derive(Default)]
pub struct InMemoryKeyValueStore {
    map: BTreeMap<Vec<u8>, Vec<u8>>,
}

impl InMemoryKeyValueStore {
    pub fn num_records(&self) -> usize {
        self.map.len()
    }

    pub fn clear(&mut self) {
        self.map.clear();
    }
}

impl KeyValueStore for InMemoryKeyValueStore {
    fn get(&self, keys: &[Vec<u8>]) -> Vec<Option<Vec<u8>>> {
        keys.iter().map(|k| self.map.get(k).cloned()).collect()
    }

    fn put(&mut self, pairs: Vec<(Vec<u8>, Vec<u8>)>) {
        for (k, v) in pairs {
            self.map.insert(k, v);
        }
    }

    fn delete(&mut self, keys: &[Vec<u8>]) -> usize {
        keys.iter().filter(|k| self.map.remove(*k).is_some()).count()
    }

    fn entries(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
        self.map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }
}

/// No-op implementation (port of `NoOpKeyValueStore`).
#[derive(Default)]
pub struct NoOpKeyValueStore;

impl KeyValueStore for NoOpKeyValueStore {
    fn get(&self, _keys: &[Vec<u8>]) -> Vec<Option<Vec<u8>>> {
        Vec::new()
    }

    fn put(&mut self, _pairs: Vec<(Vec<u8>, Vec<u8>)>) {}

    fn delete(&mut self, _keys: &[Vec<u8>]) -> usize {
        0
    }

    fn entries(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(s: &str) -> Vec<u8> {
        s.as_bytes().to_vec()
    }

    #[test]
    fn get_put_delete_round_trip() {
        let mut store = InMemoryKeyValueStore::default();
        store.put(vec![(k("a"), vec![1]), (k("b"), vec![2])]);
        assert_eq!(store.get(&[k("a"), k("b"), k("c")]), vec![Some(vec![1]), Some(vec![2]), None]);
        assert_eq!(store.delete(&[k("a"), k("c")]), 1);
        assert_eq!(store.get(&[k("a"), k("b")]), vec![None, Some(vec![2])]);
    }

    #[test]
    fn entries_returns_all_pairs() {
        let mut store = InMemoryKeyValueStore::default();
        store.put(vec![(k("b"), vec![2]), (k("a"), vec![1])]);
        assert_eq!(store.entries(), vec![(k("a"), vec![1]), (k("b"), vec![2])]);
    }

    #[test]
    fn no_op_store_is_empty() {
        let mut store = NoOpKeyValueStore;
        store.put(vec![(k("a"), vec![1])]);
        assert_eq!(store.get(&[k("a")]), Vec::<Option<Vec<u8>>>::new());
        assert_eq!(store.delete(&[k("a")]), 0);
        assert!(store.entries().is_empty());
    }
}
