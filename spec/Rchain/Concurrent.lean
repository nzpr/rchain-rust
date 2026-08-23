import Rchain.Rho

/-!
# The concurrency model (parallel reduction)

Specifies the *parallel* reduction `⟹` and the soundness theorems of the concurrency model
(`docs/src/formal/concurrency-model.md`). The sequential `Reduce` and structural congruence `StrCong`
are in `Rchain.Rho`.

**Proven here** (the foundation):

- `parStep_comm` — two independent redexes (one on each side of `parMerge`) commute; their common
  reduct is the merge of the two reducts.
- `parStep_to_reduce` — linearization: every parallel step is a finite sequence of sequential
  `Reduce` steps (so a concurrent execution is a valid refinement of the sequential one).

**Open** (the target):

- `parStep_diamond` — confluence of `⟹`: two parallel steps from `p` converge to a common reduct.
  The `comm`-vs-`comm` and `comm`-vs-`par` cases need the injectivity of `sendPar`/`receivePar`/
  `parMerge` (a COMM redex has a unique reduct).

Together with `StrCong`, `parStep_diamond` discharges the determinism clause of Law 4
(`reduce_deterministic` in `Rchain.Reduce`) and the "concurrent == sequential" invariant.
-/

namespace Rchain

/-- A parallel step `⟹`: reduce a finite number of pairwise-independent redexes at once. `refl`
    reduces nothing, `comm` contracts a COMM redex, and `par` reduces both sides of `parMerge`. -/
inductive ParStep : Par → Par → Prop where
  | refl : ParStep p p
  | comm (chan data body : Par) :
      ParStep (parMerge (sendPar chan [data]) (receivePar chan body)) body
  | par {p q p' q' : Par} : ParStep p p' → ParStep q q' →
      ParStep (parMerge p q) (parMerge p' q')

/-- Two independent redexes, one on each side of `parMerge`, commute: their common reduct is the
    merge of the two reducts. The foundation of the diamond property. -/
theorem parStep_comm {p p' q q' : Par} (hp : Reduce p p') (hq : Reduce q q') :
    Reduce (parMerge p' q) (parMerge p' q') ∧ Reduce (parMerge p q') (parMerge p' q') :=
  ⟨Reduce.parRight hq, Reduce.parLeft hp⟩

/-- Lift `Reduce*` under the left side of `parMerge`. -/
lemma reflTransGen_parLeft {p p' q : Par} (h : Relation.ReflTransGen Reduce p p') :
    Relation.ReflTransGen Reduce (parMerge p q) (parMerge p' q) := by
  induction h with
  | refl => exact Relation.ReflTransGen.refl
  | tail _ hstep ih =>
      exact Relation.ReflTransGen.tail ih (Reduce.parLeft hstep)

/-- Lift `Reduce*` under the right side of `parMerge`. -/
lemma reflTransGen_parRight {p q q' : Par} (h : Relation.ReflTransGen Reduce q q') :
    Relation.ReflTransGen Reduce (parMerge p q) (parMerge p q') := by
  induction h with
  | refl => exact Relation.ReflTransGen.refl
  | tail _ hstep ih =>
      exact Relation.ReflTransGen.tail ih (Reduce.parRight hstep)

/-- Linearization: every parallel step is a finite sequence of sequential `Reduce` steps. -/
theorem parStep_to_reduce {p q : Par} (h : ParStep p q) :
    Relation.ReflTransGen Reduce p q := by
  induction h with
  | refl => exact Relation.ReflTransGen.refl
  | comm chan data body =>
      exact Relation.ReflTransGen.tail Relation.ReflTransGen.refl (Reduce.comm chan data body)
  | par hp hq ihp ihq =>
      exact Relation.ReflTransGen.trans (reflTransGen_parLeft ihp) (reflTransGen_parRight ihq)

/-- Diamond / confluence of parallel reduction: two parallel steps from `p` converge to a common
    `ParStep`-reduct. (Target — the `comm`-vs-`comm` and `comm`-vs-`par` cases need the injectivity of
    `sendPar`/`receivePar`/`parMerge`.) -/
theorem parStep_diamond {p q r : Par} (hpq : ParStep p q) (hpr : ParStep p r) :
    ∃ s, ParStep q s ∧ ParStep r s := by
  sorry

end Rchain
