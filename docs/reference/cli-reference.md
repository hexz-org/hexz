# CLI Command Reference

Complete reference for the `hexz` command-line tool.

## Installation

```bash
cargo install hexz-cli
```

Or build from source:
```bash
git clone https://github.com/hexz-org/hexz.git
cd hexz && make rust
./target/release/hexz
```

---

## Command Structure

```
hexz <COMMAND> [OPTIONS] [ARGS]
```

---

## `hexz store`

Pack a safetensors or GGUF file into a Hexz archive. Chunks at tensor boundaries rather than using CDC.

**Synopsis:**
```bash
hexz store <INPUT> <OUTPUT> [OPTIONS]
```

**Arguments:**
- `<INPUT>` — Source file (`.safetensors` or `.gguf`). Format inferred from extension.
- `<OUTPUT>` — Output `.hxz` archive path.

**Options:**
- `--base <PATH>` — Parent `.hxz` archive for delta storage. Byte-identical blocks are referenced; changed tensors are stored compressed.
- `--compression <ALGO>` — `lz4` or `zstd` (default: `zstd`)
- `--compression-level <N>` — Zstd level 1–22 (default: 3)
- `--block-size <BYTES>` — Block alignment size (default: 65536)
- `-s, --silent` — Suppress progress output

**Example:**
```bash
# Pack a base model
hexz store llama-3-8b.safetensors llama-3-8b.hxz

# Pack a fine-tune against the base — only diffs stored
hexz store llama-3-8b-ft.safetensors llama-3-8b-ft.hxz --base llama-3-8b.hxz

# Pack a GGUF model
hexz store model-q4_K_M.gguf model-q4_K_M.hxz --compression zstd
```

---

## `hexz extract`

Reconstruct a safetensors file from a Hexz archive, or extract a single tensor.

**Synopsis:**
```bash
hexz extract <INPUT> [OUTPUT] [OPTIONS]
```

**Arguments:**
- `<INPUT>` — Source `.hxz` archive.
- `[OUTPUT]` — Output path (default: `<INPUT stem>.safetensors`).

**Options:**
- `--tensor <NAME>` — Extract only the named tensor. Writes raw tensor bytes (no safetensors header).

**Example:**
```bash
# Full round-trip
hexz extract llama-3-8b-ft.hxz llama-3-8b-ft-out.safetensors

# Single tensor
hexz extract llama-3-8b-ft.hxz --tensor lm_head.weight
```

---

## `hexz inspect`

Display the tensor manifest and archive metadata without decompressing data blocks.

**Synopsis:**
```bash
hexz inspect <ARCHIVE>
```

**Example output:**
```
Archive:        llama-3-8b-ft.hxz
Format version: 1
Compression:    zstd (level 3)
Block size:     65536
Uncompressed:   13.8 GB
Stored:         [UNTESTED — will be shown once XOR delta lands]
Parent:         llama-3-8b.hxz

Tensors (362):
  embed_tokens.weight              BF16  [32000, 4096]  256.0 MB
  layers.0.self_attn.q_proj.weight BF16  [4096, 4096]    32.0 MB
  ...
  lm_head.weight                   BF16  [32000, 4096]  256.0 MB
```

---

## `hexz diff`

Compare two archives and report shared vs changed blocks.

**Synopsis:**
```bash
hexz diff <ARCHIVE_A> <ARCHIVE_B>
```

**Example output:**
```
Comparing:
  A: llama-3-8b.hxz     (13.8 GB)
  B: llama-3-8b-ft.hxz  (13.8 GB)

Block analysis (65KB blocks):
  Total blocks:   [UNTESTED]
  Shared blocks:  [UNTESTED]  — referenced from A, not re-stored
  New blocks:     [UNTESTED]

Storage cost of B without dedup:  13.8 GB
Storage cost of B with dedup:     [UNTESTED]
```

---

## `hexz ls`

List all `.hxz` files in a directory, showing the parent chain and unique bytes on disk.

**Synopsis:**
```bash
hexz ls <DIR>
```

**Example output:**
```
./checkpoints/
  llama-3-8b.hxz         13.8 GB   (base)
  llama-3-8b-ft-v1.hxz   [UNTESTED] GB  (parent: llama-3-8b.hxz)
  llama-3-8b-ft-v2.hxz   [UNTESTED] GB  (parent: llama-3-8b-ft-v1.hxz)

Total unique bytes on disk: [UNTESTED]
```

---

## `hexz pack`

Pack a generic file or directory into a Hexz archive. For model files prefer `hexz store`, which understands tensor structure.

**Synopsis:**
```bash
hexz pack --disk <INPUT> --output <OUTPUT> [OPTIONS]
```

**Options:**
- `--disk <PATH>` — Input file or directory (required)
- `--output <PATH>` — Output path (required)
- `--compression <ALGO>` — `lz4` (default) or `zstd`
- `--compression-level <N>` — Zstd level 1–22
- `--block-size <BYTES>` — Block size (default: 65536)
- `--cdc` — Enable content-defined chunking (FastCDC)
- `--dcam` — Enable DCAM analysis before packing
- `--parent <PATH>` — Parent archive for cross-file dedup
- `--encrypt` — Enable AES-256-GCM encryption
- `--key <PATH>` — Encryption key file

---

## `hexz keygen`

Generate an Ed25519 signing keypair.

**Synopsis:**
```bash
hexz keygen [--output-dir <DIR>]
```

Creates `private.key` and `public.key` in the specified directory (default: current directory).

---

## `hexz sign`

Sign an archive with an Ed25519 private key.

**Synopsis:**
```bash
hexz sign --key <PRIVATE_KEY> <ARCHIVE>
```

---

## `hexz verify`

Verify an archive signature.

**Synopsis:**
```bash
hexz verify --key <PUBLIC_KEY> <ARCHIVE>
```

**Exit codes:** `0` = valid, `1` = invalid or missing signature

---

## `hexz bench`

Run compression and I/O benchmarks.

**Synopsis:**
```bash
hexz bench [OPTIONS]
```

**Options:**
- `--compression <ALGO>` — `lz4`, `zstd`, or `all`
- `--block-size <BYTES>` — Block size for tests
- `--threads <N>` — Thread count

---

## Global Options

- `-h, --help` — Show help
- `-V, --version` — Show version
- `-v` — Increase verbosity (repeatable: `-vv`, `-vvv`)
- `--quiet` — Suppress non-error output

---

## Environment Variables

- `HEXZ_CACHE_DIR` — Cache directory for remote archives
- `HEXZ_CACHE_SIZE` — Cache size in bytes
- `AWS_PROFILE` — AWS profile for S3 access
- `AWS_DEFAULT_REGION` — AWS region

---

## Exit Codes

- `0` — Success
- `1` — General error
- `2` — Invalid arguments
- `3` — Permission denied
- `4` — File not found
- `5` — I/O error

---

## See Also

- [Getting Started](../tutorials/getting-started.md)
- [Python API Reference](python-api.md)
- [Tensor Format Support](tensor-formats.md)
