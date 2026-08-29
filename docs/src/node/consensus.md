# Consensus (Casper)

The node reaches agreement on a single chain state using **CBC-Casper** — a *correct-by-construction*
family of consensus protocols. This chapter describes the software's behavior: what a block is, how
blocks relate, and when one is **finalized**.

## Blocks and justifications

A **block** is a validator's message: it bundles a set of deploys (the transactions it is proposing), a
pointer to the resulting state, and — crucially — its **justifications**: the hashes of the other
blocks the validator has seen and is building on.

Block production is **deploy-driven**: a validator proposes only when it has new work — user deploys,
slashes, or an epoch change — or when it attests (an empty block) to advance finality. An idle network
produces no blocks. (The devnet's `--autopropose` adds a periodic dummy-deploy tick on top of this, a
dev-mode convenience — see [Local devnet](devnet.md).)

A validator's **justifications** are its view of the network. Two blocks that justify each other's
predecessors are *consistent*; blocks that justify conflicting histories are in competition. The
collection of all blocks, joined by justification edges, is a **DAG** — the *block-DAG* — rather than a
single chain.

## The fringe

Casper's estimator selects, from the DAG, the **fringe**: the set of latest messages such that there is
**one message per bonded validator**, and the set forms an **antichain** (no message justifies another
in the same fringe). The fringe is the frontier of the "agreed" part of the DAG; the blocks behind it
are what every validator is effectively building on.

The fringe is monotone: as new blocks arrive, the fringe only advances — its heights never decrease,
and a message's **seen set** (everything it transitively justifies) only grows. This **monotonicity**
(Law 15) is what prevents the estimator from "flipping" its choice of a history.

## Finality: > 2/3 bonded stake

A block becomes **finalized** when a **supermajority** of the bonded stake has attested to it. The
threshold is *strictly more than two thirds*:

```
isSuperMajority(stake, total)  =  3 · stake  >  2 · total
```

The comparison is done in exact integer arithmetic — never in floating point — so the `2/3` boundary is
precise for stakes of any size (Law 14). A finalized block can never be reverted: no set of validators
holding at most one third of the stake can out-vote it.

Concretely, the finalizer (`Finalizer` in the DAG layer) tracks the **support** each block has from
the bonded validators, and advances the finalized fringe whenever that support crosses `> 2/3`.

### Offline validators

Losing a network connection does **not** eject a validator from consensus. Peer reachability is a
local observation (and may only be a network partition), so using a timeout to change the bonded set
would let honest nodes calculate different validator sets. The disconnected peer's last message stays
in the DAG as its latest justification.

The remaining validators continue producing and finalizing blocks when their combined stake is
strictly greater than two thirds of the bonded stake. For example, three live validators out of four
equal-stake validators can progress after the fourth dies. Two out of three equal-stake validators
cannot finalize: exactly `2/3` deliberately does not satisfy the strict threshold.

This safety statement uses the standard Byzantine assumption: less than one third of bonded stake
equivocates, and an honest validator does not attest incompatible candidates. Every finalization
certificate is checked against the exact bonded-validator set (not merely a set of the same size),
and every candidate message needs strictly more than two thirds of bonded stake observing it. Thus
two certificates overlap in more stake than the Byzantine bound. Concurrency does not change this:
calculation is deterministic for a DAG snapshot, while independently calculated certificates remain
subject to the same quorum-intersection rule.

A validator leaves the consensus set only through a deterministic state transition: withdrawal, or a
slash system deploy for protocol-invalid behavior. Silence alone is not slashable. Operationally,
recover the node or finalize an authorized bond-set change while the surviving stake still exceeds
the threshold; once more than one third of bonded stake is unavailable, the protocol intentionally
halts finalization rather than weakening safety.

## Block validity (Laws 16–17)

Before a block is added to the DAG, the node validates it:

- **Block number** is `max(parent numbers) + 1`; a validator's **sequence number** strictly increases by
  one per block (no reuse, no gaps).
- **Content addressing**: the block's hash is `Blake2b256` of the block *minus* its hash and signature —
  so the hash determines the block body, and any tampering changes the hash.
- **Bonds cache** equals the proof-of-stake state: the stake a block carries must match the stake the
  chain has actually bonded.
- **Merge determinism**: when a validator merges concurrent deploys, the result is a unique, min-cost
  selection; numeric channels are non-negative and never overflow.

These checks are what make the DAG self-consistent — a node rejects any block whose claimed view
doesn't match the real one.

## Why correctness-by-construction matters

A blockchain's consensus only works if every node computes the *same* next state from the *same*
inputs. CBC-Casper is "correct by construction" in the sense that its safety — no two finalizations of
conflicting histories — is a property of the protocol's *structure* (the `> 2/3` threshold and the
fringe's monotonicity), not of a particular implementation. The rholang layer underneath provides the
matching determinism (the canonical order of Law 1, the deterministic COMM of Law 4), so the whole
stack — from a deploy to a finalized block — is deterministic end to end.

> **Formal.** Finality and the fringe are Laws 14–15; block validity and merge determinism are Laws
> 16–17; the height map is Law 18. See [The 19 laws](../formal/the-19-laws.md) and the DAG finalizer
> in [`spec/INVENTORY.md`](../../../spec/INVENTORY.md).
