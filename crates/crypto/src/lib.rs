//! Faithful Rust port of the RChain `crypto` module.
//!
//! Mirrors `crypto/src/main/scala/coop/rchain/crypto/`. Implements Law 19 (axiomatized
//! cryptographic primitives): Blake2b256, Blake2b512Random, Keccak256, Sha256, secp256k1 /
//! Ed25519 signatures, and Curve25519 encryption. The primitives are wrapped in RChain-specific
//! interfaces and pinned by known-answer test vectors ported from the Scala test files.
//!
//! Deferred until the node/comm layer lands: `util/KeyUtil` (PEM/AES-256-CBC key files),
//! `Secp256k1.parsePemFile`, and the X.509 helpers in `util/CertificateHelper` (`from` /
//! `readKeyPair` / `generate`).

pub mod encryption;
pub mod hash;
pub mod private_key;
pub mod public_key;
pub mod signatures;
pub mod util;
