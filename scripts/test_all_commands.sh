#!/usr/bin/env bash
# Test all Strata CLI commands and flags using data/ and vm/ from the project root.
# Run from repo root: ./scripts/test_all_commands.sh
# Requires: strata built (cargo build --release), FUSE (for mount), optional QEMU (boot/install/snapshot).
# Uses: vm/ubuntu.st, vm/ubuntu_logged_in.st, data/*.iso

set -e
BIN="${BIN:-./target/release/strata}"
DATA="${DATA:-./data}"
VM="${VM:-./vm}"
TMP="${TMP:-/tmp/strata-test}"
SNAP_BASE="${VM}/ubuntu.st"
SNAP_LOGGED_IN="${VM}/ubuntu_logged_in.st"

mkdir -p "$TMP"
cd "$(dirname "$0")/.."

echo "=== Strata full command + flag test ==="
echo "BIN=$BIN  DATA=$DATA  VM=$VM  TMP=$TMP"
echo "Snapshots: $SNAP_BASE  $SNAP_LOGGED_IN"
echo ""

# --- Doctor (no flags) ---
echo "[1] doctor"
$BIN doctor

# --- Inspect both snapshots ---
echo "[2] inspect (base)"
$BIN inspect "$SNAP_BASE"
if [[ -f "$SNAP_LOGGED_IN" ]]; then
  echo "[3] inspect (logged-in snapshot)"
  $BIN inspect "$SNAP_LOGGED_IN"
else
  echo "[3] inspect (logged-in snapshot) skip (no $SNAP_LOGGED_IN)"
fi

# --- Build: all profiles, --source, -o/--output (skip --encrypt, --memory for speed) ---
dd if=/dev/zero of="$TMP/small.raw" bs=1M count=1 2>/dev/null
echo "[4] build --profile generic"
$BIN build --source "$TMP/small.raw" -o "$TMP/out-generic.st" --profile generic
echo "[5] build --profile eda"
$BIN build --source "$TMP/small.raw" --output "$TMP/out-eda.st" --profile eda
echo "[6] build --profile embedded"
$BIN build --source "$TMP/small.raw" --output "$TMP/out-embedded.st" --profile embedded
echo "[7] build --profile ml"
$BIN build --source "$TMP/small.raw" --output "$TMP/out-ml.st" --profile ml

# --- Create: --disk, -o, --compression lz4/zstd, --block-size (no --encrypt, --train-dict) ---
echo "[8] create --compression lz4 --block-size 65536"
$BIN create --disk "$TMP/small.raw" -o "$TMP/create-lz4.st" --compression lz4 --block-size 65536
echo "[9] create --compression zstd --block-size 32768"
$BIN create --disk "$TMP/small.raw" --output "$TMP/create-zstd.st" --compression zstd --block-size 32768

# --- Keygen: -o and --output-dir ---
mkdir -p "$TMP/keys"
echo "[10] keygen -o (short)"
$BIN keygen -o "$TMP/keys"
echo "[11] keygen --output-dir (explicit)"
$BIN keygen --output-dir "$TMP/keys"

# --- Sign / Verify (positional image) ---
cp "$SNAP_BASE" "$TMP/signed.st"
echo "[12] sign --key KEY IMAGE"
$BIN sign --key "$TMP/keys/private.key" "$TMP/signed.st"
echo "[13] verify --key KEY IMAGE"
$BIN verify --key "$TMP/keys/public.key" "$TMP/signed.st"

# --- Bench: image + optional --block-size, --duration, --threads ---
echo "[14] bench IMAGE"
$BIN bench "$SNAP_BASE"
echo "[15] bench IMAGE --block-size 65536 --duration 1 --threads 1"
$BIN bench "$SNAP_BASE" --block-size 65536 --duration 1 --threads 1
if [[ -f "$SNAP_LOGGED_IN" ]]; then
  echo "[16] bench (logged-in snapshot)"
  $BIN bench "$SNAP_LOGGED_IN"
else
  echo "[16] bench (logged-in) skip"
fi

# --- Analyze: input path ---
echo "[17] analyze (snapshot)"
$BIN analyze "$SNAP_BASE"

# --- Mount with -d (daemon), then unmount; test --cache-size, --uid, --gid ---
MNT="$TMP/mnt"
mkdir -p "$MNT"
echo "[18] mount SNAP MNT -d (daemon, read-only)"
$BIN mount "$SNAP_BASE" "$MNT" -d
sleep 2
ls -la "$MNT" || true
echo "[18b] unmount (after daemon mount)"
$BIN unmount "$MNT"

echo "[19] mount SNAP MNT -d --cache-size 64M --uid 1000 --gid 1000"
$BIN mount "$SNAP_BASE" "$MNT" -d --cache-size 64M --uid 1000 --gid 1000
sleep 2
$BIN unmount "$MNT"

# --- Mount RW + overlay for commit/diff ---
OVERLAY="$TMP/overlay"
echo "[20] mount SNAP MNT --overlay OVERLAY --rw -d"
$BIN mount "$SNAP_BASE" "$MNT" --overlay "$OVERLAY" --rw -d
sleep 2
touch "$MNT/.strata-rw-test" 2>/dev/null || true
$BIN unmount "$MNT"

# --- Diff: overlay + --blocks and --files ---
echo "[21] diff OVERLAY --blocks"
$BIN diff "$OVERLAY" --blocks
echo "[22] diff OVERLAY --files"
$BIN diff "$OVERLAY" --files
echo "[23] diff OVERLAY --blocks --files"
$BIN diff "$OVERLAY" --blocks --files

# --- Commit: base, overlay, output, --compression, --block-size, --keep-overlay, --flatten, --message ---
echo "[24] commit (all flags: --compression zstd --block-size 65536 --keep-overlay --flatten --message)"
$BIN commit "$SNAP_BASE" "$OVERLAY" "$TMP/committed.st" \
  --compression zstd --block-size 65536 --keep-overlay --flatten \
  --message "test commit from test_all_commands.sh"

echo "[25] inspect committed"
$BIN inspect "$TMP/committed.st"

# --- Serve: --port, -d (skip -d so we can kill by PID), then --nbd / --s3 with timeout ---
echo "[26] serve SNAP --port 18080 (HTTP, then curl)"
$BIN serve "$SNAP_BASE" --port 18080 &
SERVE_PID=$!
sleep 2
curl -s -o /dev/null -w "HTTP %{http_code}\n" "http://127.0.0.1:18080/disk" || true
kill $SERVE_PID 2>/dev/null || true
wait $SERVE_PID 2>/dev/null || true

echo "[27] serve SNAP --port 18081 --nbd (stub, 2s timeout)"
timeout 2 $BIN serve "$SNAP_BASE" --port 18081 --nbd 2>/dev/null || true

echo "[28] serve SNAP --port 18082 --s3 (stub, 2s timeout)"
timeout 2 $BIN serve "$SNAP_BASE" --port 18082 --s3 2>/dev/null || true

# --- Boot: --no-graphics, --ram, --network, --backend (short timeout if QEMU present) ---
echo "[29] boot SNAP --no-graphics --ram 2G --network user --backend qemu (5s timeout)"
timeout 5 $BIN boot "$SNAP_BASE" --no-graphics --ram 2G --network user --backend qemu 2>/dev/null || true

# --- Snapshot: needs running VM with QMP; document only / skip ---
echo "[30] snapshot (skipped: requires running VM with --qmp-socket; use manually)"
# Example: $BIN snapshot --socket /tmp/qmp.sock --base "$SNAP_BASE" --overlay /path/to/overlay -o "$TMP/snap.st"

# --- Install: long-running; optional with timeout ---
echo "[31] install (skipped: long-running; uncomment to run with timeout)"
# timeout 60 $BIN install --iso "$DATA/ubuntu-24.04.3-live-server-amd64.iso" --disk-size 10G --ram 2G -o "$TMP/installed.st" --no-graphics 2>/dev/null || true

echo ""
echo "=== Done (31 steps). Skipped: Snapshot (needs QMP), Install (long). Cleanup: rm -rf $TMP ==="
