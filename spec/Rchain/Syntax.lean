/-!
# Rholang core syntax (Phase 0 skeleton)

This mirrors the Rholang abstract data type defined in
`models/src/main/protobuf/RhoTypes.proto` (`Par`/`Send`/`Receive`/`New`/`Match`/`Expr`/`Bundle`)
and the concrete grammar `rholang/src/main/bnfc/rholang_mercury.cf`.

Phase 0 deliberately models a *core fragment* large enough to anchor the canonicalization
invariant (Law 1). Binders use de Bruijn *levels*, matching the Scala `Var`
(`bound_var`/`free_var`/`wildcard`) and the depth discipline of `Substitute.scala`/`Env.scala`.

**Phase 0 simplifications (removed in Phase 1):** `send`/`receive`/`match` are binary rather than
list-typed — i.e. a single datum, a single bind, a single match case. Phase 1 restores the
list/multiset arities (a `send` carries `List Par` of data, a `receive` a `Join` of binds, a `match`
a list of cases) together with `Bundle`, `Connective`, `GUnforgeable`, method calls, remainder
patterns, and `select`. Phase 0 keeps the constructors structurally recursive so that `Sort.sort`
and `Sort.score` are ordinary (unfoldable) definitions.

## Correspondence to Scala

| Lean | Scala / proto |
|------|---------------|
| `Proc.par`     | `Par` (a multiset of processes; here a binary tree, order made canonical by `Sort.sort`) |
| `Proc.send`    | `Send` (single-datum simplification) |
| `Proc.receive` | `Receive` (single-bind simplification) |
| `Proc.new`     | `New` (`bindCount` fresh `GPrivate` names) |
| `Proc.match`   | `Match` (single-case simplification) |
| `Var.bound` / `Var.free` | `Var.BoundVar` / `Var.FreeVar` (de Bruijn levels) |
| `Var.wildcard` | `Var.WildcardMsg` (`_`) |
-/

namespace Rchain

/-- Ground scalar values (`Expr` with a `GBool`/`GInt`/`GString` instance). -/
inductive Ground where
  | bool (b : Bool)
  | int  (n : Int)
  | str  (s : String)

/-- Variables, represented as de Bruijn levels (bound/free) or a wildcard. -/
inductive Var where
  | bound (level : Nat)  -- bound_var
  | free  (level : Nat)  -- free_var
  | wildcard             -- `_`

/-- A process: the flattened `Par` ADT. `par` is the commutative parallel composition `|`. -/
inductive Proc where
  | nil      : Proc
  | ground   : Ground → Proc
  | var      : Var → Proc
  | send     : Proc → Proc → Proc                 -- channel!(datum)
  | receive  : Proc → Proc → Proc                 -- for (bind <- …) { body }
  | new      : Nat → Proc → Proc                  -- new binds `n` fresh unforgeable names
  | match    : Proc → Proc → Proc → Proc          -- match target { pat => body }
  | par      : Proc → Proc → Proc                 -- p | q

end Rchain
