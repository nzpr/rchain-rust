# QuCalc architecture

This document describes the technical architecture of the `qucalc` crate and the
system contracts it backs. For the *governance* semantics specifically, see
[`Governance.md`](../../../qucalc/rholang/Governance.md); for runnable demos, see
[`examples.md`](examples.md).

## Layering

```
                        rholang (contracts / examples)
                                    │  rho:qucalc:* / rho:gov:*  (system channels)
                                    ▼
   rholang/src/system_processes.rs  ← ScalaBodyFn handlers, wire-form parsing
                                    │
                                    ▼
   qucalc/src/lib.rs                ← pure core (no I/O, no non-determinism)
     ├── ZFA predicate  (pauli_closed ∧ count_balanced)
     ├── dialectical_synthesis
     └── gov::{resolve_weights, trust_levels, censure, tally_ranked, tally_approval}
```

The hard rule: **everything that must be reproduced bit-for-bit by every peer
lives in the pure core** (`qucalc/src/lib.rs`). The `system_processes.rs` layer is
only parsing (rholang wire form → typed values), dispatch, and producing the
result. No state, no randomness, no wall-clock.

## The ZFA predicate

A twist history is a sequence over the 8-symbol alphabet, each symbol a Pauli
generator or ±identity:

| value | symbol | matrix |
|---|---|---|
| 0 | `^` | +σ_y |
| 1 | `v` | −σ_y |
| 2 | `>` | +σ_x |
| 3 | `<` | −σ_x |
| 4 | `/` | +σ_z |
| 5 | `\` | −σ_z |
| 6 | `+` | +I |
| 7 | `-` | −I |

The fold multiplies the 2×2 matrices left-to-right as **exact integer complex**
(entries stay in {−1, 0, 1}). A history is:

- **Pauli-closed** iff the fold lands in the scalar group {±I, ±iI} — i.e. the
  off-diagonal entries are (0,0) and the diagonal entries are equal.
- **Count-balanced** iff `count_pos == count_neg` (even symbol values are
  "positive", odd values "negative").
- **ZFA** iff both hold — a half-spin closure (a fluxoid).

`pauli_phase` returns the scalar phase as {+I, −I, +iI, −iI}; `achieves_zfa`
combines the two conditions. Because the arithmetic is exact integer complex, the
predicate is deterministic and replay-safe — a floating-point or randomized
implementation would break consensus.

## Dialectical synthesis (Blanket Fusion)

The port of quantum-os's `qlf_ai_coprocessor`:

```
S + M    premise₁  (subject + middle_pos)
- + P    premise₂  (middle_neg + predicate)
─────
S + - P  intersection   (Blanket Fusion concatenates the premises)
S P      geometry       (the middle-term gauge pair +- annihilates at the seam)
```

The middle term `+-` sits *exactly* at the seam between `subject` and `predicate`,
so the residue is `subject ++ predicate`. Annihilation is deliberately **scoped to
the seam**: the generic `annihilate_gauge` helper (which cancels the first
adjacent `+-`/`-+` anywhere) is *not* run over the whole concatenation, because
that would eat an incidental gauge pair inside a premise (or a subject ending in
`-`).

If the residue is ZFA-closed, it is a stable fluxoid — the *Synthesis* — and
`rho:qucalc:fuse` mints it as a content-addressed capability.

## The governance decision core (`qucalc::gov`)

All functions are **total, deterministic, order-insensitive folds** over
`BTreeMap`/`BTreeSet`, so results never depend on input ordering — the "no central
counter" guarantee.

| Function | Semantics |
|---|---|
| `resolve_weights(direct_voters, delegations, trust)` | Each member's vote walks its delegation chain to the first direct voter (cycles / dead-ends abstain); `weight(d) = Σ (1 + level(m))`. Clamped ≥ 0. |
| `trust_levels(ratings, admins)` | Admin-rooted web of trust as a least fixed point; conferrals capped at `min(v, level(rater) − 1)` (strictly decreasing, so two level-0 members can't bootstrap each other). |
| `censure(censures, levels, vouchers)` | A member is discredited at `max(2, ⌈⅔·\|eligible\|⌉)` censures from peers at ≥ their level; level → 0 and vouchers are slashed, iterated to a decreasing fixed point. |
| `tally_ranked` / `tally_approval` | Weighted IRV (strict majority, lowest-count elimination, deterministic tie-break) and weighted approval. |

## Native system processes

Added to the fixed-channel table (bytes 22–29) and dispatch table (`BodyRefs`
23–30) in [`system_processes.rs`](../../../rholang/src/system_processes.rs):

| URN | Arity | Semantics |
|---|---|---|
| `rho:qucalc:zfa` | 2 | `(twists, ret) -> (zfa: Bool, phase: Int)` |
| `rho:qucalc:grant` | 2 | `(twists, ret) -> capUri \| Nil` — mints a ZFA proof as a registry capability |
| `rho:qucalc:verify` | 2 | `(cap, ret) -> Bool` — re-verifies across deploys |
| `rho:qucalc:fuse` | 3 | `(subject, predicate, ret) -> (geometry, capUri) \| Nil` |
| `rho:gov:resolveWeights` | 4 | `(voters, delegations, trust, ret) -> Map<voter, weight>` |
| `rho:gov:trustLevels` | 3 | `(ratings, admins, ret) -> Map<member, level>` |
| `rho:gov:censure` | 4 | `(censures, levels, vouchers, ret) -> (discredited, levels)` |
| `rho:gov:tally` | 4 | `(ballots, weights, mode, ret) -> winner \| Nil` |

Member ids are either strings or `deployerId` unforgeables (canonicalized to the
base16 hex of the public key); capability minting is content-addressed
(`blake2b256` of the value), so a given proof always maps to the same URI.

## Determinism & replay guarantees

1. Exact integer arithmetic only — no floats, no `rand`, no wall-clock in the core.
2. `BTreeMap`/`BTreeSet` ordering — results independent of input order.
3. Content-addressed URIs — capability identity is a pure function of the value.
4. Envelopes bound to `*deployerId` — identity is unforgeable, not a spoofable
   string.
5. Registry persistence — proofs/decisions survive across deploys and replay.
