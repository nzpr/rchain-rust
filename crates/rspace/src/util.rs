//! RSpace byte-vector ordering helpers.
//!
//! Mirrors `rspace/src/main/scala/coop/rchain/rspace/util/package.scala`.

use std::cmp::Ordering;
use std::fmt;

use crate::tuple_space::{ContResult, Result};

/// A replay verification failure (port of `util/ReplayException.scala`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayException(pub String);

impl fmt::Display for ReplayException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ReplayException: {}", self.0)
    }
}

impl std::error::Error for ReplayException {}

/// Compare byte vectors length-first, then unsigned bytewise (port of `veccmp`).
pub fn veccmp(a: &[u8], b: &[u8]) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

/// The canonical byte-vector ordering (port of `ordByteVector`).
pub fn ord_byte_vector(a: &[u8], b: &[u8]) -> Ordering {
    veccmp(a, b)
}

/// Unwrap a sequence of results into `(continuation, matchedData)` (port of `unpackSeq`).
#[allow(dead_code)] // used by the deferred casper/node layers
pub fn unpack_seq<C, P, K, R>(
    v: Vec<Option<(ContResult<C, P, K>, Vec<Result<C, R>>)>>,
) -> Vec<Option<(K, Vec<R>)>> {
    v.into_iter().map(unpack_option).collect()
}

/// Unwrap an optional result into `(continuation, matchedData)` (port of `unpackOption`).
#[allow(dead_code)] // used by the deferred casper/node layers
pub fn unpack_option<C, P, K, R>(
    v: Option<(ContResult<C, P, K>, Vec<Result<C, R>>)>,
) -> Option<(K, Vec<R>)> {
    v.map(unpack_tuple)
}

/// Unwrap a result into `(continuation, matchedData)` (port of `unpackTuple`).
#[allow(dead_code)] // used by the deferred casper/node layers
pub fn unpack_tuple<C, P, K, R>(v: (ContResult<C, P, K>, Vec<Result<C, R>>)) -> (K, Vec<R>) {
    let (cont, data) = v;
    (
        cont.continuation,
        data.into_iter().map(|d| d.matched_datum).collect(),
    )
}

/// Unwrap an optional result keeping channel/removed/persistent metadata (port of
/// `unpackOptionWithPeek`).
#[allow(dead_code)] // used by the deferred casper/node layers
pub fn unpack_option_with_peek<C, P, K, R>(
    v: Option<(ContResult<C, P, K>, Vec<Result<C, R>>)>,
) -> Option<(K, Vec<(C, R, R, bool)>, bool)> {
    v.map(unpack_tuple_with_peek)
}

/// Unwrap a result keeping channel/removed/persistent metadata (port of `unpackTupleWithPeek`).
#[allow(dead_code)] // used by the deferred casper/node layers
pub fn unpack_tuple_with_peek<C, P, K, R>(
    v: (ContResult<C, P, K>, Vec<Result<C, R>>),
) -> (K, Vec<(C, R, R, bool)>, bool) {
    let (cont, data) = v;
    (
        cont.continuation,
        data.into_iter()
            .map(|d| (d.channel, d.matched_datum, d.removed_datum, d.persistent))
            .collect(),
        cont.peek,
    )
}

/// Extract the continuation from an optional pair (port of `getK`).
#[allow(dead_code)] // used by the deferred casper/node layers
pub fn get_k<A, K>(t: Option<(K, A)>) -> Option<K> {
    t.map(|x| x.0)
}

/// Run a continuation with its accompanying data (port of `runK`).
#[allow(dead_code)] // used by the deferred casper/node layers
pub fn run_k<T, F: FnOnce(T)>(e: Option<(F, T)>) {
    if let Some((k, data)) = e {
        k(data);
    }
}

/// Run a list of continuations with their accompanying data (port of `runKs`).
#[allow(dead_code)] // used by the deferred casper/node layers
pub fn run_ks<T, F: FnOnce(T)>(t: Vec<Option<(F, T)>>) {
    for e in t {
        if let Some((k, data)) = e {
            k(data);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shorter_comes_first() {
        assert_eq!(veccmp(&[0xff], &[0x00, 0x00]), Ordering::Less);
    }

    #[test]
    fn equal_length_compares_unsigned() {
        // unsigned: 0x80 > 0x7f
        assert_eq!(veccmp(&[0x80], &[0x7f]), Ordering::Greater);
        assert_eq!(veccmp(&[0x01, 0x00], &[0x00, 0xff]), Ordering::Greater);
    }
}
