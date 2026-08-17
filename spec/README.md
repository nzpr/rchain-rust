# RChain formal specification (Phase 0)

A machine-checked specification of the mathematical invariants that govern the RChain node, written
as a [Lean 4](https://lean-lang.org/) formalization. It is the source of truth the Rust rewrite is
written against: every law here maps to a Rust property/differential test in later phases.

## Why

The full motivation — memory safety and the calculus-native expression of the node (λ → π → ρ →
Calculus of Constructions) — is in [`../docs/src/why-rust.md`](../docs/src/why-rust.md). The node
(Scala + a C++ Rosette VM) is broadly sound, so the rewrite is not a correctness repair. This spec's
job is therefore **preservation under translation** — pin down the invariants so the port cannot
silently drop them.

## Building

```sh
cd spec
lake build        # compiles the formalization
```

Phase 0 is dependency-free (no Mathlib) so it builds offline and fast. Mathlib is added in Phase 1
when proofs need `Multiset`/`Finset`/`Order`.

## Layout

```
spec/
  Rchain.lean          root module (imports everything)
  Rchain/
    Syntax.lean        Ground / Var / Proc — the core Rholang ADT (de Bruijn levels)
    Sort.lean          canonicalization `sort` + Law 1 theorem statements
    RSpace/            (Phase 2) join commutativity, deterministic COMM, merge monoid, Merkle
    Rosette/           (Phase 3) actor atomicity, reflection, fork-join barrier
    Casper/            (Phase 4) >2/3 finality, fringe, block validation invariants
    Crypto/            (Phase 5) RNG merge + abstract crypto axioms
  INVENTORY.md         the 19-law catalog: source-of-truth → theorem → Rust test
```

## Proven vs stated

- **Phase 0** stands up the syntax, proves the leaf laws (the `Ground`/`Var` comparators, the
  `sortList` canonicality lemmas, `StrCong` equivalence, and the closedness theorems in `Rchain.Ty`),
  and **proves Law 1** (`sortPar_idempotent`, `sortPar_comm`). The single residual is the lawfulness
  of the 23-function structural comparator family (`cmpPar`/`cmpSend`/…/`cmpListParPair`), declared as
  69 `cmpX_eq_iff`/`cmpX_swap`/`cmpX_lt_trans` axioms in `Rchain/Sort.lean` (see `INVENTORY.md`).
- **Phase 1–5** prove each law in `INVENTORY.md`.
- **Axiomatized** (never proven, by design): cryptographic primitives (Blake2b, secp256k1,
  Curve25519) are modeled as abstract interfaces whose required properties are *postulated*. Proving
  real crypto is out of scope. The algebraic/combinatorial laws (sort idempotence, merge monoid,
  De Bruijn substitution, join commutativity, Merkle structure) are provable and are the target.

## How laws drive the Rust port

| Spec artifact | Rust counterpart |
|---------------|------------------|
| `Sort.sort` (canonical form) | the `Ord`/`Hash` impl that makes `Par` an order-insensitive container |
| a proven law `L` | a `proptest`/`quickcheck` property asserting `L`, plus a differential test feeding identical inputs to the Scala node and comparing state hashes |
| `Syntax.Proc` (the ADT) | reference for the Rust data model and its `Eq`/`Hash` derivations |
| axiomatized crypto interfaces | Rust traits pinned by known-answer vectors |

## Ground truth

The laws are validated against — not replacing — the Scala tests that already encode them:

- `rholang/src/test/scala/coop/rchain/rholang/interpreter/{ReduceSpec,ReplaySpec}.scala`
- `models/src/test/scala/coop/rchain/models/rholang/SortTest.scala`
- `node/src/test/scala/coop/rchain/node/mergeablity/MergeabilityRules.scala`
- `casper/src/test/scala/coop/rchain/casper/batch1/MultiParentCasperReportingSpec.scala`
