# Booting Your First VM

**Time to Complete**: 15 minutes

**What You'll Learn**: Boot a virtual machine directly from a compressed Hexz snapshot without extraction.

**What You'll Build**: A bootable Ubuntu VM running from a Hexz snapshot, demonstrating instant boot capabilities.

## Prerequisites

- Completed [Getting Started](getting-started.md)
- Hexz CLI installed (`make rust`)
- QEMU installed
- KVM support (Linux) or hardware virtualization enabled
- 4GB of free disk space

## Learning Objectives

By the end of this tutorial, you will:

1. Understand how Hexz enables VM boot from compressed snapshots
2. Create or download a bootable VM snapshot
3. Boot a VM with custom resource allocation
4. Configure networking and port forwarding
5. Create snapshots of running VMs

## Step 1: Verify System Requirements

Check that your system supports virtualization:

```bash
# Run system diagnostics
./target/release/hexz sys doctor
```

**Expected Output**:
```
[x] QEMU found: /usr/bin/qemu-system-x86_64 (v7.2.0)
[x] KVM available: /dev/kvm
[x] FUSE support: OK
[x] Library versions: OK
```

**If KVM permission denied**:
```bash
sudo usermod -a -G kvm $(whoami)
newgrp kvm  # Or log out and back in
```

## Step 2: Obtain a VM Snapshot

**Option A: Download Pre-Built Snapshot** (Quickest):
```bash
# Download a minimal Ubuntu snapshot (example URL)
wget https://example.com/ubuntu-minimal.hxz -O /tmp/ubuntu.st
```

**Option B: Install from ISO**:
```bash
# Download Ubuntu Server ISO
wget https://releases.ubuntu.com/22.04/ubuntu-22.04-live-server-amd64.iso

# Install interactively via VNC
./target/release/hexz vm install \\
  --iso ubuntu-22.04-live-server-amd64.iso \\
  --output /tmp/ubuntu.hxz \\
  --disk-size 20G \\
  --ram 2G \\
  --vnc

# Connect via VNC client to localhost:5900 and complete installation
```

## Step 3: Boot the VM

Boot with default settings:

```bash
./target/release/hexz vm boot /tmp/ubuntu.st
```

**What Just Happened**:
- Hexz mounted the snapshot via FUSE
- QEMU booted using the mounted disk image
- VM started reading blocks on-demand (no extraction)
- Console appeared in terminal window

**To exit**: Press `Ctrl+A`, then `X`

## Step 4: Boot with Networking

Enable network access and SSH port forwarding:

```bash
./target/release/hexz vm boot /tmp/ubuntu.hxz \\
  --ram 4G \\
  --cpus 4 \\
  --net \\
  --forward 2222:22
```

**Connect via SSH** (from another terminal):
```bash
ssh -p 2222 user@localhost
```

## Step 5: Boot in Snapshot Mode (Ephemeral)

Run the VM without saving changes:

```bash
./target/release/hexz vm boot /tmp/ubuntu.hxz --snapshot
```

**Use Case**: Testing, debugging, or running untrusted code without risk.

## What You've Accomplished

- [x] Verified virtualization support
- [x] Obtained a bootable VM snapshot
- [x] Booted a VM from compressed storage
- [x] Configured networking and port forwarding
- [x] Understood ephemeral snapshot mode

## Next Steps

- [Create VM Snapshots](../how-to/vm-management/create-vm-snapshots.md)
- [Setup VM Networking](../how-to/vm-management/setup-vm-networking.md)
- [CLI Reference](../reference/cli-reference.md)

## See Also

- [Explanation: Architecture](../explanation/architecture.md) — How instant boot works
- [ADR-0004: Storage Backend Abstraction](../adr/0004-storage-backend-abstraction.md)
