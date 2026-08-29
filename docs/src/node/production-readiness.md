# Production-readiness plan

This is the living gate for operating a validator network. A release is **not production-ready**
until every required gate below is checked in CI or explicitly accepted as an operational exception.
The protocol target is the standard weighted Byzantine model:

- safety while less than one third of bonded stake is Byzantine;
- finality progress when the network is connected and more than two thirds of bonded stake is live;
- no finality during a partition that leaves either side at or below two thirds;
- deterministic convergence after the partition heals.

No protocol can guarantee both safety and progress during an arbitrary partition or when Byzantine
stake reaches one third. In those cases this node must halt finalization, never lower its quorum, and
require an explicit, deterministic bond-set transition to recover.

## Gates

### Protocol and implementation

- [x] Exact integer `3 * stake > 2 * total` quorum check (no floating point).
- [x] Certificates require the exact bonded-validator key set and ignore unknown identities without
      granting them stake.
- [x] Quorum-intersection property test covering uneven weighted committees.
- [x] Block hash/signature, sequence, parent, bonds-cache, and deterministic-merge validation.
- [x] Verify every signed block/deploy payload and reject malformed/oversized messages before DAG insertion;
      record the result in `spec/AUDIT.md`.
- [ ] Define deterministic membership epochs, withdrawal, key rotation, and replay rules. Never
      remove a validator solely because it is unreachable.
- [ ] Complete the remaining Lean/Coq obligations and independently review cryptographic assumptions
      (the current formal crypto model is axiomatized).

### Fault and partition testing

- [x] Four-validator devnet: stop one validator, continue concurrent deploys, and verify all three
      survivors finalize the same block and post-state:

      ```sh
      tools/devnet.sh build
      tools/devnet.sh up --validators 4 --no-autopropose
      tools/devnet.sh verify-resilience
      tools/devnet.sh down -v
      ```

- [x] Unit tests prove 2/3 is not a quorum and 3/4 is a quorum.
- [x] Add a repeatable 2–2 network-partition test: assert neither side finalizes, heal, then assert
      one common finalized hash/state root.
- [ ] Add Byzantine simulations: equivocation, conflicting justifications, forged signatures,
      unknown senders, replayed sequence numbers, and resource-exhaustion inputs. Assert safety
      (never two conflicting finality certificates) and bounded failure behavior.
- [ ] Run restart/crash tests at every persistence boundary and compare state hashes after replay.
- [ ] Run a 24-hour multi-process soak with packet delay/loss/reordering and record convergence,
      memory, disk, and queue bounds.

### Operations and release

- [ ] Operate at least `3f + 1` independent validators for the intended `f` Byzantine failures;
      distribute operators, regions, keys, and backups.
- [ ] Enable authenticated peer transport, secure key storage/rotation, firewalling, rate limits,
      disk/log rotation, metrics, alerts, and snapshot/restore drills.
- [ ] Publish and stage-test runbooks for partition recovery, validator replacement, slashing
      evidence, and emergency halt.
- [ ] Reproducible release build, dependency/license/SBOM scan, fuzzing, external security review,
      and signed release artifacts.

## Current status

The checked gates are validated by the Rust workspace tests and the live four-validator resilience
run. The unchecked gates are deliberate blockers for an arbitrary-Byzantine production claim; until
they are completed, this repository should be treated as a controlled testnet implementation.
