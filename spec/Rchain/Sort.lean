import Rchain.Syntax

/-!
# Canonicalization (Law 1)

Mirrors `models/src/main/scala/coop/rchain/models/rholang/sorter/ScoreTree.scala` and
`ordering.scala`. The Scala code assigns every term constructor a unique absolute `Score` and
lexicographically compares the resulting score trees; this turns `Par` (and `ESet`/`EMap`) into a
commutative structure by sorting their insides, so structural equality of the sorted form *is*
process equality up to α.

Here we build the same score tree as a `List Nat` (constructor constant first, then children's
scores), compare two trees lexicographically (`lexNat`), and define `sort` to recursively
canonicalize every subterm, ordering the two children of a `par` node by that total order.

Law 1 is the statement that this is a projection (`sort (sort p) = sort p`) and that `par` is
commutative after normalization (`sort (p | q) = sort (q | p)`). Phase 0 proves the atomic fixed
points; the two deep theorems are Phase 1 proof obligations (admitted with `sorry`).
-/

namespace Rchain
open Proc

/-- Lexicographic comparison of two `List Nat` score trees. -/
def lexNat : List Nat → List Nat → Ordering
  | [], []       => .eq
  | [], _ :: _   => .lt
  | _ :: _, []   => .gt
  | a :: as, b :: bs =>
    match compare a b with
    | .eq => lexNat as bs
    | o   => o

/-- Score of a ground value. String comparison is a Phase 1 refinement (constant for now). -/
def scoreGround : Ground → List Nat
  | .bool b => [1, if b then 1 else 0]
  | .int n  => [2, n.natAbs]
  | .str _  => [3]  -- TODO(Phase 1): lexicographic string comparison

/-- Score of a variable (de Bruijn level distinguishes bound/free; wildcard is a constant). -/
def scoreVar : Var → List Nat
  | .bound l  => [1, l]
  | .free l   => [2, l]
  | .wildcard => [3]

/-- The flattened score tree of a process (constructor constant, then children). -/
def score : Proc → List Nat
  | .nil         => [0]
  | .ground g    => [10] ++ scoreGround g
  | .var v       => [20] ++ scoreVar v
  | .send c d    => [30] ++ score c ++ score d
  | .receive b e => [40] ++ score b ++ score e
  | .new n e     => [50, n] ++ score e
  | .match t p b => [60] ++ score t ++ score p ++ score b
  | .par p q     => [999] ++ score p ++ score q

/-- Total order on processes (lexicographic on their score trees). -/
def cmpProc (a b : Proc) : Ordering := lexNat (score a) (score b)

/-- Order two already-sorted subterms of a `par` into a canonical (smaller-first) pair. -/
def parPair (a b : Proc) : Proc × Proc :=
  if cmpProc a b = Ordering.gt then (b, a) else (a, b)

/-- Canonicalization: recursively sort every subterm and order `par` children. -/
def sort : Proc → Proc
  | .nil         => .nil
  | .ground g    => .ground g
  | .var v       => .var v
  | .send c d    => .send (sort c) (sort d)
  | .receive b e => .receive (sort b) (sort e)
  | .new n e     => .new n (sort e)
  | .match t p b => .match (sort t) (sort p) (sort b)
  | .par p q     => let (a, b) := parPair (sort p) (sort q); .par a b

/-- `Nil` is a fixed point of `sort`. -/
theorem sort_nil : sort .nil = .nil := rfl

/-- Ground atoms are fixed points of `sort`. -/
theorem sort_ground (g : Ground) : sort (.ground g) = .ground g := rfl

/-- Variables are fixed points of `sort`. -/
theorem sort_var (v : Var) : sort (.var v) = .var v := rfl

/-- Law 1 (idempotence): canonicalization is a projection. -/
theorem sort_idempotent (p : Proc) : sort (sort p) = sort p := by
  -- TODO(Phase 1): structural induction on p; the `par` case needs that `cmpProc` is a total
  -- order (so `parPair` is idempotent and symmetric) and the induction hypothesis.
  sorry

/-- Law 1 (commutativity of `par` under normalization): `sort (p | q) = sort (q | p)`. -/
theorem sort_par_comm (p q : Proc) : sort (.par p q) = sort (.par q p) := by
  -- TODO(Phase 1): reduces to `parPair` symmetry, i.e. antisymmetry of `cmpProc`.
  sorry

end Rchain
