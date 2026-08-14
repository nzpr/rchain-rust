import Rchain.Par

/-!
# The ρ-calculus base sort over the flat `Par`

The node executes the reflective higher-order **ρ-calculus**: a single grammar in which a *name* is a
quoted process (`@Proc`) and a *process* can dereference a name (`*Name`), with parallel composition
`|`, replication (`!`, carried by the `persistent` flags), `new` (fresh unforgeable names), `match`,
and the ground/arithmetic expressions. The flat `Par` ADT in `Rchain.Par` *erases* the quote/eval
distinction — a `Par` in name position *is* a name — so the reflective core is recovered in the type
layer (`Rchain.Ty`).

This module gives the ρ-calculus **structural congruence** `≡` (Law 2 core): parallel composition is
commutative/associative with identity `nilPar`, and `≡` is a congruence. This is the
`P | Q = Q | P`, `P = P | Nil`, `(P|Q)|R = P|(Q|R)` fragment of `name-equivalence.k` and
`rholangmatchingtut.md`. Deep α-equivalence and capture-avoiding substitution stay Coq's obligation
(Laws 2–3, per `AGENTS.md`); here we state the congruence the type system needs and prove the
fundamentals (`Rchain.Ty`) over it.
-/

namespace Rchain

/-- Structural congruence `≡` on processes (Law 2 core: par order, `| Nil`, associativity, congruence). -/
inductive StrCong : Par → Par → Prop where
  | refl  : ∀ p, StrCong p p
  | symm  : ∀ {p q}, StrCong p q → StrCong q p
  | trans : ∀ {p q r}, StrCong p q → StrCong q r → StrCong p r
  | comm  : ∀ p q, StrCong (parMerge p q) (parMerge q p)
  | assoc : ∀ p q r, StrCong (parMerge (parMerge p q) r) (parMerge p (parMerge q r))
  | ident : ∀ p, StrCong (parMerge p nilPar) p
  | par   : ∀ {p p' q q'}, StrCong p p' → StrCong q q' → StrCong (parMerge p q) (parMerge p' q')

/-- `≡` is an equivalence relation (refl/symm/trans are built in). -/
theorem strCong_equivalence : Equivalence StrCong :=
  ⟨StrCong.refl, StrCong.symm, StrCong.trans⟩

theorem strCong_comm (p q : Par) : StrCong (parMerge p q) (parMerge q p) := StrCong.comm p q

theorem strCong_assoc (p q r : Par) :
    StrCong (parMerge (parMerge p q) r) (parMerge p (parMerge q r)) := StrCong.assoc p q r

theorem strCong_ident (p : Par) : StrCong (parMerge p nilPar) p := StrCong.ident p

/-- `nilPar` is also a *left* identity, by commutativity + right identity. -/
theorem strCong_nil_left (p : Par) : StrCong (parMerge nilPar p) p :=
  StrCong.trans (StrCong.comm nilPar p) (StrCong.ident p)

end Rchain
