# The concurrency model

> This document is the **specification of the node's concurrency model** — the statement of what may run
> concurrently, what must serialize, and *why*, founded in the 19 laws. It is the target the Lean
> formalization (`spec/Rchain/`) proves. The two prior documents spell out the machinery:
> [Concurrent reduction](concurrent-reduction.md) (the reducer) and
> [Effect scheduling](effect-scheduling.md) (the tuple space). This page ties them together and adds the
> block level.

## The invariant in one line

> **Concurrency is a scheduling freedom, never a semantics change.** Every concurrent execution — at
> any layer — must reach the same canonical state as the purely-sequential execution. The 19 laws make
> this a *theorem*, not a convention: reduction is deterministic up to `≡` (Law 4), and the state is
> content-addressed (Law 10).

## The three levels

Concurrency appears at three levels, each with its own enabling and constraining laws.

| Level | What runs concurrently | What serializes | Enabling laws | Constraining laws |
|-------|------------------------|-----------------|---------------|-------------------|
| **Reducer** (within a deploy) | the *pure* resolution of a `Par`'s terms — substitution, spatial matching, name allocation | the tuple-space effects, in DFS order | 1, 2, 19 | 4, 8 |
| **Effect** (matching + scheduling) | *disjoint-channel* `produce`/`consume` | *same-channel* effects, in DFS order | 9, 7 | 4, 8, 11 |
| **Block** (validation) | replay of dependency-free blocks | DAG insertion, in topological order | 11 | 14, 15, 16, 18 |

## Level 1 — the reducer (fork-join over `|`)

A `Par` is a parallel composition (`|`, Law 2), so its sub-terms are *concurrent by construction*. The
reducer realizes this with a **fork-join**: `expand_par` resolves every term's *pure* part concurrently
(`rholang/src/reduce.rs`), then applies the *effects* (the `produce`/`consume` calls that touch the
tuple space) in DFS order.

- **Why the pure part is concurrent.** Substitution, spatial matching, and `new`-name allocation are
  side-effect-free with respect to the tuple space; cost charges are atomic and each term's RNG is
  pre-split (`split_byte`/`split_short`, Law 19). So concurrent resolution is schedule-independent.
- **Why the effects are serial.** The effect *order* fixes which datum/continuation a comm consumes
  (Law 4/8), and a continuation's channel footprint is only known after its trigger effect runs — so
  the reducer keeps effects in DFS order to reproduce the sequential candidate choices.

See [Concurrent reduction](concurrent-reduction.md) for the theorems (diamond, linearization).

## Level 2 — effect selection (content-addressed matching)

When a channel has more than one matching datum or continuation, the space must pick one. Selection is
**sorted-first by content hash** (`rspace/src/space_matcher.rs`, Law 8) rather than newest-first. This
removes the order-sensitivity of *which stored candidate* a comm consumes. See
[Sorted matching](../node/sorted-matching.md).

## Level 3 — effect scheduling (disjoint channels commute)

Two effects on **disjoint channels** touch disjoint state and commute (Law 9), so they may apply
concurrently. Two effects on the **same channel** must apply in DFS order: the *arrival order* — which
produce/consume matches a waiting continuation first — is order-sensitive, and sorted selection does not
remove it. This is the one place the naive "spawn all effects" scheduler is unsound, and it is the reason
the reducer keeps same-channel effects serial. See [Effect scheduling](effect-scheduling.md) for the
disjoint-commute and sharded-scheduler-linearization theorems.

## Level 4 — cross-block validation (replay is verify-only)

Block validation *replays* a block's deploys against its recorded trace and checks the recomputed
post-state root (Law 11). Replay is **verify-only**: it does not change the committed state, and each
block's replay is self-contained (it starts from the block's `pre_state_hash`, a parent's committed
root). So **dependency-free blocks** — siblings whose parents are already in the DAG — can be replayed
concurrently, each on a freshly-forked `ReplayRhoRuntime` (`casper/src/runtime_manager.rs`
`fork_replay_runtime`), and the `block_processor` then inserts them serially in topological order.

This is the node's block-throughput scaling: it uses the *same* Laws 4/8/11 determinism as the other two
levels, but at the granularity of whole blocks.

## Soundness theorems (the Lean targets)

The model is sound if these hold. Each is the statement that a concurrent execution matches the
sequential one.

1. **Diamond / confluence** — two parallel steps on independent redexes commute (`P ⟹ Q₁ ∧ P ⟹ Q₂ ⇒
   Q₁ ≡ Q₂`). Corollary: every schedule reaches the same canonical normal form.
2. **Linearization** — the sequential reducer (DFS, canonical order) is a valid refinement of the
   concurrent one; both reach the same `≡`-canonical state.
3. **Disjoint commute** — `chans(e₁) ∩ chans(e₂) = ∅ ⇒ apply(e₁; e₂) ≡ apply(e₂; e₁)` (Law 9).
4. **Sharded-scheduler soundness** — parallel-disjoint + serial-same-channel (DFS) == sequential.
5. **Replay determinism** — recomputed COMM ⊆ recorded trace (Law 11), so concurrent re-validation
   reaches the recorded root.

## Formalization plan (`spec/Rchain/`)

- Already in `Rho.lean`: `StrCong` (`comm`/`assoc`/`ident`/`par`) and `Reduce` (`comm`/`parLeft`/
  `parRight`) — the *permission* for concurrent reduction.
- Already in `Random.lean` (axiom): the associative splittable RNG merge (Law 19).
- **To add** (`Concurrent.lean`): a parallel-step relation `⟹` (a set of pairwise-independent redexes),
  then (1) diamond/confluence via `parLeft`/`parRight` commuting on disjoint redexes, and (2)
  linearization of `⟹` to `⟶`-sequences. The effect-level theorems (3, 4) lift the same argument to the
  tuple-space state monoid (Law 9/10).

> **Formal.** The full per-law catalog is [The 19 laws](the-19-laws.md) /
> [`spec/INVENTORY.md`](../../../spec/INVENTORY.md). The machine realization of each law is
> [The 19 laws → Rust code](../contributor/laws-to-rust.md).
