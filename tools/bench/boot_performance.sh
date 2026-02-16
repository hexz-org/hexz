#!/usr/bin/env bash
set -e

# Load common library
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../scripts/lib/common.sh"

PROJECT_ROOT="$(get_project_root)"
BIN="${BIN:-$PROJECT_ROOT/target/release/hexz}"

WORK_DIR="vm_test_work"
IMAGE_RAW="${WORK_DIR}/disk.raw"
IMAGE_SNAP="${WORK_DIR}/disk.hxz"
MOUNT_DIR="${WORK_DIR}/mnt"

# Prerequisites
check_cmd bc
check_cmd qemu-system-x86_64

if [[ ! -f "${IMAGE_RAW}" ]] || [[ ! -f "${IMAGE_SNAP}" ]]; then
    fail "Error: Run ./scripts/vm_test.sh first to create test images"
fi

ensure_build "$BIN"

mount_snapfs() {
    fusermount -u "${MOUNT_DIR}" 2>/dev/null || true
    FUSE_LOG="${WORK_DIR}/fuse_${1}.log"
    
    # Mount command
    $BIN vm mount "${2}" "${MOUNT_DIR}" > "${FUSE_LOG}" 2>&1 &
    FUSE_PID=$!
    
    sleep 2
    for i in {1..10}; do
        if mountpoint -q "${MOUNT_DIR}"; then
            echo "$FUSE_PID"
            return 0
        fi
        sleep 0.5
    done
    kill $FUSE_PID 2>/dev/null || true
    return 1
}

unmount_snapfs() {
    local pid=$1
    $BIN vm unmount "${MOUNT_DIR}"
    wait $pid 2>/dev/null || true
}

# --- Compression Stats ---
RAW_SIZE=$(stat -c "%s" "${IMAGE_RAW}")
SNAP_SIZE=$(stat -c "%s" "${IMAGE_SNAP}")
COMPRESSION_RATIO=$(echo "scale=1; (1 - $SNAP_SIZE / $RAW_SIZE) * 100" | bc)
IO_SAVED=$(echo "scale=1; ($RAW_SIZE - $SNAP_SIZE) / 1024 / 1024" | bc)
RAW_MB=$(echo "scale=1; $RAW_SIZE / 1024 / 1024" | bc)
SNAP_MB=$(echo "scale=2; $SNAP_SIZE / 1024 / 1024" | bc)

info "Compression Ratio"
info "Raw: ${RAW_MB} MB"
info "Hexz: ${SNAP_MB} MB"
info "Reduction: ${COMPRESSION_RATIO}% (${IO_SAVED} MB saved)"
echo ""

# --- Test 1: Full Sequential Read ---
info "Test 1: Full Sequential Read"
RAW_START=$(date +%s.%N)
qemu-system-x86_64 \
    -kernel "${WORK_DIR}/vmlinuz" \
    -initrd "${WORK_DIR}/initramfs-bench" \
    -append "console=ttyS0 quiet" \
    -drive "file=${IMAGE_RAW},format=raw,if=virtio" \
    -snapshot \
    -nographic \
    -m 512 \
    -no-reboot \
    >/dev/null 2>&1
RAW_END=$(date +%s.%N)
RAW_DURATION=$(echo "$RAW_END - $RAW_START" | bc)

FUSE_PID=$(mount_snapfs "test1" "${IMAGE_SNAP}")
SNAP_START=$(date +%s.%N)
qemu-system-x86_64 \
    -kernel "${WORK_DIR}/vmlinuz" \
    -initrd "${WORK_DIR}/initramfs-bench" \
    -append "console=ttyS0 quiet" \
    -drive "file=${MOUNT_DIR}/disk,format=raw,if=virtio" \
    -snapshot \
    -nographic \
    -m 512 \
    -no-reboot \
    >/dev/null 2>&1
SNAP_END=$(date +%s.%N)
SNAP_DURATION=$(echo "$SNAP_END - $SNAP_START" | bc)
unmount_snapfs $FUSE_PID

DIFF1=$(echo "$SNAP_DURATION - $RAW_DURATION" | bc)
info "Raw: ${RAW_DURATION}s"
info "Hexz: ${SNAP_DURATION}s"
# ... (Leaving rest of stats logic unchanged but using info/ok could be nice, keeping echo for now for simplicity of diff)
if (( $(echo "$DIFF1 > 0" | bc -l) )); then
    OVERHEAD=$(echo "scale=1; ($DIFF1 / $RAW_DURATION) * 100" | bc)
    warn "Overhead: +${DIFF1}s (+${OVERHEAD}%)"
else
    SPEEDUP_S=$(echo "$RAW_DURATION - $SNAP_DURATION" | bc)
    SPEEDUP_PCT=$(echo "scale=1; ($SPEEDUP_S / $RAW_DURATION) * 100" | bc)
    ok "Speedup: ${SPEEDUP_S}s (${SPEEDUP_PCT}%)"
fi
echo ""

# --- Test 2: Partial Read ---
if [[ ! -f "${WORK_DIR}/initramfs-partial" ]]; then
    mkdir -p "${WORK_DIR}/initramfs_build"
    (
        cd "${WORK_DIR}/initramfs_build"
        zcat "../initramfs" | cpio -idm 2>/dev/null || true
        
        cat > init <<'EOF'
#!/bin/busybox sh
/bin/busybox --install /bin
mount -t devtmpfs devtmpfs /dev
mount -t proc proc /proc
mount -t sysfs sysfs /sys
modprobe virtio
modprobe virtio_pci
modprobe virtio_blk
sleep 0.5
mdev -s
if [ -b /dev/vda ]; then
    dd if=/dev/vda of=/dev/null bs=1M count=10 status=noxfer
fi
sync
poweroff -f
EOF
        chmod +x init
        find . | cpio -o -H newc 2>/dev/null | gzip > "../initramfs-partial"
    )
    rm -rf "${WORK_DIR}/initramfs_build"
fi

info "Test 2: Partial Read (10MB)"
RAW_PARTIAL_START=$(date +%s.%N)
qemu-system-x86_64 \
    -kernel "${WORK_DIR}/vmlinuz" \
    -initrd "${WORK_DIR}/initramfs-partial" \
    -append "console=ttyS0 quiet" \
    -drive "file=${IMAGE_RAW},format=raw,if=virtio" \
    -snapshot \
    -nographic \
    -m 512 \
    -no-reboot \
    >/dev/null 2>&1
RAW_PARTIAL_END=$(date +%s.%N)
RAW_PARTIAL_DURATION=$(echo "$RAW_PARTIAL_END - $RAW_PARTIAL_START" | bc)

FUSE_PID=$(mount_snapfs "test2" "${IMAGE_SNAP}")
SNAP_PARTIAL_START=$(date +%s.%N)
qemu-system-x86_64 \
    -kernel "${WORK_DIR}/vmlinuz" \
    -initrd "${WORK_DIR}/initramfs-partial" \
    -append "console=ttyS0 quiet" \
    -drive "file=${MOUNT_DIR}/disk,format=raw,if=virtio" \
    -snapshot \
    -nographic \
    -m 512 \
    -no-reboot \
    >/dev/null 2>&1
SNAP_PARTIAL_END=$(date +%s.%N)
SNAP_PARTIAL_DURATION=$(echo "$SNAP_PARTIAL_END - $SNAP_PARTIAL_START" | bc)
unmount_snapfs $FUSE_PID

DIFF2=$(echo "$SNAP_PARTIAL_DURATION - $RAW_PARTIAL_DURATION" | bc)
info "Raw: ${RAW_PARTIAL_DURATION}s"
info "Hexz: ${SNAP_PARTIAL_DURATION}s"
if (( $(echo "$DIFF2 > 0" | bc -l) )); then
    OVERHEAD=$(echo "scale=1; ($DIFF2 / $RAW_PARTIAL_DURATION) * 100" | bc)
    warn "Overhead: +${DIFF2}s (+${OVERHEAD}%)"
else
    SPEEDUP_S=$(echo "$RAW_PARTIAL_DURATION - $SNAP_PARTIAL_DURATION" | bc)
    SPEEDUP_PCT=$(echo "scale=1; ($SPEEDUP_S / $RAW_PARTIAL_DURATION) * 100" | bc)
    ok "Speedup: ${SPEEDUP_S}s (${SPEEDUP_PCT}%)"
fi
echo ""

# --- Test 3: Random Access ---
if [[ ! -f "${WORK_DIR}/initramfs-random" ]]; then
    mkdir -p "${WORK_DIR}/initramfs_build"
    (
        cd "${WORK_DIR}/initramfs_build"
        zcat "../initramfs" | cpio -idm 2>/dev/null || true
        
        cat > init <<'EOF'
#!/bin/busybox sh
/bin/busybox --install /bin
mount -t devtmpfs devtmpfs /dev
mount -t proc proc /proc
mount -t sysfs sysfs /sys
modprobe virtio
modprobe virtio_pci
modprobe virtio_blk
sleep 0.5
mdev -s
if [ -b /dev/vda ]; then
    for i in 0 1 2 3 4 5 6 7 8 9; do
        offset=$((i * 20 * 1024 * 1024))
        dd if=/dev/vda of=/dev/null bs=64K count=1 skip=$((offset / 65536)) status=none
    done
fi
sync
poweroff -f
EOF
        chmod +x init
        find . | cpio -o -H newc 2>/dev/null | gzip > "../initramfs-random"
    )
    rm -rf "${WORK_DIR}/initramfs_build"
fi

info "Test 3: Random Access (10 blocks)"
RAW_RANDOM_START=$(date +%s.%N)
qemu-system-x86_64 \
    -kernel "${WORK_DIR}/vmlinuz" \
    -initrd "${WORK_DIR}/initramfs-random" \
    -append "console=ttyS0 quiet" \
    -drive "file=${IMAGE_RAW},format=raw,if=virtio" \
    -snapshot \
    -nographic \
    -m 512 \
    -no-reboot \
    >/dev/null 2>&1
RAW_RANDOM_END=$(date +%s.%N)
RAW_RANDOM_DURATION=$(echo "$RAW_RANDOM_END - $RAW_RANDOM_START" | bc)

FUSE_PID=$(mount_snapfs "test3" "${IMAGE_SNAP}")
SNAP_RANDOM_START=$(date +%s.%N)
qemu-system-x86_64 \
    -kernel "${WORK_DIR}/vmlinuz" \
    -initrd "${WORK_DIR}/initramfs-random" \
    -append "console=ttyS0 quiet" \
    -drive "file=${MOUNT_DIR}/disk,format=raw,if=virtio" \
    -snapshot \
    -nographic \
    -m 512 \
    -no-reboot \
    >/dev/null 2>&1
SNAP_RANDOM_END=$(date +%s.%N)
SNAP_RANDOM_DURATION=$(echo "$SNAP_RANDOM_END - $SNAP_RANDOM_START" | bc)
unmount_snapfs $FUSE_PID

DIFF3=$(echo "$SNAP_RANDOM_DURATION - $RAW_RANDOM_DURATION" | bc)
info "Raw: ${RAW_RANDOM_DURATION}s"
info "Hexz: ${SNAP_RANDOM_DURATION}s"
if (( $(echo "$DIFF3 > 0" | bc -l) )); then
    OVERHEAD=$(echo "scale=1; ($DIFF3 / $RAW_RANDOM_DURATION) * 100" | bc)
    warn "Overhead: +${DIFF3}s (+${OVERHEAD}%)"
else
    SPEEDUP_S=$(echo "$RAW_RANDOM_DURATION - $SNAP_RANDOM_DURATION" | bc)
    SPEEDUP_PCT=$(echo "scale=1; ($SPEEDUP_S / $RAW_RANDOM_DURATION) * 100" | bc)
    ok "Speedup: ${SPEEDUP_S}s (${SPEEDUP_PCT}%)"
fi
echo ""

# --- Test 4: Gzip vs Hexz ---
info "Test 4: Gzip vs Hexz (Reading 10MB from middle)"
GZIP_FILE="${WORK_DIR}/disk.raw.gz"
if [[ ! -f "${GZIP_FILE}" ]]; then
    gzip -c "${IMAGE_RAW}" > "${GZIP_FILE}"
fi
GZIP_SIZE=$(stat -c "%s" "${GZIP_FILE}")
GZIP_MB=$(echo "scale=2; $GZIP_SIZE / 1024 / 1024" | bc)

GZIP_START=$(date +%s.%N)
gunzip -c "${GZIP_FILE}" 2>/dev/null | dd of=/dev/null bs=1M count=10 skip=100 status=none 2>&1
GZIP_END=$(date +%s.%N)
GZIP_DURATION=$(echo "$GZIP_END - $GZIP_START" | bc)

FUSE_PID=$(mount_snapfs "test4" "${IMAGE_SNAP}")
SNAP_GZIP_START=$(date +%s.%N)
dd if="${MOUNT_DIR}/disk" of=/dev/null bs=1M count=10 skip=100 status=none 2>&1
SNAP_GZIP_END=$(date +%s.%N)
SNAP_GZIP_DURATION=$(echo "$SNAP_GZIP_END - $SNAP_GZIP_START" | bc)
unmount_snapfs $FUSE_PID

DIFF4=$(echo "$SNAP_GZIP_DURATION - $GZIP_DURATION" | bc)
info "Gzip (${GZIP_MB} MB, decompresses 0-110MB): ${GZIP_DURATION}s"
info "Hexz (${SNAP_MB} MB, reads only blocks 100-110): ${SNAP_GZIP_DURATION}s"
if (( $(echo "$DIFF4 > 0" | bc -l) )); then
    OVERHEAD=$(echo "scale=1; ($DIFF4 / $GZIP_DURATION) * 100" | bc)
    warn "Overhead: +${DIFF4}s (+${OVERHEAD}%)"
else
    SPEEDUP_S=$(echo "$GZIP_DURATION - $SNAP_GZIP_DURATION" | bc)
    SPEEDUP_PCT=$(echo "scale=1; ($SPEEDUP_S / $GZIP_DURATION) * 100" | bc)
    ok "Speedup: ${SPEEDUP_S}s (${SPEEDUP_PCT}%)"
fi
echo ""

# --- Test 5: Sparse Access ---
info "Test 5: Sparse Access (10 scattered 64KB reads)"
SPARSE_RAW="${WORK_DIR}/sparse.raw"
SPARSE_SNAP="${WORK_DIR}/sparse.hxz"
if [[ ! -f "${SPARSE_RAW}" ]]; then
    dd if=/dev/urandom of="${SPARSE_RAW}" bs=1M count=500 status=none
fi
if [[ ! -f "${SPARSE_SNAP}" ]]; then
    $BIN data pack --disk "${SPARSE_RAW}" --output "${SPARSE_SNAP}" > /dev/null 2>&1
fi
SPARSE_RAW_SIZE=$(stat -c "%s" "${SPARSE_RAW}")
SPARSE_SNAP_SIZE=$(stat -c "%s" "${SPARSE_SNAP}")
SPARSE_RAW_MB=$(echo "scale=1; $SPARSE_RAW_SIZE / 1024 / 1024" | bc)
SPARSE_SNAP_MB=$(echo "scale=2; $SPARSE_SNAP_SIZE / 1024 / 1024" | bc)

SPARSE_GZIP="${WORK_DIR}/sparse.raw.gz"
if [[ ! -f "${SPARSE_GZIP}" ]]; then
    gzip -c "${SPARSE_RAW}" > "${SPARSE_GZIP}"
fi
SPARSE_GZIP_SIZE=$(stat -c "%s" "${SPARSE_GZIP}")
SPARSE_GZIP_MB=$(echo "scale=2; $SPARSE_GZIP_SIZE / 1024 / 1024" | bc)

GZIP_SPARSE_START=$(date +%s.%N)
for offset in 0 50 100 150 200 250 300 350 400 450; do
    gunzip -c "${SPARSE_GZIP}" 2>/dev/null | dd of=/dev/null bs=64K count=1 skip=$((offset * 16)) status=none 2>&1
done
GZIP_SPARSE_END=$(date +%s.%N)
GZIP_SPARSE_DURATION=$(echo "$GZIP_SPARSE_END - $GZIP_SPARSE_START" | bc)

FUSE_PID=$(mount_snapfs "test5" "${SPARSE_SNAP}")
SNAP_SPARSE_START=$(date +%s.%N)
for offset in 0 50 100 150 200 250 300 350 400 450; do
    dd if="${MOUNT_DIR}/disk" of=/dev/null bs=64K count=1 skip=$((offset * 16)) status=none 2>&1
done
SNAP_SPARSE_END=$(date +%s.%N)
SNAP_SPARSE_DURATION=$(echo "$SNAP_SPARSE_END - $SNAP_SPARSE_START" | bc)
unmount_snapfs $FUSE_PID

DIFF5=$(echo "$SNAP_SPARSE_DURATION - $GZIP_SPARSE_DURATION" | bc)
info "Gzip (${SPARSE_GZIP_MB} MB, decompresses 500MB each read): ${GZIP_SPARSE_DURATION}s"
info "Hexz (${SPARSE_SNAP_MB} MB, reads only 10 blocks): ${SNAP_SPARSE_DURATION}s"
if (( $(echo "$DIFF5 > 0" | bc -l) )); then
    OVERHEAD=$(echo "scale=1; ($DIFF5 / $GZIP_SPARSE_DURATION) * 100" | bc)
    warn "Overhead: +${DIFF5}s (+${OVERHEAD}%)"
else
    SPEEDUP_S=$(echo "$GZIP_SPARSE_DURATION - $SNAP_SPARSE_DURATION" | bc)
    SPEEDUP_PCT=$(echo "scale=1; ($SPEEDUP_S / $GZIP_SPARSE_DURATION) * 100" | bc)
    ok "Speedup: ${SPEEDUP_S}s (${SPEEDUP_PCT}%)"
fi
echo ""

ok "Summary"
echo "Test 1 (Sequential): Raw ${RAW_DURATION}s, Hexz ${SNAP_DURATION}s"
echo "Test 2 (Partial): Raw ${RAW_PARTIAL_DURATION}s, Hexz ${SNAP_PARTIAL_DURATION}s"
echo "Test 3 (Random): Raw ${RAW_RANDOM_DURATION}s, Hexz ${SNAP_RANDOM_DURATION}s"
echo "Test 4 (vs Gzip): Gzip ${GZIP_DURATION}s, Hexz ${SNAP_GZIP_DURATION}s"
echo "Test 5 (Sparse): Gzip ${GZIP_SPARSE_DURATION}s, Hexz ${SNAP_SPARSE_DURATION}s"
echo "Test 5 Compression: Raw ${SPARSE_RAW_MB} MB, Hexz ${SPARSE_SNAP_MB} MB, Gzip ${SPARSE_GZIP_MB} MB"
