//! Stable hashing for channels, joins, produces, and consumes (Law 7: join commutativity).
//!
//! Mirrors `rspace/src/main/scala/coop/rchain/rspace/hashing/StableHashProvider.scala`. Channel
//! keys are hashed in sorted order so that a join's hash is independent of the order the channels
//! were supplied in.

use rchain_crypto::hash::blake2b256_hash::Blake2b256Hash;
use rchain_shared::serialize::Serialize;

use crate::serializers::scodec_serialize::{
    bool8, encode_seq_byte_vectors, to_ordered_byte_vectors,
};

/// Hash a single channel (port of `hash[C](channel)`).
pub fn hash_channel<C>(channel: &C) -> Blake2b256Hash
where
    C: Serialize<C>,
{
    Blake2b256Hash::create(&<C as Serialize<C>>::encode(channel))
}

/// Hash each channel and sort the hashes (port of `hashSeq[C]`).
pub fn hash_seq<C>(channels: &[C]) -> Vec<Blake2b256Hash>
where
    C: Serialize<C>,
{
    let mut hashes: Vec<Blake2b256Hash> = channels.iter().map(hash_channel).collect();
    hashes.sort();
    hashes
}

/// Hash a join of channels (port of `hash[C](channels)`).
pub fn hash_channels<C>(channels: &[C]) -> Blake2b256Hash
where
    C: Serialize<C>,
{
    hash_hashes(&hash_seq(channels))
}

/// Hash a sorted sequence of channel hashes (port of `hash(channelsHashes)`).
pub fn hash_hashes(channel_hashes: &[Blake2b256Hash]) -> Blake2b256Hash {
    let mut sorted: Vec<&[u8; 32]> = channel_hashes.iter().map(|h| h.as_bytes()).collect();
    sorted.sort();
    let parts: Vec<&[u8]> = sorted.into_iter().map(|b| b as &[u8]).collect();
    Blake2b256Hash::create_many(&parts)
}

/// Hash a consume: sorted channel hashes + sorted patterns + continuation + persist (port of the
/// `hash[P, K]` overload used by `Consume.apply`).
pub fn hash_consume<P, K>(
    encoded_channels: &[Vec<u8>],
    patterns: &[P],
    continuation: &K,
    persist: bool,
) -> Blake2b256Hash
where
    P: Serialize<P>,
    K: Serialize<K>,
{
    let mut encoded_seq: Vec<Vec<u8>> = encoded_channels.to_vec();
    encoded_seq.extend(to_ordered_byte_vectors(patterns));
    encoded_seq.push(<K as Serialize<K>>::encode(continuation));
    encoded_seq.push(bool8(persist));
    Blake2b256Hash::create(&encode_seq_byte_vectors(&encoded_seq))
}

/// Hash a produce: channel bytes + datum + persist (port of the `hash[A](channel, datum, persist)`
/// overload used by `Produce.apply`).
pub fn hash_produce<A>(channel: &[u8], datum: &A, persist: bool) -> Blake2b256Hash
where
    A: Serialize<A>,
{
    let encoded_seq = vec![
        channel.to_vec(),
        <A as Serialize<A>>::encode(datum),
        bool8(persist),
    ];
    Blake2b256Hash::create(&encode_seq_byte_vectors(&encoded_seq))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct ByteChannel(Vec<u8>);
    impl Serialize<ByteChannel> for ByteChannel {
        fn encode(a: &ByteChannel) -> Vec<u8> {
            a.0.clone()
        }
        fn decode(bytes: &[u8]) -> Result<ByteChannel, String> {
            Ok(ByteChannel(bytes.to_vec()))
        }
    }

    #[test]
    fn hash_seq_sorts_channel_hashes() {
        // Two channels supplied in one order hash the same as the reverse order.
        let a = ByteChannel(vec![1]);
        let b = ByteChannel(vec![2]);
        let forward = hash_seq(&[a.clone(), b.clone()]);
        let backward = hash_seq(&[b, a]);
        assert_eq!(forward, backward);
        // hashes are sorted ascending
        assert!(forward.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn join_hash_is_order_independent() {
        let a = ByteChannel(vec![1]);
        let b = ByteChannel(vec![2]);
        assert_eq!(hash_channels(&[a.clone(), b.clone()]), hash_channels(&[b, a]));
    }

    #[test]
    fn produce_hash_depends_on_channel_datum_persist() {
        let d1 = ByteChannel(vec![9]);
        let d2 = ByteChannel(vec![10]);
        let h = hash_produce(&[1, 2, 3], &d1, true);
        assert_ne!(h, hash_produce(&[1, 2, 3], &d2, true));
        assert_ne!(h, hash_produce(&[1, 2, 3], &d1, false));
    }
}
