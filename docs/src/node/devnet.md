# Local devnet (Docker)

A local Docker **testnet** for deploying and testing rholang smart contracts, driven by the
`tools/devnet.sh` script. It brings up 1–3 bonded validators (full consensus, autopropose) plus optional
unbonded observers, seeds genesis with a funded deployer wallet, and exposes `deploy`/`query` helpers.

It is deliberately separate from [`tools/docker-network.sh`](../../tools/docker-network.sh), which is a
1–5 node *network-topology* harness (no autopropose, no deployer wallet, no deploy helpers).

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
tools/devnet.sh down [-v]                    stop the devnet (+ drop volumes)
```

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

- **Validators (1–3)** — each has its own keypair, is bonded in the shared `bonds.txt`, and runs
  `--autopropose`. Validator 0 (`devnet-bootstrap`) is the genesis ceremony-master (`--standalone`);
  validators 1–2 bootstrap from it.
- **Observers (0–3)** — unbonded, no autopropose; they bootstrap and replicate the chain.

Genesis is written to a temp dir and mounted read-only into the bootstrap: `bonds.txt` (one
`<pubkey> <stake>` line per validator) and `wallets.txt` (a funded deployer vault so deploys can pay
phlo). Contracts in `examples/` are mounted read-only at `/contracts` inside every node.

## Ports

Deploy is served on gRPC `40401` and Propose+Repl on `40402`; the helpers run the Rust `rnode` client
*inside* a node container (`docker exec`) so they reach both via `localhost`. The public HTTP API
(in-container `40403`) and the admin HTTP API (in-container `40405`) are also published to the host,
so a browser can reach them directly.

The host maps each node's deploy gRPC port to `40402 + 1000·i`, its public HTTP port to
`40403 + 1000·i`, and its admin HTTP port to `40405 + 1000·i` — the bootstrap is `i = 0`, so it
publishes `40402`/`40403`/`40405` directly.
