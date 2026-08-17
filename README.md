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
| `shared` | **core done** (Base16/Serialize/DagOps/store+KeyValueCache/Stopwatch/LongOps/PathOps/SeqOps/Matcher/Language/Time/Debug helpers + LMDB store under the `lmdb` feature) |
| `regex` | **done** (FSM engine + regex AST/parser + path-to-regex tokenizer) |
| `crypto` | **done** (Law 19: Blake2b256, Blake2b512Random, secp256k1/Ed25519, Curve25519, PEM key writing) |
| `graphz` | **done** (DOT builder) |
| `models` | **done** (rholang AST + Law 1 sorter + Casper/routing wire layer + JSON serde) |
| `block-storage` | **done** (DAG finalizer + BlockStore/ApprovedStore/BlockDagStorage) |
| `rspace` | **done** (hashing/radix-tree/history/merger + play/replay engine, merger execution `computeTrieActions`, replay verification, reporting, hot-store back-fill, util, state/exporters incl. `traverseHistory`/`validateStateItems` + store-backed instances, store→ReplayRSpace factory); LMDB FFI (`RSpaceExporterDisk`) deferred |
| `comm` | **done** (PeerNode/PeerTable + Kademlia gRPC discovery, gRPC/TLS transport client/server/receiver, buffers/PacketOps/StreamHandler, rp Connect/HandleMessages, WhoAmI external-IP discovery + UPnP port-forwarding orchestration); weupnp SSDP/SOAP gateway discovery deferred |
| `rholang` | **done** (Env + de Bruijn substitution + accounting + spatial matcher + Reduce/dispatch + normalizer/compiler/parser + system processes + PrettyPrinter/StoragePrinter + RhoRuntime/ReplayRhoRuntime/ReportingRuntime + `par_ops` in `models`) |
| `casper` | **done** (validate effectful checks, RuntimeManager, merge index/merging, BlockApi/BlockApiImpl, BlockReportApi, GraphGenerator, reporting/rhoReporter, multi-parent Casper, genesis, protocol/engine/storage) |
| `node` | **done** (configuration, diagnostics, api incl. Deploy/Propose/Repl gRPC adapters + WebApi/AdminWebApi + DTOs, web routes/status/version/transaction, effects/runtime REPL, dag, instances incl. ProposerInstance, revvaultexport); tonic/axum transport binding + NodeRuntime/Setup glue deferred (gated on a `Send` runtime + comm/discovery) |
| `rspace-bench` | gated |

Deferred (orphaned, not wired into `build.sbt`): `legacy/roscala/`, `legacy/rosette/` (C++ VM).

## Build & test

```sh
cargo build
cargo test
```

## Remaining work

The port is functionally complete for the execution core, RSpace, rholang, casper, and the node's
pure/API surface. Remaining (Phase 4):

- **LMDB** — `LmdbKeyValueStore`/`LmdbStoreManager` are ported (the `lmdb` feature); the
  `RSpaceExporterDisk.writeToDisk` consumer is deferred (needs the `getHistoryAndData` API that was
  simplified to `getNodes`/`getHistoryItems`/`getDataItems`).
- **comm** — the weupnp SSDP/SOAP gateway-discovery protocol is deferred (UPnP orchestration +
  `WhoAmI` are ported).
- **node transport** — the tonic/axum binding of the gRPC adapters and the `NodeRuntime`/`Setup`
  glue are gated on a `Send` runtime refactor (the `Rc`-based `RhoRuntime` is `!Send`) plus the
  deferred comm/discovery layer.
- **Formalization** — Laws 2–18 statements exist in Lean (`spec/Rchain/`); proofs are residual
  obligations (Laws 14–18 = Casper/storage/crypto, Phases 4–5 per [`spec/INVENTORY.md`](spec/INVENTORY.md)).
- **A7 property tests** — RSpace Laws 7–11 property tests are pending.
- **`rspace-bench`** — gated.

## Where the Scala went

The upstream Scala fork — the sbt modules (`node/`, `sdk/`, `shared/`, `crypto/`, `models/`,
`rspace/`, `comm/`, `casper/`, `rholang/`, `block-storage/`, `regex/`, `graphz/`, `roscala/`,
`rosette/`, `rspace-bench/`), the sbt build (`build.sbt`, `project/`), configuration, CI, docs,
tooling, and data files — now lives under [`legacy/`](legacy/), with the original directory names.
