# Effect scheduling

[Concurrent reduction](concurrent-reduction.md) established the *process-level* contract: reduction is
permitted anywhere inside `|`, and any schedule reaches the same canonical state. This document moves
down one level to the **effect level** — the concrete `produce`/`consume` operations a reduction step
issues against the tuple space — and states the contract the *effect scheduler* must satisfy, founded in
the 19 laws.

## The effect level of reduction

A reduction step `P ⟹ Q` is realized by a set of **effects**:

- a **produce** `send(c, d, persistent)` — a send of datum `d` on channel `c`;
- a **consume** `receive(C, p, persistent)` — a receive on a **channel set** `C` (a join, Law 7) with
  patterns `p`.

Each effect has a **channel footprint** `chans(e)`: the singleton `{c}` for a send, and the channel set
`C` for a join receive. Two effects are **independent** when their footprints are disjoint.

The effects a step issues are exactly the COMM events that the RSpace turns into state changes; the
scheduler's job is to choose the order — and the concurrency — in which those effects are applied.

## Concurrency profile of the 19 laws (effect level)

**Enables — "you may apply disjoint effects concurrently":**

| # | Law | What it grants | Rust realization |
|---|-----|----------------|------------------|
| **9** | Merge is a monoid; non-conflicting logs commute | disjoint-channel effects commute — they may be applied in any order and merged | `rspace/src/merger/*` (`StateChange`/`ChannelChange` monoid) |
| **7** | Join commutativity | a join's channel set is hashed in sorted order, so its identity is order-independent | `rspace/src/hashing/stable_hash_provider.rs` (`hash_seq`) |
| **19** | `Blake2b512Random` associative splittable merge | each effect's RNG is pre-split, so `new`-freshness is schedule-independent | `crypto/src/hash/blake2b512_random.rs` |

**Constrains — "the *order* of same-channel effects is fixed":**

| # | Law | What it fixes | Rust realization |
|---|-----|---------------|------------------|
| **4** | `reduce_deterministic` | the consumed candidate is fixed, not scheduler-chosen | `rholang/src/reduce.rs` |
| **8** | Deterministic COMM | candidate selection is sorted-first by content hash | `rspace/src/space_matcher.rs`, `rspace/src/rspace.rs` |
| **11** | Replay determinism | the effect *order* is fixed — replay must reproduce the recorded trace | `rspace/src/replay_rspace.rs` |
| **10** | Merkle determinism | the trie root is the state — a given effect *set* yields the same root | `rspace/src/history/*` (canonical leaf serializers) |

The subtle point the rest of this document makes precise: **sorted selection (Law 8) removes the
order-sensitivity of *which stored candidate* a comm consumes, but not the *arrival order* — which
produce/consume matches a waiting continuation first.** A continuation is consumed once; the first effect
to reach its channel wins. That residual sensitivity is what forces same-channel effects to keep a fixed
total order.

## Soundness theorems

### S.1 Disjoint commute (Law 9)

```
chans(e₁) ∩ chans(e₂) = ∅   ⇒   apply(e₁; e₂)  ≡  apply(e₂; e₁)
```

Two effects on disjoint channels touch disjoint state, so their state changes commute (Law 9) and their
relative order — and concurrency — is irrelevant. This is the *permission* to run disjoint-channel
effects concurrently.

### S.2 Same-channel order (Law 4/8/11)

For effects sharing a channel, the candidate a comm consumes depends on arrival order (a waiting
continuation is consumed by the first produce to reach it; a stored datum is consumed by the
sorted-first, but *which* data are stored at all depends on arrival). So same-channel effects must be
applied in a **fixed total order** — the reducer's depth-first (DFS) order, which is exactly the order
the sequential scheduler uses and the order replay re-derives (Law 11).

### S.3 Sharded-scheduler linearization (the soundness of the design)

A scheduler that

1. applies **disjoint-channel effects concurrently**, and
2. serializes **same-channel effects in DFS order**,

reaches the **same canonical state** as the purely-sequential scheduler.

*Argument.* By S.1, reordering disjoint effects is state-invariant, so the only constraint is the
per-channel order; by S.2, applying same-channel effects in DFS order reproduces the sequential
candidate choices; by Law 10 the canonical root then matches. The one place the naive "spawn all effects"
approach fails — a continuation's channel footprint is unknown until its trigger runs — is handled by
the **prepend invariant** below.

## Realization map — theorem → mechanism

| Theorem | Mechanism | Location |
|---------|-----------|----------|
| S.1 Disjoint commute | per-channel effect FIFOs drained concurrently | `rholang/src/reduce.rs` |
| S.2 Same-channel DFS order | FIFO filled in DFS order + the **continuation prepend** invariant | `rholang/src/reduce.rs` |
| Law 7 (join key) | channel-set keys (`Vec<C>`) | `rholang/src/reduce.rs` |
| Law 8 (sorted selection) | candidate sort by content hash | `rspace/src/space_matcher.rs` |
| Atomicity | per-channel `TwoStepLock` | `rspace/src/concurrent/{multi_lock,two_step_lock}.rs` |
| Law 11 oracle | `ReplayRSpace` trace check | `rspace/src/replay_rspace.rs` |

### The continuation-prepend invariant

When a `produce`/`consume` on channel `c` matches a continuation, the continuation's resolved effects
must run **before the remaining pending effects on `c`** — i.e. they are *prepended* to `c`'s queue, not
appended. This reproduces the DFS order `trigger → continuation subtree → next sibling`, and it is what
lets a worker apply the trigger concurrently with other channels' workers while still keeping the
per-channel order. (A persistent/peek re-produce is the one effect that must instead land *after* the
continuation's whole subtree; the implementation defers it with a per-channel continuation-depth
counter.)

> Next: how the *data* moves in a comm — [Substitution and matching](substitution-matching.md).
