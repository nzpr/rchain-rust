//! Rholang trie traversal (port of `node/revvaultexport/RhoTrieTraverser.scala`).

use std::collections::{BTreeMap, VecDeque};

use rchain_crypto::hash::keccak256;
use rchain_models::ast::{EList, ETuple, Expr, Par, ParMap};
use rchain_models::rholang::RhoType::RhoByteArray;
use rchain_rholang::runtime::RhoRuntime;
use rchain_rspace::errors::RSpaceError;
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

/// A trie node value: a child pointer (int) or a leaf map (port of the `Left`/`Right` in
/// `TreeHashMapGetter`).
enum NodeValue {
    Pointer(i64),
    Map(ParMap),
}

fn node_map_list(map_par: &Par, store_token_par: &Par, nyb_list: &[i32]) -> Par {
    let map_with_nyb = Par {
        exprs: vec![Expr::ETuple(ETuple {
            ps: vec![map_par.clone(), node_list(nyb_list)],
            ..ETuple::default()
        })],
        ..Par::default()
    };
    node_map_store(&map_with_nyb, store_token_par)
}

fn node_map_store(map_with_nyb: &Par, store_token_par: &Par) -> Par {
    Par {
        exprs: vec![Expr::EList(EList {
            ps: vec![map_with_nyb.clone(), store_token_par.clone()],
            ..EList::default()
        })],
        ..Par::default()
    }
}

/// Extend a key with each set bit of `value` (port of `extendKey`).
fn extend_key(head: &[i32], value: i64) -> Vec<Vec<i32>> {
    (0..16i32)
        .filter(|i| (value >> i) & 1 != 0)
        .map(|i| {
            let mut new_head = head.to_vec();
            new_head.push(i);
            new_head
        })
        .collect()
}

/// Read the trie node at `nyb_list` (port of `TreeHashMapGetter`).
async fn tree_hash_map_getter(
    map_par: &Par,
    store_token_par: &Par,
    nyb_list: &[i32],
    runtime: &RhoRuntime,
) -> Result<Option<NodeValue>, RSpaceError> {
    let channel = node_map_list(map_par, store_token_par, nyb_list);
    let data = runtime.get_data(&channel).await?;
    Ok(data.first().and_then(|datum| {
        let head_par = match datum.a.pars.as_slice() {
            [single] => single,
            _ => return None,
        };
        let head_expr = match head_par.exprs.as_slice() {
            [single] => single,
            _ => return None,
        };
        match head_expr {
            Expr::GInt(i) => Some(NodeValue::Pointer(*i)),
            Expr::EMap(value) => Some(NodeValue::Map(value.clone())),
            _ => None,
        }
    }))
}

/// Traverse the TreeHashMap trie and collect the leaf maps (port of `traverseTrie`).
pub async fn traverse_trie(
    depth: i32,
    map_par: &Par,
    store_token_par: &Par,
    runtime: &RhoRuntime,
) -> Result<Vec<ParMap>, RSpaceError> {
    let depth = depth as usize * 2;
    let mut keys: VecDeque<Vec<i32>> = VecDeque::from([Vec::new()]);
    let mut collected: Vec<ParMap> = Vec::new();

    while let Some(key) = keys.pop_front() {
        match tree_hash_map_getter(map_par, store_token_par, &key, runtime).await? {
            Some(NodeValue::Pointer(i)) => {
                if key.is_empty() || depth != key.len() {
                    keys.extend(extend_key(&key, i));
                }
            }
            Some(NodeValue::Map(map)) => collected.push(map),
            None => {}
        }
    }
    Ok(collected)
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
