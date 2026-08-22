# Operating the node: REPL and the Docker multi-node network

Two thin-client surfaces ship with the `rnode` binary: the interactive rholang **REPL** and the
scripted **Docker multi-node network**. This page is the operation guide for both.

---

## The REPL

`rnode repl` is a thin gRPC client: the interactive loop runs on your machine and forwards each line
to a running node, which parses, normalizes, and evaluates it and returns the rendered result. The
evaluation happens on the node — inside its **isolated `eval-*` store** — so REPL terms never touch
the node's live chain state (`rspace-*`).

### Start a node

```sh
# Local standalone node (creates genesis on first boot):
cargo run --release -p rchain-node --bin rnode -- run -s
```

The node serves **Deploy** on the external gRPC port **40401** and **Propose + Repl** on the internal
loopback port **40402** (propose/repl are not network-reachable — `spec/AUDIT.md` C1).

### Standalone genesis prerequisites

`run -s` makes the node the **genesis master**, and it needs two things the bare command above does
not set up for you:

1. **A validator key.** The genesis block must be signed, so provide a secp256k1 private key (32
   bytes, base16) through the *hidden* `--validator-private-key` flag:

   ```sh
   rnode run -s --validator-private-key 67e56582298859ddae725f972992a07c6c4fb9f62a8fff58ce3ca926a1063530
   ```

   Without it the node exits with `To create genesis block node must provide validator private key`.
   (`--validator-private-key-path` accepts a PEM file, but the Rust runtime currently reads only the
   hex form.)

2. **A wallets file.** The genesis ceremony parses `~/.rnode/genesis/wallets.txt` *strictly*, so the
   file must exist — an empty file is fine. `bonds.txt` is auto-generated when absent, but to be
   bonded as a validator, provide a `<public_key> <stake>` line (public key = uncompressed 65-byte
   point, base16) matching your key:

   ```sh
   mkdir -p ~/.rnode/genesis
   touch ~/.rnode/genesis/wallets.txt
   echo "04c591a8ff19ac9c4e4e5793673b83123437e975285e7b442f4ee2654dffca5e2d2103ed494718c697ac9aebcfd19612e224db46661011863ed2fc54e71861e2a6 100" \
     > ~/.rnode/genesis/bonds.txt
   ```

   A missing wallets file exits with `FAILED PARSING WALLETS FILE: … No such file or directory`.

### Bind to loopback (localhost-only)

The API server builds its listen address with `SocketAddr::from_str`, so `--api-host` must be a
**literal IP**, not a hostname — `--api-host localhost` fails with `invalid socket address syntax`.
For a localhost-only node, use `127.0.0.1` and skip the UPnP probe and external-IP guessing:

```sh
rnode run -s \
  --validator-private-key 67e56582298859ddae725f972992a07c6c4fb9f62a8fff58ce3ca926a1063530 \
  --host 127.0.0.1 --api-host 127.0.0.1 --no-upnp
```

`--host 127.0.0.1` sets the advertised protocol address to loopback and also stops the node probing
external services to guess its public IP (otherwise it logs `guessing your external IP address…`);
`--no-upnp` disables the gateway probe. All node data lives under `~/.rnode` by default.

### Run the REPL

```sh
# In a second terminal, against the local node (defaults: localhost:40402):
cargo run --release -p rchain-node --bin rnode -- repl

# Against a remote node:
rnode --grpc-host <host> --grpc-port 40402 repl
```

The prompt is `rholang $ ` with line-editing, history, and tab-completion over the REPL keywords
(`stdout`, `stdoutack`, `stderr`, `stderrack`, `for`, `!!`). Each submitted term:

1. is parsed and normalized; a syntax error surfaces as `Error: …` without evaluating;
2. on success, the normalized term is echoed on the **node console** as an `Evaluating:` line;
3. is evaluated, and the result is printed:

```
Deployment cost: 33
Storage Contents:
@{Unforgeable(0x…)}!(0) |
for( … ) { Nil } | …
```

`:q` (or EOF/`Ctrl-D`) quits. A single evaluation error stops the loop — matching the Scala
thin-client REPL.

### Evaluate files

```sh
rnode eval file.rho [--print-unmatched-sends-only]
```

`eval` reads the files client-side, sends each whole program to the node, and prints a `Result for
<file>:` header per file.

### REPL internals (short)

| Piece | File |
|---|---|
| client loop (`:q`, coloring) | `node/src/runtime/repl_runtime.rs` |
| rustyline console (prompt, history, completion) | `node/src/effects/console_io.rs` |
| gRPC client | `node/src/effects/repl_client.rs` |
| server eval (`Evaluating:` echo + result) | `node/src/api/grpc/repl_grpc_service.rs` |
| isolated eval store | `node/src/runtime/node_runtime.rs` (`"eval"` prefix → `rspace/src/factory.rs`) |

`rho:io:stdout` / `rho:io:stderr` print on the **node** process (server side), matching the Scala
thin-client model — not on the REPL client.

---

## The Docker multi-node network

`tools/docker-network.sh` builds the `rnode` image and boots a local network of **1–5 nodes** on a
Docker bridge, then lets the CLI drive it. The image is built from
[`docker/rnode/Dockerfile`](../../../docker/rnode/Dockerfile).

### Prereqs

- `docker` (a running daemon)
- `openssl` (to read the bootstrap node-id from its generated TLS cert)

### Commands

```sh
tools/docker-network.sh build            # build the rnode:local image
tools/docker-network.sh up [N]           # start a bootstrap + N-1 peers (default 3, N in 1..5)
tools/docker-network.sh status           # docker ps for the network
tools/docker-network.sh logs <node>      # tail a node's logs (bootstrap, peer1, …)
tools/docker-network.sh cli <node> <subcommand…>   # run the Rust client against <node>
tools/docker-network.sh down             # stop the network
tools/docker-network.sh down -v          # stop + delete the data volumes
```

### Topology and genesis

`up N` starts:

1. **`bootstrap`** — `rnode run -s` (standalone). It generates its own TLS cert, **creates the
   genesis block**, and is bonded as the sole validator via a fixed validator key
   (`VALIDATOR_PRIV_HEX` in the script) and a generated `bonds.txt`/`wallets.txt` mounted read-only.
2. **`peer1` … `peer{N-1}`** — `rnode run --bootstrap rnode://<bootstrap-id>@bootstrap?protocol=40400&discovery=40404`.
   Each peer gets its own data volume (so its node identity — the TLS cert — is stable), connects to
   the bootstrap over the **TLS protocol transport** (port 40400), and syncs the finalized fringe.

The bootstrap's node-id is read from its generated certificate (the cert CommonName is the base16
keccak-20 address). The script waits for the cert, extracts the id with `openssl`, and passes it into
each peer's bootstrap URL.

### Ports

Each node binds the same in-container ports; `up` maps each node's **deploy** gRPC port — the only
network-reachable gRPC service — to a distinct host port so you can also reach a node from the host.
Propose/repl bind loopback-only (`127.0.0.1:40402`), so they are not host-mapped:

| Service | In-container | Host (bootstrap / peerN) |
|---|---|---|
| protocol (TLS peer transport) | 40400 | not mapped (network-internal) |
| discovery (Kademlia) | 40404 | not mapped |
| gRPC API — Deploy | 40401 | 40402 / 40402 + 1000·N |
| gRPC API — Propose/Repl | 40402 | not mapped (loopback-only) |
| HTTP | 40403 | not mapped |
| admin HTTP | 40405 | not mapped (loopback-only) |

### Interacting from the CLI

`cli <node> <subcommand…>` runs the Rust client **inside the node container** via `docker exec`, so it
can reach both the deploy server (external port **40401**) and the loopback-only propose/repl server
(`127.0.0.1:40402`):

```sh
tools/docker-network.sh cli bootstrap status
tools/docker-network.sh cli bootstrap repl          # interactive REPL against the bootstrap
tools/docker-network.sh cli peer1 status
```

No `--grpc-port` is passed: the client defaults to the right port per subcommand (`deploy`/`status`/
`show-block`/… → 40401; `repl`/`eval`/`propose` → 40402), matching the node's port split.

`deploy` / `eval` read a file at a path inside the container; copy a local file in first:

```sh
docker cp demo.rho bootstrap:/tmp/demo.rho
tools/docker-network.sh cli bootstrap deploy /tmp/demo.rho
```
