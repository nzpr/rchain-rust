# RChain — Rust rewrite (workspace)

A Cargo workspace, one crate per original sbt module, mirroring the Scala dependency graph. The
rewrite is a **faithful port** — semantics are preserved exactly, per [../AGENTS.md](../AGENTS.md)
and the formalizations in [../spec/](../spec/).

## Layout & rewrite order

Ported in dependency-respecting order (easiest/leaf modules first):

| Crate | Source module | Status |
|-------|---------------|--------|
| `sdk` | `sdk/` | **in progress** (root leaf; Laws 14 & 17) |
| `regex` | `regex/` | next (orphaned leaf, pure algorithm) |
| `shared` | `shared/` | pending |
| `crypto` | `crypto/` | pending |
| `graphz` | `graphz/` | pending |
| `models` | `models/` | pending |
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
