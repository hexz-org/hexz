# ──────────────────────────────────────────────────────────────────────────────
# Hexz — Benchmark Container
# ──────────────────────────────────────────────────────────────────────────────
# Minimal image for reproducible performance testing. No Python, no FUSE,
# no dev tools — just the Rust workspace and criterion benchmarks.
#
#   docker build -f docker/bench.Dockerfile -t hexz-bench .
#   docker run --rm hexz-bench cargo bench --package hexz
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

COPY --from=builder /build/target/release/hexz /usr/local/bin/hexz
COPY --from=builder /build /workspace

WORKDIR /workspace

ENTRYPOINT ["cargo"]
CMD ["bench", "--package", "hexz"]
