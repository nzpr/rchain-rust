# Local devnet (Docker)

A local Docker **testnet** for deploying and testing rholang smart contracts, driven by the
`tools/devnet.sh` script. It brings up 1–4 bonded validators (full consensus, autopropose) plus optional
unbonded observers, seeds genesis with a funded deployer wallet, and exposes `deploy`/`query` helpers.

The same script's `up --nodes N` mode is the bare 1–5 node *network-topology* harness (no autopropose,
no deployer wallet, no deploy helpers) — see [Operating the node](operating.md).

> **Security.** The validator and deployer keys baked into the script are throwaway dev keys for a
> *local* testnet only. Never reuse them for anything with real value.

## Prereqs

- `docker`
- `openssl` (to read the bootstrap node-id from its generated TLS certificate)

## Commands

```text
tools/devnet.sh build                        build the rnode:local image
tools/devnet.sh up --validators N [--observers M]
                                             start N validators (default 1) + M observers (default 0)
tools/devnet.sh deploy <contract.rho>        signed deploy to the bootstrap (file lives in examples/)
tools/devnet.sh eval <file.rho>              thin-client REPL eval of a file on the bootstrap
tools/devnet.sh query <name>                 listen for data at a public name
tools/devnet.sh propose                      force the bootstrap to propose a block
tools/devnet.sh status                       docker ps for the devnet
tools/devnet.sh logs <node>                  tail a node's logs
tools/devnet.sh diagnose                     per-node health check (PASS/FAIL)
tools/devnet.sh verify-resilience            stop validator 3; verify 3/4 finality + convergence
tools/devnet.sh down [-v]                    stop the devnet (+ drop volumes)
```

**Autopropose and friends are `up` flags, not build flags** — set them per run:

```text
tools/devnet.sh up --validators 1                    # autopropose ON (default)
tools/devnet.sh up --validators 1 --no-autopropose   # autopropose OFF — blocks only via `propose`
tools/devnet.sh up --validators 1 --no-propose-on-deploy
                                                     # deploy no longer auto-proposes
```

`--admin` (default on) publishes the admin HTTP port `40405`, which exposes only `POST /api/v1/propose`
(*force a block*) — deploys always go through the public `40403` surface or the deploy gRPC, never
`40405`. Run `tools/devnet.sh help` for the full matrix (autopropose / propose-on-deploy / admin /
deployer-key / nodes).

The resilience check is destructive to the local devnet: it stops `devnet-validator-3`. Recreate the
four-node network before repeating it.

## Worked example

```bash
# 1. Build the image.
tools/devnet.sh build

# 2. Start a single validator (creates genesis and autoproposes).
tools/devnet.sh up --validators 1

# 3. Deploy a contract (examples/hello.rho sends "world" on the public name "hello").
tools/devnet.sh deploy hello.rho

# 4. The bootstrap autoproposes a block containing the deploy; then query the result.
tools/devnet.sh query hello          # -> "world"

# 5. Or a 3-validator + 1-observer network:
tools/devnet.sh up --validators 3 --observers 1
tools/devnet.sh deploy hello.rho
tools/devnet.sh query hello

# 6. Tear down.
tools/devnet.sh down -v
```

## Node roles

- **Validators (1–4)** — each has its own keypair, is bonded in the shared `bonds.txt`, and runs
  `--autopropose`. Validator 0 (`devnet-bootstrap`) is the genesis ceremony-master (`--standalone`);
  validators 1–2 bootstrap from it.
- **Observers (0–3)** — unbonded, no autopropose; they bootstrap and replicate the chain.

Genesis is written to a temp dir and mounted read-only into the bootstrap: `bonds.txt` (one
`<pubkey> <stake>` line per validator) and `wallets.txt` (a funded deployer vault so deploys can pay
phlo). Contracts in `examples/` are mounted read-only at `/contracts` inside every node.

Blocks are produced **on their own**: every node runs with `--dev-mode --deployer-private-key`, so
`--autopropose` injects a signed `Nil` dummy deploy whenever the pool is empty and keeps proposing.
`up` blocks until `latestBlockNumber` is advancing.

## Casper fidelity & gaps

The devnet runs the real CBC-Casper data path — genesis bonding, deploy-driven proposals, a
cross-justified block-DAG, the monotone fringe estimator, and `> 2/3`-stake finality — with these
inputs simplified (theory in [Consensus (Casper)](consensus.md)):

- **Threshold-dependent availability.** Validators have equal stake `100`. A three-validator devnet
  requires all three to finalize because two is exactly `2/3`. A four-validator devnet tolerates one
  stopped validator because the remaining `3/4` is a strict supermajority. Run
  Start with `tools/devnet.sh up --validators 4 --no-autopropose`, then run
  `tools/devnet.sh verify-resilience` to exercise that path end to end with paced round-robin
  proposals. Pacing is intentional: it ensures each proposal incorporates the preceding peer views
  instead of using the continuous dev-mode timer as a throughput stress test.
- **Dummy `Nil` deploys** stand in for real user traffic, and **autopropose is a tight loop** (no
  backoff), so the block rate is unbounded rather than a production cadence.
- **Single shard** (`root`); **no Byzantine behavior** (all validators honest — the slash path exists
  but is never exercised); **no partitions/latency** (local Docker bridge); **throwaway keys** with no
  economic security; a small fixed validator set (≤4).

## Ports

Deploy is served on gRPC `40401` and Propose+Repl on `40402`; the helpers run the Rust `rnode` client
*inside* a node container (`docker exec`) so they reach both via `localhost`. The public HTTP API
(in-container `40403`) and the admin HTTP API (in-container `40405`) are also published to the host,
so a browser can reach them directly.

The host maps each node's deploy gRPC port to `40402 + 1000·i`, its public HTTP port to
`40403 + 1000·i`, and its admin HTTP port to `40405 + 1000·i` — the bootstrap is `i = 0`, so it
publishes `40402`/`40403`/`40405` directly.
