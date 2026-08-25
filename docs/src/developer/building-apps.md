# Building applications on the local devnet

This page is for developers building apps against a running RNode — wallets, block explorers, dapps,
indexers, and tooling. It covers standing up the local Docker **devnet** and configuring your app to
**deploy rholang** and **read back the results**.

The devnet is a throwaway local testnet: 1–3 bonded validators with real consensus and autopropose, a
funded deployer wallet, and every API surface published to `localhost`. It is *not* for production —
the validator and deployer keys are public dev keys baked into the script.

## 1. Bring up the devnet

Prereqs: `docker` (a running daemon) and `openssl`.

```sh
tools/devnet.sh build                 # build the rnode:local image (once)
tools/devnet.sh up --validators 1     # 1 validator, autoproposing (the default)
# or a small multi-validator net:
tools/devnet.sh up --validators 3 --observers 1
```

The **bootstrap** node (validator 0) is the one apps should talk to. Tear down with
`tools/devnet.sh down -v` (drop the `-v` to keep the data volumes).

## 2. Endpoints

There are two wire surfaces:

- **HTTP (JSON)** — the primary surface for apps. Read-only queries and signed deploys live on the
  *public* API (`40403`); block production lives on the *admin* API (`40405`).
- **gRPC (protobuf)** — the lower-level `DeployService` (external, `40401`) and the
  `ProposeService`/REPL (loopback-only, `40402`).

### Ports (bootstrap = node 0)

| Service | In-container | Host: bootstrap | Host: node *i* |
|---|---|---|---|
| Deploy gRPC (`DeployService`) | 40401 | 40402 | 40402 + 1000·*i* |
| Propose / REPL gRPC | 40402 | — (loopback) | — |
| Public HTTP API | 40403 | 40403 | 40403 + 1000·*i* |
| Admin HTTP API | 40405 | 40405 | 40405 + 1000·*i* |
| protocol (peer TLS) | 40400 | — | — |
| discovery (Kademlia) | 40404 | — | — |

Against the bootstrap: public HTTP at `http://localhost:40403`, admin HTTP at
`http://localhost:40405`, deploy gRPC at `localhost:40402`.

> **Propose/REPL is loopback-only.** It binds `127.0.0.1:40402` inside the container and is not
> host-mapped; `tools/devnet.sh` reaches it via `docker exec`. From an app, drive block production via
> the admin HTTP `POST /api/v1/propose` instead.

### The OpenAPI document

The v1 HTTP API is described by a hand-written OpenAPI 3.0 document the node serves itself:

```sh
curl -s http://localhost:40403/api/v1/openapi.json | jq
```

## 3. The three operations an app needs

1. **Deploy** — submit signed rholang for inclusion in a block.
2. **Read responses** — observe data a contract produced at a name.
3. **Force a block** — propose (only needed when the node is *not* autoproposing; the devnet
   autoproposes).

### 3.1 Deploy rholang

The fastest path during development is the CLI helper, which signs with the devnet's funded deployer
key and pays phlo:

```sh
tools/devnet.sh deploy hello.rho   # deploys examples/hello.rho to the bootstrap
```

From an app, submit a signed deploy over HTTP:

```http
POST /api/v1/deploy                 # public HTTP 40403
Content-Type: application/json

{
  "data": {
    "term": "@\"hello\"!(\"world\")",
    "timestamp": 1724500000000,
    "phloPrice": 1,
    "phloLimit": 1000000,
    "validAfterBlockNumber": -1,
    "shardId": "root"
  },
  "deployer": "<base16 secp256k1 public key>",
  "signature": "<base16 secp256k1 signature>",
  "sigAlgorithm": "secp256k1"
}
```

The response is the **deploy signature** (a hex string); keep it to poll status. The `signature` is a
secp256k1 signature over the protobuf-serialized `data` object; `deployer` is the signer's public key
and `signature` its signature, both base16. The devnet's funded deployer is validator 0
(`DEPLOYER_PRIV` in `tools/devnet.sh`); use the CLI to sign during development, and in production sign
with your own key and fund the corresponding wallet in genesis.

### 3.2 Read responses

Three ways, in order of increasing specificity:

1. **Poll a deploy's result** — after a deploy, `GET /api/v1/deploy-status/{deploySignature}` returns a
   `DeployExecStatus`: `processedWithSuccess` (with the `deployResult` expression), `processedWithError`,
   or `notProcessed` (not yet in a block).
2. **Run a term and read its result** — `POST /api/v1/explore-deploy` runs a rholang term against the
   current state without signing or persisting a deploy, and returns the reduced expression.
3. **Read data at a name** — subscribe with the CLI, or read point-in-time by block hash over HTTP.

CLI (streaming subscribe — blocks until data arrives):

```sh
tools/devnet.sh query hello          # listens for data at the public name @"hello"
```

HTTP (point-in-time at a block hash; an empty `blockHash` means the current state):

```http
POST /api/v1/data-at-name-by-block-hash
Content-Type: application/json

{ "name": { "ExprString": "hello" }, "blockHash": "", "usePreStateHash": false }
```

A public name `@"hello"` is expressed as the rholang expression `{"ExprString": "hello"}`; the full
`RhoExpr` JSON shape (ints, lists, maps, tuples, bytes, unforgeables) is in the OpenAPI schema and in
`node/src/api/rho_expr.rs`.

### 3.3 Force a block

```sh
tools/devnet.sh propose
# or over HTTP (admin):
curl -s -X POST http://localhost:40405/api/v1/propose
```

## 4. End-to-end example (curl)

```bash
# Is the bootstrap up?
curl -s http://localhost:40403/api/v1/status

# Deploy + observe with the CLI helpers (signs + pays phlo for you):
tools/devnet.sh deploy hello.rho
tools/devnet.sh query hello          # -> "world"

# Run a term without a deploy and read the result:
curl -s -X POST http://localhost:40403/api/v1/explore-deploy \
  -H 'Content-Type: application/json' \
  -d '"1 + 1"'

# Inspect the chain:
curl -s http://localhost:40403/api/v1/blocks
curl -s http://localhost:40403/api/v1/block/<blockHash>
```

## 5. Reference — HTTP routes

All routes are also served without the `/v1` segment (the legacy `/api/…` forms) except where noted.
Requests and responses are JSON (`camelCase` keys).

| Method | Path | Purpose | Body / response |
|---|---|---|---|
| GET | `/api/v1/status` | node version, address, peers, block height | `ApiStatus` |
| POST | `/api/v1/deploy` | submit a signed deploy | `DeployRequest` → deploy signature (string) |
| GET | `/api/v1/deploy-status/{sig}` | a deploy's execution status | `DeployExecStatus` |
| POST | `/api/v1/explore-deploy` | run a term, return its result | raw JSON string term → `ExploratoryDeployResponse` |
| POST | `/api/v1/explore-deploy-by-block-hash` | run a term at a block hash | `{term, blockHash, usePreStateHash}` |
| POST | `/api/v1/data-at-name-by-block-hash` | data at a name, at a block hash | `{name, blockHash, usePreStateHash}` → `RhoDataResponse` |
| GET | `/api/v1/blocks` | recent blocks | `LightBlockInfo[]` |
| GET | `/api/v1/block/{hash}` | a block and its deploys | `BlockInfo` |
| GET | `/api/v1/openapi.json` | the OpenAPI document | JSON schema |
| POST | `/api/v1/propose` | **admin (40405)** — force a block | string |

The public API also serves `/api/last-finalized-block`, `/api/blocks/{start}/{end}`,
`/api/blocks/{depth}`, `/api/deploy/{deployId}`, `/api/is-finalized/{hash}`, `/version`, `/status`,
`/metrics`, and `/api/transactions/{hash}` (the transactions route answers 404 unless
`api-server.enable-reporting` is on).

## 6. Reference — gRPC services

`DeployService` (external, host `40402`) exposes `doDeploy`, `deployStatus`, `getBlock`, `getBlocks`,
`listenForDataAtName`, `getDataAtName`, `listenForContinuationAtName`, `findDeploy`,
`lastFinalizedBlock`, `isFinalized`, `bondStatus`, `exploratoryDeploy`, `getBlocksByHeights`,
`getEventByHash`, `visualizeDag`, `machineVerifiableDag`, and `status`. `ProposeService` (loopback)
exposes `propose` and `proposeResult`. The protobuf definitions live in
`models/proto/{deploy_service_v1,propose_service_v1,deploy_service_common}.proto`.

## 7. Notes and gotchas

- **Throwaway keys.** The validator/deployer keys in `tools/devnet.sh` are public dev keys; never reuse
  them for anything with real value.
- **CORS.** The admin HTTP API is CORS-restricted by default (`api-server.enable-devnet-cors = false`)
  so a foreign origin cannot trigger block production. The devnet opts in with `--api-enable-devnet-cors`.
- **Rate limit.** Deploy/explore endpoints on the public HTTP API are rate-limited; exceeding the limit
  returns HTTP 429.
- **Phlo.** Deploys pay `phloLimit × phloPrice`; the devnet seeds a funded deployer wallet so the
  helpers work out of the box. In your own genesis, fund the wallet that signs your deploys.
- **Reporting is off.** `api-server.enable-reporting` is `false` by default, so the transaction/event
  reporting routes are unavailable unless you enable them.
- **One node vs many.** Non-bootstrap nodes offset their host ports by `1000·i`; for a single-validator
  devnet the bootstrap numbers above are the only ones you need.
