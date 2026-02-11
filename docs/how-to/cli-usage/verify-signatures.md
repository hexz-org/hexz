# Verify Snapshot Signatures

**Goal**: Verify integrity and authenticity of signed Strata snapshots.

## Prerequisites

- Strata CLI installed
- Public key from snapshot publisher
- Signed snapshot file

## Why Verify Snapshots

Verification ensures:
- Snapshot hasn't been tampered with
- Snapshot comes from trusted source
- Data integrity during transfer

## Verify Signed Snapshot

```bash
strata sys verify --key public.key snapshot.st
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
curl -O https://strata.example.com/keys/public.key

# Verify fingerprint matches documentation
sha256sum public.key
```

## Verify in Scripts

```bash
#!/bin/bash

if strata sys verify --key public.key dataset.st; then
    echo "Verification successful"
    # Proceed with usage
    strata vm boot dataset.st
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
        ["strata", "sys", "verify", "--key", public_key_path, snapshot_path],
        capture_output=True
    )
    return result.returncode == 0

if not verify_snapshot("dataset.st", "public.key"):
    print("Snapshot verification failed!", file=sys.stderr)
    sys.exit(1)

# Safe to use
import strata
dataset = strata.open("dataset.st")
```

## Create Signed Snapshots

If you're publishing snapshots:

### Generate Keypair

```bash
strata sys keygen --output-dir ./keys
```

Creates:
- `keys/private.key` (keep secret!)
- `keys/public.key` (distribute publicly)

### Sign Snapshot

```bash
# Pack snapshot
strata data pack --disk data/ --output dataset.st

# Sign snapshot
strata sys sign --key keys/private.key dataset.st
```

### Distribute

Distribute:
- `dataset.st` (signed snapshot)
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
if ! strata sys verify --key "$PUBLIC_KEY" snapshot.st; then
    echo "ERROR: Snapshot verification failed" >&2
    rm snapshot.st
    exit 1
fi

echo "Snapshot verified successfully"
```

Usage:
```bash
./download-and-verify.sh https://example.com/dataset.st
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
- [How-To: Install Strata](install-strata.md)
