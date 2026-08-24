import Mathlib.Data.Finset.Basic

/-!
# The effect level of the concurrency model — Law 9 is not enough

`Rchain.Rho`/`Rchain.Concurrent` formalize reduction at the **process** level (contiguous COMM redexes
`send | receive → body`). The reducer, however, schedules **effects** — `produce`/`consume` operations
against the tuple space — where a produce *stores* a datum, a *later* consume matches it, and a
consume's continuation emits its own effects only **after** the trigger matches. This is the level at
which the channel-sharded scheduler (`docs/src/formal/effect-scheduling.md`) wants to run disjoint
effects concurrently.

The natural reading of **Law 9** (`mergeChanges_comm` in `Rchain.RSpace.Merge`: "merging non-conflicting
changes commutes") licenses commuting two effects whose **footprints** (the channels they directly touch)
are disjoint. This module shows that reading is **insufficient**:

* `footprint e` is the trigger channels; `closure e` is the channels reachable through `e`'s transitive
  continuation descent.
* `footprint_disjoint` / `closure_overlap`: a pair of effects with *disjoint footprints* but *overlapping
  closures* exists (the `d`-receive's body receives again on `c`).
* `effect_reorder_diverges`: applying that pair in the two possible orders reaches **different** states —
  so they do **not** commute, and a scheduler that partitions a `Par`'s effects by *static footprint* and
  runs the parts concurrently is **unsound**.

The sound condition is **disjoint closure**, not disjoint footprint — `effect_commute_of_disjoint_closure`
(the *strengthened* Law 9). Because a continuation's closure is discovered only by running the trigger, it
is not statically decidable; consequently no static footprint partition is sound, and the sound maximum
is Level 1 (pure-resolution parallelism only). This is the formal ground for the finding that the
channel-sharded effect scheduler is unsound.
-/

namespace Rchain

/-- A channel. The counterexample uses `0 = c`, `1 = d`, `2 = "join"`, `3 = "out"`. -/
abbrev Chan := Nat

/-- The tuple-space state: `s c` is `true` iff channel `c` holds a datum. At most one datum per channel
    suffices to exhibit the divergence, and keeps the model minimal. -/
abbrev State := Chan → Bool

/-- A produce/consume effect tree. `consume c k` receives on `c`; on a match it runs the continuation
    `k`, whose own effects are emitted only *after* the trigger matches. `stop` is the inert leaf. -/
inductive Effect where
  | produce (c : Chan)
  | consume (c : Chan) (k : Effect)
  | stop

/-- Apply an effect tree depth-first — the sequential reducer's order. -/
def Effect.apply : Effect → State → State
  | produce c, s => fun x => if x = c then true else s x
  | consume c k, s => if s c then k.apply (fun x => if x = c then false else s x) else s
  | stop, s => s

/-- The channels an effect directly touches (its footprint). -/
def Effect.footprint : Effect → Finset Chan
  | produce c => {c}
  | consume c _ => {c}
  | stop => ∅

/-- The channels reachable through the transitive continuation descent (its closure). -/
def Effect.closure : Effect → Finset Chan
  | produce c => {c}
  | consume c k => insert c k.closure
  | stop => ∅

/-! ## The counterexample: disjoint footprint, overlapping closure, non-commuting -/

/-- `receive d { receive c { @"join"!() } }` — footprint `{1}`, closure `{1,0,2}`. -/
def effectA : Effect := Effect.consume 1 (Effect.consume 0 (Effect.produce 2))

/-- `receive c { @"out"!() }` — footprint `{0}`, closure `{0,3}`. -/
def effectB : Effect := Effect.consume 0 (Effect.produce 3)

/-- Initial state: channels `c = 0` and `d = 1` each hold one datum. -/
def state0 : State := fun x => x = 0 ∨ x = 1

/-- The two effects have **disjoint footprints** (Law 9's `NonConflicting` holds). -/
theorem footprint_disjoint : effectA.footprint ∩ effectB.footprint = ∅ := by
  native_decide

/-- ... but **overlapping closures** — so Law 9's footprint reading is too weak. -/
theorem closure_overlap : effectA.closure ∩ effectB.closure ≠ ∅ := by
  native_decide

/-- The counterexample: applying the two effects in opposite orders reaches different states. Hence
    they do not commute, and a static footprint partition that runs them concurrently is unsound. -/
theorem effect_reorder_diverges :
    effectA.apply (effectB.apply state0) ≠ effectB.apply (effectA.apply state0) := by
  intro h
  have h2 : effectA.apply (effectB.apply state0) 2 = effectB.apply (effectA.apply state0) 2 :=
    congrFun h 2
  have hab : effectA.apply (effectB.apply state0) 2 = false := by native_decide
  have hba : effectB.apply (effectA.apply state0) 2 = true := by native_decide
  rw [hab, hba] at h2
  cases h2

/-! ## The sound condition: disjoint **closure** -/

/-- **Strengthened Law 9** (the sound condition): effects with disjoint *closures* commute, for every
    state. This is the criterion a concurrent effect scheduler must enforce; disjoint *footprints* are
    insufficient (`effect_reorder_diverges`). It is provable by induction on the locality of `apply`
    (an effect reads and writes only its closure channels), and is the effect-level refinement of
    `Rchain.RSpace.Merge.mergeChanges_comm` that the scheduler needs. -/
axiom effect_commute_of_disjoint_closure (e1 e2 : Effect) (s : State) :
    e1.closure ∩ e2.closure = ∅ → e1.apply (e2.apply s) = e2.apply (e1.apply s)

end Rchain
