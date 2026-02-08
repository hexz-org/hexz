#!/usr/bin/env bash
#
# QEMU VM Test Script for SnapFS
#
# This script automates the process of creating a minimal bootable VM,
# converting it to a SnapFS snapshot, and booting it via FUSE.

set -e

# Check for required commands
check_command() {
    if ! command -v "$1" &> /dev/null; then
        echo "!!! Error: $1 not found!"
        exit 1
    fi
}

echo ">>> Checking prerequisites..."
check_command qemu-system-x86_64
check_command wget
check_command cargo

# Configuration
KERNEL_VER="virt"  # Alpine flavor
ALPINE_VER="3.19.1"
WORK_DIR="vm_test_work"
KERNEL_URL="https://dl-cdn.alpinelinux.org/alpine/v3.19/releases/x86_64/netboot/vmlinuz-virt"
INITRAMFS_URL="https://dl-cdn.alpinelinux.org/alpine/v3.19/releases/x86_64/netboot/initramfs-virt"

# Paths
IMAGE_RAW="${WORK_DIR}/disk.raw"
IMAGE_SNAP="${WORK_DIR}/disk.st"
MOUNT_DIR="${WORK_DIR}/mnt"
BINARY="$(pwd)/target/release/snapfs"

echo ">>> Setting up VM Test Environment in ${WORK_DIR}..."

# 1. Prepare Workspace
mkdir -p "${WORK_DIR}"
mkdir -p "${MOUNT_DIR}"

# 2. Download Kernel/Initrd if missing
if [ ! -f "${WORK_DIR}/vmlinuz" ]; then
    echo ">>> Downloading Linux Kernel..."
    wget -q "${KERNEL_URL}" -O "${WORK_DIR}/vmlinuz"
fi

if [ ! -f "${WORK_DIR}/initramfs" ]; then
    echo ">>> Downloading Initramfs..."
    wget -q "${INITRAMFS_URL}" -O "${WORK_DIR}/initramfs"
fi

# 3. Create a Dummy Disk Image
if [ ! -f "${IMAGE_RAW}" ]; then
    echo ">>> Creating 200MB compressible raw disk image..."
    yes "This is a test of SnapFS compression speed. We want some repeated text that compresses well." | \
        dd of="${IMAGE_RAW}" bs=1M count=200 status=progress iflag=fullblock
fi

# 3b. Create Benchmark Initramfs
if [ ! -f "${WORK_DIR}/initramfs-bench" ]; then
    echo ">>> Building Custom Benchmark Initramfs..."
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

# 4. Build SnapFS
echo ">>> Building SnapFS..."
cargo build --release --quiet

# 5. Create Snapshot
echo ">>> Converting Raw Image to SnapFS..."
if [ -f "${IMAGE_SNAP}" ]; then rm "${IMAGE_SNAP}"; fi
${BINARY} create --disk "${IMAGE_RAW}" --output "${IMAGE_SNAP}"

# 6. Mount via FUSE (Background)
echo ">>> Mounting Snapshot..."
fusermount -u "${MOUNT_DIR}" 2>/dev/null || true

FUSE_LOG="${WORK_DIR}/fuse.log"
${BINARY} mount "${IMAGE_SNAP}" "${MOUNT_DIR}" > "${FUSE_LOG}" 2>&1 &
FUSE_PID=$!

sleep 1

if ! mountpoint -q "${MOUNT_DIR}"; then
    echo "!!! Mount failed!"
    exit 1
fi

echo ">>> Mount successful. Verifying read access..."
if dd if="${MOUNT_DIR}/disk" of=/dev/null bs=1M count=1 status=none 2>/dev/null; then
    echo "    ✓ Successfully read from mounted snapshot!"
else
    echo "    ✗ Failed to read from mount"
    exit 1
fi

echo ""
echo ">>> Booting QEMU with mounted snapshot..."
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

echo ""
echo ">>> QEMU exited."
echo "    Boot duration: ${BOOT_DURATION}s"

echo ""
echo ">>> Unmounting..."
${BINARY} unmount "${MOUNT_DIR}"
wait $FUSE_PID 2>/dev/null || true

echo ">>> Test Complete."
