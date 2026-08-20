## Title

**Rewrite the RChain node in Rust (Scala/JVM → Rust)**

## Summary

This PR replaces the entire Scala/JVM node (with its C++ Rosette VM) with a faithful Rust rewrite — a single Cargo workspace, one crate per original sbt module, **~100 commits and ~66k lines of Rust**. The upstream Scala code is preserved under `legacy/` for reference. The node boots, creates genesis, runs the full interpreter and consensus stack, and serves the same gRPC/HTTP/CLI surfaces.

## Why Rust

**1. Memory safety and deterministic resource use.** The JVM node leaked memory and paused on garbage collection — it shipped `Memory`/`MemoryPool`/`GarbageCollector` diagnostics to operators and needed `SBT_OPTS="-Xmx4g -Xss2m"` just to run. Rust's ownership and borrow checker make leaks and use-after-free unrepresentable, with **no tracing GC** — so there is no stop-the-world pause and no heap to tune. Resource lifetime becomes a compile-time, statically checked property.

**2. The calculus is native to Rust.** The node executes Rholang — the reflective, higher-order **ρ-calculus**. Rust expresses each rung of the calculus hierarchy directly:

- **λ** — closures (`fn`, `Fn`/`FnMut`/`FnOnce`);
- **π** — `mpsc` channels and `Send`/`Sync` name mobility;
- **ρ** — reflection: a name *is* a quoted process, expressed as the first-class, sortable, hashed `Par` value;
- **Calculus of Constructions** — the Lean 4 / Coq formalizations.

## What's ported

All twelve modules, end-to-end:

| Layer | Crate(s) | Status |
|---|---|---|
| Rholang | `models`, `rholang` | parser (full grammar) → normalizer (de Bruijn, Law-1 canonical sort) → reducer (spatial matching, COMM, substitution) → `RhoRuntime`/`Replay`/`Reporting` |
| RSpace | `rspace` | Merkle radix trie, history, replay, merge/event log, hot store |
| Storage | `block-storage`, `shared` | BlockStore/ApprovedStore/DAG storage, LMDB, typed stores |
| Consensus | `casper`, `sdk` | CBC-Casper, genesis, block validation, proposer, finalizer |
| Network | `comm` | Kademlia discovery, gRPC/TLS transport, peer table |
| Node | `node` | `rnode` binary: gRPC + HTTP API, `runCLI` (deploy/propose/repl/…), REPL |
| Supporting | `crypto`, `regex`, `graphz` | Blake2b/secp256k1/Curve25519, regex FSM, DOT builder |

The genesis ceremony boots end-to-end (verified by node-level integration tests), and a scripted Docker pipeline (`tools/docker-network.sh`) spins up a 1–5 node network on a bridge.

## The type discipline

The port embeds ρ as the **base sort of a Calculus of Constructions**, and carries the invariants *structurally* rather than by convention:

- a sort-indexed `Par<S>` — compile-time Name/Proc (`quote`/`eval`, `TryFrom<Par> for Name`);
- load-bearing refinements: `Closed` (Law 6, no free variables), `WellScoped` (bound levels in scope), `BindsAtMostOnce` (Law 5), each a validated newtype with one-way discharge;
- domain newtypes (`BlockHeight`, `SeqNum`, `NonNegI64`, `Port`, `WireLen`, …) — **no silent partiality**: zero production `.unwrap()`/`.expect()`/`unsafe`, enforced by a re-runnable machine gate (`tools/audit-type-system.sh`).

## Prime directive

The **19 laws** (`spec/INVENTORY.md`) and the **ρ→CoC type discipline** (`spec/TYPE-SYSTEM.md`) are the oracle — not the Scala code. Rust carries the invariants structurally rather than reproducing the JVM's patterns, *including its latent bugs*. Every deliberate deviation is recorded in `spec/AUDIT.md`.

## Verification

- `cargo test` across all crates; node integration tests (genesis boot, HTTP surface, consensus pipeline).
- `spec/` — Lean 4 + Coq machine-checked definitions and theorems (fundamentals F1–F6).
- An adversarial audit register (`spec/AUDIT.md`) — type-system violations fixed, red-team findings documented.

## Deferred / follow-ups

- `/api/v1` OpenAPI schema route (needs `endpoints4s`).
- Formalization proofs for Laws 14–18.
- `rspace-bench` (gated).
