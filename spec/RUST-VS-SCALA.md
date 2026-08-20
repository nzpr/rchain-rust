# Rust vs. Scala — how the rewrite made the fragile explicit

This page records how porting the RChain node from Scala/JVM (+ the C++ Rosette VM) to Rust
changed *how we reason about* the code's fragile patterns, bugs, and exploits — and why the Rust
node can now **surpass** the Scala original for production readiness, on top of the JVM's garbage
collection and memory problems.

It is the companion to [`AUDIT.md`](AUDIT.md) (the findings register) and
[`TYPE-SYSTEM.md`](TYPE-SYSTEM.md) (the type discipline). Where Scala and the specification
disagree, the specification is the oracle; the Scala code is reference material whose latent bugs are
**documented, not reproduced**.

---

## 1. The Scala fragility catalog (concrete, caught in the port)

The Scala node was not merely GC- and heap-bound — it was **notoriously fragile** at the exact
boundaries where correctness matters. The port caught each of these as a *type error* or a *code
review finding*, rather than as a runtime incident:

| # | Scala behavior | Why it is a bug / exploit | How Rust makes it impossible or explicit |
|---|---|---|---|
| 1 | `Costs.toProto` = `PCost(c.value)` — a negative `Long` gas cost wraps into a `uint64` | Over-charging a deploy wraps its cost to a huge unsigned value, corrupting accounting | Negative cost is **rejected** at the boundary (`casper/src/runtime_manager.rs`), not wrapped |
| 2 | Super-majority computed as `stake.toDouble / totalStake > 2d/3` | `f64` loses precision for stakes ≥ 2⁵³; two sides of a fork can disagree on a finality vote | Exact integer `3·stake > 2·total` in `i128` (`sdk/src/consensus.rs`) |
| 3 | `spatial_match_fn(…).ok()?.next()` — a `RholangError` swallowed as "no match" | A Law-5 `BugFoundError` is silently treated as a non-match, corrupting reduction | The error is recorded and propagated; matching is total in `Result` |
| 4 | `getUnsafe` / `.get(...).get` / `unwrap_or(0)` on a negative gas cost | Silent partiality: a missing key or negative value becomes `0`/`None` and the node keeps running on corrupt data | Refinement newtypes (`NonNegI64`, `BlockHeight`, `SeqNum`, `Port`, `Cost`, `ByteLen`) carry the invariant *structurally*; no `Deref`, no public `.0` |
| 5 | `maxMessageSize - 2048` in the chunker | Underflows (wraps) when the max size is small, disabling the size guard | `checked_sub` returns `Err` on a too-small max |
| 6 | Radix-tree node as `Vec[Item]` with `NUM_ITEMS = 256` | The "exactly 256 slots" invariant is implicit; a short/corrupt node panics on indexing | `[Item; 256]` fixed array — the invariant is the type |
| 7 | Exceptions as control flow (`throw`/`catch`, `???`, `NotImplementedError`, `BugFoundError`) | A `???` stub or a `BugFoundError` thrown deep in a `Future` aborts a task silently | `Result`/`Option` everywhere; `todo!()`/`unimplemented!()` are compile-time-visible and gated by the audit script |
| 8 | `HashMap` iteration order feeding `New.injections` / matcher state | Non-deterministic ordering → two nodes reach different state hashes → consensus fork | `BTreeMap`/`BTreeSet` (sorted) and explicit sorts at every canonicalization point |

The "fragility" was not incidental: the JVM hid these behind `null`, unchecked casts, boxed
primitives, and catch-all `Try`/`Either` recovery. The Scala node ran *despite* them — the Rust node
refuses to compile *until* they are made explicit.

---

## 2. How Rust's model enables the reasoning

The port does not just *translate* the Scala; it re-expresses each invariant so that the compiler
enforces it:

- **Ownership & the borrow checker** eliminate the aliasing races that made Scala's shared mutable
  `Ref`/`var` state (e.g. the global `connections` write-lock held across I/O, the `BlockRetriever`
  map) hard to audit. Rust's `Mutex`/`RwLock`/`Arc` make *who may mutate what, when* explicit.
- **`enum` + exhaustive `match`** replace null/partial-functions with closed sum types. A
  `NotImplementedError` in Scala becomes a `Result` arm in Rust — the compiler forces you to handle
  the failure case.
- **`Result`/`Option`** replace exceptions and `null`; every partial boundary is a type. The audit
  script (`tools/audit-type-system.sh`) then machine-gates the remaining `unwrap`/`expect`/`panic!`/
  `unsafe`/`assert!` sites.
- **Refinement newtypes** (`NonNegI64`, `BlockHeight`, `SeqNum`, `Port`, `Cost`, `ByteLen`,
  `ShortLen`) carry domain invariants in the type, so "is this stake negative?" or "is this height
  `-1`?" is not a runtime question — it cannot be represented.
- **`Send`/`Sync`** make concurrency safety a compile-time property, not a code-review convention.
- **Zero `unsafe`** across the crate graph — the entire node is safe Rust, so the class of memory
  bugs Scala/Rosette could hit (use-after-free in the C++ VM, JNI boundary errors) is absent by
  construction.
- **Deterministic collections** (`BTreeMap`/`BTreeSet`, explicit sorts) remove hash-iteration
  non-determinism, which is load-bearing for a consensus node.

---

## 3. Surpassing Scala for production readiness

Beyond the memory-safety argument, the Rust node is *more* production-ready than the Scala one in
concrete, auditable ways:

1. **No GC pauses / no JVM heap blowup.** Scala boxed every `Par`/`Expr` node and every event in the
   hot reduction path; long-running nodes suffered stop-the-world pauses and heap pressure. Rust's
   value semantics and explicit allocation give predictable latency and a small, bounded footprint.
2. **No `Vec::with_capacity(attacker_count)` OOM.** The Scala scodec decoders (and the naive Rust
   port of them) trusted 32/64-bit length prefixes from the wire; a malicious length could allocate
   gigabytes. Rust made these *visible* as `with_capacity`/`try_into` sites, so they could be audited
   and bounded (see `AUDIT.md` C2/C3 and the scodec findings).
3. **The defensive wins are only expressible in a safe language** — semaphore-bounded dispatch,
   per-peer rate limits, a content-addressed Merkle radix tree, mutual-TLS identity pinning, and an
   exact integer consensus — and are now *structural*, not advisory.
4. **The port actively fixed Scala bugs** rather than reproducing them: negative cost, f64
   finality, the swallowed matcher error, the underflowing chunker, the `Vec` radix node, and —
   through this remediation — the unenforced `phlo_limit`, the equivocation/failed-block liveness
   gaps, and the remotely-triggerable panics that were faithful Scala behavior.

The honest caveat is in §5: the port is not yet *done* surpassing Scala. Several Scala behaviors were
initially carried over faithfully (the "deferred" surface, the panic-vs-exception sites) precisely
because they were faithful — and the remediation plan exists to convert those into Rust-strength
invariants.

---

## 4. What still lags (honest)

- A large **"deferred"/unwired** surface — Kademlia discovery, the HTTP transaction API, block
  reporting, the `rho:regex` system process, peer store-items serving, and the rholang parser's
  remaining genesis gaps — is still faithful-to-Scala-or-absent rather than surpassed.
- **Residual panic sites** (`assert!`/`panic!`) remain in internal invariant paths (radix-tree
  corrupt-node detection, constructor length checks); the audit gate is being extended to flag them.
- **Liveness gaps** (equivocation handling, fringe pruning) are newly fixed in this remediation but
  need end-to-end integration coverage.

These are tracked in `AUDIT.md` and the remediation plan, and are the active work — not the end state.

---

## 5. Cross-links

- [`AUDIT.md`](AUDIT.md) — the adversarial findings register and Scala-deviation log.
- [`TYPE-SYSTEM.md`](TYPE-SYSTEM.md) — the ρ→CoC type discipline and refinement types.
- [`RHO-CALCULUS.md`](RHO-CALCULUS.md) — the ρ-calculus grammar, sorts, and operations.
- [`INVENTORY.md`](INVENTORY.md) — the 19-law invariant catalog.
