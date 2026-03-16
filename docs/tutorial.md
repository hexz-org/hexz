# Getting Started with Hexz

This tutorial walks through the core Hexz workflow: packing data into archives, creating thin deltas, and syncing them to a remote.

## Install

Pick one:

```bash
# Pre-built binary (Linux/macOS)
curl -fsSL https://raw.githubusercontent.com/hexz-org/hexz/main/install.sh | sh

# Homebrew (macOS)
brew install hexz-org/tap/hexz

# From source
cargo install hexz-cli

# Or from the repo
git clone https://github.com/hexz-org/hexz && cd hexz && make install
```

## 1. Pack your first archive

Create a deduplicated, compressed archive from any file or directory:

```bash
hexz pack ./my-data data-v1.hxz
```

Inspect what was created:

```bash
hexz show data-v1.hxz
```

```
╭ data-v1.hxz
│ format      v1, LZ4, 64 KiB blocks
│ size        48.2 MiB on disk, 120.0 MiB uncompressed (2.49x)
│ blocks      1920 data (1843 unique)
╰
```

Extract it back:

```bash
hexz extract data-v1.hxz ./restored-data
```

## 2. Create a thin delta

When your data changes, you don't need to store a full copy. A thin archive stores only the changed blocks:

```bash
# Make some changes to your data...
echo "updated" >> my-data/README.md

# Pack against the base — only new/changed blocks are stored
hexz pack ./my-data data-v2.hxz --base data-v1.hxz
```

The thin archive `data-v2.hxz` will be much smaller than a full archive because it references `data-v1.hxz` for unchanged blocks.

Compare two versions:

```bash
hexz diff data-v1.hxz data-v2.hxz
```

## 3. Mount an archive (Linux with FUSE)

Access archive contents without extracting:

```bash
mkdir /mnt/data
hexz mount data-v1.hxz /mnt/data
ls -lh /mnt/data/

# When done
hexz unmount /mnt/data
```

Or drop into an interactive shell:

```bash
hexz shell data-v1.hxz
# browse files, then exit the shell to auto-unmount
```

## 4. Git-like workspaces

For iterative workflows — check out, edit, commit:

```bash
# Create a writable workspace from an archive
hexz checkout data-v1.hxz ./workspace
cd workspace

# Edit files normally
echo "new content" > notes.txt

# See what changed
hexz status

# Commit changes as a thin delta
hexz commit ../data-v2.hxz
```

## 5. Sync with a remote

Push archives to S3-compatible storage (AWS S3, MinIO, R2, etc.):

```bash
# Add a remote to your workspace
hexz remote add origin s3://my-bucket/hexz-archives

# Push an archive (and any parent archives it depends on)
hexz push origin data-v2.hxz

# From another machine, pull everything
hexz pull origin

# Or pull a specific archive
hexz pull origin data-v2.hxz
```

### S3 credentials

Set standard AWS environment variables:

```bash
export AWS_ACCESS_KEY_ID=your-key
export AWS_SECRET_ACCESS_KEY=your-secret
export AWS_REGION=us-east-1

# For self-hosted (MinIO, etc.)
export AWS_ENDPOINT=http://localhost:9000
```

## 6. Encryption and signing

Encrypt an archive:

```bash
hexz pack ./sensitive-data encrypted.hxz --encrypt
# You'll be prompted for a password
```

Sign and verify archives with Ed25519:

```bash
hexz keygen
hexz sign hexz-private.key data-v1.hxz
hexz verify hexz-public.key data-v1.hxz
```

## What's next

- `hexz show --json <archive>` for machine-readable output
- `hexz predict <path>` to estimate compression/dedup savings before packing
- `hexz serve <archive>` to serve an archive over HTTP with range requests
- `hexz convert tar <input.tar> output.hxz` to import existing tar archives
