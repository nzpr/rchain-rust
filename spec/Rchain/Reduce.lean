import Rchain.Rho
import Rchain.Ty
import Rchain.FreeVars

/-!
# Law 4 — reduction (COMM), first-match-wins, `new` freshness

The COMM contraction and the `|`-congruence are the `Reduce` relation in `Rchain.Rho`; closedness
preservation under `⟶` is proven in `Rchain.Ty` (`reduce_closed`). This module states the remaining
two clauses of Law 4: reduction is **deterministic** (first-match-wins) and `new` yields **fresh**
unforgeable names. The Scala oracle is `Reduce.scala`; the Rust realization is
`rholang::reduce::DebruijnInterpreter`.
-/

namespace Rchain

/- Law 4 (corrected): reduction is **confluent** up to structural congruence, not single-step
   deterministic. The single-step form `Reduce p q → Reduce p q' → StrCong q q'` is **false**: two
   independent COMM redexes in a `parMerge` reduce to non-`≡`-equivalent results. The correct
   invariant is confluence — see `Rchain.Concurrent.reduce_confluent`. -/

/-- Law 4: `new` binds names that are fresh — reduction cannot mention a `new`-bound name outside
    its binder, so a fresh name never clashes with an existing one. Phrased as: the free variables
    of the reduct are a subset of the free variables of the redex. -/
axiom reduce_freeVars_subset {p q : Par} (h : Reduce p q) : ∀ n, freeVarOf q n → freeVarOf p n

end Rchain
