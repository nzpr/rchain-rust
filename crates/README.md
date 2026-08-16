# RChain — Rust rewrite (workspace)

A Cargo workspace, one crate per original sbt module, mirroring the Scala dependency graph. The
rewrite is a **faithful port** — semantics are preserved exactly, per [../AGENTS.md](../AGENTS.md)
and the formalizations in [../spec/](../spec/).

## Layout & rewrite order

Ported in dependency-respecting order (easiest/leaf modules first):

| Crate | Source module | Status |
|-------|---------------|--------|
| `sdk` | `sdk/` | **done** (root leaf; Laws 14 & 17; DAG interface: `BlockRequester`/`DagManager`/`DagView`/`DagData`) |
| `shared` | `shared/` | **core done** (Base16/Serialize/DagOps/store); LMDB FFI deferred |
| `regex` | `regex/` | **done** (FSM engine + regex AST/parser + path-to-regex tokenizer) |
| `crypto` | `crypto/` | **done** (Law 19: Blake2b256, Blake2b512Random, secp256k1/Ed25519, Curve25519) |
| `graphz` | `graphz/` | **done** (DOT builder) |
| `models` | `models/` | **done** (rholang AST + Law 1 sorter + Casper/routing wire layer) |
| `block-storage` | `block-storage/` | **done** (DAG finalizer + BlockStore/ApprovedStore/BlockDagStorage) |
| `rspace` | `rspace/` | **done** (hashing/radix-tree/history/merger + play/replay engine, merger execution `computeTrieActions`, replay verification, reporting, hot-store back-fill, util); state/exporters scaffolding ported — `traverseHistory`/`validateStateItems`/store-backed instances deferred pending the radix-tree export traversal |
| `comm` | `comm/` | **done** (PeerNode/PeerTable + Kademlia gRPC discovery, gRPC/TLS transport client/server/receiver, buffers/PacketOps/StreamHandler, rp Connect/HandleMessages); UPnP/WhoAmI deferred |
| `rholang` | `rholang/` | **in progress** (Env + errors; `par_ops` moved into `models`); Substitute/accounting/Reduce/matcher/dispatch pending |
| `casper` | `casper/` | **in progress** (block-validation, DAG storage/message, merge index); `CasperConf`/`GenesisBlockData`/`ListenAtName.Name` added |
| `node` | `node/` | **in progress** (configuration: CLI + HOCON merge + `NodeConf`; diagnostics: metric registry + Prometheus/InfluxDB reporters/encoders + tracing context (`Trace`/`TraceId`/`NodeCallCtx`); api: `RhoExpr` Par⇄JSON tree + WebApi/AdminWebApi interfaces + DTOs + conversion/syntax helpers; web: transaction DTOs + status/version; effects/runtime: REPL/console; dag: block-requester/DAG-manager stubs); gRPC/runtime-wiring pending |
| `rspace-bench` | `rspace-bench/` | gated |

Deferred (orphaned, not wired into `build.sbt`): `roscala/`, `rosette/` (C++ VM).

See the full difficulty/time scoping in [../AGENTS.md](../AGENTS.md).

## Build & test

```sh
cd crates
cargo build
cargo test
```
