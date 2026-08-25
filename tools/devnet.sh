#!/usr/bin/env bash
#
# devnet — a local Docker testnet for deploying and testing rholang smart contracts.
#
# Unlike tools/docker-network.sh (a 1..5 node *network-topology* harness), this is a *contract
# devnet*: it brings up 1..3 bonded validators (full consensus, autopropose) plus optional
# observers, seeds genesis with a funded deployer wallet, and exposes deploy/query helpers.
#
#   tools/devnet.sh build                        build the rnode:local image
#   tools/devnet.sh up --validators N [--observers M]
#                                                start N validators (default 1) + M observers (default 0)
#   tools/devnet.sh deploy <contract.rho>        signed deploy to the bootstrap (file lives in examples/)
#   tools/devnet.sh eval <file.rho>              thin-client REPL eval of a file on the bootstrap
#   tools/devnet.sh query <name>                 listen for data at a public name
#   tools/devnet.sh propose                      force the bootstrap to propose a block
#   tools/devnet.sh status                       docker ps for the devnet
#   tools/devnet.sh logs <node>                  tail a node's logs
#   tools/devnet.sh down [-v]                    stop the devnet (+ drop volumes)
#
# Nodes publish Deploy gRPC (in-container 40401), the public HTTP API (in-container 40403), and the
# admin HTTP API (in-container 40405) to the host; Propose+Repl stay in-container on 40402, reached
# by the helpers via `docker exec`. The host needs only docker + openssl.
#
# SECURITY: the validator/deployer keys below are throwaway keys for a LOCAL testnet only.
# Never reuse them for anything with real value.

set -euo pipefail

cd "$(dirname "$0")/.."

IMAGE="${RNODE_IMAGE:-rnode:local}"
NETWORK="devnet"
BOOTSTRAP="devnet-bootstrap"
PREFIX="devnet"

# Throwaway validator keypairs (secp256k1, base16). validator[0] also funds the deployer wallet, so
# the deployer private key is validator[0]'s private key.
VALIDATOR_PRIV=(
  "a68a6e6cca30f81bd24a719f3145d20e8424bd7b396309b0708a16c7d8000b76"
  "b8a48b02757c0cfc9325498a93c3b28582e3967072f8fab0cdc8bd04d0d401ee"
  "d78ff60a424d71ce99d6b7d7f44a8c49b38a3757ff9e6fa9b32fcba8aa2c973b"
)
VALIDATOR_PUB=(
  "04f700a417754b775d95421973bdbdadb2d23c8a5af46f1829b1431f5c136e549e8a0d61aa0c793f1a614f8e437711c7758473c6ceb0859ac7e9e07911ca66b5c4"
  "04dbe32c2062240a4ba0bcad01d7edd98c78b51c77765d5e1e5e9fa3743d2f12a1f82f42cd7dc4f41445979117d790f23e9b3d08d0aa06d527c236172043e747fc"
  "04d8b6c325ae12e89823866b2a292a62d7acee520954761890a1621fef79dca1c8e8df79dd8519480e5c015ae6cf3ba7de8669e260561616a36eb9c308b5983ab0"
)
MAX_VALIDATORS=${#VALIDATOR_PRIV[@]}

# Deployer = validator[0] (its pubkey -> REV address is funded in genesis).
DEPLOYER_PRIV="${VALIDATOR_PRIV[0]}"
DEPLOYER_REV_ADDR="11112VYAt8rUGNRRZX3eJdgagaAhtWTK8Js7F7X5iqddMVqyDTtYau"
DEPLOYER_BALANCE=1000000000000

# Contract sources are mounted read-only at /contracts; deploy/eval reference them by basename.
CONTRACTS_DIR="$(pwd)/examples"

GRPC_BASE=40402   # host port mapped to the bootstrap's deploy gRPC (in-container 40401)
HTTP_BASE=40403   # host port mapped to the bootstrap's public HTTP API (in-container 40403)
ADMIN_BASE=40405  # host port mapped to the bootstrap's admin HTTP API (in-container 40405)

usage() {
  sed -n '2,24p' "$0" >&2
  exit 2
}

validator_name() { echo "${PREFIX}-validator-${1}"; }
observer_name()   { echo "${PREFIX}-observer-${1}"; }

cmd_build() {
  docker build -f docker/rnode/Dockerfile -t "$IMAGE" .
}

# Write genesis files (N validators + a funded deployer wallet) into `$1`.
genesis_files() {
  local dir="$1" n="$2" i
  : > "$dir/bonds.txt"
  for (( i = 0; i < n; i++ )); do
    echo "${VALIDATOR_PUB[$i]} 100" >> "$dir/bonds.txt"
  done
  echo "$DEPLOYER_REV_ADDR,$DEPLOYER_BALANCE" > "$dir/wallets.txt"
}

wait_for_cert() {
  local c="$1"
  for _ in $(seq 1 60); do
    if docker exec "$c" test -f /var/lib/rnode/node.certificate.pem 2>/dev/null; then
      return 0
    fi
    sleep 1
  done
  echo "timed out waiting for $c to generate its TLS cert" >&2
  return 1
}

bootstrap_id() {
  docker exec "$BOOTSTRAP" cat /var/lib/rnode/node.certificate.pem \
    | openssl x509 -noout -subject \
    | awk -F'=' '{print $NF}'
}

# `docker run` flags shared by every node (container name/network/ports + data + contracts mounts).
# Publishes deploy gRPC (40401), the public HTTP API (40403), and the admin HTTP API (40405).
docker_opts() {
  local name="$1" grpc_host="$2" http_host="$3" admin_host="$4"
  echo "-d --name $name --network $NETWORK \
    -p ${grpc_host}:40401 -p ${http_host}:40403 -p ${admin_host}:40405 \
    -v ${name}-data:/var/lib/rnode \
    -v ${CONTRACTS_DIR}:/contracts:ro"
}

# `rnode run` flags shared by every node (identity/ports/data-dir).
rnode_run_common() {
  local name="$1"
  echo "run --host $name --api-host 0.0.0.0 --data-dir /var/lib/rnode \
    --protocol-port 40400 --discovery-port 40404 \
    --api-port-grpc-external 40401 --api-port-grpc-internal 40402 \
    --api-port-http 40403 --api-port-admin-http 40405 \
    --api-enable-devnet-cors"
}

cmd_up() {
  local n=1 m=0
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --validators) n="${2:?}"; shift 2 ;;
      --observers)  m="${2:?}"; shift 2 ;;
      *) echo "unknown flag: $1" >&2; usage ;;
    esac
  done
  if (( n < 1 || n > MAX_VALIDATORS )); then
    echo "--validators must be in 1..$MAX_VALIDATORS" >&2; exit 2
  fi
  if (( m < 0 || m > 3 )); then
    echo "--observers must be in 0..3" >&2; exit 2
  fi

  echo "==> devnet: $n validator(s) + $m observer(s) (throwaway dev keys — local use only)"
  docker network create "$NETWORK" >/dev/null 2>&1 || true

  local genesis_dir
  genesis_dir="$(mktemp -d)"
  genesis_files "$genesis_dir" "$n"

  # Validator 0 = bootstrap: creates + approves genesis, autoproposes.
  echo "==> starting $BOOTSTRAP (validator 0, standalone, creates genesis)"
  # shellcheck disable=SC2046
  docker run $(docker_opts "$BOOTSTRAP" "$GRPC_BASE" "$HTTP_BASE" "$ADMIN_BASE") \
    -v "${genesis_dir}:/genesis:ro" \
    "$IMAGE" $(rnode_run_common "$BOOTSTRAP") -s --autopropose \
      --bonds-file /genesis/bonds.txt --wallets-file /genesis/wallets.txt \
      --validator-private-key "${VALIDATOR_PRIV[0]}"

  wait_for_cert "$BOOTSTRAP"
  local id
  id="$(bootstrap_id)"
  echo "==> bootstrap id: $id"

  # Validators 1..n-1: bonded in genesis, autopropose with their own key.
  local i name host_port http_port admin_port
  for (( i = 1; i < n; i++ )); do
    name="$(validator_name "$i")"
    host_port=$((GRPC_BASE + i * 1000))
    http_port=$((HTTP_BASE + i * 1000))
    admin_port=$((ADMIN_BASE + i * 1000))
    echo "==> starting $name (validator $i, bootstraps from $BOOTSTRAP)"
    # shellcheck disable=SC2046
    docker run $(docker_opts "$name" "$host_port" "$http_port" "$admin_port") \
      "$IMAGE" $(rnode_run_common "$name") --autopropose \
        --bootstrap "rnode://${id}@${BOOTSTRAP}?protocol=40400&discovery=40404" \
        --validator-private-key "${VALIDATOR_PRIV[$i]}"
  done

  # Observers: unbonded, no autopropose; they replicate the chain.
  for (( i = 1; i <= m; i++ )); do
    name="$(observer_name "$i")"
    host_port=$((GRPC_BASE + (n + i) * 1000))
    http_port=$((HTTP_BASE + (n + i) * 1000))
    admin_port=$((ADMIN_BASE + (n + i) * 1000))
    echo "==> starting $name (observer $i, bootstraps from $BOOTSTRAP)"
    # shellcheck disable=SC2046
    docker run $(docker_opts "$name" "$host_port" "$http_port" "$admin_port") \
      "$IMAGE" $(rnode_run_common "$name") \
        --bootstrap "rnode://${id}@${BOOTSTRAP}?protocol=40400&discovery=40404"
  done

  echo ""
  echo "==> up. Interact with:"
  echo "    tools/devnet.sh deploy <contract.rho>   # signed deploy to $BOOTSTRAP"
  echo "    tools/devnet.sh query <name>            # listen for data at a public name"
  echo "    tools/devnet.sh status | logs <node> | down"
  echo ""
  echo "    Public HTTP API:  http://localhost:${HTTP_BASE}/api/v1/status"
  echo "    OpenAPI document: http://localhost:${HTTP_BASE}/api/v1/openapi.json"
  echo "    Admin HTTP API:   http://localhost:${ADMIN_BASE}/api/v1/propose"
}

cmd_down() {
  local remove_volumes=false
  [[ "${1:-}" == "-v" ]] && remove_volumes=true
  local names=("$BOOTSTRAP")
  local i
  for (( i = 1; i <= MAX_VALIDATORS; i++ )); do names+=("$(validator_name "$i")"); done
  for (( i = 1; i <= 3; i++ )); do names+=("$(observer_name "$i")"); done
  for c in "${names[@]}"; do
    docker rm -f "$c" >/dev/null 2>&1 || true
    if $remove_volumes; then docker volume rm "${c}-data" >/dev/null 2>&1 || true; fi
  done
  docker network rm "$NETWORK" >/dev/null 2>&1 || true
}

cmd_status() {
  docker ps --filter "network=$NETWORK" --format 'table {{.Names}}\t{{.Status}}\t{{.Ports}}'
}

cmd_logs() {
  docker logs -f "${1:?node name required}"
}

# Run `rnode` inside a node container (reaches deploy 40401 + propose/repl 40402 via localhost).
node_cli() {
  local node="$1"; shift
  docker exec -i "$node" rnode --grpc-host localhost "$@"
}

cmd_deploy() {
  local file="${1:?contract file required (relative to examples/)}"; shift
  local node="$BOOTSTRAP"
  if [[ "${1:-}" == "--to" ]]; then node="${2:?}"; shift 2; fi
  local base; base="$(basename "$file")"
  echo "==> deploying $base to $node"
  node_cli "$node" deploy \
    --phlo-limit 1000000 --phlo-price 1 \
    --private-key "$DEPLOYER_PRIV" \
    --shard-id root \
    "/contracts/$base"
}

cmd_eval() {
  local file="${1:?file required (relative to examples/)}"
  local base; base="$(basename "$file")"
  node_cli "$BOOTSTRAP" eval "/contracts/$base"
}

cmd_query() {
  local name="${1:?public name required}"
  # The name is a public (forgeable) name; quote it so the client normalizes it as a rholang
  # *ground string* (matching `@"hello"!("world")`), not as a free variable.
  node_cli "$BOOTSTRAP" listen-data-at-name -t pub -c "\"$name\""
}

cmd_propose() {
  node_cli "$BOOTSTRAP" propose
}

case "${1:-}" in
  build) cmd_build ;;
  up) shift; cmd_up "$@" ;;
  down) cmd_down "${2:-}" ;;
  status) cmd_status ;;
  logs) cmd_logs "${2:-}" ;;
  deploy) shift; cmd_deploy "$@" ;;
  eval) shift; cmd_eval "$@" ;;
  query) shift; cmd_query "$@" ;;
  propose) cmd_propose ;;
  *) usage ;;
esac
