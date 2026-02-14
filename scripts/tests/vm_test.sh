#!/usr/bin/env bash
#
# QEMU VM Test Script for Hexz
#
# This script automates the process of creating a minimal bootable VM,
# converting it to a Hexz snapshot, and booting it via FUSE.

set -e

# Load common library
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../lib/common.sh"

PROJECT_ROOT="$(get_project_root)"
BIN="${BIN:-$PROJECT_ROOT/target/release/hexz}"
WORK_DIR="${WORK_DIR:-vm_test_work}"
MOUNT_DIR="${MOUNT_DIR:-$WORK_DIR/mnt}"

# Cleanup function
cleanup() {
    info "Cleaning up..."
    if [[ -n "$FUSE_PID" ]]; then
        kill "$FUSE_PID" 2>/dev/null || true
    fi
    if mountpoint -q "$MOUNT_DIR"; then
        $BIN vm unmount "$MOUNT_DIR" 2>/dev/null || fusermount -u "$MOUNT_DIR" 2>/dev/null || true
    fi
}
trap cleanup EXIT

info "=== Hexz VM Test ==="
info "BIN=$BIN"
info "WORK_DIR=$WORK_DIR"

# Prerequisites
check_cmd qemu-system-x86_64
check_cmd wget
check_cmd cargo
check_cmd bc

# Configuration
KERNEL_VER="virt"
ALPINE_VER="3.19.1"
KERNEL_URL="https://dl-cdn.alpinelinux.org/alpine/v3.19/releases/x86_64/netboot/vmlinuz-virt"
INITRAMFS_URL="https://dl-cdn.alpinelinux.org/alpine/v3.19/releases/x86_64/netboot/initramfs-virt"

# Paths
IMAGE_RAW="${WORK_DIR}/disk.raw"
IMAGE_SNAP="${WORK_DIR}/disk.hxz"

mkdir -p "${WORK_DIR}"
mkdir -p "${MOUNT_DIR}"

# Download Kernel/Initrd
if [[ ! -f "${WORK_DIR}/vmlinuz" ]]; then
    info "Downloading Linux Kernel..."
    wget -q "${KERNEL_URL}" -O "${WORK_DIR}/vmlinuz"
fi

if [[ ! -f "${WORK_DIR}/initramfs" ]]; then
    info "Downloading Initramfs..."
    wget -q "${INITRAMFS_URL}" -O "${WORK_DIR}/initramfs"
fi

# Create Dummy Disk Image
if [[ ! -f "${IMAGE_RAW}" ]]; then
    info "Creating 200MB compressible raw disk image..."
    yes "This is a test of Hexz compression speed. We want some repeated text that compresses well." | \
        dd of="${IMAGE_RAW}" bs=1M count=200 status=progress iflag=fullblock 2>/dev/null
fi

# Create Benchmark Initramfs
if [[ ! -f "${WORK_DIR}/initramfs-bench" ]]; then
    info "Building Custom Benchmark Initramfs..."
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

echo "--- Bench Start ---"
modprobe virtio
modprobe virtio_pci
modprobe virtio_blk
sleep 0.5
mdev -s

if [ -b /dev/vda ]; then
    echo "Reading /dev/vda..."
    time dd if=/dev/vda of=/dev/null bs=1M status=noxfer
else
    echo "Error: /dev/vda not found"
fi
echo "--- Bench End ---"
sync
poweroff -f
EOF
        chmod +x init
        find . | cpio -o -H newc 2>/dev/null | gzip > "../initramfs-bench"
    )
    rm -rf "${WORK_DIR}/initramfs_build"
fi

# Build Hexz
ensure_build "$BIN"

# Create Snapshot
info "Converting Raw Image to Hexz Snapshot..."
if [[ -f "${IMAGE_SNAP}" ]]; then rm "${IMAGE_SNAP}"; fi
$BIN data pack --disk "${IMAGE_RAW}" --output "${IMAGE_SNAP}"

# Mount via FUSE
info "Mounting Snapshot..."
fusermount -u "${MOUNT_DIR}" 2>/dev/null || true

FUSE_LOG="${WORK_DIR}/fuse.log"
$BIN vm mount "${IMAGE_SNAP}" "${MOUNT_DIR}" > "${FUSE_LOG}" 2>&1 &
FUSE_PID=$!

sleep 1

if ! mountpoint -q "${MOUNT_DIR}"; then
    fail "Mount failed! Check $FUSE_LOG"
fi

info "Mount successful. Verifying read access..."
if dd if="${MOUNT_DIR}/disk" of=/dev/null bs=1M count=1 status=none 2>/dev/null; then
    ok "Successfully read from mounted snapshot!"
else
    fail "Failed to read from mount"
fi

info "Booting QEMU with mounted snapshot..."
QEMU_LOG="${WORK_DIR}/qemu.log"
BOOT_START=$(date +%s.%N)

qemu-system-x86_64 \
    -kernel "${WORK_DIR}/vmlinuz" \
    -initrd "${WORK_DIR}/initramfs-bench" \
    -append "console=ttyS0 quiet" \
    -drive "file=${MOUNT_DIR}/disk,format=raw,if=virtio" \
    -snapshot \
    -nographic \
    -m 512 \
    -no-reboot \
    2>&1 | tee "${QEMU_LOG}"

BOOT_END=$(date +%s.%N)
BOOT_DURATION=$(echo "$BOOT_END - $BOOT_START" | bc)

ok "QEMU exited. Boot duration: ${BOOT_DURATION}s"

# Cleanup handled by trap
ok "Test Complete."
