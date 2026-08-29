# Production-readiness handoff

Use this file to resume work in a fresh agent context. The authoritative checklist is
[`docs/src/node/production-readiness.md`](docs/src/node/production-readiness.md).

## Current checkout

Changes are intentionally uncommitted. Do not reset or discard them. The main implementation work
is in `sdk/src/consensus.rs`, `block-storage/src/dag/finalizer.rs`, `casper/src/blocks/block_receiver.rs`,
`casper/src/merging.rs`, and `tools/devnet.sh`.

## Verification

```sh
cargo test --workspace
git diff --check
bash -n tools/devnet.sh
```

Live honest-fault test:

```sh
tools/devnet.sh build
tools/devnet.sh up --validators 4 --no-autopropose
tools/devnet.sh verify-resilience
tools/devnet.sh verify-partition
tools/devnet.sh down -v
```

`verify-resilience` stops one validator and proves 3/4 convergence. `verify-partition` disconnects
two validators, proves finality does not advance at 2/2, reconnects them, and proves finality resumes.

`verify-partition` waits for all APIs, drives one proposer per isolated side through container-local
HTTP, distinguishes pre-cut blocks from blocks created by a partition side using the cut-time DAG
tip, verifies all four healed nodes accept one finalized hash/state, and reconnects on every exit.
On 2026-08-29 it passed live: neither 2/2 side finalized a post-cut block; after healing all four
validators accepted finalized block 16 with the same post-state root. The run exposed and fixed a
receiver/processor observation race; see `spec/AUDIT.md` R7d.

Signed ingress was hardened and verified: supported block version, content hash, proposer signature,
and every embedded user-deploy signature are checked before a block is accepted; decoded Casper
packets have parser-level size ceilings. See `spec/AUDIT.md` R7c.

## Do not overclaim

The implementation targets safety with less than one third Byzantine stake and liveness only when a
connected supermajority exists. Arbitrary Byzantine behavior, arbitrary partitions, membership/key
epochs, transport fuzzing, long-duration soak, formal crypto proofs, and independent security review
remain release gates in the checklist. A fresh context should implement those gates incrementally and
update the checklist only after running evidence exists.
