# RChain — Rust rewrite

A faithful Rust rewrite of the [RChain](https://rchain.coop) node. The Rust implementation is a
Cargo workspace at the top level (one crate per original sbt module); the entire upstream Scala
fork is preserved for reference under [`legacy/`](legacy/).

The rewrite is governed by [`AGENTS.md`](AGENTS.md) — the authoritative intent + formal
specification — and the machine-checked formalizations in [`spec/`](spec/) (the 19-law invariant
inventory in [`spec/INVENTORY.md`](spec/INVENTORY.md), plus the Lean/Coq tracks). The prime
directive is **faithful porting**: no behavior is "fixed" or reordered relative to the Scala node.

## Layout

Ported in dependency-respecting order (easiest/leaf modules first). Each crate mirrors one upstream
Scala module (now under `legacy/`):

| Crate | Status |
|-------|--------|
| `sdk` | **done** (root leaf; Laws 14 & 17; DAG interface: `BlockRequester`/`DagManager`/`DagView`/`DagData` + Casper validation syntax + `FatalError` + primitive syntax) |
| `shared` | **core done** (Base16/Serialize/DagOps/store+KeyValueCache/Stopwatch/LongOps/PathOps/SeqOps/Matcher/Language/Time/Debug helpers); LMDB FFI deferred |
| `regex` | **done** (FSM engine + regex AST/parser + path-to-regex tokenizer) |
| `crypto` | **done** (Law 19: Blake2b256, Blake2b512Random, secp256k1/Ed25519, Curve25519, PEM key writing) |
| `graphz` | **done** (DOT builder) |
| `models` | **done** (rholang AST + Law 1 sorter + Casper/routing wire layer + JSON serde) |
| `block-storage` | **done** (DAG finalizer + BlockStore/ApprovedStore/BlockDagStorage) |
| `rspace` | **done** (hashing/radix-tree/history/merger + play/replay engine, merger execution `computeTrieActions`, replay verification, reporting, hot-store back-fill, util); state/exporters scaffolding ported — `traverseHistory`/`validateStateItems`/store-backed instances deferred pending the radix-tree export traversal |
| `comm` | **done** (PeerNode/PeerTable + Kademlia gRPC discovery, gRPC/TLS transport client/server/receiver, buffers/PacketOps/StreamHandler, rp Connect/HandleMessages + UPnP private-IP classifier); UPnP port-forwarding deferred |
| `rholang` | **in progress** (Env + errors; `par_ops` moved into `models`); Substitute/accounting/Reduce/matcher/dispatch pending |
| `casper` | **in progress** (block-validation, DAG storage/message, merge index); `CasperConf`/`GenesisBlockData`/`ListenAtName.Name` added |
| `node` | **in progress** (configuration: CLI + HOCON merge + `NodeConf`; diagnostics: metric registry + Prometheus/InfluxDB reporters/encoders + tracing context; api: `RhoExpr` Par⇄JSON tree + WebApi/AdminWebApi interfaces + DTOs + conversion/syntax helpers; web: transaction DTOs + status/version + HTTP routes; effects/runtime: REPL/console; dag: block-requester/DAG-manager stubs); gRPC/runtime-wiring pending |
| `rspace-bench` | gated |

Deferred (orphaned, not wired into `build.sbt`): `legacy/roscala/`, `legacy/rosette/` (C++ VM).

## Build & test

```sh
cargo build
cargo test
```

## Where the Scala went

The upstream Scala fork — the sbt modules (`node/`, `sdk/`, `shared/`, `crypto/`, `models/`,
`rspace/`, `comm/`, `casper/`, `rholang/`, `block-storage/`, `regex/`, `graphz/`, `roscala/`,
`rosette/`, `rspace-bench/`), the sbt build (`build.sbt`, `project/`), configuration, CI, docs,
tooling, and data files — now lives under [`legacy/`](legacy/), with the original directory names.
