import Rchain.Rho

/-!
# The tree model (explicit `par` nodes)

The flat `Par` (in `Rchain.Par`) erases the tree structure of parallel composition: `parMerge` is a
field-wise monoid, so `sendPar c [d₁] | receivePar c b | sendPar c [d₂]` is a redex in two ways, and
`Reduce` is **not** confluent (see `Rchain.Concurrent.reduce_not_deterministic`).

This module restores the tree structure: a process is a *tree* with explicit `par` nodes. Because
`par` is now a constructor (injective), a redex has a **unique** decomposition, and reduction is
**confluent up to `StrCongT`** — `reduceT_confluent` below. The flat `Par` is the field-wise
flattening of this tree (`flatten`), and `flatten` maps tree reduction/congruence soundly onto the
flat `Reduce`/`StrCong` (`flatten_reduce`, `flatten_strCong`).

**Proven here:**

- `reduceT_confluent` — confluence of tree reduction up to `StrCongT` (the diamond).
- `reduceT_redex_unique` — an isolated tree redex reduces uniquely up to `StrCongT`.
- `flatten_reduce`, `flatten_strCong` — the tree model refines the flat model.
- `reduceT_{send,receive,nil,leaf}_impossible` — tree inertness (trivial: `par` is injective).
-/
namespace Rchain

/-- The tree process grammar. `par` is an explicit, *injective* node; `send`/`receive` are the two
    halves of a COMM redex (the receive body is itself a tree); `leaf` carries any other flat `Par`
    (new, match, expr, unforgeable, bundle, connective, non-singleton sends/receives) as an inert
    leaf. -/
inductive Proc where
  | nil : Proc
  | par : Proc → Proc → Proc
  | send : Par → Par → Proc
  | receive : Par → Proc → Proc
  | leaf : Par → Proc

/-- Flatten a tree `Proc` to the flat `Par` (`par ↦ parMerge`). -/
def flatten : Proc → Par
  | Proc.nil => nilPar
  | Proc.par p q => parMerge (flatten p) (flatten q)
  | Proc.send c d => sendPar c [d]
  | Proc.receive c body => receivePar c (flatten body)
  | Proc.leaf p => p

/-- Structural congruence on the tree (`par` comm/assoc/ident + congruence). -/
inductive StrCongT : Proc → Proc → Prop where
  | refl  : ∀ p, StrCongT p p
  | symm  : ∀ {p q}, StrCongT p q → StrCongT q p
  | trans : ∀ {p q r}, StrCongT p q → StrCongT q r → StrCongT p r
  | comm  : ∀ p q, StrCongT (Proc.par p q) (Proc.par q p)
  | assoc : ∀ p q r, StrCongT (Proc.par (Proc.par p q) r) (Proc.par p (Proc.par q r))
  | ident : ∀ p, StrCongT (Proc.par p Proc.nil) p
  | par   : ∀ {p p' q q'}, StrCongT p p' → StrCongT q q' → StrCongT (Proc.par p q) (Proc.par p' q')

/-- Tree reduction: COMM contracts `send | receive`; a congruence under `par`. -/
inductive ReduceT : Proc → Proc → Prop where
  | comm (chan data : Par) (body : Proc) :
      ReduceT (Proc.par (Proc.send chan data) (Proc.receive chan body)) body
  | parLeft  {p p' q : Proc} : ReduceT p p' → ReduceT (Proc.par p q) (Proc.par p' q)
  | parRight {p q q' : Proc} : ReduceT q q' → ReduceT (Proc.par p q) (Proc.par p q')

/-! ## Inertness (trivial because `par` is a constructor) -/

lemma reduceT_send_impossible {chan data : Par} {r : Proc}
    (h : ReduceT (Proc.send chan data) r) : False := by
  cases h

lemma reduceT_receive_impossible {chan : Par} {body r : Proc}
    (h : ReduceT (Proc.receive chan body) r) : False := by
  cases h

lemma reduceT_nil_impossible {r : Proc} (h : ReduceT Proc.nil r) : False := by
  cases h

lemma reduceT_leaf_impossible {p : Par} {r : Proc} (h : ReduceT (Proc.leaf p) r) : False := by
  cases h

/-! ## Redex decomposition and uniqueness -/

/-- `Proc.par p q = Proc.par (send chan data) (receive chan body)` splits uniquely (`par` is
    injective). -/
lemma par_eq_send_receive {p q : Proc} {chan data : Par} {body : Proc}
    (h : Proc.par p q = Proc.par (Proc.send chan data) (Proc.receive chan body)) :
    p = Proc.send chan data ∧ q = Proc.receive chan body := by
  injection h with hp hq
  exact ⟨hp, hq⟩

/-- An isolated tree redex has a unique reduct up to `StrCongT`. -/
lemma reduceT_redex_unique {chan data : Par} {body q' : Proc}
    (h : ReduceT (Proc.par (Proc.send chan data) (Proc.receive chan body)) q') :
    StrCongT q' body :=
  reduceT_redex_unique_aux h rfl
where
  reduceT_redex_unique_aux : ∀ {p q' : Proc},
      ReduceT p q' → p = Proc.par (Proc.send chan data) (Proc.receive chan body) → StrCongT q' body := by
    intro p q' h hp
    induction h with
    | comm chan' data' body' =>
        have hrecv : Proc.receive chan' body' = Proc.receive chan body := by
          injection hp
        have hbody : body' = body := by
          injection hrecv
        rw [hbody]
        exact StrCongT.refl body
    | @parLeft p1 p1' p2 hpl ih =>
        rcases par_eq_send_receive hp with ⟨hp1, hp2⟩
        subst hp1
        exact False.elim (reduceT_send_impossible hpl)
    | @parRight p1 p2 p2' hpr ih =>
        rcases par_eq_send_receive hp with ⟨hp1, hp2⟩
        subst hp2
        exact False.elim (reduceT_receive_impossible hpr)

/-! ## Lifting `ReduceT*` under `par` -/

lemma reflTransGenT_parLeft {p p' q : Proc} (h : Relation.ReflTransGen ReduceT p p') :
    Relation.ReflTransGen ReduceT (Proc.par p q) (Proc.par p' q) := by
  induction h with
  | refl => exact Relation.ReflTransGen.refl
  | tail _ hstep ih => exact Relation.ReflTransGen.tail ih (ReduceT.parLeft hstep)

lemma reflTransGenT_parRight {p q q' : Proc} (h : Relation.ReflTransGen ReduceT q q') :
    Relation.ReflTransGen ReduceT (Proc.par p q) (Proc.par p q') := by
  induction h with
  | refl => exact Relation.ReflTransGen.refl
  | tail _ hstep ih => exact Relation.ReflTransGen.tail ih (ReduceT.parRight hstep)

/-! ## The tree model refines the flat model -/

/-- `flatten` maps tree reduction to flat reduction. -/
lemma flatten_reduce {p q : Proc} (h : ReduceT p q) : Reduce (flatten p) (flatten q) := by
  induction h with
  | comm chan data body => exact Reduce.comm chan data (flatten body)
  | parLeft hpl ih => exact Reduce.parLeft ih
  | parRight hpr ih => exact Reduce.parRight ih

/-- `flatten` maps tree structural congruence to flat structural congruence. -/
lemma flatten_strCong {p q : Proc} (h : StrCongT p q) : StrCong (flatten p) (flatten q) := by
  induction h with
  | refl p => exact StrCong.refl _
  | symm hp ih => exact StrCong.symm ih
  | trans hp hq ihp ihq => exact StrCong.trans ihp ihq
  | comm p q => exact StrCong.comm (flatten p) (flatten q)
  | assoc p q r => exact StrCong.assoc (flatten p) (flatten q) (flatten r)
  | ident p => exact StrCong.ident (flatten p)
  | par hp hq ihp ihq => exact StrCong.par ihp ihq

/-! ## Confluence (the diamond) -/

/-- **Confluence of tree reduction up to `StrCongT`.** Two single-step reductions of the same tree
    converge to `StrCongT`-equivalent reducts. The proof is a clean induction: `comm` is deterministic
    (the `parLeft`/`parRight` sub-cases are vacuous by inertness), `parLeft`/`parRight` on disjoint
    sides commute, and two reductions of the same side use the induction hypothesis. -/
theorem reduceT_confluent {p q r : Proc} (hpq : ReduceT p q) (hpr : ReduceT p r) :
    ∃ s t, Relation.ReflTransGen ReduceT q s ∧ Relation.ReflTransGen ReduceT r t ∧ StrCongT s t := by
  induction hpq generalizing r with
  | comm chan data body =>
      exact ⟨body, r, Relation.ReflTransGen.refl, Relation.ReflTransGen.refl,
             StrCongT.symm (reduceT_redex_unique hpr)⟩
  | @parLeft p1 p1' p2 hpl ih =>
      generalize hp : Proc.par p1 p2 = P at hpr
      cases hpr with
      | comm c d body =>
          rcases par_eq_send_receive hp with ⟨hp1, hp2⟩
          subst hp1
          exact False.elim (reduceT_send_impossible hpl)
      | @parLeft a a' b hpr1 =>
          injection hp with hp1_eq hp2_eq
          subst a
          subst b
          rcases ih hpr1 with ⟨s1, t1, hs1, ht1, hcong⟩
          exact ⟨Proc.par s1 p2, Proc.par t1 p2,
                 reflTransGenT_parLeft hs1, reflTransGenT_parLeft ht1,
                 StrCongT.par hcong (StrCongT.refl p2)⟩
      | @parRight a b b' hpr1 =>
          injection hp with hp1_eq hp2_eq
          subst a
          subst b
          exact ⟨Proc.par p1' b', Proc.par p1' b',
                 Relation.ReflTransGen.tail Relation.ReflTransGen.refl (ReduceT.parRight hpr1),
                 Relation.ReflTransGen.tail Relation.ReflTransGen.refl (ReduceT.parLeft hpl),
                 StrCongT.refl _⟩
  | @parRight p1 p2 p2' hpr1 ih =>
      generalize hp : Proc.par p1 p2 = P at hpr
      cases hpr with
      | comm c d body =>
          rcases par_eq_send_receive hp with ⟨hp1, hp2⟩
          subst hp2
          exact False.elim (reduceT_receive_impossible hpr1)
      | @parLeft a a' b hpl2 =>
          injection hp with hp1_eq hp2_eq
          subst a
          subst b
          exact ⟨Proc.par a' p2', Proc.par a' p2',
                 Relation.ReflTransGen.tail Relation.ReflTransGen.refl (ReduceT.parLeft hpl2),
                 Relation.ReflTransGen.tail Relation.ReflTransGen.refl (ReduceT.parRight hpr1),
                 StrCongT.refl _⟩
      | @parRight a b b' hpl2 =>
          injection hp with hp1_eq hp2_eq
          subst a
          subst b
          rcases ih hpl2 with ⟨s1, t1, hs1, ht1, hcong⟩
          exact ⟨Proc.par p1 s1, Proc.par p1 t1,
                 reflTransGenT_parRight hs1, reflTransGenT_parRight ht1,
                 StrCongT.par (StrCongT.refl p1) hcong⟩

end Rchain
