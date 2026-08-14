# RChain — Rust rewrite (workspace)

A Cargo workspace, one crate per original sbt module, mirroring the Scala dependency graph. The
rewrite is a **faithful port** — semantics are preserved exactly, per [../AGENTS.md](../AGENTS.md)
and the formalizations in [../spec/](../spec/).

## Layout & rewrite order

Ported in dependency-respecting order (easiest/leaf modules first):

| Crate | Source module | Status |
|-------|---------------|--------|
| `sdk` | `sdk/` | **done** (root leaf; Laws 14 & 17) |
| `shared` | `shared/` | **core done** (Base16/Serialize/DagOps/store); LMDB FFI deferred |
| `regex` | `regex/` | orphaned leaf, pure algorithm (deferred — off critical path) |
| `crypto` | `crypto/` | **done** (Law 19: Blake2b256, Blake2b512Random, secp256k1/Ed25519, Curve25519) |
| `graphz` | `graphz/` | **done** (DOT builder) |
| `models` | `models/` | **core done** (rholang AST + Law 1 sorter); wire serialization + Casper types deferred |
| `block-storage` | `block-storage/` | pending |
| `rspace` | `rspace/` | pending |
| `comm` | `comm/` | pending |
| `rholang` | `rholang/` | pending |
| `casper` | `casper/` | pending |
| `node` | `node/` | pending |
| `rspace-bench` | `rspace-bench/` | gated |

Deferred (orphaned, not wired into `build.sbt`): `roscala/`, `rosette/` (C++ VM).

See the full difficulty/time scoping in [../AGENTS.md](../AGENTS.md).

## Build & test

```sh
cd crates
cargo build
cargo test
```
