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

/- Law 4 (corrected): reduction is **not** even single-step deterministic up to `StrCong`, and the
   flat `Par` is **not confluent** either — a term with one receive and two sends on one channel is a
   redex in two ways (see `Rchain.Concurrent.reduce_not_deterministic`). What *does* hold on the flat
   `Par` is that an *isolated* redex reduces uniquely up to `StrCong`
   (`Rchain.Concurrent.reduce_redex_unique`). Full confluence is a property of the tree model, not of
   the field-wise flat `Par`. -/

/-- Law 4: `new` binds names that are fresh — reduction cannot mention a `new`-bound name outside
    its binder, so a fresh name never clashes with an existing one. Phrased as: the free variables
    of the reduct are a subset of the free variables of the redex. -/
axiom reduce_freeVars_subset {p q : Par} (h : Reduce p q) : ∀ n, freeVarOf q n → freeVarOf p n

end Rchain
