# RChain formal specification — Coq track (Phase 0)

The Coq formalization, parallel to the Lean 4 track in [`../`](../). Coq is the home of the
**substitution / α-equivalence / programming-language metatheory** (capture-avoiding de Bruijn
substitution, α-equivalence), while Lean 4 covers the algebraic/order laws. The two tracks define the
same core `Proc` syntax and the same canonicalization `sort`, so their Phase-0 skeletons are
structurally identical.

## Building

Requires **Coq 8.x** (the files use only the standard library — `Arith`, `ZArith`, `String`, `List`;
no `Autosubst`/`ssreflect` yet).

```sh
make                 # = coq_makefile -f _CoqProject -o CoqMakefile && make -f CoqMakefile
```

Install Coq if needed: `opam install coq` (or `apt install coq`).

## Status

Phase 0 skeleton: `Syntax.v` defines `Ground`/`Var`/`Proc`; `Sort.v` defines the score-tree total
order and the canonicalization `sort`. The atomic fixed-point lemmas (`sort_nil`, `sort_ground`,
`sort_var`) are proven; the deep Law-1 theorems (`sort_idempotent`, `sort_par_comm`) are **admitted**
(Phase 1 proof obligations), mirroring the `sorry`-admitted theorems in [`../Rchain/Sort.lean`](../Rchain/Sort.lean).

## Mapping to the Scala source of truth

| Coq | Scala |
|-----|-------|
| `score` / `cmpProc` / `parPair` | `models/.../rholang/sorter/ScoreTree.scala`, `ordering.scala` |
| `sort` | `models/.../rholang/sorter/ordering.scala` |
| `Proc` (binary Phase-0 form) | `models/src/main/protobuf/RhoTypes.proto` (`Par`/`Send`/`Receive`/`New`/`Match`) |

See [`../INVENTORY.md`](../INVENTORY.md) for the full 19-law catalog.
