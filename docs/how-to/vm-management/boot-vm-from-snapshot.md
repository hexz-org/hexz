# Boot VM from Snapshot

**Goal**: Start a virtual machine directly from a Hexz snapshot.

## Prerequisites

- Hexz CLI installed
- QEMU and KVM installed
- Bootable VM snapshot

## Basic Boot

```bash
hexz vm boot vm-snapshot.st
```

This opens a console window with the booted VM.

## Boot with Custom Resources

```bash
hexz vm boot vm-snapshot.st \\
  --ram 4G \\
  --cpus 4
```

Supported RAM formats: `512M`, `1G`, `2048M`, `8G`

## Boot with Networking

```bash
hexz vm boot vm-snapshot.st \\
  --net \\
  --forward 2222:22
```

Then access via SSH:
```bash
ssh -p 2222 user@localhost
```

## Boot Headless (No Display)

```bash
hexz vm boot vm-snapshot.st \\
  --headless \\
  --net \\
  --forward 2222:22
```

Useful for server VMs accessed only via SSH.

## Boot with VNC

```bash
hexz vm boot vm-snapshot.st \\
  --vnc
```

Connect via VNC client to `localhost:5900`.

## Boot in Snapshot Mode (Ephemeral)

```bash
hexz vm boot vm-snapshot.st --snapshot
```

All changes discarded on VM shutdown. Useful for testing.

## Complete Example

```bash
hexz vm boot ubuntu-dev.st \\
  --ram 8G \\
  --cpus 4 \\
  --net \\
  --forward 2222:22 \\
  --forward 8080:80
```

## Exit VM

From VM console: Press `Ctrl+A`, then `X`

Or shutdown from inside VM:
```bash
sudo shutdown -h now
```

## Troubleshooting

**"Permission denied on /dev/kvm"**:
```bash
sudo usermod -a -G kvm $(whoami)
newgrp kvm
```

**"Snapshot not found"**:
- Verify file exists: `ls -lh vm-snapshot.st`
- Use absolute path: `/home/user/vm-snapshot.st`

**VM boots slowly**:
- Ensure KVM enabled (not emulation)
- Check with: `grep -E '(vmx|svm)' /proc/cpuinfo`

## See Also

- [Tutorial: Booting Your First VM](../../tutorials/booting-your-first-vm.md)
- [How-To: Setup VM Networking](setup-vm-networking.md)
- [How-To: Create VM Snapshots](create-vm-snapshots.md)
- [Reference: CLI Commands](../../reference/cli-reference.md)
