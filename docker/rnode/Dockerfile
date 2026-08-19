# syntax=docker/dockerfile:1

# --- Build stage ---------------------------------------------------------------
FROM rust:1.95.0 AS builder

WORKDIR /app

# The full rust image carries build-essential + perl, which `ring` (rustls/rcgen)
# and `lmdb-rkv` (bundled C) need.
COPY . .

RUN cargo build --release -p rchain-node --bin rnode

# --- Runtime stage -------------------------------------------------------------
FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/rnode /usr/local/bin/rnode

# `--profile docker` points `storage.data-dir` at /var/lib/rnode.
RUN useradd --create-home rnode \
    && mkdir -p /var/lib/rnode \
    && chown -R rnode:rnode /var/lib/rnode

USER rnode
WORKDIR /var/lib/rnode

ENTRYPOINT ["rnode"]
