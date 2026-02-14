#!/usr/bin/env bash
# ──────────────────────────────────────────────────────────────────────────────
# Hexz — Local MinIO (S3-compatible) server for testing
# ──────────────────────────────────────────────────────────────────────────────
# Usage:  ./scripts/run_minio.sh [start|stop|status]
#
# Starts a MinIO container with a pre-configured test bucket.
# Environment variables for connecting:
#   AWS_ENDPOINT_URL=http://localhost:9000
#   AWS_ACCESS_KEY_ID=minioadmin
#   AWS_SECRET_ACCESS_KEY=minioadmin
# ──────────────────────────────────────────────────────────────────────────────

set -euo pipefail

CONTAINER_NAME="hexz-minio"
MINIO_PORT="${MINIO_PORT:-9000}"
MINIO_CONSOLE_PORT="${MINIO_CONSOLE_PORT:-9001}"
MINIO_ROOT_USER="${MINIO_ROOT_USER:-minioadmin}"
MINIO_ROOT_PASSWORD="${MINIO_ROOT_PASSWORD:-minioadmin}"
BUCKET_NAME="${BUCKET_NAME:-hexz-test}"
DATA_DIR="${MINIO_DATA_DIR:-/tmp/hexz-minio-data}"

info()  { printf '\033[36m[minio]\033[0m %s\n' "$*"; }
ok()    { printf '\033[32m[minio]\033[0m %s\n' "$*"; }
fail()  { printf '\033[31m[minio]\033[0m %s\n' "$*" >&2; exit 1; }

start() {
    if docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        info "MinIO is already running"
        status
        return
    fi

    # Clean up stopped container if it exists
    docker rm -f "$CONTAINER_NAME" 2>/dev/null || true

    info "Starting MinIO on :${MINIO_PORT}…"
    mkdir -p "$DATA_DIR"

    docker run -d \
        --name "$CONTAINER_NAME" \
        -p "${MINIO_PORT}:9000" \
        -p "${MINIO_CONSOLE_PORT}:9001" \
        -e "MINIO_ROOT_USER=${MINIO_ROOT_USER}" \
        -e "MINIO_ROOT_PASSWORD=${MINIO_ROOT_PASSWORD}" \
        -v "${DATA_DIR}:/data" \
        minio/minio:latest server /data --console-address ":9001"

    # Wait for MinIO to be ready
    info "Waiting for MinIO to start…"
    for i in $(seq 1 30); do
        if curl -sf "http://localhost:${MINIO_PORT}/minio/health/live" &>/dev/null; then
            break
        fi
        sleep 1
    done

    # Create test bucket
    if command -v mc &>/dev/null; then
        mc alias set hexz-local "http://localhost:${MINIO_PORT}" \
            "$MINIO_ROOT_USER" "$MINIO_ROOT_PASSWORD" 2>/dev/null
        mc mb "hexz-local/${BUCKET_NAME}" 2>/dev/null || true
        ok "Test bucket '${BUCKET_NAME}' ready"
    else
        info "Install 'mc' (MinIO Client) to auto-create buckets"
    fi

    ok "MinIO running"
    status
}

stop() {
    info "Stopping MinIO…"
    docker stop "$CONTAINER_NAME" 2>/dev/null || true
    docker rm "$CONTAINER_NAME" 2>/dev/null || true
    ok "MinIO stopped"
}

status() {
    if docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        ok "MinIO is running"
        printf "  API:     http://localhost:%s\n" "$MINIO_PORT"
        printf "  Console: http://localhost:%s\n" "$MINIO_CONSOLE_PORT"
        printf "  User:    %s\n" "$MINIO_ROOT_USER"
        printf "\n  export AWS_ENDPOINT_URL=http://localhost:%s\n" "$MINIO_PORT"
        printf "  export AWS_ACCESS_KEY_ID=%s\n" "$MINIO_ROOT_USER"
        printf "  export AWS_SECRET_ACCESS_KEY=%s\n" "$MINIO_ROOT_PASSWORD"
    else
        info "MinIO is not running"
    fi
}

case "${1:-start}" in
    start)  start  ;;
    stop)   stop   ;;
    status) status ;;
    *)      fail "Usage: $0 [start|stop|status]" ;;
esac
