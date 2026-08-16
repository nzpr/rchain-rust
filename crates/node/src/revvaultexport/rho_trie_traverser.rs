//! Rholang trie traversal (port of `node/revvaultexport/RhoTrieTraverser.scala`).
//!
//! The effectful traversal (`traverseTrie` + `TreeHashMapGetter`) is deferred; only the pure
//! key/node helpers are ported.

use std::collections::BTreeMap;

use rchain_crypto::hash::keccak256;
use rchain_models::ast::{EList, Expr, Par, ParMap};
use rchain_models::rholang::RhoType::RhoByteArray;
use rchain_shared::serialize::Serialize;

fn keccak_hash(input: &[u8]) -> Par {
    RhoByteArray::apply(keccak256::hash(input))
}

/// The `n`th byte of a `GByteArray` `Par`, as an unsigned `i32` (port of `nthOfPar`).
pub fn nth_of_par(p: &Par, nth: usize) -> i32 {
    match p.exprs.first() {
        Some(Expr::GByteArray(bs)) if nth < bs.len() => bs[nth] as i32,
        _ => panic!("Par {p:?} is not valid for nthOfPar method"),
    }
}

/// Convert the first `length` bytes of a `GByteArray` `Par` into a nybble list (port of
/// `byteArrayToNybbleList`): each byte becomes `(byte % 16, byte / 16)`.
pub fn byte_array_to_nybble_list(binary_array: &Par, length: usize) -> Vec<i32> {
    let mut acc = Vec::with_capacity(length * 2);
    for n in 0..length {
        let b = nth_of_par(binary_array, n);
        acc.push(b % 16);
        acc.push(b / 16);
    }
    acc
}

fn par_string(s: &str) -> Par {
    Par {
        exprs: vec![Expr::GString(s.to_string())],
        ..Par::default()
    }
}

fn par_to_byte_array(p: &Par) -> Vec<u8> {
    <Par as Serialize<Par>>::encode(p)
}

/// The keccak key of a string, wrapped as a `GByteArray` (port of `keccakKey`).
pub fn keccak_key(s: &str) -> Par {
    keccak_hash(&par_to_byte_array(&par_string(s)))
}

/// The keccak key of a string, as raw bytes (port of `keccakParString`).
pub fn keccak_par_string(s: &str) -> Vec<u8> {
    keccak256::hash(&par_to_byte_array(&par_string(s)))
}

/// Build the node-list `Par` from a nybble list (port of `nodeList`).
pub fn node_list(nyb_list: &[i32]) -> Par {
    Par {
        exprs: vec![Expr::EList(EList {
            ps: nyb_list
                .iter()
                .map(|n| Par {
                    exprs: vec![Expr::GInt(*n as i64)],
                    ..Par::default()
                })
                .collect(),
            ..EList::default()
        })],
        ..Par::default()
    }
}

/// Flatten a vector of `ParMap`s into a map (port of `vecParMapToMap`).
pub fn vec_par_map_to_map<K: Ord, V>(
    values: &[ParMap],
    get_key: impl Fn(&Par) -> K,
    get_value: impl Fn(&Par) -> V,
) -> BTreeMap<K, V> {
    let mut out = BTreeMap::new();
    for m in values {
        for (k, v) in &m.kvs {
            out.insert(get_key(k), get_value(v));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nybble_list_splits_each_byte() {
        // 0xab = 171 → low 11, high 10.
        let par = RhoByteArray::apply(vec![0xab]);
        assert_eq!(byte_array_to_nybble_list(&par, 1), vec![11, 10]);
    }

    #[test]
    fn keccak_key_is_a_byte_array() {
        let par = keccak_key("hello");
        assert!(matches!(par.exprs.first(), Some(Expr::GByteArray(_))));
    }

    #[test]
    fn vec_par_map_to_map_flattens() {
        let key = |p: &Par| match p.exprs.first() {
            Some(Expr::GInt(i)) => *i,
            _ => 0,
        };
        let value = |p: &Par| match p.exprs.first() {
            Some(Expr::GString(s)) => s.clone(),
            _ => String::new(),
        };
        let map = ParMap {
            kvs: vec![(
                Par {
                    exprs: vec![Expr::GInt(1)],
                    ..Par::default()
                },
                Par {
                    exprs: vec![Expr::GString("one".to_string())],
                    ..Par::default()
                },
            )],
            ..ParMap::default()
        };
        let flattened = vec_par_map_to_map(&[map], key, value);
        assert_eq!(flattened.get(&1), Some(&"one".to_string()));
    }
}
