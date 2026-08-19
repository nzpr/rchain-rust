//! Registry URI derivation and the registry bootstrap AST (port of `interpreter/registry/`).
//!
//! `Registry.build_uri` mirrors `Registry.scala` (including the nested `CRC14`); `ZBase32` is a
//! faithful port of the `org.lightningj.util.ZBase32` codec that the Scala `buildURI` uses.
//! `RegistryBootstrap` mirrors `RegistryBootstrap.scala` — the genesis AST that installs the
//! registry contracts on the fixed REG_* channels.

use std::collections::BTreeMap;

use rchain_models::ast::{AlwaysEqual, Expr, New, Par, Receive, ReceiveBind, Send, Var};
use rchain_models::par_ops::from_expr;

use crate::system_processes::FixedChannels;

/// The z-base-32 alphabet (port of `ZBase32.ALPHABET`).
const ZBASE32_ALPHABET: &[u8; 32] = b"ybndrfg8ejkmcpqxot1uwisza345h769";

/// Encode the first `bit_length` bits of `data` (MSB-first) as z-base-32 (port of
/// `ZBase32.encodeToString(data, length)`).
fn zbase32_encode(data: &[u8], bit_length: usize) -> String {
    let mut out = String::with_capacity((bit_length + 4) / 5);
    let mut acc: u32 = 0;
    let mut acc_bits: u32 = 0;
    for i in 0..bit_length {
        let byte = data[i / 8];
        let bit = (byte >> (7 - (i % 8))) & 1;
        acc = (acc << 1) | bit as u32;
        acc_bits += 1;
        if acc_bits == 5 {
            out.push(ZBASE32_ALPHABET[acc as usize] as char);
            acc = 0;
            acc_bits = 0;
        }
    }
    out
}

/// The CRC-14 used by the registry URI (port of `Registry.CRC14`).
mod crc14 {
    const INIT_REMAINDER: i16 = 0;

    fn update(rem: i16, b: u8) -> i16 {
        fn loop_(i: i32, rem: i16) -> i16 {
            if i < 8 {
                let shift_rem: i16 = rem << 1;
                if (shift_rem & 0x4000i16) != 0 {
                    loop_(i + 1, shift_rem ^ 0x4805i16)
                } else {
                    loop_(i + 1, shift_rem)
                }
            } else {
                rem
            }
        }
        // The Scala `Byte` is signed; sign-extend `b` before the shift to match.
        let init = (rem as i32 ^ (((b as i8 as i32) << 6) & 0xffff)) as i16;
        loop_(0, init)
    }

    pub fn compute(bytes: &[u8]) -> i16 {
        bytes.iter().fold(INIT_REMAINDER, |rem, &b| update(rem, b))
    }
}

/// Build a registry URI from a 32-byte hash (port of `Registry.buildURI`).
pub fn build_uri(arr: &[u8]) -> String {
    let mut full_key = [0u8; 34];
    full_key[..32].copy_from_slice(&arr[..32]);
    let crc = crc14::compute(&full_key[..32]) as u16;
    full_key[32] = (crc & 0xff) as u8;
    full_key[33] = ((crc & 0xff00) >> 6) as u8;
    format!("rho:id:{}", zbase32_encode(&full_key, 270))
}

/// The registry bootstrap AST (port of `RegistryBootstrap.AST`).
pub fn registry_bootstrap_ast() -> Par {
    Par {
        news: vec![
            bootstrap(&FixedChannels::reg_lookup()),
            bootstrap(&FixedChannels::reg_insert_random()),
            bootstrap(&FixedChannels::reg_insert_signed()),
        ],
        ..Default::default()
    }
}

/// A single registry bootstrap contract: `new { for (x <- channel) { x!(channel) } }` (port of
/// `RegistryBootstrap.bootstrap`).
fn bootstrap(channel: &Par) -> New {
    New {
        bind_count: 1,
        p: Box::new(Par {
            receives: vec![Receive {
                binds: vec![ReceiveBind {
                    patterns: vec![from_expr(Expr::EVar(Box::new(Var::FreeVar(0)))).quote()],
                    source: Box::new(channel.clone().quote()),
                    remainder: None,
                    free_count: 1,
                }],
                body: Box::new(Par {
                    sends: vec![Send {
                        chan: Box::new(from_expr(Expr::EVar(Box::new(Var::BoundVar(0)))).quote()),
                        data: vec![channel.clone().quote()],
                        persistent: false,
                        locally_free: AlwaysEqual(vec![]),
                        connective_used: false,
                    }],
                    ..Default::default()
                }),
                persistent: false,
                peek: false,
                bind_count: 1,
                locally_free: AlwaysEqual(vec![]),
                connective_used: false,
            }],
            ..Default::default()
        }),
        uri: vec![],
        injections: BTreeMap::new(),
        locally_free: AlwaysEqual(vec![]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_uri_produces_prefixed_identifier() {
        // The Scala oracle vector (`rho:id:pnrunpy1...` in `MultiParentCasperMergeSpec`) is derived
        // from a *fresh* random seed inside `insertArbitrary`, so it is not reproducible from a
        // fixed input. Verify the structural contract instead: `rho:id:` + 54 z-base-32 chars.
        let uri = build_uri(&[0u8; 32]);
        assert!(uri.starts_with("rho:id:"));
        assert_eq!(uri.len(), "rho:id:".len() + 54);
    }
}
