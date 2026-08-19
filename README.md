# RChain — Rust rewrite

A faithful Rust rewrite of the [RChain](https://rchain.coop) node. The Rust implementation is a
Cargo workspace at the top level (one crate per original sbt module); the entire upstream Scala
fork is preserved for reference under [`legacy/`](legacy/).

## Why Rust

Two reasons drive the rewrite.

**Memory safety.** The Scala/JVM node leaked memory and paused on garbage collection — it shipped
JVM `Memory`/`GarbageCollector` diagnostics and needed `SBT_OPTS="-Xmx4g -Xss2m"` to run. Rust's
ownership model and lack of a tracing GC eliminate the leak and the stop-the-world pause by
construction.

**The calculus hierarchy.** Rust natively expresses the λ-calculus (closures), the π-calculus
(channels and `Send`/`Sync` name mobility), and the ρ-calculus (the reflective π-calculus: a name is
a quoted process, expressed here as the sortable `Par` value). The port's type discipline embeds ρ as
the base sort of a Calculus of Constructions, constructible and provable in Lean 4 and Coq.

The full argument — with a Rust → calculus → formalization correspondence table — is in
[docs/src/why-rust.md](docs/src/why-rust.md). The prose documentation is also served as a book:
`mdbook serve docs`.

## Governance

The rewrite is governed by [`AGENTS.md`](AGENTS.md) — the binding intent + formal specification —
and the machine-checked formalizations in [`spec/`](spec/) (the 19-law invariant inventory in
[`spec/INVENTORY.md`](spec/INVENTORY.md), plus the Lean/Coq tracks). The prime directive is a
**faithful implementation of the ρ-calculus**: the 19 laws are the oracle; the Scala node was the
port reference.

## Layout

Twelve crates mirror the original sbt modules, ported in dependency order. The per-crate status, the
layer map, the rewrite order, and the remaining work are consolidated in
[docs/src/architecture.md](docs/src/architecture.md).

## Build & test

```sh
cargo build
cargo test
```

Build and serve the documentation book:

```sh
mdbook serve docs   # or: mdbook build docs
```

## Where the Scala went

The upstream Scala fork — the sbt modules (`node/`, `sdk/`, `shared/`, `crypto/`, `models/`,
`rspace/`, `comm/`, `casper/`, `rholang/`, `block-storage/`, `regex/`, `graphz/`, `roscala/`,
`rosette/`, `rspace-bench/`), the sbt build (`build.sbt`, `project/`), configuration, CI, docs,
tooling, and data files — now lives under [`legacy/`](legacy/), with the original directory names.
