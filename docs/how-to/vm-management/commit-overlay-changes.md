# Commit Overlay Changes

**Goal**: Save changes made to a read-write overlay back into a new Strata snapshot.

## Prerequisites

- Base snapshot
- Overlay image with changes
- Strata CLI installed

## Understanding Overlays

When you boot a VM with `--overlay`, writes go to a separate overlay file while the base snapshot remains unchanged.

```
Base Snapshot (read-only) + Overlay (writes) = Running VM State
```

## Create Overlay During Boot

```bash
strata vm boot base-vm.st --overlay changes.img
```

Make changes in the VM, then shutdown.

## Commit Overlay to New Snapshot

```bash
strata vm commit \\
  --base base-vm.st \\
  --overlay changes.img \\
  --output updated-vm.st
```

**Result**: `updated-vm.st` contains base data + all changes from overlay.

## Complete Workflow Example

```bash
# 1. Boot with overlay
strata vm boot ubuntu-base.st --overlay dev-setup.img
```

Inside VM:
```bash
sudo apt update
sudo apt install build-essential git vim
# ... make other changes ...
sudo shutdown -h now
```

```bash
# 2. Commit changes
strata vm commit \\
  --base ubuntu-base.st \\
  --overlay dev-setup.img \\
  --output ubuntu-dev.st \\
  --cdc  # Enable deduplication
```

```bash
# 3. New snapshot ready to use
strata vm boot ubuntu-dev.st
```

## Incremental Updates

Make additional changes:

```bash
# Boot the updated snapshot with new overlay
strata vm boot ubuntu-dev.st --overlay new-changes.img

# Make more changes...

# Commit again
strata vm commit \\
  --base ubuntu-dev.st \\
  --overlay new-changes.img \\
  --output ubuntu-dev-v2.st \\
  --cdc
```

## Discard Overlay

To throw away changes without committing:

```bash
rm changes.img
```

That's it. The overlay file contains all changes.

## Mount and Commit

Alternative approach using FUSE mount:

```bash
# Mount with overlay
mkdir /tmp/vm-mount
strata vm mount base-vm.st /tmp/vm-mount --overlay changes.img

# Make changes to mounted filesystem
sudo cp /etc/config /tmp/vm-mount/etc/config

# Unmount
sudo umount /tmp/vm-mount

# Commit
strata vm commit \\
  --base base-vm.st \\
  --overlay changes.img \\
  --output updated-vm.st
```

## Best Practices

1. **Test before commit**: Boot with overlay to verify changes work
2. **Enable CDC**: Always use `--cdc` flag for better deduplication
3. **Naming convention**: Use semantic versioning (vm-v1.0.st, vm-v1.1.st)
4. **Backup base**: Keep base snapshot until new snapshot is verified
5. **Sign snapshots**: Sign production snapshots for integrity

## Troubleshooting

**"Overlay file not found"**:
- Verify file exists: `ls -lh changes.img`
- Use absolute path

**"Base snapshot not found"**:
- Ensure base snapshot path is correct
- Use absolute paths for both base and overlay

**Commit takes long time**:
- Normal for large overlays
- Enable `--cdc` for faster subsequent commits

**Committed snapshot too large**:
- Enable `--cdc` flag for deduplication
- Check overlay doesn't contain temp files:
  ```bash
  # Inside VM before shutdown
  sudo apt clean
  sudo rm -rf /tmp/*
  ```

## See Also

- [How-To: Create VM Snapshots](create-vm-snapshots.md)
- [How-To: Boot VM from Snapshot](boot-vm-from-snapshot.md)
- [Reference: CLI Commands](../../reference/cli-reference.md)
