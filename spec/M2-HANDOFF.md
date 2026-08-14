# M2 gate — handoff note

Status as of the "ship the green core" checkpoint. Both `lake build` (Lean 4.12.0 + Mathlib) and
`make -C spec/coq` (Coq 8.18) pass. What is **proven** vs. **admitted** is recorded precisely below
so the remaining proof obligations can be picked up cleanly.

## Proven (no `sorry`, no residual axioms)

- **Flat `Par` ADT** — `spec/Rchain/Par.lean` (Lean) and `spec/coq/Syntax.v` (Coq) define the
  identical flat `Par` record (8 `list` fields) + `Send`/`Receive`/`ReceiveBind`/`New`/`MatchCase`/
  `Match`/`Expr`/`Bundle`/`GUnforgeable`/`Connective`. `GStr` is `list N`/`List Nat` code points in
  both tracks (this is what eliminated the Phase-0 String-comparison axiom).
- **Generic comparator framework** — `spec/Rchain/Cmp.lean`: `Comparator` (the total-order bundle),
  `lex`, `cmpListF`/`cmpPairF` (bare), `listComparator`, `cmpPair`, `linearOrderComparator`, and the
  canonical `sortList` with `sortList_idempotent` / `sortList_perm` / `sortList_append_comm` /
  `sortList_map_idempotent`. All proven.
- **Leaf comparators** — `groundComparator`, `varComparator` (over `Ground`/`Var`) are fully proven
  (`eq_iff`, `swap`, `lt_trans`) using Mathlib's `linearOrderComparator` on `Bool`/`Int`/`Nat` and
  the code-point list order.
- **Comparator definitions** — the 23-function direct mutual recursion (`cmpPar`, `cmpSend`, …,
  `cmpListParPair`) in `spec/Rchain/Sort.lean` compiles (termination via `sizeOf`, `maxHeartbeats
  2000000`).

## Admitted (residual axioms, each with a `RESIDUAL AXIOMS` block)

`spec/Rchain/Sort.lean` declares, as `axiom`, the total-order bundle for the **flat `Par` family**:

- `cmpX_eq_iff` × 23 (the `eq_iff` laws) — *note: these were proven once by mutual induction, but a
  Lean `termination_by`-vs-`cases` quirk in the mutual block forced re-declaration as axioms.*
- `cmpX_swap` × 23.
- `cmpX_lt_trans` × 23.
- `sortPar`/`sortSend`/…/`sortConnective` × 11 (the canonical sort; the `List.map`-based recursion
  does not satisfy `termination_by sizeOf`).
- `sortPar_idempotent` / `sortPar_comm` × 2 (Law 1 itself).

`spec/coq/Sort.v` declares `cmpPar`, `sortPar`, `sortPar_idempotent`, `sortPar_comm` as `Axiom`
(the flat well-founded mutual recursion is not yet ported).

`#print axioms parComparator` reports: `cmpPar_eq_iff`, `cmpPar_lt_trans`, `cmpPar_swap` (plus the
two always-present core axioms `propext`, `Quot.sound`).

## The two concrete blockers (for whoever picks this up)

1. **`swap`/`lt_trans` mutual induction.** The definitions use a *direct* 23-function mutual block
   (each `List` field has its own comparator) so `termination_by sizeOf` succeeds. The law proofs
   hit two distinct Lean issues:
   - `cmpPar_swap`: `cases p <;> cases q` followed by a *separate* `simp only [cmpPar]` line makes
     `termination_by p q => sizeOf p + sizeOf q` report "body binds 0 parameters"; and the 7-fold
     `rw [swap_lex, …]` times out at 2M heartbeats.
   - `lt_trans`: `lex_lt_trans`'s `hD : Dcmp x y = .lt → …` (implicit `Dcmp`, `x y z`) cannot be
     inferred for a *nested* lexicographic rest; the fix is explicit "tail" comparators
     (`cmpParTail2..7`, `cmpSendTail`, …) whose `lt_trans` applies `hD` *directly* (e.g.
     `hD := cmpParTail2_lt_trans (r, (n, …)) …`). The tails were drafted; the last snag was
     `cases p with | mk …` rejecting the alternative name `mk` (use `cases p <;> cases q <;> cases r`
     + `rename_i` instead).

2. **`sort` termination.** `sortList sendComparator (s.map sortSend)` hides the list membership from
   the termination checker. Either define per-field `mapSortX`/`sortListX` direct mutual helpers
   (as was done for the comparators), or prove termination with a custom `decreasing_by`.

## Coq port guidance

The Coq side needs the same well-founded mutual recursion. Use `Program Fixpoint` over a sum type
`ProcSortable` with a `measure` (the plan's "`Program Fixpoint` + measure `parSize p + parSize q`"),
with `cmpList`/`cmpPair` as structural `Fixpoint`s taking the element comparator as a parameter.
The Phase-0 binary Law 1 (`sort_idempotent`/`sort_par_comm` over the binary `Proc`) is committed in
git history as a regression anchor.
