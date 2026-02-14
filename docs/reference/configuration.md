# Configuration Options

Hexz configuration via environment variables and API parameters.

## Environment Variables

### AWS/S3 Configuration

- `AWS_ACCESS_KEY_ID` — AWS access key for S3
- `AWS_SECRET_ACCESS_KEY` — AWS secret key for S3
- `AWS_DEFAULT_REGION` — Default AWS region (e.g., `us-west-2`)
- `AWS_PROFILE` — AWS CLI profile name

### Hexz Configuration

- `HEXZ_CACHE_DIR` — Default cache directory (default: `$TMPDIR/hexz-cache`)
- `HEXZ_CACHE_SIZE` — Default cache size in bytes (default: `268435456` = 256MB)
- `HEXZ_LOG_LEVEL` — Logging level: `error`, `warn`, `info`, `debug`, `trace`

### Example

```bash
export HEXZ_CACHE_SIZE=$((2 * 1024 * 1024 * 1024))  # 2GB
export HEXZ_LOG_LEVEL=debug
export AWS_DEFAULT_REGION=us-west-2

python train.py
```

## Python API Parameters

### `hexz.open()` Read Mode

```python
hexz.open(
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

### `hexz.open()` Write Mode

```python
hexz.open(
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
hexz -v ...      # Verbose
hexz -vv ...     # Very verbose (debug)
hexz -vvv ...    # Trace level
hexz --quiet ... # Suppress output

# Help
hexz --help
hexz data --help
hexz data pack --help
```

## Configuration File (Future)

Not yet implemented. Planned format:

```toml
# ~/.config/hexz/config.toml

[cache]
size = 2147483648  # 2GB
dir = "/tmp/hexz-cache"

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
