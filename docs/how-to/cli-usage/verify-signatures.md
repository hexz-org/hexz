# Verify Snapshot Signatures

**Goal**: Verify integrity and authenticity of signed Hexz snapshots.

## Prerequisites

- Hexz CLI installed
- Public key from snapshot publisher
- Signed snapshot file

## Why Verify Snapshots

Verification ensures:
- Snapshot hasn't been tampered with
- Snapshot comes from trusted source
- Data integrity during transfer

## Verify Signed Snapshot

```bash
hexz sys verify --key public.key snapshot.st
```

**Success output** (silent):
```
(no output, exit code 0)
```

**Failure output**:
```
Error: Signature verification failed
(exit code 1)
```

## Obtain Public Key

Public keys should be distributed via secure channel:
- Official documentation
- HTTPS download from project website
- Package repository
- Direct from publisher

**Example**:
```bash
# Download official public key
curl -O https://hexz.example.com/keys/public.key

# Verify fingerprint matches documentation
sha256sum public.key
```

## Verify in Scripts

```bash
#!/bin/bash

if hexz sys verify --key public.key dataset.hxz; then
    echo "Verification successful"
    # Proceed with usage
    hexz vm boot dataset.hxz
else
    echo "Verification failed!" >&2
    exit 1
fi
```

## Verify Before Use (Python)

```python
import subprocess
import sys

def verify_snapshot(snapshot_path, public_key_path):
    result = subprocess.run(
        ["hexz", "sys", "verify", "--key", public_key_path, snapshot_path],
        capture_output=True
    )
    return result.returncode == 0

if not verify_snapshot("dataset.hxz", "public.key"):
    print("Snapshot verification failed!", file=sys.stderr)
    sys.exit(1)

# Safe to use
import hexz
dataset = hexz.open("dataset.hxz")
```

## Create Signed Snapshots

If you're publishing snapshots:

### Generate Keypair

```bash
hexz sys keygen --output-dir ./keys
```

Creates:
- `keys/private.key` (keep secret!)
- `keys/public.key` (distribute publicly)

### Sign Snapshot

```bash
# Pack snapshot
hexz data pack --disk data/ --output dataset.hxz

# Sign snapshot
hexz sys sign --key keys/private.key dataset.hxz
```

### Distribute

Distribute:
- `dataset.hxz` (signed snapshot)
- `public.key` (for verification)

Keep secret:
- `private.key` (never distribute!)

## Verification in Production

Automate verification for all downloads:

```bash
#!/bin/bash
# download-and-verify.sh

SNAPSHOT_URL="$1"
PUBLIC_KEY="./trusted-keys/public.key"

# Download snapshot
wget "$SNAPSHOT_URL" -O snapshot.st

# Verify
if ! hexz sys verify --key "$PUBLIC_KEY" snapshot.st; then
    echo "ERROR: Snapshot verification failed" >&2
    rm snapshot.st
    exit 1
fi

echo "Snapshot verified successfully"
```

Usage:
```bash
./download-and-verify.sh https://example.com/dataset.hxz
```

## Key Management Best Practices

**For Publishers**:
- Store private key in secure location (HSM, vault)
- Never commit private key to version control
- Rotate keys periodically
- Publish key fingerprints via multiple channels

**For Users**:
- Verify public key fingerprint from multiple sources
- Store public keys in secure, version-controlled location
- Always verify before using snapshots
- Reject unsigned snapshots in production

## Troubleshooting

**"Public key not found"**:
- Verify file exists: `ls -l public.key`
- Use absolute path

**"Signature verification failed"**:
- Snapshot may be corrupted (re-download)
- Wrong public key
- Snapshot unsigned
- Snapshot modified after signing

**"Invalid key format"**:
- Key file corrupted
- Wrong key type (need Ed25519 key)

## See Also

- [Reference: CLI Commands](../../reference/cli-reference.md)
- [How-To: Pack Datasets](pack-datasets.md)
- [How-To: Install Hexz](install-hexz.md)
