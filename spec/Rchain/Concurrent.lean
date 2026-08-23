import Rchain.Rho

/-!
# The concurrency model (parallel reduction)

Specifies the *parallel* reduction `⟹` and the soundness theorems of the concurrency model
(`docs/src/formal/concurrency-model.md`). The sequential `Reduce` and structural congruence `StrCong`
are in `Rchain.Rho`.

**Proven here:**

- `parStep_comm` — two independent redexes (one on each side of `parMerge`) commute.
- `parStep_to_reduce` — linearization: every parallel step is a finite sequence of sequential
  `Reduce` steps.
- `List.append_eq_singleton`, `parMerge_eq_nilPar`, `sendPar_eq_parMerge`, `receivePar_eq_parMerge` —
  the field-wise decomposition of the flat `Par` under `parMerge`.
- `reduce_nilPar_impossible`, `reduce_sendPar_impossible`, `reduce_receivePar_impossible` — inertness:
  a nil/send-only/receive-only process cannot reduce (the `comm`-vs-`parLeft` cases are vacuous).

**Open** (the targets):

- `parStep_diamond` — confluence of `⟹` up to `StrCong`.
- `reduce_confluent` — confluence of `Reduce` up to `StrCong` (the corrected Law-4 clause; single-step
  determinism is false — two independent COMM redexes reduce to non-`≡` results).

The remaining step for confluence is the **COMM-redex decomposition** (`parMerge p q = parMerge
(sendPar c [d]) (receivePar c b)` ⟹ the summands are `nilPar`/`sendPar`/`receivePar`/the redex), which
combines the already-proven inertness and decomposition lemmas.
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

/-- `p ++ q = [s]` splits as `[s] ++ []` or `[] ++ [s]`. -/
lemma List.append_eq_singleton {α : Type} {p q : List α} {s : α} (h : p ++ q = [s]) :
    (p = [s] ∧ q = []) ∨ (p = [] ∧ q = [s]) := by
  cases p with
  | nil => simp at h; exact Or.inr ⟨rfl, h⟩
  | cons a ps =>
      cases ps with
      | nil =>
          cases q with
          | nil => exact Or.inl ⟨by simpa using h, rfl⟩
          | cons b qs => cases h
      | cons b pss => cases h

/-- `parMerge p q = nilPar` forces both summands to `nilPar`. -/
lemma parMerge_eq_nilPar {p q : Par} (h : parMerge p q = nilPar) : p = nilPar ∧ q = nilPar := by
  cases p <;> cases q <;> simp [parMerge, nilPar] at h ⊢
  · simp_all [nilPar]

/-- `sendPar chan data = parMerge p q` splits as `sendPar | nilPar` (or `nilPar | sendPar`). -/
lemma sendPar_eq_parMerge {chan : Par} {data : List Par} {p q : Par}
    (h : sendPar chan data = parMerge p q) :
    (p = sendPar chan data ∧ q = nilPar) ∨ (p = nilPar ∧ q = sendPar chan data) := by
  cases p <;> cases q <;> simp [sendPar, parMerge] at h
  rcases List.append_eq_singleton h.1.symm with ⟨hps, hqs⟩ | ⟨hps, hqs⟩
  · left; subst hps; subst hqs; simp_all [sendPar, nilPar, List.append_eq_nil]
  · right; subst hps; subst hqs; simp_all [sendPar, nilPar, List.append_eq_nil]

/-- `receivePar chan body = parMerge p q` splits as `receivePar | nilPar` (or `nilPar | receivePar`). -/
lemma receivePar_eq_parMerge {chan body : Par} {p q : Par}
    (h : receivePar chan body = parMerge p q) :
    (p = receivePar chan body ∧ q = nilPar) ∨ (p = nilPar ∧ q = receivePar chan body) := by
  cases p <;> cases q <;> simp [receivePar, parMerge] at h
  rcases List.append_eq_singleton h.2.1.symm with ⟨hpr, hqr⟩ | ⟨hpr, hqr⟩
  · left; subst hpr; subst hqr; simp_all [receivePar, nilPar, List.append_eq_nil]
  · right; subst hpr; subst hqr; simp_all [receivePar, nilPar, List.append_eq_nil]

/-- `nilPar` cannot reduce. -/
lemma reduce_nilPar_impossible {r : Par} (h : Reduce nilPar r) : False :=
  reduce_nilPar_impossible_aux h rfl
where
  reduce_nilPar_impossible_aux : ∀ {p r : Par}, Reduce p r → p = nilPar → False := by
    intro p r h hp
    induction h with
    | comm c d b =>
        simp [parMerge, sendPar, receivePar, nilPar] at hp
    | parLeft hp1 ih =>
        rcases parMerge_eq_nilPar hp with ⟨hp_eq, _⟩
        subst hp_eq
        exact ih rfl
    | parRight hq1 ih =>
        rcases parMerge_eq_nilPar hp with ⟨_, hq_eq⟩
        subst hq_eq
        exact ih rfl

/-- A send-only process cannot reduce. -/
lemma reduce_sendPar_impossible {chan : Par} {data : List Par} {r : Par}
    (h : Reduce (sendPar chan data) r) : False :=
  reduce_sendPar_impossible_aux h rfl
where
  reduce_sendPar_impossible_aux : ∀ {p r : Par}, Reduce p r → p = sendPar chan data → False := by
    intro p r h hp
    induction h with
    | comm c d b =>
        simp [parMerge, sendPar, receivePar] at hp
    | parLeft hp1 ih =>
        rcases sendPar_eq_parMerge hp.symm with ⟨hp_eq, hq_eq⟩ | ⟨hp_eq, hq_eq⟩
        · subst hp_eq; subst hq_eq; exact ih rfl
        · subst hp_eq; subst hq_eq; exact reduce_nilPar_impossible hp1
    | parRight hq1 ih =>
        rcases sendPar_eq_parMerge hp.symm with ⟨hp_eq, hq_eq⟩ | ⟨hp_eq, hq_eq⟩
        · subst hp_eq; subst hq_eq; exact reduce_nilPar_impossible hq1
        · subst hp_eq; subst hq_eq; exact ih rfl

/-- A receive-only process cannot reduce. -/
lemma reduce_receivePar_impossible {chan body r : Par}
    (h : Reduce (receivePar chan body) r) : False :=
  reduce_receivePar_impossible_aux h rfl
where
  reduce_receivePar_impossible_aux : ∀ {p r : Par}, Reduce p r → p = receivePar chan body → False := by
    intro p r h hp
    induction h with
    | comm c d b =>
        simp [parMerge, sendPar, receivePar] at hp
    | parLeft hp1 ih =>
        rcases receivePar_eq_parMerge hp.symm with ⟨hp_eq, hq_eq⟩ | ⟨hp_eq, hq_eq⟩
        · subst hp_eq; subst hq_eq; exact ih rfl
        · subst hp_eq; subst hq_eq; exact reduce_nilPar_impossible hp1
    | parRight hq1 ih =>
        rcases receivePar_eq_parMerge hp.symm with ⟨hp_eq, hq_eq⟩ | ⟨hp_eq, hq_eq⟩
        · subst hp_eq; subst hq_eq; exact reduce_nilPar_impossible hq1
        · subst hp_eq; subst hq_eq; exact ih rfl

/-- Confluence of parallel reduction up to structural congruence. -/
theorem parStep_diamond {p q r : Par} (hpq : ParStep p q) (hpr : ParStep p r) :
    ∃ s t, ParStep q s ∧ ParStep r t ∧ StrCong s t := by
  sorry

/-- Law 4, correctly stated: *confluence* up to structural congruence. Two single-step reductions
    converge to `StrCong`-equivalent reducts.

    Note: single-step determinism `Reduce p q → Reduce p q' → StrCong q q'` is **false** — two
    independent COMM redexes in a `parMerge` reduce to non-`≡`-equivalent results (reducing the left
    vs the right redex). Confluence (a common reduct) is the correct invariant. -/
theorem reduce_confluent {p q r : Par} (hpq : Reduce p q) (hpr : Reduce p r) :
    ∃ s t, Relation.ReflTransGen Reduce q s ∧ Relation.ReflTransGen Reduce r t ∧ StrCong s t := by
  sorry

end Rchain
