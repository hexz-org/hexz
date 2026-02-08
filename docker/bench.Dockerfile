# ──────────────────────────────────────────────────────────────────────────────
# Strata — Benchmark Container
# ──────────────────────────────────────────────────────────────────────────────
# Minimal image for reproducible performance testing. No Python, no FUSE,
# no dev tools — just the Rust workspace and criterion benchmarks.
#
#   docker build -f docker/bench.Dockerfile -t strata-bench .
#   docker run --rm strata-bench cargo bench --package strata
# ──────────────────────────────────────────────────────────────────────────────

FROM rust:1.85-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
        libfuse-dev \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY . .
RUN cargo build --release --workspace

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
        libfuse2 \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/strata /usr/local/bin/strata
COPY --from=builder /build /workspace

WORKDIR /workspace

ENTRYPOINT ["cargo"]
CMD ["bench", "--package", "strata"]
