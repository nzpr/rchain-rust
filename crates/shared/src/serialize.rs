//! The `Serialize` typeclass.
//!
//! Mirrors `shared/src/main/scala/coop/rchain/shared/Serialize.scala`. The scodec `ByteVector`
//! becomes `Vec<u8>`, and the scodec `Codec` interop (`toSizeHeadCodec`, `codecByteVector`) is
//! dropped — it is a scodec concern, not part of the serialization contract.

/// Typeclass for serializing and deserializing values of type `A`.
pub trait Serialize<A> {
    fn encode(a: &A) -> Vec<u8>;

    fn decode(bytes: &[u8]) -> Result<A, String>;
}

#[cfg(test)]
mod tests {
    use super::Serialize;

    struct U32(u32);

    impl Serialize<U32> for U32 {
        fn encode(a: &U32) -> Vec<u8> {
            a.0.to_be_bytes().to_vec()
        }
        fn decode(bytes: &[u8]) -> Result<U32, String> {
            if bytes.len() != 4 {
                return Err(format!("expected 4 bytes, got {}", bytes.len()));
            }
            Ok(U32(u32::from_be_bytes(bytes.try_into().unwrap())))
        }
    }

    #[test]
    fn round_trips() {
        let v = U32(0xdead_beef);
        let bytes = <U32 as Serialize<U32>>::encode(&v);
        assert_eq!(bytes, vec![0xde, 0xad, 0xbe, 0xef]);
        assert_eq!(<U32 as Serialize<U32>>::decode(&bytes).unwrap().0, 0xdead_beef);
    }

    #[test]
    fn decode_rejects_wrong_length() {
        assert!(<U32 as Serialize<U32>>::decode(&[1, 2, 3]).is_err());
    }
}
