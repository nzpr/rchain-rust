import Rchain.Syntax

/-!
# Canonicalization (Law 1)

Mirrors `models/src/main/scala/coop/rchain/models/rholang/sorter/ScoreTree.scala` and
`ordering.scala`: `Par` (and `ESet`/`EMap`) are canonicalized to a total order so that structural
equality of the sorted form *is* process equality up to α.

The total order is a **hand-rolled structural `cmpProc`** (constructor declaration order,
lexicographic via `lex`). It is order-isomorphic to the Scala `ScoreTree` order on this fragment.
The flattened `score : Proc → List Nat` of the earlier skeleton was unsound and is removed.

Law 1 is `sort (sort p) = sort p` (idempotence) and `sort (p | q) = sort (q | p)` (commutativity),
proven from the total-order bundle on `cmpProc` (`eq_iff_eq`, `swap`, `gt_iff_lt`, totality) via two
`parPair` laws (`parPair_comm`, `parPair_idem`).
-/

namespace Rchain
open Proc

/-! ## Leaf lawfulness of `compare` on base types

The derived `compare` on `Bool`/`Int`/`String` is lawful. `Bool` is proven; `Int` and `String` are
admitted as base axioms (TODO: prove against the derived instances, or replace with a zigzag/code
point encoding).
-/

theorem compare_bool_eq_iff (a b : Bool) : compare a b = Ordering.eq ↔ a = b := by
  cases a <;> cases b <;> simp [compare]

axiom compare_int_eq_iff (a b : Int) : compare a b = Ordering.eq ↔ a = b
axiom compare_int_gt_iff_lt (a b : Int) : compare a b = Ordering.gt ↔ compare b a = Ordering.lt
axiom compare_str_eq_iff (a b : String) : compare a b = Ordering.eq ↔ a = b
axiom compare_str_gt_iff_lt (a b : String) : compare a b = Ordering.gt ↔ compare b a = Ordering.lt

/-! ## `lex` helper and hand-rolled structural comparator -/

def lex (o1 o2 : Ordering) : Ordering :=
  match o1 with
  | .eq => o2
  | o   => o

def cmpGround : Ground → Ground → Ordering
  | .bool b, .bool b' => compare b b'
  | .bool _, _ => .lt
  | _, .bool _ => .gt
  | .int n, .int n' => compare n n'
  | .int _, _ => .lt
  | _, .int _ => .gt
  | .str s, .str s' => compare s s'

def cmpVar : Var → Var → Ordering
  | .bound n, .bound m => compare n m
  | .bound _, _ => .lt
  | _, .bound _ => .gt
  | .free n, .free m => compare n m
  | .free _, _ => .lt
  | _, .free _ => .gt
  | .wildcard, .wildcard => .eq

def cmpProc : Proc → Proc → Ordering
  | .nil, .nil => .eq
  | .nil, _ => .lt
  | _, .nil => .gt
  | .ground g, .ground h => cmpGround g h
  | .ground _, _ => .lt
  | _, .ground _ => .gt
  | .var v, .var w => cmpVar v w
  | .var _, _ => .lt
  | _, .var _ => .gt
  | .send c d, .send e f => lex (cmpProc c e) (cmpProc d f)
  | .send _ _, _ => .lt
  | _, .send _ _ => .gt
  | .receive c d, .receive e f => lex (cmpProc c e) (cmpProc d f)
  | .receive _ _, _ => .lt
  | _, .receive _ _ => .gt
  | .new n p, .new m q => lex (compare n m) (cmpProc p q)
  | .new _ _, _ => .lt
  | _, .new _ _ => .gt
  | .match t p b, .match t' p' b' => lex (cmpProc t t') (lex (cmpProc p p') (cmpProc b b'))
  | .match _ _ _, _ => .lt
  | _, .match _ _ _ => .gt
  | .par p q, .par r s => lex (cmpProc p r) (cmpProc q s)
termination_by a b => sizeOf a + sizeOf b

/-! ## `lex` / `Ordering.swap` lemmas -/

theorem swap_lex (o1 o2 : Ordering) : Ordering.swap (lex o1 o2) = lex (Ordering.swap o1) (Ordering.swap o2) := by
  cases o1 <;> cases o2 <;> simp [lex, Ordering.swap]

theorem swap_eq_iff (o : Ordering) : Ordering.swap o = .eq ↔ o = .eq := by
  cases o <;> simp [Ordering.swap]

theorem swap_gt_iff_lt (o : Ordering) : Ordering.swap o = .gt ↔ o = .lt := by
  cases o <;> simp [Ordering.swap]

theorem swap_lt_iff_gt (o : Ordering) : Ordering.swap o = .lt ↔ o = .gt := by
  cases o <;> simp [Ordering.swap]

theorem lex_eq_iff (o1 o2 : Ordering) : lex o1 o2 = .eq ↔ o1 = .eq ∧ o2 = .eq := by
  cases o1 <;> cases o2 <;> simp [lex]

/-! ## Leaf compare facts (Bool / Nat) -/

theorem compare_bool_gt_iff_lt (a b : Bool) : compare a b = Ordering.gt ↔ compare b a = Ordering.lt := by
  cases a <;> cases b <;> simp [compare]

theorem compare_nat_swap (n m : Nat) : compare n m = Ordering.swap (compare m n) := by
  rcases Nat.lt_trichotomy n m with hlt | heq | hgt
  · have hnm : compare n m = .lt := by simp [compare, compareOfLessAndEq, hlt]
    have hmn : compare m n = .gt := by
      simp [compare, compareOfLessAndEq, Nat.lt_asymm hlt, Nat.ne_of_gt hlt]
    rw [hnm, hmn]; rfl
  · subst heq
    simp [compare, compareOfLessAndEq, Ordering.swap]
  · have hnm : compare n m = .gt := by
      simp [compare, compareOfLessAndEq, Nat.lt_asymm hgt, Nat.ne_of_gt hgt]
    have hmn : compare m n = .lt := by simp [compare, compareOfLessAndEq, hgt]
    rw [hnm, hmn]; rfl

theorem compare_nat_gt_iff_lt (n m : Nat) : compare n m = .gt ↔ compare m n = .lt := by
  rw [compare_nat_swap]
  exact swap_gt_iff_lt (compare m n)

/-! ## Leaf lawfulness (`cmpGround` / `cmpVar`) -/

theorem cmpGround_eq_iff_eq (g h : Ground) : cmpGround g h = .eq ↔ g = h := by
  cases g <;> cases h <;> simp [cmpGround, compare_bool_eq_iff, compare_int_eq_iff, compare_str_eq_iff, Ground.bool.injEq, Ground.int.injEq, Ground.str.injEq]

theorem cmpGround_gt_iff_lt (g h : Ground) : cmpGround g h = .gt ↔ cmpGround h g = .lt := by
  cases g <;> cases h <;> simp [cmpGround, compare_bool_gt_iff_lt, compare_int_gt_iff_lt, compare_str_gt_iff_lt]

theorem cmpGround_swap (g h : Ground) : cmpGround h g = Ordering.swap (cmpGround g h) := by
  cases hc : cmpGround g h with
  | lt =>
      simp [hc, Ordering.swap]
      exact (cmpGround_gt_iff_lt h g).2 hc
  | eq =>
      have hgh : g = h := (cmpGround_eq_iff_eq g h).1 hc
      simp [hc, Ordering.swap]
      exact (cmpGround_eq_iff_eq h g).2 hgh.symm
  | gt =>
      simp [hc, Ordering.swap]
      exact (cmpGround_gt_iff_lt g h).1 hc

theorem cmpVar_eq_iff_eq (v w : Var) : cmpVar v w = .eq ↔ v = w := by
  cases v <;> cases w <;> simp [cmpVar, Nat.compare_eq_eq, Var.bound.injEq, Var.free.injEq]

theorem cmpVar_gt_iff_lt (v w : Var) : cmpVar v w = .gt ↔ cmpVar w v = .lt := by
  cases v <;> cases w <;> simp [cmpVar, compare_nat_gt_iff_lt]

theorem cmpVar_swap (v w : Var) : cmpVar w v = Ordering.swap (cmpVar v w) := by
  cases hc : cmpVar v w with
  | lt =>
      simp [hc, Ordering.swap]
      exact (cmpVar_gt_iff_lt w v).2 hc
  | eq =>
      have hvw : v = w := (cmpVar_eq_iff_eq v w).1 hc
      simp [hc, Ordering.swap]
      exact (cmpVar_eq_iff_eq w v).2 hvw.symm
  | gt =>
      simp [hc, Ordering.swap]
      exact (cmpVar_gt_iff_lt v w).1 hc

/-! ## The total-order bundle -/

theorem cmpProc_eq_iff_eq (a b : Proc) : cmpProc a b = .eq ↔ a = b := by
  induction a generalizing b with
  | nil => cases b <;> simp [cmpProc]
  | ground g => cases b <;> simp [cmpProc, cmpGround_eq_iff_eq, Proc.ground.injEq]
  | var v => cases b <;> simp [cmpProc, cmpVar_eq_iff_eq, Proc.var.injEq]
  | send c d ihc ihd =>
      cases b with
      | nil => simp [cmpProc]
      | ground g => simp [cmpProc]
      | var v => simp [cmpProc]
      | send e f => simp [cmpProc, lex_eq_iff, ihc, ihd, Proc.send.injEq]
      | receive e f => simp [cmpProc]
      | new n e => simp [cmpProc]
      | «match» t p body => simp [cmpProc]
      | par p q => simp [cmpProc]
  | receive c d ihc ihd =>
      cases b with
      | nil => simp [cmpProc]
      | ground g => simp [cmpProc]
      | var v => simp [cmpProc]
      | send e f => simp [cmpProc]
      | receive e f => simp [cmpProc, lex_eq_iff, ihc, ihd, Proc.receive.injEq]
      | new n e => simp [cmpProc]
      | «match» t p body => simp [cmpProc]
      | par p q => simp [cmpProc]
  | new n p ihp =>
      cases b with
      | nil => simp [cmpProc]
      | ground g => simp [cmpProc]
      | var v => simp [cmpProc]
      | send e f => simp [cmpProc]
      | receive e f => simp [cmpProc]
      | new m q => simp [cmpProc, lex_eq_iff, Nat.compare_eq_eq, ihp, Proc.new.injEq]
      | «match» t p body => simp [cmpProc]
      | par p q => simp [cmpProc]
  | «match» t p body iht ihp ihbody =>
      cases b with
      | nil => simp [cmpProc]
      | ground g => simp [cmpProc]
      | var v => simp [cmpProc]
      | send e f => simp [cmpProc]
      | receive e f => simp [cmpProc]
      | new n e => simp [cmpProc]
      | «match» t' p' body' => simp [cmpProc, lex_eq_iff, iht, ihp, ihbody, Proc.match.injEq]
      | par p q => simp [cmpProc]
  | par p q ihp ihq =>
      cases b with
      | nil => simp [cmpProc]
      | ground g => simp [cmpProc]
      | var v => simp [cmpProc]
      | send e f => simp [cmpProc]
      | receive e f => simp [cmpProc]
      | new n e => simp [cmpProc]
      | «match» t p body => simp [cmpProc]
      | par r s => simp [cmpProc, lex_eq_iff, ihp, ihq, Proc.par.injEq]

theorem cmpProc_swap (a b : Proc) : cmpProc b a = Ordering.swap (cmpProc a b) := by
  induction a generalizing b with
  | nil => cases b <;> simp [cmpProc, Ordering.swap]
  | ground g =>
      cases b with
      | nil => simp [cmpProc, Ordering.swap]
      | ground h => simpa [cmpProc] using cmpGround_swap g h
      | var v => simp [cmpProc, Ordering.swap]
      | send e f => simp [cmpProc, Ordering.swap]
      | receive e f => simp [cmpProc, Ordering.swap]
      | new n e => simp [cmpProc, Ordering.swap]
      | «match» t p body => simp [cmpProc, Ordering.swap]
      | par p q => simp [cmpProc, Ordering.swap]
  | var v =>
      cases b with
      | nil => simp [cmpProc, Ordering.swap]
      | ground g => simp [cmpProc, Ordering.swap]
      | var w => simpa [cmpProc] using cmpVar_swap v w
      | send e f => simp [cmpProc, Ordering.swap]
      | receive e f => simp [cmpProc, Ordering.swap]
      | new n e => simp [cmpProc, Ordering.swap]
      | «match» t p body => simp [cmpProc, Ordering.swap]
      | par p q => simp [cmpProc, Ordering.swap]
  | send c d ihc ihd =>
      cases b with
      | nil => simp [cmpProc, Ordering.swap]
      | ground g => simp [cmpProc, Ordering.swap]
      | var v => simp [cmpProc, Ordering.swap]
      | send e f =>
          simp [cmpProc, swap_lex]
          rw [ihc e, ihd f]
      | receive e f => simp [cmpProc, Ordering.swap]
      | new n e => simp [cmpProc, Ordering.swap]
      | «match» t p body => simp [cmpProc, Ordering.swap]
      | par p q => simp [cmpProc, Ordering.swap]
  | receive c d ihc ihd =>
      cases b with
      | nil => simp [cmpProc, Ordering.swap]
      | ground g => simp [cmpProc, Ordering.swap]
      | var v => simp [cmpProc, Ordering.swap]
      | send e f => simp [cmpProc, Ordering.swap]
      | receive e f =>
          simp [cmpProc, swap_lex]
          rw [ihc e, ihd f]
      | new n e => simp [cmpProc, Ordering.swap]
      | «match» t p body => simp [cmpProc, Ordering.swap]
      | par p q => simp [cmpProc, Ordering.swap]
  | new n p ihp =>
      cases b with
      | nil => simp [cmpProc, Ordering.swap]
      | ground g => simp [cmpProc, Ordering.swap]
      | var v => simp [cmpProc, Ordering.swap]
      | send e f => simp [cmpProc, Ordering.swap]
      | receive e f => simp [cmpProc, Ordering.swap]
      | new m q =>
          simp [cmpProc, swap_lex]
          rw [compare_nat_swap m n, ihp q]
      | «match» t p body => simp [cmpProc, Ordering.swap]
      | par p q => simp [cmpProc, Ordering.swap]
  | «match» t p body iht ihp ihbody =>
      cases b with
      | nil => simp [cmpProc, Ordering.swap]
      | ground g => simp [cmpProc, Ordering.swap]
      | var v => simp [cmpProc, Ordering.swap]
      | send e f => simp [cmpProc, Ordering.swap]
      | receive e f => simp [cmpProc, Ordering.swap]
      | new n e => simp [cmpProc, Ordering.swap]
      | «match» t' p' body' =>
          simp [cmpProc, swap_lex]
          rw [iht t', ihp p', ihbody body']
      | par p q => simp [cmpProc, Ordering.swap]
  | par p q ihp ihq =>
      cases b with
      | nil => simp [cmpProc, Ordering.swap]
      | ground g => simp [cmpProc, Ordering.swap]
      | var v => simp [cmpProc, Ordering.swap]
      | send e f => simp [cmpProc, Ordering.swap]
      | receive e f => simp [cmpProc, Ordering.swap]
      | new n e => simp [cmpProc, Ordering.swap]
      | «match» t p body => simp [cmpProc, Ordering.swap]
      | par r s =>
          simp [cmpProc, swap_lex]
          rw [ihp r, ihq s]

theorem cmpProc_gt_iff_lt (a b : Proc) : cmpProc a b = .gt ↔ cmpProc b a = .lt := by
  rw [cmpProc_swap a b]
  exact (swap_lt_iff_gt (cmpProc a b)).symm

theorem cmpProc_total (a b : Proc) : cmpProc a b = .lt ∨ cmpProc a b = .eq ∨ cmpProc a b = .gt := by
  exact match cmpProc a b with
    | .lt => Or.inl rfl
    | .eq => Or.inr (Or.inl rfl)
    | .gt => Or.inr (Or.inr rfl)

/-! ## `parPair` and canonical `sort` -/

def parPair (a b : Proc) : Proc × Proc :=
  if cmpProc a b = Ordering.gt then (b, a) else (a, b)

def sort : Proc → Proc
  | .nil         => .nil
  | .ground g    => .ground g
  | .var v       => .var v
  | .send c d    => .send (sort c) (sort d)
  | .receive b e => .receive (sort b) (sort e)
  | .new n e     => .new n (sort e)
  | .match t p b => .match (sort t) (sort p) (sort b)
  | .par p q     => let (a, b) := parPair (sort p) (sort q); .par a b

theorem sort_nil : sort .nil = .nil := rfl
theorem sort_ground (g : Ground) : sort (.ground g) = .ground g := rfl
theorem sort_var (v : Var) : sort (.var v) = .var v := rfl

/-! ## `parPair` laws and Law 1 sort theorems -/

theorem ne_gt_of_lt (a b : Proc) (h : cmpProc a b = Ordering.lt) : ¬ cmpProc a b = Ordering.gt := by
  intro hg
  rw [h] at hg
  cases hg

theorem parPair_gt (a b : Proc) (h : cmpProc a b = Ordering.gt) : parPair a b = (b, a) := by
  simp [parPair, h]

theorem parPair_le (a b : Proc) (h : ¬ cmpProc a b = Ordering.gt) : parPair a b = (a, b) := by
  simp [parPair, h]

theorem parPair_comm (a b : Proc) : parPair a b = parPair b a := by
  rcases cmpProc_total a b with hlt | heq | hgt
  · have hba : cmpProc b a = Ordering.gt := (cmpProc_gt_iff_lt b a).2 hlt
    rw [parPair_le a b (ne_gt_of_lt a b hlt), parPair_gt b a hba]
  · have hab : a = b := (cmpProc_eq_iff_eq a b).1 heq
    subst hab
    rfl
  · have hba : cmpProc b a = Ordering.lt := (cmpProc_gt_iff_lt a b).1 hgt
    rw [parPair_gt a b hgt, parPair_le b a (ne_gt_of_lt b a hba)]

theorem parPair_idem (a b : Proc) : parPair (parPair a b).1 (parPair a b).2 = parPair a b := by
  by_cases h : cmpProc a b = Ordering.gt
  · have hba : cmpProc b a = Ordering.lt := (cmpProc_gt_iff_lt a b).1 h
    rw [parPair_gt a b h, parPair_le b a (ne_gt_of_lt b a hba)]
  · simp [parPair, h]

theorem sort_par_gt (p q : Proc) (h : cmpProc (sort p) (sort q) = Ordering.gt) :
    sort (.par p q) = .par (sort q) (sort p) := by
  simp [sort, parPair, h]

theorem sort_par_le (p q : Proc) (h : ¬ cmpProc (sort p) (sort q) = Ordering.gt) :
    sort (.par p q) = .par (sort p) (sort q) := by
  simp [sort, parPair, h]

theorem sort_par_proj (p q : Proc) :
    sort (.par p q) = .par (parPair (sort p) (sort q)).1 (parPair (sort p) (sort q)).2 := by
  by_cases h : cmpProc (sort p) (sort q) = Ordering.gt
  · rw [sort_par_gt p q h, parPair_gt (sort p) (sort q) h]
  · rw [sort_par_le p q h, parPair_le (sort p) (sort q) h]

theorem sort_idempotent (p : Proc) : sort (sort p) = sort p := by
  induction p with
  | nil => rfl
  | ground g => rfl
  | var v => rfl
  | send c d ih_c ih_d => simp [sort, ih_c, ih_d]
  | receive b e ih_b ih_e => simp [sort, ih_b, ih_e]
  | new n e ih_e => simp [sort, ih_e]
  | «match» t m b ih_t ih_m ih_b => simp [sort, ih_t, ih_m, ih_b]
  | par p q ih_p ih_q =>
      by_cases h : cmpProc (sort p) (sort q) = Ordering.gt
      · have hsq : cmpProc (sort q) (sort p) = Ordering.lt := (cmpProc_gt_iff_lt (sort p) (sort q)).1 h
        rw [sort_par_gt p q h]
        have h' : ¬ cmpProc (sort (sort q)) (sort (sort p)) = Ordering.gt := by
          rw [ih_q, ih_p]
          exact ne_gt_of_lt (sort q) (sort p) hsq
        rw [sort_par_le (sort q) (sort p) h']
        rw [ih_q, ih_p]
      · rw [sort_par_le p q h]
        have h' : ¬ cmpProc (sort (sort p)) (sort (sort q)) = Ordering.gt := by
          rw [ih_p, ih_q]
          exact h
        rw [sort_par_le (sort p) (sort q) h']
        rw [ih_p, ih_q]

theorem sort_par_comm (p q : Proc) : sort (.par p q) = sort (.par q p) := by
  rw [sort_par_proj p q, sort_par_proj q p, parPair_comm (sort p) (sort q)]

end Rchain
