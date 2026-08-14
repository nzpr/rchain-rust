//! The in-memory hot store overlay over a history snapshot.
//!
//! Mirrors `rspace/src/main/scala/coop/rchain/rspace/HotStore.scala`. The `Deferred`-based
//! memoized back-fill is simplified to a direct read-and-cache from the history reader.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::history::history_reader::HistoryReaderBase;
use crate::hot_store_action::HotStoreAction;
use crate::internal::{Datum, Row, WaitingContinuation};

/// The hot-store overlay state (port of `HotStoreState`).
#[derive(Clone, Debug)]
pub struct HotStoreState<C, P, A, K> {
    pub continuations: BTreeMap<Vec<C>, Vec<WaitingContinuation<P, K>>>,
    pub installed_continuations: BTreeMap<Vec<C>, WaitingContinuation<P, K>>,
    pub data: BTreeMap<C, Vec<Datum<A>>>,
    pub joins: BTreeMap<C, Vec<Vec<C>>>,
    pub installed_joins: BTreeMap<C, Vec<Vec<C>>>,
}

impl<C, P, A, K> Default for HotStoreState<C, P, A, K> {
    fn default() -> Self {
        HotStoreState {
            continuations: BTreeMap::new(),
            installed_continuations: BTreeMap::new(),
            data: BTreeMap::new(),
            joins: BTreeMap::new(),
            installed_joins: BTreeMap::new(),
        }
    }
}

fn remove_index<E: Clone>(col: &[E], index: usize) -> Vec<E> {
    let mut out = col.to_vec();
    out.remove(index);
    out
}

/// The hot store interface (port of `HotStore[F]`).
#[async_trait]
pub trait HotStore<C, P, A, K>: Send + Sync {
    async fn get_continuations(&self, channels: &[C]) -> Vec<WaitingContinuation<P, K>>;
    async fn put_continuation(&self, channels: &[C], wc: WaitingContinuation<P, K>);
    async fn install_continuation(&self, channels: &[C], wc: WaitingContinuation<P, K>);
    async fn remove_continuation(&self, channels: &[C], index: usize);

    async fn get_data(&self, channel: &C) -> Vec<Datum<A>>;
    async fn put_datum(&self, channel: &C, datum: Datum<A>);
    async fn remove_datum(&self, channel: &C, index: i64);

    async fn get_joins(&self, channel: &C) -> Vec<Vec<C>>;
    async fn put_join(&self, channel: &C, join: &[C]);
    async fn install_join(&self, channel: &C, join: &[C]);
    async fn remove_join(&self, channel: &C, join: &[C]);

    async fn changes(&self) -> Vec<HotStoreAction<C, P, A, K>>;
    async fn to_map(&self) -> BTreeMap<Vec<C>, Row<P, A, K>>;
    async fn snapshot(&self) -> HotStoreState<C, P, A, K>;
}

/// The in-memory hot store (port of `InMemHotStore`).
pub struct InMemHotStore<C, P, A, K> {
    state: Mutex<HotStoreState<C, P, A, K>>,
    reader_base: Arc<dyn HistoryReaderBase<C, P, A, K>>,
}

impl<C, P, A, K> InMemHotStore<C, P, A, K>
where
    C: Ord + Clone + Send + Sync + 'static,
    P: Clone + Send + Sync + 'static,
    A: Clone + Send + Sync + 'static,
    K: Clone + Send + Sync + 'static,
{
    pub fn new(reader_base: Arc<dyn HistoryReaderBase<C, P, A, K>>) -> Self {
        InMemHotStore {
            state: Mutex::new(HotStoreState::default()),
            reader_base,
        }
    }

    pub fn from_state(
        state: HotStoreState<C, P, A, K>,
        reader_base: Arc<dyn HistoryReaderBase<C, P, A, K>>,
    ) -> Self {
        InMemHotStore {
            state: Mutex::new(state),
            reader_base,
        }
    }
}

#[async_trait]
impl<C, P, A, K> HotStore<C, P, A, K> for InMemHotStore<C, P, A, K>
where
    C: Ord + Clone + Send + Sync + 'static,
    P: Clone + Send + Sync + 'static,
    A: Clone + Send + Sync + 'static,
    K: Clone + Send + Sync + 'static,
{
    async fn get_continuations(&self, channels: &[C]) -> Vec<WaitingContinuation<P, K>> {
        let mut state = self.state.lock().await;
        match state.continuations.get(channels) {
            Some(conts) => {
                let mut out = Vec::new();
                if let Some(installed) = state.installed_continuations.get(channels) {
                    out.push(installed.clone());
                }
                out.extend(conts.clone());
                out
            }
            None => {
                let from_base = self.reader_base.get_continuations(channels).await;
                state
                    .continuations
                    .insert(channels.to_vec(), from_base.clone());
                let mut out = Vec::new();
                if let Some(installed) = state.installed_continuations.get(channels) {
                    out.push(installed.clone());
                }
                out.extend(from_base);
                out
            }
        }
    }

    async fn put_continuation(&self, channels: &[C], wc: WaitingContinuation<P, K>) {
        let mut state = self.state.lock().await;
        let from_base = if state.continuations.contains_key(channels) {
            Vec::new()
        } else {
            self.reader_base.get_continuations(channels).await
        };
        let cur = state
            .continuations
            .entry(channels.to_vec())
            .or_insert(from_base);
        cur.insert(0, wc);
    }

    async fn install_continuation(&self, channels: &[C], wc: WaitingContinuation<P, K>) {
        let mut state = self.state.lock().await;
        state
            .installed_continuations
            .insert(channels.to_vec(), wc);
    }

    async fn remove_continuation(&self, channels: &[C], index: usize) {
        let mut state = self.state.lock().await;
        let is_installed = state.installed_continuations.contains_key(channels);
        let removed_index = if is_installed {
            if index == 0 {
                // Attempted to remove the installed continuation — skip.
                return;
            }
            index - 1
        } else {
            index
        };
        let from_base = if state.continuations.contains_key(channels) {
            Vec::new()
        } else {
            self.reader_base.get_continuations(channels).await
        };
        let cur = state
            .continuations
            .entry(channels.to_vec())
            .or_insert(from_base);
        if removed_index < cur.len() {
            *cur = remove_index(cur, removed_index);
        }
    }

    async fn get_data(&self, channel: &C) -> Vec<Datum<A>> {
        let mut state = self.state.lock().await;
        match state.data.get(channel) {
            Some(data) => data.clone(),
            None => {
                let from_base = self.reader_base.get_data(channel).await;
                state.data.insert(channel.clone(), from_base.clone());
                from_base
            }
        }
    }

    async fn put_datum(&self, channel: &C, datum: Datum<A>) {
        let mut state = self.state.lock().await;
        let from_base = if state.data.contains_key(channel) {
            Vec::new()
        } else {
            self.reader_base.get_data(channel).await
        };
        let cur = state.data.entry(channel.clone()).or_insert(from_base);
        cur.insert(0, datum);
    }

    async fn remove_datum(&self, channel: &C, index: i64) {
        let mut state = self.state.lock().await;
        let from_base = if state.data.contains_key(channel) {
            Vec::new()
        } else {
            self.reader_base.get_data(channel).await
        };
        let cur = state.data.entry(channel.clone()).or_insert(from_base);
        if index >= 0 && (index as usize) < cur.len() {
            *cur = remove_index(cur, index as usize);
        }
    }

    async fn get_joins(&self, channel: &C) -> Vec<Vec<C>> {
        let mut state = self.state.lock().await;
        match state.joins.get(channel) {
            Some(joins) => {
                let mut out = state.installed_joins.get(channel).cloned().unwrap_or_default();
                out.extend(joins.clone());
                out
            }
            None => {
                let from_base = self.reader_base.get_joins(channel).await;
                state.joins.insert(channel.clone(), from_base.clone());
                let mut out = state.installed_joins.get(channel).cloned().unwrap_or_default();
                out.extend(from_base);
                out
            }
        }
    }

    async fn put_join(&self, channel: &C, join: &[C]) {
        let mut state = self.state.lock().await;
        let from_base = if state.joins.contains_key(channel) {
            Vec::new()
        } else {
            self.reader_base.get_joins(channel).await
        };
        let cur = state.joins.entry(channel.clone()).or_insert(from_base);
        if !cur.contains(&join.to_vec()) {
            cur.insert(0, join.to_vec());
        }
    }

    async fn install_join(&self, channel: &C, join: &[C]) {
        let mut state = self.state.lock().await;
        let cur = state.installed_joins.entry(channel.clone()).or_default();
        if !cur.contains(&join.to_vec()) {
            cur.insert(0, join.to_vec());
        }
    }

    async fn remove_join(&self, channel: &C, join: &[C]) {
        let mut state = self.state.lock().await;
        let from_base = if state.joins.contains_key(channel) {
            Vec::new()
        } else {
            self.reader_base.get_joins(channel).await
        };
        let cur = state.joins.entry(channel.clone()).or_insert(from_base);
        if let Some(index) = cur.iter().position(|j| j == join) {
            *cur = remove_index(cur, index);
        }
    }

    async fn changes(&self) -> Vec<HotStoreAction<C, P, A, K>> {
        let state = self.state.lock().await;
        let mut out = Vec::new();
        for (k, v) in &state.continuations {
            if v.is_empty() {
                out.push(HotStoreAction::DeleteContinuations(k.clone()));
            } else {
                out.push(HotStoreAction::InsertContinuations(k.clone(), v.clone()));
            }
        }
        for (k, v) in &state.data {
            if v.is_empty() {
                out.push(HotStoreAction::DeleteData(k.clone()));
            } else {
                out.push(HotStoreAction::InsertData(k.clone(), v.clone()));
            }
        }
        for (k, v) in &state.joins {
            if v.is_empty() {
                out.push(HotStoreAction::DeleteJoins(k.clone()));
            } else {
                out.push(HotStoreAction::InsertJoins(k.clone(), v.clone()));
            }
        }
        out
    }

    async fn to_map(&self) -> BTreeMap<Vec<C>, Row<P, A, K>> {
        let state = self.state.lock().await;
        let mut out: BTreeMap<Vec<C>, Row<P, A, K>> = BTreeMap::new();
        for (k, v) in &state.data {
            out.entry(vec![k.clone()]).or_default().data = v.clone();
        }
        for (k, v) in &state.continuations {
            out.entry(k.clone()).or_default().wks.extend(v.clone());
        }
        for (k, v) in &state.installed_continuations {
            out.entry(k.clone()).or_default().wks.insert(0, v.clone());
        }
        out.retain(|_, row| !(row.data.is_empty() && row.wks.is_empty()));
        out
    }

    async fn snapshot(&self) -> HotStoreState<C, P, A, K> {
        self.state.lock().await.clone()
    }
}
