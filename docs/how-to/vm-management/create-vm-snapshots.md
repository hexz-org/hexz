# Create VM Snapshots

**Goal**: Capture the state of a running or stopped VM as a Strata snapshot.

## Prerequisites

- Strata CLI installed
- QEMU and KVM configured
- Source VM disk image or running VM

## Create Snapshot from Disk Image

### From Raw Disk Image

```bash
strata data pack \
  --disk /path/to/vm-disk.img \
  --output vm-snapshot.st \
  --compression zstd \
  --compression-level 9
```

### From QCOW2 Image

Convert QCOW2 to raw first:

```bash
qemu-img convert -f qcow2 -O raw vm-disk.qcow2 vm-disk.raw

strata data pack \
  --disk vm-disk.raw \
  --output vm-snapshot.st \
  --compression zstd
```

## Create Incremental Snapshot

Capture only changes since last snapshot:

```bash
# Create base snapshot
strata data pack \
  --disk base-vm.img \
  --output vm-base.st \
  --cdc

# Later, create incremental snapshot
strata data pack \
  --disk updated-vm.img \
  --output vm-v2.st \
  --parent vm-base.st \
  --cdc
```

**Result**: `vm-v2.st` only stores changed blocks, referencing `vm-base.st` for unchanged data.

## Snapshot Running VM

### Method 1: Using Overlay

Boot VM with overlay to capture changes:

```bash
# Boot with overlay
strata vm boot base-vm.st --overlay changes.img

# ... use VM ...

# Shutdown VM, then commit changes
strata vm commit \
  --base base-vm.st \
  --overlay changes.img \
  --output updated-vm.st
```

### Method 2: QEMU Monitor

For VMs not booted via Strata:

```bash
# In QEMU monitor
(qemu) stop
(qemu) savevm snapshot-name
(qemu) quit

# Convert saved snapshot
qemu-img convert -f qcow2 -O raw vm-disk.qcow2 vm-disk.raw
strata data pack --disk vm-disk.raw --output vm-snapshot.st
```

## Snapshot with Deduplication

Enable content-defined chunking for better deduplication:

```bash
strata data pack \
  --disk vm-disk.img \
  --output vm.st \
  --compression zstd \
  --cdc
```

**Benefit**: Multiple VM snapshots sharing common OS blocks will deduplicate automatically.

## Compress Different Disk Regions

For VMs with distinct regions (OS, data, swap):

```bash
# Pack with block size tuned for VM workload
strata data pack \
  --disk vm-disk.img \
  --output vm.st \
  --block-size 4096  # 4KB for VM (matches page size)
  --compression lz4  # Fast for VM boot
```

## Snapshot Verification

After creating snapshot, verify integrity:

```bash
# Check snapshot info
strata data info vm-snapshot.st

# Test mount
mkdir /tmp/test-mount
strata vm mount vm-snapshot.st /tmp/test-mount --readonly

# Verify files
ls -la /tmp/test-mount/

# Unmount
sudo umount /tmp/test-mount
```

## Snapshot Management

### List Snapshots

```bash
ls -lh *.st
```

### Compare Snapshots

```bash
strata data diff vm-v1.st vm-v2.st
```

### Sign Snapshot

```bash
# Generate key (once)
strata sys keygen --output-dir ./keys

# Sign snapshot
strata sys sign --key ./keys/private.key vm-snapshot.st

# Verify
strata sys verify --key ./keys/public.key vm-snapshot.st
```

## Best Practices

1. **Shutdown before snapshot**: Stop VM cleanly to ensure filesystem consistency
2. **Enable CDC**: Always use `--cdc` for better deduplication across versions
3. **Sign snapshots**: Use signing for production snapshots to verify integrity
4. **Test snapshots**: Always test boot snapshot before distributing
5. **Version naming**: Use semantic versioning (vm-ubuntu-v1.0.st, vm-ubuntu-v1.1.st)

## Example Workflow: Development VM Lifecycle

```bash
# 1. Install base OS
strata vm install \
  --iso ubuntu-22.04.iso \
  --output ubuntu-base.st \
  --disk-size 40G \
  --vnc

# 2. Boot and customize
strata vm boot ubuntu-base.st --overlay dev-setup.img
# Install tools, configure system, then shutdown

# 3. Commit changes
strata vm commit \
  --base ubuntu-base.st \
  --overlay dev-setup.img \
  --output ubuntu-dev-v1.0.st \
  --cdc

# 4. Use for development
strata vm boot ubuntu-dev-v1.0.st --snapshot  # Changes discarded on exit

# 5. Create updated version
strata vm boot ubuntu-dev-v1.0.st --overlay updates.img
# Make updates, then shutdown

strata vm commit \
  --base ubuntu-dev-v1.0.st \
  --overlay updates.img \
  --output ubuntu-dev-v1.1.st \
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
