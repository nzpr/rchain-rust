import Rchain.Par
import Rchain.Cmp
import Rchain.Rho

set_option maxHeartbeats 1000000

/-!
# The type system over the ρ-calculus (the CoC layer)

Lean 4 *is* a Calculus of Constructions (CIC); this module uses it to type the node, taking the
ρ-calculus (`Rchain.Rho`, `Rchain.Par`) as its base sort. The goal is the **totality invariant**: the
node's own code admits no silent partiality (no `unwrap`/panic); every partial operation is either
proven total on a refinement, or returns `Option`/`Except` at a declared boundary.

Two layers, both here:

1. **The language sorts** — the one real rholang type distinction: a term is used in *process*
   position or *name* position (see `rholang/reference_doc/pattern_matching/rholangmatchingtut.md`).
   `PSort`/`Ctx` give the variable-level judgment; `isPureName`/`classify` give the structural
   name-vs-process classification of a term.
2. **Well-formedness refinements** — `Closed` (no free variables, Law 6), the refinement that makes
   the interpreter's partiality impossible, together with the proof that it is preserved by
   composition (`parMerge`), structural congruence (`≡`), and canonicalization (`sortList`).

The theorems at the bottom are the **fundamentals** the specification document cites.

## Canonical order (Law 1) — the one residual assumption

`Rchain.Sort` *defines* the canonical `sortPar` and *proves* Law 1 (`sortPar_idempotent`,
`sortPar_comm`). The single remaining assumption is the **lawfulness of the structural
comparators** — that `cmpPar`/`cmpSend`/…/`cmpListParPair` form a total order, stated as the 69
`cmpX_eq_iff` / `cmpX_swap` / `cmpX_lt_trans` axioms in `Rchain.Sort`. This is the standard "the
canonical order is total" postulate (every canonicalization needs a lawful order); discharging it is
a separate, later proof obligation (the 23-function mutual induction over the flat `Par` family).
-/

namespace Rchain

/-! ## Language sorts: process vs name -/

/-- The two syntactic sorts: a term in *process* position or *name* position. -/
inductive PSort where
  | proc
  | name
deriving DecidableEq, BEq, Repr

/-- A de Bruijn context: the sort of each level in scope. -/
def Ctx := List PSort

/-- The sort of a variable occurrence: a bound level is looked up in the context; a free variable or
    wildcard has no local sort (it is assigned by the top-level environment / pattern). -/
def varSort (Γ : Ctx) : Var → Option PSort
  | .bound l => Γ.get? l
  | .free _ => none
  | .wildcard => none

/-- `HasVarSort Γ v s` — the context classifies the variable occurrence `v` as sort `s`. -/
def HasVarSort (Γ : Ctx) (v : Var) (s : PSort) : Prop := varSort Γ v = some s

/-- The context judgment is decidable. -/
instance hasVarSort_decidable (Γ : Ctx) (v : Var) (s : PSort) : Decidable (HasVarSort Γ v s) := by
  unfold HasVarSort
  infer_instance

/-- The context judgment is functional: a variable has at most one sort. -/
theorem hasVarSort_functional {Γ : Ctx} {v : Var} {s s' : PSort}
    (h : HasVarSort Γ v s) (h' : HasVarSort Γ v s') : s = s' := by
  unfold HasVarSort at h h'
  rw [h] at h'
  exact Option.some.inj h'

/-! ## Structural name-vs-process classification -/

/-- A *pure name*: a `Par` with no process constructors at the top (empty sends/receives/news/
    matches). These are the terms that occur in name position: `Nil`, ground/expressions, bundles,
    unforgeables, connectives. Anything with a top-level send/receive/new/match is process-like. -/
def isPureName (p : Par) : Bool :=
  p.sends.isEmpty && p.receives.isEmpty && p.news.isEmpty && p.matches.isEmpty

/-- The `Prop` reading of `isPureName`: a pure name is a term the classifier accepts. -/
def IsPureName (p : Par) : Prop := isPureName p = true

/-- The sort classification: a pure name is a `name`, otherwise a `proc`. -/
def classify (p : Par) : PSort :=
  if isPureName p then PSort.name else PSort.proc

instance isPureName_decidable (p : Par) : Decidable (isPureName p = true) := inferInstance

instance IsPureName_decidable (p : Par) : Decidable (IsPureName p) := by
  unfold IsPureName
  infer_instance

theorem IsPureName_nil : IsPureName nilPar := by simp [IsPureName, isPureName, nilPar]

theorem classify_nil : classify nilPar = PSort.name := by simp [classify, isPureName, nilPar]

/-! ## Closedness (Law 6): no free variables -/

/-- A closed variable occurrence is one that is not a free de Bruijn level. -/
def closedVar (v : Var) : Bool :=
  match v with
  | .free _ => false
  | .bound _ => true
  | .wildcard => true

mutual
  def closed : Par → Bool
    | Par.mk s r n e m u b c =>
        closedListSend s && closedListReceive r && closedListNew n &&
        closedListExpr e && closedListMatch m && closedListGUnforgeable u &&
        closedListBundle b && closedListConnective c
  termination_by p => sizeOf p
  def closedSend : Send → Bool
    | Send.mk c d _ => closed c && closedListPar d
  termination_by s => sizeOf s
  def closedReceiveBind : ReceiveBind → Bool
    | ReceiveBind.mk ps s _ => closedListPar ps && closed s
  termination_by s => sizeOf s
  def closedReceive : Receive → Bool
    | Receive.mk bs b _ _ => closedListReceiveBind bs && closed b
  termination_by s => sizeOf s
  def closedNew : New → Bool
    | New.mk _ b => closed b
  termination_by s => sizeOf s
  def closedMatchCase : MatchCase → Bool
    | MatchCase.mk p s _ => closed p && closed s
  termination_by s => sizeOf s
  def closedMatch : Match → Bool
    | Match.mk t cs => closed t && closedListMatchCase cs
  termination_by s => sizeOf s
  def closedExpr : Expr → Bool
    | Expr.ground _ => true
    | Expr.evar v => closedVar v
    | Expr.eneg p => closed p
    | Expr.enot p => closed p
    | Expr.eplus p q => closed p && closed q
    | Expr.eminus p q => closed p && closed q
    | Expr.emult p q => closed p && closed q
    | Expr.ediv p q => closed p && closed q
    | Expr.emod p q => closed p && closed q
    | Expr.elt p q => closed p && closed q
    | Expr.ele p q => closed p && closed q
    | Expr.egt p q => closed p && closed q
    | Expr.ege p q => closed p && closed q
    | Expr.eeq p q => closed p && closed q
    | Expr.eneq p q => closed p && closed q
    | Expr.eand p q => closed p && closed q
    | Expr.eor p q => closed p && closed q
    | Expr.elist ps => closedListPar ps
    | Expr.etuple ps => closedListPar ps
    | Expr.eset ps => closedListPar ps
    | Expr.emap kvs => closedListParPair kvs
  termination_by s => sizeOf s
  def closedBundle : Bundle → Bool
    | Bundle.mk b _ _ => closed b
  termination_by s => sizeOf s
  def closedGUnforgeable : GUnforgeable → Bool
    | _ => true
  termination_by s => sizeOf s
  def closedConnective : Connective → Bool
    | Connective.connAnd ps => closedListPar ps
    | Connective.connOr ps => closedListPar ps
    | Connective.connNot p => closed p
    | Connective.connVarRef _ _ => true
  termination_by s => sizeOf s
  def closedListSend : List Send → Bool
    | [] => true
    | a :: as => closedSend a && closedListSend as
  termination_by l => sizeOf l
  def closedListReceive : List Receive → Bool
    | [] => true
    | a :: as => closedReceive a && closedListReceive as
  termination_by l => sizeOf l
  def closedListNew : List New → Bool
    | [] => true
    | a :: as => closedNew a && closedListNew as
  termination_by l => sizeOf l
  def closedListExpr : List Expr → Bool
    | [] => true
    | a :: as => closedExpr a && closedListExpr as
  termination_by l => sizeOf l
  def closedListMatch : List Match → Bool
    | [] => true
    | a :: as => closedMatch a && closedListMatch as
  termination_by l => sizeOf l
  def closedListGUnforgeable : List GUnforgeable → Bool
    | [] => true
    | a :: as => closedGUnforgeable a && closedListGUnforgeable as
  termination_by l => sizeOf l
  def closedListBundle : List Bundle → Bool
    | [] => true
    | a :: as => closedBundle a && closedListBundle as
  termination_by l => sizeOf l
  def closedListConnective : List Connective → Bool
    | [] => true
    | a :: as => closedConnective a && closedListConnective as
  termination_by l => sizeOf l
  def closedListPar : List Par → Bool
    | [] => true
    | a :: as => closed a && closedListPar as
  termination_by l => sizeOf l
  def closedListReceiveBind : List ReceiveBind → Bool
    | [] => true
    | a :: as => closedReceiveBind a && closedListReceiveBind as
  termination_by l => sizeOf l
  def closedListMatchCase : List MatchCase → Bool
    | [] => true
    | a :: as => closedMatchCase a && closedListMatchCase as
  termination_by l => sizeOf l
  def closedListParPair : List (Par × Par) → Bool
    | [] => true
    | (a, b) :: as => closed a && closed b && closedListParPair as
  termination_by l => sizeOf l
end

/-- `Closed p` — the process has no free variables (Law 6). Decidable (via the `closed*` `Bool`
    functions) and preserved by composition, `≡`, and canonicalization (below). -/
def Closed (p : Par) : Prop :=
  closedListSend p.sends = true ∧ closedListReceive p.receives = true ∧
  closedListNew p.news = true ∧ closedListExpr p.exprs = true ∧
  closedListMatch p.matches = true ∧ closedListGUnforgeable p.unforgeables = true ∧
  closedListBundle p.bundles = true ∧ closedListConnective p.connectives = true

/-- Closedness is decidable (fundamental: the checker can actually run). -/
instance closed_decidable (p : Par) : Decidable (Closed p) := by
  unfold Closed
  infer_instance

/-! ## The fundamentals -/

-- Closedness distributes over list append, for each of the 8 top-level fields.
@[simp] theorem closedListSend_append (l l' : List Send) :
    closedListSend (l ++ l') = (closedListSend l && closedListSend l') := by
  induction l with
  | nil => simp [closedListSend]
  | cons a as ih => simp [closedListSend, ih, Bool.and_assoc]
@[simp] theorem closedListReceive_append (l l' : List Receive) :
    closedListReceive (l ++ l') = (closedListReceive l && closedListReceive l') := by
  induction l with
  | nil => simp [closedListReceive]
  | cons a as ih => simp [closedListReceive, ih, Bool.and_assoc]
@[simp] theorem closedListNew_append (l l' : List New) :
    closedListNew (l ++ l') = (closedListNew l && closedListNew l') := by
  induction l with
  | nil => simp [closedListNew]
  | cons a as ih => simp [closedListNew, ih, Bool.and_assoc]
@[simp] theorem closedListExpr_append (l l' : List Expr) :
    closedListExpr (l ++ l') = (closedListExpr l && closedListExpr l') := by
  induction l with
  | nil => simp [closedListExpr]
  | cons a as ih => simp [closedListExpr, ih, Bool.and_assoc]
@[simp] theorem closedListMatch_append (l l' : List Match) :
    closedListMatch (l ++ l') = (closedListMatch l && closedListMatch l') := by
  induction l with
  | nil => simp [closedListMatch]
  | cons a as ih => simp [closedListMatch, ih, Bool.and_assoc]
@[simp] theorem closedListGUnforgeable_append (l l' : List GUnforgeable) :
    closedListGUnforgeable (l ++ l') = (closedListGUnforgeable l && closedListGUnforgeable l') := by
  induction l with
  | nil => simp [closedListGUnforgeable]
  | cons a as ih => simp [closedListGUnforgeable, ih, Bool.and_assoc]
@[simp] theorem closedListBundle_append (l l' : List Bundle) :
    closedListBundle (l ++ l') = (closedListBundle l && closedListBundle l') := by
  induction l with
  | nil => simp [closedListBundle]
  | cons a as ih => simp [closedListBundle, ih, Bool.and_assoc]
@[simp] theorem closedListConnective_append (l l' : List Connective) :
    closedListConnective (l ++ l') = (closedListConnective l && closedListConnective l') := by
  induction l with
  | nil => simp [closedListConnective]
  | cons a as ih => simp [closedListConnective, ih, Bool.and_assoc]

-- Closedness of the empty field lists.
@[simp] theorem closedListSend_nil : closedListSend ([] : List Send) = true := by simp [closedListSend]
@[simp] theorem closedListReceive_nil : closedListReceive ([] : List Receive) = true := by simp [closedListReceive]
@[simp] theorem closedListNew_nil : closedListNew ([] : List New) = true := by simp [closedListNew]
@[simp] theorem closedListExpr_nil : closedListExpr ([] : List Expr) = true := by simp [closedListExpr]
@[simp] theorem closedListMatch_nil : closedListMatch ([] : List Match) = true := by simp [closedListMatch]
@[simp] theorem closedListGUnforgeable_nil : closedListGUnforgeable ([] : List GUnforgeable) = true := by simp [closedListGUnforgeable]
@[simp] theorem closedListBundle_nil : closedListBundle ([] : List Bundle) = true := by simp [closedListBundle]
@[simp] theorem closedListConnective_nil : closedListConnective ([] : List Connective) = true := by simp [closedListConnective]

/-- Fundamental: `nilPar` is closed. -/
theorem Closed_nil : Closed nilPar := by
  simp [Closed, nilPar]

/-- Fundamental: closedness is a **monoid invariant** — `|` (parMerge) of two closed processes is
    closed, and conversely. -/
theorem Closed_parMerge_iff (p q : Par) : Closed (parMerge p q) ↔ Closed p ∧ Closed q := by
  cases p <;> cases q <;> simp [Closed, parMerge]

theorem Closed_parMerge {p q : Par} (hp : Closed p) (hq : Closed q) : Closed (parMerge p q) :=
  (Closed_parMerge_iff p q).mpr ⟨hp, hq⟩

/-- Fundamental: structural congruence preserves closedness (well-formedness is invariant under
    the `≡` fragment of reduction). -/
theorem strCong_closed_iff {p q : Par} : StrCong p q → (Closed p ↔ Closed q) := by
  intro h
  induction h with
  | refl => exact Iff.rfl
  | symm ih => exact ih.symm
  | trans ih₁ ih₂ => exact ih₁.trans ih₂
  | comm => simp [Closed_parMerge_iff]
  | assoc => simp [Closed_parMerge_iff]
  | ident => simp [Closed_parMerge_iff, Closed_nil]
  | par ih₁ ih₂ => simpa [Closed_parMerge_iff] using and_congr ih₁ ih₂

theorem strCong_closed {p q : Par} (h : StrCong p q) (hp : Closed p) : Closed q :=
  (strCong_closed_iff h).mp hp

/-- Fundamental: canonicalization (`sortList`) is a permutation, so it preserves every
    element-wise predicate — in particular `Closed`. Instantiate with `P := Closed` (and, for `Par`,
    `C := parComparator` from Law 1) to get "canonicalization preserves well-formedness". -/
theorem sortList_mem_pred {α : Type} (C : Comparator α) (P : α → Prop) (l : List α) :
    (∀ x ∈ l, P x) → (∀ x ∈ Comparator.sortList C l, P x) := by
  intro h x hx
  have hp : List.Perm (Comparator.sortList C l) l := by
    unfold Comparator.sortList
    exact List.perm_insertionSort C.le l
  exact h x ((hp.mem_iff).mp hx)

/-! ## Totality (the effect model bridging to Rust) -/

/-- `TotalOn f` — the operation `f` on processes is *total*: it maps closed processes to closed
    processes. This is the Lean spelling of "no `unwrap`/panic on the happy path": a Rust function
    `f : Par → Par` (not `→ Option`) whose spec is `TotalOn f` cannot panic on a free variable. -/
def TotalOn (f : Par → Par) : Prop := ∀ p, Closed p → Closed (f p)

/-- The identity is total. -/
theorem TotalOn_id : TotalOn (fun p => p) := by intro p hp; exact hp

/-- Totality is closed under composition. -/
theorem TotalOn_comp {f g : Par → Par} (hf : TotalOn f) (hg : TotalOn g) :
    TotalOn (fun p => g (f p)) := by
  intro p hp
  exact hg (f p) (hf p hp)

end Rchain
