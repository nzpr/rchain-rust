# syntax=docker/dockerfile:1

# --- Build stage ---------------------------------------------------------------
FROM rust:1.95.0 AS builder

# `protoc` (prost-build .proto codegen) + the protobuf well-known types
# (`google/protobuf/empty.proto` etc., shipped by libprotobuf-dev) + build-essential
# + perl (ring/lmdb-rkv C builds, already in the full rust image).
RUN apt-get update \
    && apt-get install -y --no-install-recommends protobuf-compiler libprotobuf-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY . .

RUN cargo build --release -p rchain-node --bin rnode

# --- Runtime stage -------------------------------------------------------------
# Must match the builder's glibc (rust:1.95.0 = Debian trixie / glibc 2.41).
FROM debian:trixie-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/rnode /usr/local/bin/rnode

# `--profile docker` points `storage.data-dir` at /var/lib/rnode.
RUN useradd --create-home rnode \
    && mkdir -p /var/lib/rnode \
    && chown -R rnode:rnode /var/lib/rnode

# Healthy once the public HTTP API answers /api/status.
HEALTHCHECK --interval=10s --timeout=3s --start-period=30s --retries=5 \
    CMD curl -fsS http://localhost:40403/api/status || exit 1

USER rnode
WORKDIR /var/lib/rnode

ENTRYPOINT ["rnode"]
