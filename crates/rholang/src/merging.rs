//! Mergeable (number) channel merging (port of
//! `interpreter/merging/RholangMergingLogic.scala` + `RhoHistoryRepositorySyntax.scala`).
//!
//! Number channels are merged arithmetically: the merged value is the base value plus the sum of
//! the branch diffs, and the merged random generator is the merge of the branch generators.

use std::collections::{BTreeMap, BTreeSet};

use rchain_crypto::hash::blake2b256_hash::Blake2b256Hash;
use rchain_crypto::hash::blake2b512_random::Blake2b512Random;
use rchain_models::ast::Par;
use rchain_models::rholang::RhoType::RhoNumber;
use rchain_models::runtime::{BindPattern, ListParWithRandom, TaggedContinuation};
use rchain_rspace::hashing::stable_hash_provider::hash_produce;
use rchain_rspace::history::history_reader::HistoryReader;
use rchain_rspace::hot_store_trie_action::HotStoreTrieAction;
use rchain_rspace::internal::Datum;
use rchain_rspace::merger::channel_change::ChannelChange;
use rchain_rspace::serializers::scodec_serialize::{decode_datum, encode_datum_bytes};
use rchain_rspace::trace::event::Produce;

use crate::storage::RhoHistoryRepository;

/// The concrete hot-store trie action type for the rholang runtime.
pub type RhoHotStoreTrieAction =
    HotStoreTrieAction<Par, BindPattern, ListParWithRandom, TaggedContinuation>;

/// The concrete (decoded) history reader.
pub type RhoHistoryReader = dyn HistoryReader<Par, BindPattern, ListParWithRandom, TaggedContinuation>;

/// Extract the number + random state from a number-channel datum (port of `getNumberWithRnd`).
pub fn get_number_with_rnd(par_with_rnd: &ListParWithRandom) -> (i64, Blake2b512Random) {
    assert_eq!(
        par_with_rnd.pars.len(),
        1,
        "Number channel should contain single Int term."
    );
    let num = RhoNumber::unapply(&par_with_rnd.pars[0])
        .expect("Number channel should contain single Int term.");
    (num, par_with_rnd.random_state.clone())
}

/// Decode the random state from a raw number-channel datum (port of `decodeRnd`).
pub fn decode_rnd(raw: &[u8]) -> Blake2b512Random {
    let datum: Datum<ListParWithRandom> = decode_datum(raw).expect("decode number-channel datum");
    datum.a.random_state
}

/// Encode a merged number-channel datum (port of `createDatumEncoded`).
pub fn create_datum_encoded(
    channel_hash: Blake2b256Hash,
    num: i64,
    rnd: Blake2b512Random,
) -> Vec<u8> {
    let num_par = RhoNumber::apply(num);
    let par_with_rnd = ListParWithRandom {
        pars: vec![num_par],
        random_state: rnd,
    };
    let data_hash = hash_produce(channel_hash.as_bytes(), &par_with_rnd, false);
    let produce = Produce::from_hash(channel_hash, data_hash, false);
    let datum = Datum {
        a: par_with_rnd,
        persist: false,
        source: produce,
    };
    encode_datum_bytes(&datum)
}

/// Merge a number-channel value from multiple changes + base state (port of
/// `calculateNumberChannelMerge`).
pub async fn calculate_number_channel_merge(
    channel_hash: Blake2b256Hash,
    diff: i64,
    changes: &ChannelChange<Vec<u8>>,
    base_reader: &(dyn HistoryReader<Par, BindPattern, ListParWithRandom, TaggedContinuation> + Sync),
) -> Result<RhoHotStoreTrieAction, String> {
    // Read the initial value of the number channel from the base state.
    let data = base_reader
        .get_data(channel_hash)
        .await
        .map_err(|e| e.to_string())?;
    assert!(
        data.len() <= 1,
        "To calculate difference on a number channel, single value is expected."
    );
    let init_num = data.first().map(|d| get_number_with_rnd(&d.a).0).unwrap_or(0);

    let new_val = init_num + diff;

    let unique_added: BTreeSet<&Vec<u8>> = changes.added.iter().collect();
    let new_rnd = if unique_added.len() == 1 {
        decode_rnd(&changes.added[0])
    } else {
        // Multiple branches: merge the distinct, sorted random generators.
        let mut randoms: Vec<Blake2b512Random> =
            changes.added.iter().map(|raw| decode_rnd(raw)).collect();
        let mut seen: BTreeSet<Vec<u8>> = BTreeSet::new();
        randoms.retain(|r| seen.insert(r.to_bytes()));
        randoms.sort_by_key(|r| r.to_bytes());
        Blake2b512Random::merge(&randoms)
    };

    let datum_encoded = create_datum_encoded(channel_hash, new_val, new_rnd);
    Ok(HotStoreTrieAction::TrieInsertBinaryProduce(
        channel_hash,
        vec![datum_encoded],
    ))
}

/// Read the numeric values of mergeable channels from the base state (port of
/// `readMergeableValues`).
pub async fn read_mergeable_values(
    history_repository: &RhoHistoryRepository,
    base_state: Blake2b256Hash,
    channel_hashes: &BTreeSet<Blake2b256Hash>,
) -> Result<BTreeMap<Blake2b256Hash, i64>, String> {
    let history_reader = history_repository.get_history_reader(base_state).await;
    let binary = history_reader.reader_binary();
    let mut out = BTreeMap::new();
    for ch in channel_hashes {
        let data = binary.get_data(*ch).await.map_err(|e| e.to_string())?;
        assert!(
            data.len() <= 1,
            "To calculate difference on a number channel, single value is expected."
        );
        let num = data
            .first()
            .map(|d| get_number_with_rnd(&d.decoded.a).0)
            .unwrap_or(0);
        out.insert(*ch, num);
    }
    Ok(out)
}
