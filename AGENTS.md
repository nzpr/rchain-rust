# AGENTS.md — RChain → Rust rewrite: intent & formal specification

This file is the **authoritative intent + formal specification** for rewriting this node in Rust. It
is written for both AI coding agents and humans: read it in full before writing or changing any Rust
code, and treat its statements as binding constraints, not suggestions.

## Documentation map

Single sources of truth (do not duplicate these):

| Content | Canonical location |
|---|---|
| Why the rewrite (memory safety + λ → π → ρ → CoC) | [`docs/src/why-rust.md`](docs/src/why-rust.md) |
| Layer map, module status, rewrite order, remaining work | [`docs/src/architecture.md`](docs/src/architecture.md) |
| The 19-law invariant catalog | [`spec/INVENTORY.md`](spec/INVENTORY.md) |
| The ρ→CoC type-system spec | [`spec/TYPE-SYSTEM.md`](spec/TYPE-SYSTEM.md) |
| Machine-checked Lean/Coq definitions & proofs | [`spec/`](spec/) |

## Intent

The RChain node runs Rholang natively. We are rewriting it in **Rust**, absorbing both the Scala/JVM
code and the C++ Rosette VM. The motivation — memory safety and the calculus-native expression of the
node (λ → π → ρ → Calculus of Constructions) — is laid out in
[`docs/src/why-rust.md`](docs/src/why-rust.md); the short version is that this is **not** a
correctness repair, since the existing logic is broadly sound.

**Prime directive:** the rewrite is a *faithful port*. Do **not** "fix", "improve", refactor, or
reorder behavior. Every mathematical invariant listed in [`spec/INVENTORY.md`](spec/INVENTORY.md) must
hold in the Rust port, exactly as it holds today. Where the port and the Scala node disagree on
behavior, the Scala node is correct and the port is wrong.

## How to use this file

For any component you are about to write in Rust:

1. Find its layer below and its law numbers in [`spec/INVENTORY.md`](spec/INVENTORY.md).
2. Read the corresponding formalization (Lean 4 and/or Coq) and the source-of-truth Scala/C++ file.
3. Read the ground-truth Scala test that already encodes the law.
4. Write Rust that satisfies the law, gated by a property test and a differential test (see
   *Translation contract*).

## The formal specifications

| Track | Scope | Location | Build |
|-------|-------|----------|-------|
| **Lean 4** (primary) | algebraic/order laws, canonicalization, merge monoids, consensus arithmetic | [`spec/`](spec/) | `cd spec && lake build` |
| **Coq** | substitution, α-equivalence, and programming-language metatheory (Autosubst in Phase 1) | [`spec/coq/`](spec/coq/) | `make -C spec/coq` |
| **Inventory** | the 19 laws, each with source-of-truth + formalization status | [`spec/INVENTORY.md`](spec/INVENTORY.md) | — |
| **Type system** | the port's own type discipline: ρ-calculus as the base sort of a Calculus of Constructions, no silent partiality | [`spec/TYPE-SYSTEM.md`](spec/TYPE-SYSTEM.md), `Rchain/Rho.lean`, `Rchain/Ty.lean` | `cd spec && lake build` |

The **type-system spec** ([`spec/TYPE-SYSTEM.md`](spec/TYPE-SYSTEM.md)) overlaps the Lean/Coq split
deliberately: Lean 4 proves the six fundamentals over the flat `Par` (sort classification, `≡`,
minimal substitution, minimal COMM reduction, canonicalization, totality); Coq keeps the deep
Autosubst α-equivalence reconciliation. It is a hardening of the port, not a new law.

The Lean 4 and Coq tracks are deliberately parallel: both define the same core `Proc` syntax and the
same canonicalization (`sort`) as Phase 0, and both state Law 1 (`sort` is idempotent and `par`
commutative) as a Phase 1 proof obligation. Coq is the home of the substitution metatheory (capture-
avoiding de Bruijn substitution, α-equivalence); Lean 4 is the home of the algebraic/order laws.

## Formal proof plan — rspace, rholang & Rosette

The two Meredith-designed cores — the **rholang executor** (`rholang/`, the ρ-calculus interpreter)
and **RSpace** (`rspace/`, the concurrent tuple space) — get the deepest machine-checked treatment, in
both **Coq** and **Lean 4**, *before* their Rust is written (**proofs-first**). The **Rosette VM**
(`rosette/`, `roscala/`) is also **in scope for the formalization** (Laws 12–13: actor atomicity,
reflection), formalized in a later phase. Note this is independent of the *rewrite*, where
`rosette`/`roscala` are deferred (orphaned). The split is by
strength:

| Tool | Laws | Scope |
|------|------|-------|
| **Coq** (Autosubst, de Bruijn) | 2–6 | ρ-calculus PL metatheory: α-equivalence, structural congruence `≡`, capture-avoiding substitution, reduction (comm), spatial matching, free variables |
| **Lean 4** (Mathlib) | 1, 7–11 | order/algebra: canonicalization (`sort`), RSpace join commutativity, deterministic COMM, merge monoid, Merkle determinism, replay determinism |

**Proofs-first policy**: the Rust rewrite (including `crypto`/`models`/…) is **paused** until Laws 1–11
are proven; then `rspace`/`rholang` (and the rest) are ported against the verified spec.

### Phase sequence

- **P0** — full ADT in both tools; add Mathlib (Lean) and Autosubst (Coq); pin Meredith citations.
- **P1** — Law 1 (canonicalization) in both tools — the shared linchpin.
- **P2** — Coq: α-equivalence + substitution (Laws 2–3).
- **P3** — Coq: reduction + matching + free variables (Laws 4–6).
- **P4** — Lean: RSpace (Laws 7–11).
- **P5** — reconcile, update status, resume the rewrite.

### Meredith lineage (foundational references)

- Meredith & Radestock, *A Reflective Higher-Order Calculus* (2005) — the ρ-calculus.
- Meredith, *Higher Category Models of the π-Calculus* — the categorical semantics.
- The RChain architecture / RSpace model.
- In-repo executable semantics: `rholang/src/main/k/rholang/*.k` (`name-equivalence.k`,
  `processes-semantics.k`, `sending-receiving.k`, `matching-function.k`, `free.k`).

Per-law proof status lives in [`spec/INVENTORY.md`](spec/INVENTORY.md).

## What must be preserved

The full 19-law table (with per-law formalization status and line-level source pointers) lives in
[`spec/INVENTORY.md`](spec/INVENTORY.md); it is the canonical catalog and is not repeated here.

**Proven vs. axiomatized:** laws 1–3, 7–11, 12–13, 15–18 and the merge part of 19 are provable
algebraic/combinatorial statements (targets for the proof assistants). Cryptographic primitives
(Blake2b, secp256k1, Curve25519) are **axiomatized** — modeled as abstract interfaces whose required
properties are postulated, not proven. Liveness (eventual finality) is an open question, not an
inductive invariant.

## Layer map

- **Rholang** (`rholang/`, `models/`) — the ρ-calculus interpreter (canonical order, substitution,
  reduction, spatial matching).
- **RSpace** (`rspace/`) — the concurrent tuple space (join commutativity, deterministic COMM, merge
  monoid, Merkle radix trie, replay).
- **Rosette** (`rosette/`, `roscala/`) — the C++ actor VM (actor atomicity, reflection, fork-join).
- **Casper** (`casper/`, `block-storage/`, `sdk/`) — CBC-Casper consensus + DAG (>2/3 finality,
  fringe/estimator, block validation, merge determinism).
- **Crypto** (`crypto/`) — Blake2b256, `Blake2b512Random`, secp256k1, Curve25519.

Per-layer Scala source-of-truth files are listed in
[`docs/src/architecture.md`](docs/src/architecture.md).

## Translation contract (spec → Rust)

For every law in the inventory:

1. **Property test** — a `proptest`/`quickcheck` property asserting the law on the Rust
   implementation (e.g. `sort(sort(p)) == sort(p)`, merge is associative/commutative).
2. **Differential test** — feed identical inputs to the Scala node and the Rust node and compare
   state hashes / results; they must match exactly.
3. **Type-level fidelity** — the Lean/Coq types are the reference for the Rust data model and its
   `Ord`/`Hash`/`Eq` derivations. In particular `sort` becomes the `Ord` implementation that makes
   `Par` an order-insensitive (canonical) container.
4. **Axiomatized crypto** maps to Rust traits whose implementations are the *only* places the
   primitive may differ in behavior, and are pinned by known-answer test vectors.

## Ground truth

The formalization is validated against — and must agree with — the Scala tests that already encode
the laws:

- `rholang/src/test/scala/coop/rchain/rholang/interpreter/{ReduceSpec,ReplaySpec}.scala`
- `models/src/test/scala/coop/rchain/models/rholang/SortTest.scala`
- `node/src/test/scala/coop/rchain/node/mergeablity/MergeabilityRules.scala`
- `casper/src/test/scala/coop/rchain/casper/batch1/MultiParentCasperReportingSpec.scala`

These are the oracle. If the formalization and these tests disagree, reconcile them before writing Rust.

## Module scoping & rewrite order

The rewrite order is dependency-driven and easiest-first; it deliberately differs from the
*formalization* phases above (which rank by invariant value). Both run in parallel: laws are proven
in phase order while code is ported in dependency order. The per-module LOC/difficulty/person-day
ratings, the full bottom-up order, and the workspace layout live in
[`docs/src/architecture.md`](docs/src/architecture.md).

Bottom-up order: `sdk` (and `regex`, in parallel) → `shared` → `crypto` + `graphz` → `models` →
`block-storage` + `rspace` + `comm` → `rholang` → `casper` → `node` → `rspace-bench`. **Defer**
`roscala`/`rosette`.

### Findings

- **`rosette`/`roscala` are orphaned** (absent from `build.sbt`, imported by nothing) — deferred.
- **Hoist `Blake2b256Hash`** out of `rspace` into `crypto`/`shared` so `models` stops depending on
  `rspace` (`models/.../ByteStringSyntax.scala`, `FringeData.scala`, `BlockMetadata.scala`).

## Open questions

1. `casper/src/main/resources/casper.tla` models only the genesis **bootstrap ceremony**, not the
   finality rule — the formal finality spec must be reconstructed from `Finalizer.scala` and
   `MessageMapSyntax.scala`.
2. The `faultTolerance` field asserted in `integration-tests/test/test_dag_correctness.py` is not
   computed anywhere in this tree — its formula must be recovered or declared an open question.
3. **Rosette scope** — two independent decisions: (a) **formalization**: the Rosette VM is **in
   scope** (Laws 12–13, actor atomicity + reflection, a later proof phase); (b) **rewrite**:
   `rosette`/`roscala` are **deferred** — orphaned (absent from `build.sbt`, imported by nothing).

## Status

- **Phase 0 — complete**: Lean 4 skeleton (`spec/`), Coq skeleton (`spec/coq/`), the 19-law
  inventory, and this document.
- **Rewrite — in progress**: the leaf modules (`sdk`, `shared`, `crypto`, `graphz`, `models`,
  `block-storage`, `rspace`, `comm`) are ported at the workspace root; `rholang` is in progress.
  The proofs-first *pause* has been lifted in practice — `rspace`/`rholang` are being ported against
  the verified spec rather than waiting on Laws 1–11.
- **Formalization — proofs-first**: Laws 1–11 (rholang + rspace) are being proven machine-checkably —
  Coq for the ρ-calculus PL metatheory (Laws 2–6), Lean 4 for order/algebra (Laws 1, 7–11) — before
  `rspace`/`rholang` are ported.
