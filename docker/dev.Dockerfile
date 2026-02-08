# ──────────────────────────────────────────────────────────────────────────────
# Strata — Development Container
# ──────────────────────────────────────────────────────────────────────────────
# Provides Rust, Python, MinIO, and all system dependencies needed to build,
# test, and benchmark Strata in an isolated environment.
#
#   docker build -f docker/dev.Dockerfile -t strata-dev .
#   docker run --rm -it -v $(pwd):/workspace strata-dev
# ──────────────────────────────────────────────────────────────────────────────

FROM rust:1.85-bookworm AS base

# System dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
        libfuse-dev \
        pkg-config \
        python3 \
        python3-pip \
        python3-venv \
        qemu-system-x86 \
        qemu-utils \
        curl \
        git \
    && rm -rf /var/lib/apt/lists/*

# Rust tooling
RUN rustup component add rustfmt clippy \
    && cargo install cargo-deny cargo-fuzz maturin criterion

# MinIO (local S3)
RUN curl -sSL https://dl.min.io/server/minio/release/linux-amd64/minio \
        -o /usr/local/bin/minio \
    && chmod +x /usr/local/bin/minio \
    && curl -sSL https://dl.min.io/client/mc/release/linux-amd64/mc \
        -o /usr/local/bin/mc \
    && chmod +x /usr/local/bin/mc

# Python environment
RUN python3 -m venv /opt/venv
ENV PATH="/opt/venv/bin:$PATH"
RUN pip install --no-cache-dir \
        pytest \
        pytest-asyncio \
        numpy \
        torch --index-url https://download.pytorch.org/whl/cpu

WORKDIR /workspace

# Pre-fetch dependencies (speeds up incremental builds)
COPY Cargo.toml Cargo.lock ./
COPY crates/common/Cargo.toml crates/common/Cargo.toml
COPY crates/core/Cargo.toml crates/core/Cargo.toml
COPY crates/cli/Cargo.toml crates/cli/Cargo.toml
COPY crates/fuse/Cargo.toml crates/fuse/Cargo.toml
COPY crates/server/Cargo.toml crates/server/Cargo.toml
COPY crates/ffi/Cargo.toml crates/ffi/Cargo.toml
COPY crates/loader/Cargo.toml crates/loader/Cargo.toml
RUN mkdir -p crates/common/src crates/core/src crates/cli/src \
             crates/fuse/src crates/server/src crates/ffi/src crates/loader/src \
    && echo '// stub' > crates/common/src/lib.rs \
    && echo '// stub' > crates/core/src/lib.rs \
    && echo 'fn main() {}' > crates/cli/src/main.rs \
    && echo '// stub' > crates/cli/src/lib.rs \
    && echo '// stub' > crates/fuse/src/lib.rs \
    && echo '// stub' > crates/server/src/lib.rs \
    && echo '// stub' > crates/ffi/src/lib.rs \
    && echo '// stub' > crates/loader/src/lib.rs \
    && cargo fetch \
    && rm -rf crates/

COPY . .

CMD ["bash"]
