//! DER signature helpers and public-address derivation.
//!
//! Mirrors the DER and `publicAddress` portions of
//! `crypto/src/main/scala/coop/rchain/crypto/util/CertificateHelper.scala`. The X.509 helpers
//! (`from` / `readKeyPair` / `generate`) are deferred until the node/comm layer lands.

use crate::hash::keccak256;

/// Keccak256-hash the input and drop the leading 12 bytes, yielding a 20-byte address.
pub fn public_address(input: &[u8]) -> Vec<u8> {
    keccak256::hash(input)[12..].to_vec()
}

/// Encode a raw 64-byte RS signature as a DER `SEQUENCE { INTEGER r, INTEGER s }`.
pub fn encode_signature_rs_to_der(signature_rs: &[u8]) -> Result<Vec<u8>, String> {
    if signature_rs.is_empty() {
        return Err("Input array must not be empty".to_string());
    }
    let taken = &signature_rs[..signature_rs.len().min(64)];
    let (r, s) = taken.split_at(taken.len().min(32));

    let r_enc = der_integer(r);
    let s_enc = der_integer(s);
    let content_len = 2 + r_enc.len() + 2 + s_enc.len();

    let mut out = vec![0x30];
    encode_len(content_len, &mut out);
    out.push(0x02);
    out.push(r_enc.len() as u8);
    out.extend_from_slice(&r_enc);
    out.push(0x02);
    out.push(s_enc.len() as u8);
    out.extend_from_slice(&s_enc);
    Ok(out)
}

/// Decode a DER signature back into a 64-byte RS signature (each integer left-padded to 32 bytes).
pub fn decode_signature_der_to_rs(signature_der: &[u8]) -> Result<Vec<u8>, String> {
    if signature_der.is_empty() {
        return Err("Input array must not be empty".to_string());
    }
    if signature_der[0] != 0x30 {
        return Err("Input array is not valid DER message format".to_string());
    }
    let (seq_len, mut pos) = read_length(signature_der, 1)?;
    let end = pos + seq_len;

    if signature_der.get(pos) != Some(&0x02) {
        return Err("Input array is not valid DER message format".to_string());
    }
    let (r_len, r_pos) = read_length(signature_der, pos + 1)?;
    let r = &signature_der[r_pos..r_pos + r_len];
    pos = r_pos + r_len;

    if signature_der.get(pos) != Some(&0x02) {
        return Err("Input array is not valid DER message format".to_string());
    }
    let (s_len, s_pos) = read_length(signature_der, pos + 1)?;
    let s = &signature_der[s_pos..s_pos + s_len];

    if end > signature_der.len() {
        return Err("Input array is not valid DER message format".to_string());
    }

    let mut out = Vec::with_capacity(64);
    out.extend_from_slice(&left_pad_32(r)?);
    out.extend_from_slice(&left_pad_32(s)?);
    Ok(out)
}

/// Encode an unsigned big-endian integer as a minimal DER INTEGER.
fn der_integer(unsigned: &[u8]) -> Vec<u8> {
    let mut bytes = unsigned;
    while bytes.len() > 1 && bytes[0] == 0 {
        bytes = &bytes[1..];
    }
    let mut out = Vec::with_capacity(bytes.len() + 1);
    if bytes[0] & 0x80 != 0 {
        out.push(0x00);
    }
    out.extend_from_slice(bytes);
    out
}

fn encode_len(len: usize, out: &mut Vec<u8>) {
    if len < 0x80 {
        out.push(len as u8);
    } else {
        let bytes = len.to_be_bytes();
        let start = bytes.iter().position(|&b| b != 0).unwrap_or(bytes.len() - 1);
        out.push(0x80 | (bytes.len() - start) as u8);
        out.extend_from_slice(&bytes[start..]);
    }
}

fn read_length(buf: &[u8], pos: usize) -> Result<(usize, usize), String> {
    let first = *buf.get(pos).ok_or("truncated DER")?;
    if first & 0x80 == 0 {
        Ok((first as usize, pos + 1))
    } else {
        let n = (first & 0x7f) as usize;
        if n == 0 || n > 4 {
            return Err("invalid DER length".to_string());
        }
        let mut len = 0usize;
        for i in 0..n {
            len = (len << 8) | *buf.get(pos + 1 + i).ok_or("truncated DER")? as usize;
        }
        Ok((len, pos + 1 + n))
    }
}

fn left_pad_32(bytes: &[u8]) -> Result<[u8; 32], String> {
    let mut start = 0;
    while start < bytes.len() && bytes[start] == 0 {
        start += 1;
    }
    let significant = &bytes[start..];
    if significant.len() > 32 {
        return Err("integer too large".to_string());
    }
    let mut out = [0u8; 32];
    out[32 - significant.len()..].copy_from_slice(significant);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn der_round_trips_rs_signature() {
        // A representative 64-byte RS signature.
        let rs: Vec<u8> = (0..64).collect();
        let der = encode_signature_rs_to_der(&rs).expect("encode RS signature to DER");
        assert_eq!(
            decode_signature_der_to_rs(&der).expect("decode DER signature to RS"),
            rs
        );
    }

    #[test]
    fn encoder_rejects_empty_input() {
        assert!(encode_signature_rs_to_der(&[]).is_err());
    }

    #[test]
    fn decoder_rejects_empty_input() {
        assert!(decode_signature_der_to_rs(&[]).is_err());
    }

    #[test]
    fn decoder_rejects_invalid_der() {
        assert!(decode_signature_der_to_rs(&[0xff, 0x00, 0x00, 0x00]).is_err());
    }

    #[test]
    fn public_address_is_last_20_bytes_of_keccak() {
        let input = b"hello";
        let addr = public_address(input);
        assert_eq!(addr.len(), 20);
        assert_eq!(addr, crate::hash::keccak256::hash(input)[12..].to_vec());
    }
}
