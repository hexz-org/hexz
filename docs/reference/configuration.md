# Configuration Options

Strata configuration via environment variables and API parameters.

## Environment Variables

### AWS/S3 Configuration

- `AWS_ACCESS_KEY_ID` — AWS access key for S3
- `AWS_SECRET_ACCESS_KEY` — AWS secret key for S3
- `AWS_DEFAULT_REGION` — Default AWS region (e.g., `us-west-2`)
- `AWS_PROFILE` — AWS CLI profile name

### Strata Configuration

- `STRATA_CACHE_DIR` — Default cache directory (default: `$TMPDIR/strata-cache`)
- `STRATA_CACHE_SIZE` — Default cache size in bytes (default: `268435456` = 256MB)
- `STRATA_LOG_LEVEL` — Logging level: `error`, `warn`, `info`, `debug`, `trace`

### Example

```bash
export STRATA_CACHE_SIZE=$((2 * 1024 * 1024 * 1024))  # 2GB
export STRATA_LOG_LEVEL=debug
export AWS_DEFAULT_REGION=us-west-2

python train.py
```

## Python API Parameters

### `strata.open()` Read Mode

```python
strata.open(
    path,
    mode='r',
    # S3 options
    s3_region=None,              # AWS region (default: auto-detect)
    # Caching
    cache_size=256*1024*1024,    # Cache size in bytes
    cache_dir=None,              # Disk cache directory (default: memory only)
    # Network
    retry_attempts=3,            # Retry count for remote reads
    connect_timeout=10,          # Connection timeout (seconds)
    read_timeout=30              # Read timeout (seconds)
)
```

### `strata.open()` Write Mode

```python
strata.open(
    path,
    mode='w',
    # Compression
    compression='lz4',           # Algorithm: 'lz4' or 'zstd'
    compression_level=3,         # Zstd level (1-22, ignored for lz4)
    block_size=65536,            # Block size in bytes
    # Deduplication
    cdc=False,                   # Enable content-defined chunking
    # Encryption
    encrypt=False,               # Enable AES-256-GCM
    encryption_key=None          # 32-byte key (required if encrypt=True)
)
```

## CLI Configuration

CLI options override environment variables.

### Common Flags

```bash
# Verbosity
strata -v ...      # Verbose
strata -vv ...     # Very verbose (debug)
strata -vvv ...    # Trace level
strata --quiet ... # Suppress output

# Help
strata --help
strata data --help
strata data pack --help
```

## Configuration File (Future)

Not yet implemented. Planned format:

```toml
# ~/.config/strata/config.toml

[cache]
size = 2147483648  # 2GB
dir = "/tmp/strata-cache"

[s3]
region = "us-west-2"
retry_attempts = 5

[logging]
level = "info"
```

## See Also

- [Reference: Python API](python-api.md)
- [Reference: CLI Reference](cli-reference.md)
- [How-To: Setup S3 Streaming](../how-to/ml-workflows/setup-s3-streaming.md)
