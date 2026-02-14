#!/usr/bin/env bash
# Test all Hexz CLI commands and flags.
# Run from repo root: ./scripts/tests/test_all_commands.sh

set -e

# Load common library
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../lib/common.sh"

PROJECT_ROOT="$(get_project_root)"
BIN="${BIN:-$PROJECT_ROOT/target/release/hexz}"
TMP="${TMP:-/tmp/hexz-test}"

# Ensure binary exists
ensure_build "$BIN"

# Cleanup on exit
cleanup() {
    if [[ -d "$TMP" ]]; then
        info "Cleaning up $TMP..."
        rm -rf "$TMP"
    fi
}
trap cleanup EXIT

mkdir -p "$TMP"
cd "$PROJECT_ROOT"

# Create a small test image and snapshot to use for most tests (no dependency on vm/ or data/)
dd if=/dev/zero of="$TMP/small.raw" bs=1M count=1 2>/dev/null
$BIN data pack --disk "$TMP/small.raw" --output "$TMP/base.hxz"
SNAP_BASE="$TMP/base.hxz"

info "=== Hexz full command + flag test ==="
info "BIN=$BIN"
info "TMP=$TMP"
info "Snapshot: $SNAP_BASE"

# --- Doctor ---
info "[1] sys doctor"
$BIN sys doctor

# --- Inspect ---
info "[2] data info (base)"
$BIN data info "$SNAP_BASE"

# --- Build ---
for profile in generic eda embedded ml; do
    info "[build] profile=$profile"
    $BIN data build --source "$TMP/small.raw" --output "$TMP/out-$profile.hxz" --profile "$profile"
done

# --- Create (data pack) ---
info "[create] compression=lz4 block_size=65536"
$BIN data pack --disk "$TMP/small.raw" --output "$TMP/create-lz4.hxz" --compression lz4 --block-size 65536
info "[create] compression=zstd block_size=32768"
$BIN data pack --disk "$TMP/small.raw" --output "$TMP/create-zstd.hxz" --compression zstd --block-size 32768

# --- Keygen ---
mkdir -p "$TMP/keys"
info "[keygen] Generating keys..."
$BIN sys keygen --output-dir "$TMP/keys"

# --- Sign / Verify ---
cp "$SNAP_BASE" "$TMP/signed.hxz"
info "[sign] Signing snapshot..."
$BIN sys sign --key "$TMP/keys/private.key" "$TMP/signed.hxz"
info "[verify] Verifying snapshot..."
$BIN sys verify --key "$TMP/keys/public.key" "$TMP/signed.hxz"

# --- Bench ---
info "[bench] Standard benchmark..."
$BIN sys bench "$SNAP_BASE"
info "[bench] Custom parameters..."
$BIN sys bench "$SNAP_BASE" --block-size 65536 --duration 1 --threads 1

# --- Analyze ---
info "[analyze] Analyzing snapshot..."
$BIN data analyze "$SNAP_BASE"

# --- Mount / Unmount ---
MNT="$TMP/mnt"
mkdir -p "$MNT"
info "[mount] Mounting (daemon mode)..."
$BIN vm mount "$SNAP_BASE" "$MNT" -d
sleep 2
ls -la "$MNT" || true
$BIN vm unmount "$MNT"
ok "Unmounted successfully"

# --- Mount RW with Overlay ---
OVERLAY="$TMP/overlay"
info "[mount] Mounting with overlay (RW)..."
$BIN vm mount "$SNAP_BASE" "$MNT" --overlay "$OVERLAY" --rw -d
sleep 2
touch "$MNT/.hexz-rw-test" 2>/dev/null || true
$BIN vm unmount "$MNT"

# --- Diff ---
info "[diff] Checking overlay diffs..."
$BIN data diff "$OVERLAY" --blocks --files

# --- Commit ---
info "[commit] Committing changes..."
$BIN vm commit "$SNAP_BASE" "$OVERLAY" "$TMP/committed.hxz" \
  --compression zstd --block-size 65536 --keep-overlay --flatten \
  --message "test commit"
$BIN data info "$TMP/committed.hxz"

# --- Serve ---
info "[serve] Testing HTTP server..."
$BIN sys serve "$SNAP_BASE" --port 18080 &
SERVE_PID=$!
sleep 2
curl -s -o /dev/null -w "HTTP %{http_code}\n" "http://127.0.0.1:18080/disk" || true
kill $SERVE_PID 2>/dev/null || true
wait $SERVE_PID 2>/dev/null || true

# --- Boot (if qemu exists) ---
if command -v qemu-system-x86_64 &>/dev/null; then
    info "[boot] Testing boot command (timeout 5s)..."
    timeout 5 $BIN vm boot "$SNAP_BASE" --no-graphics --ram 2G --network user --backend qemu 2>/dev/null || true
else
    warn "qemu-system-x86_64 not found, skipping boot test"
fi

ok "All tests passed!"
