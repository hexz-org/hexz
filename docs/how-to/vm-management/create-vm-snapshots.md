# Create VM Snapshots

**Goal**: Capture the state of a running or stopped VM as a Hexz snapshot.

## Prerequisites

- Hexz CLI installed
- QEMU and KVM configured
- Source VM disk image or running VM

## Create Snapshot from Disk Image

### From Raw Disk Image

```bash
hexz data pack \
  --disk /path/to/vm-disk.img \
  --output vm-snapshot.hxz \
  --compression zstd \
  --compression-level 9
```

### From QCOW2 Image

Convert QCOW2 to raw first:

```bash
qemu-img convert -f qcow2 -O raw vm-disk.qcow2 vm-disk.raw

hexz data pack \
  --disk vm-disk.raw \
  --output vm-snapshot.hxz \
  --compression zstd
```

## Create Incremental Snapshot

Capture only changes since last snapshot:

```bash
# Create base snapshot
hexz data pack \
  --disk base-vm.img \
  --output vm-base.hxz \
  --cdc

# Later, create incremental snapshot
hexz data pack \
  --disk updated-vm.img \
  --output vm-v2.hxz \
  --parent vm-base.hxz \
  --cdc
```

**Result**: `vm-v2.hxz` only stores changed blocks, referencing `vm-base.hxz` for unchanged data.

## Snapshot Running VM

### Method 1: Using Overlay

Boot VM with overlay to capture changes:

```bash
# Boot with overlay
hexz vm boot base-vm.hxz --overlay changes.img

# ... use VM ...

# Shutdown VM, then commit changes
hexz vm commit \
  --base base-vm.hxz \
  --overlay changes.img \
  --output updated-vm.hxz
```

### Method 2: QEMU Monitor

For VMs not booted via Hexz:

```bash
# In QEMU monitor
(qemu) stop
(qemu) savevm snapshot-name
(qemu) quit

# Convert saved snapshot
qemu-img convert -f qcow2 -O raw vm-disk.qcow2 vm-disk.raw
hexz data pack --disk vm-disk.raw --output vm-snapshot.hxz
```

## Snapshot with Deduplication

Enable content-defined chunking for better deduplication:

```bash
hexz data pack \
  --disk vm-disk.img \
  --output vm.hxz \
  --compression zstd \
  --cdc
```

**Benefit**: Multiple VM snapshots sharing common OS blocks will deduplicate automatically.

## Compress Different Disk Regions

For VMs with distinct regions (OS, data, swap):

```bash
# Pack with block size tuned for VM workload
hexz data pack \
  --disk vm-disk.img \
  --output vm.hxz \
  --block-size 4096  # 4KB for VM (matches page size)
  --compression lz4  # Fast for VM boot
```

## Snapshot Verification

After creating snapshot, verify integrity:

```bash
# Check snapshot info
hexz data info vm-snapshot.hxz

# Test mount
mkdir /tmp/test-mount
hexz vm mount vm-snapshot.hxz /tmp/test-mount --readonly

# Verify files
ls -la /tmp/test-mount/

# Unmount
sudo umount /tmp/test-mount
```

## Snapshot Management

### List Snapshots

```bash
ls -lh *.hxz
```

### Compare Snapshots

```bash
hexz data diff vm-v1.hxz vm-v2.hxz
```

### Sign Snapshot

```bash
# Generate key (once)
hexz sys keygen --output-dir ./keys

# Sign snapshot
hexz sys sign --key ./keys/private.key vm-snapshot.hxz

# Verify
hexz sys verify --key ./keys/public.key vm-snapshot.hxz
```

## Best Practices

1. **Shutdown before snapshot**: Stop VM cleanly to ensure filesystem consistency
2. **Enable CDC**: Always use `--cdc` for better deduplication across versions
3. **Sign snapshots**: Use signing for production snapshots to verify integrity
4. **Test snapshots**: Always test boot snapshot before distributing
5. **Version naming**: Use semantic versioning (vm-ubuntu-v1.0.st, vm-ubuntu-v1.1.hxz)

## Example Workflow: Development VM Lifecycle

```bash
# 1. Install base OS
hexz vm install \
  --iso ubuntu-22.04.iso \
  --output ubuntu-base.hxz \
  --disk-size 40G \
  --vnc

# 2. Boot and customize
hexz vm boot ubuntu-base.hxz --overlay dev-setup.img
# Install tools, configure system, then shutdown

# 3. Commit changes
hexz vm commit \
  --base ubuntu-base.hxz \
  --overlay dev-setup.img \
  --output ubuntu-dev-v1.0.hxz \
  --cdc

# 4. Use for development
hexz vm boot ubuntu-dev-v1.0.hxz --snapshot  # Changes discarded on exit

# 5. Create updated version
hexz vm boot ubuntu-dev-v1.0.hxz --overlay updates.img
# Make updates, then shutdown

hexz vm commit \
  --base ubuntu-dev-v1.0.hxz \
  --overlay updates.img \
  --output ubuntu-dev-v1.1.hxz \
  --cdc
```

## Troubleshooting

**"Permission denied" when accessing disk image**:
```bash
sudo chmod +r /path/to/vm-disk.img
```

**"Out of memory" during packing**:
- Use smaller block size: `--block-size 16384`
- Disable CDC: Remove `--cdc` flag

**Snapshot too large**:
- Enable CDC: Add `--cdc` flag
- Increase compression: `--compression-level 9`
- Zero unused space before snapshot:
  ```bash
  # Inside VM before snapshot
  sudo dd if=/dev/zero of=/zerofile bs=1M
  sudo rm /zerofile
  ```

## See Also

- [How-To: Boot VM from Snapshot](boot-vm-from-snapshot.md)
- [How-To: Setup VM Networking](setup-vm-networking.md)
- [How-To: Commit Overlay Changes](commit-overlay-changes.md)
- [Reference: CLI Commands](../../reference/cli-reference.md)
