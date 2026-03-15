# API Reference

## Rust API (rustdoc)

The full Rust API documentation is built from source and available at [`/rustdoc/hexz_core/`](rustdoc/hexz_core/).

### Key Crates

| Crate | Description |
| :--- | :--- |
| `hexz-core` | Archive engine: format, compression, encryption, hashing, dedup |
| `hexz-store` | Storage backends: local, HTTP, S3 |
| `hexz-ops` | High-level operations: pack, write, inspect, signing |
| `hexz-common` | Shared error types, configuration, logging |
| `hexz-fuse` | FUSE filesystem adapter |
| `hexz-server` | HTTP and NBD servers |
| `hexz-vfs` | Platform-agnostic virtual filesystem logic |

## Python API

```bash
pip install hexz
```

```python
import hexz

# Open an archive
archive = hexz.open("data.hxz")

# Read bytes at offset
data = archive.read(offset=0, size=4096)

# Get archive metadata
info = archive.info()
```

## CLI Reference

```bash
hexz --help
```

See the [README](index.md) for usage examples.
