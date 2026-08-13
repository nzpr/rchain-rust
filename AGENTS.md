# AGENTS.md — RChain → Rust rewrite: intent & formal specification

This file is the **authoritative intent + formal specification** for rewriting this node in Rust. It
is written for both AI coding agents and humans: read it in full before writing or changing any Rust
code, and treat its statements as binding constraints, not suggestions.

## Intent

The RChain node runs Rholang natively. We are rewriting it in **Rust**, absorbing both the Scala/JVM
code and the C++ Rosette VM. The motivation is **memory safety and memory bloat** — GC overhead,
boxing, and JVM heap pressure — **not** a correctness repair: the existing logic is broadly sound.

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

The Lean 4 and Coq tracks are deliberately parallel: both define the same core `Proc` syntax and the
same canonicalization (`sort`) as Phase 0, and both state Law 1 (`sort` is idempotent and `par`
commutative) as a Phase 1 proof obligation. Coq is the home of the substitution metatheory (capture-
avoiding de Bruijn substitution, α-equivalence); Lean 4 is the home of the algebraic/order laws.

## What must be preserved

The full table (with per-law formalization status and line-level source pointers) lives in
[`spec/INVENTORY.md`](spec/INVENTORY.md). The condensed form:

| # | Layer | Law |
|---|-------|-----|
| 1 | Rholang | `Par`/`ESet`/`EMap` are commutative; canonicalization to a total order is idempotent; `sort(p\|q)=sort(q\|p)` |
| 2 | Rholang | α/name equivalence = par order + `\| Nil` + top-level arithmetic + α + added eval/quote |
| 3 | Rholang | capture-avoiding de Bruijn substitution; `sort(subst t)=subst(sort t)` |
| 4 | Rholang | reduction (comm); first-match-wins; `new` yields fresh unforgeable names |
| 5 | Rholang | spatial matching binds a free variable at most once |
| 6 | Rholang | a program has no globally free variables |
| 7 | RSpace | join commutativity (channel keys hashed in sorted order) |
| 8 | RSpace | deterministic COMM (sorted produce refs; content-addressed events) |
| 9 | RSpace | merge is a monoid; non-conflicting logs commute |
| 10 | RSpace | Merkle determinism (content-addressed radix trie, collision-free) |
| 11 | RSpace | replay determinism (recomputed COMM ⊆ recorded trace) |
| 12 | Rosette | actor atomicity (single-threaded `mbox.nextMsg`) |
| 13 | Rosette | reflection (everything-is-an-`Ob`, meta/parent chain, fork-join barrier) |
| 14 | Casper | finality requires > 2/3 stake; fringe = one message per bonded validator (antichain) |
| 15 | Casper | fringe monotone by height; seen-set monotone (no regression) |
| 16 | Casper | block number = max(parent)+1; seqNum strictly +1; content addressing; bonds cache = PoS state |
| 17 | Casper | merge determinism (unique min-cost rejection); numeric channels non-negative/no-overflow; RNG merge commutative |
| 18 | Storage | height map contiguous; fringe identity order-independent |
| 19 | Crypto | Blake2b256 canonical hash; `Blake2b512Random` associative splittable merge; sig/sign; Curve25519 round-trip |

**Proven vs. axiomatized:** laws 1–3, 7–11, 12–13, 15–18 and the merge part of 19 are provable
algebraic/combinatorial statements (targets for the proof assistants). Cryptographic primitives
(Blake2b, secp256k1, Curve25519) are **axiomatized** — modeled as abstract interfaces whose required
properties are postulated, not proven. Liveness (eventual finality) is an open question, not an
inductive invariant.

## Layer map

- **Rholang** (`rholang/`, `models/`) — the ρ-calculus interpreter. Key invariants: canonical total
  order (`models/.../rholang/sorter/ScoreTree.scala`), capture-avoiding substitution
  (`rholang/.../interpreter/Substitute.scala`), reduction (`Reduce.scala`), spatial matching
  (`interpreter/matcher/SpatialMatcher.scala`). The K-framework semantics under
  `rholang/src/main/k/rholang/` are the (unfinished) reference semantics.
- **RSpace** (`rspace/`) — the concurrent tuple space. Key invariants: join commutativity
  (`hashing/StableHashProvider.scala`), deterministic COMM (`trace/Event.scala`), merge monoid
  (`merger/StateChange.scala`, `merger/EventLogMergingLogic.scala`), Merkle radix trie
  (`history/RadixTree.scala`), replay (`ReplayRSpace.scala`).
- **Rosette** (`rosette/`, `roscala/`) — the C++ actor VM. Key invariants: actor atomicity, reflective
  meta/parent chain, fork-join barrier.
- **Casper** (`casper/`, `block-storage/`, `sdk/`) — CBC-Casper consensus + DAG. Key invariants: >2/3
  finality (`sdk/.../consensus/Stake.scala`), fringe/estimator (`block-storage/.../dag/Finalizer.scala`,
  `MessageMapSyntax.scala`), block validation (`casper/.../Validate.scala`), merge determinism
  (`sdk/.../merging/ConflictResolutionLogic.scala`).
- **Crypto** (`crypto/`) — Blake2b256, `Blake2b512Random`, secp256k1, Curve25519.

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

## Open questions

1. `casper/src/main/resources/casper.tla` models only the genesis **bootstrap ceremony**, not the
   finality rule — the formal finality spec must be reconstructed from `Finalizer.scala` and
   `MessageMapSyntax.scala`.
2. The `faultTolerance` field asserted in `integration-tests/test/test_dag_correctness.py` is not
   computed anywhere in this tree — its formula must be recovered or declared an open question.
3. The Rosette VM is **in scope** but its exact runtime role should be confirmed before Phase 3.

## Status

- **Phase 0 — complete**: Lean 4 skeleton (`spec/`), Coq skeleton (`spec/coq/`), the 19-law
  inventory, and this document.
- **Phase 1 — execution core** (Rholang reduction, substitution, spatial matching, canonicalization):
  highest value, next.
- **Phases 2–5** — RSpace → Rosette → Casper → crypto/storage, in that order.
