# Version Compatibility

Compatibility matrix for Strata with dependencies and platforms.

## Strata Versions

Current version: 0.1.0-alpha

## Python Version Support

| Strata Version | Python Versions | Notes |
|----------------|----------------|-------|
| 0.1.x | 3.8, 3.9, 3.10, 3.11, 3.12 | Minimum 3.8 required |

## PyTorch Compatibility

| Strata Version | PyTorch Versions | Notes |
|----------------|-----------------|-------|
| 0.1.x | 1.12+ | Tested with 1.12, 1.13, 2.0, 2.1 |

DataLoader features require PyTorch 1.12 or later for proper multi-worker support.

## NumPy Compatibility

| Strata Version | NumPy Versions | Notes |
|----------------|---------------|-------|
| 0.1.x | 1.19+ | Zero-copy buffer protocol support |

## Platform Support

### Operating Systems

| Platform | Supported | Notes |
|----------|-----------|-------|
| Linux | Yes | Primary development platform |
| macOS | Yes | Intel and Apple Silicon (ARM) |
| Windows | Partial | CLI works, FUSE/VM features limited |

### CPU Architectures

| Architecture | Supported | Notes |
|--------------|-----------|-------|
| x86_64 (Intel/AMD) | Yes | Full optimization |
| ARM64 (Apple Silicon) | Yes | Native support |
| ARM64 (Linux) | Yes | Tested on AWS Graviton |

## Rust Toolchain

For building from source:

| Strata Version | Minimum Rust Version | Notes |
|----------------|---------------------|-------|
| 0.1.x | 1.70+ | Stable channel required |

## Dependency Versions

### Core Dependencies

| Dependency | Version | Purpose |
|------------|---------|---------|
| lz4 | 1.9+ | LZ4 compression |
| zstd | 1.5+ | Zstandard compression |
| blake3 | Latest | Content hashing |
| PyO3 | 0.19+ | Python bindings |

### Optional Dependencies

| Dependency | Version | Purpose |
|------------|---------|---------|
| QEMU | 7.0+ | VM boot support |
| KVM | N/A | Linux VM acceleration |
| FUSE | 2.9+ or 3.0+ | Filesystem mounting |
| AWS CLI | 2.0+ | S3 operations |

## File Format Compatibility

| Format Version | Strata Versions | Status |
|----------------|----------------|--------|
| v1 | 0.1.x | Current |

**Forward Compatibility**: Newer Strata versions can read older format versions.

**Backward Compatibility**: Older Strata versions cannot read newer format versions.

## S3 Compatibility

| S3 Implementation | Supported | Notes |
|-------------------|-----------|-------|
| AWS S3 | Yes | Full support |
| MinIO | Yes | S3-compatible storage |
| Ceph | Yes | S3-compatible storage |
| Google Cloud Storage | Partial | S3 compatibility mode |
| Azure Blob Storage | No | Not S3-compatible |

## Python Package Distribution

| Platform | Package Format | Notes |
|----------|---------------|-------|
| Linux (x86_64) | manylinux wheel | glibc 2.17+ |
| Linux (ARM64) | manylinux wheel | glibc 2.17+ |
| macOS (x86_64) | wheel | macOS 10.12+ |
| macOS (ARM64) | wheel | macOS 11.0+ |
| Windows | wheel | Limited features |

## Breaking Changes

### 0.1.0-alpha

- Initial release
- Format version 1

## Upgrading

### From Earlier Alpha Versions

Re-pack snapshots with new version:

```bash
# Extract old snapshot
strata data info old.st  # Check format version

# If format incompatible, repack
strata data pack --disk old.st --output new.st
```

### Python Package

```bash
pip install --upgrade strata
```

### CLI

```bash
cd strata
git pull
make rust
```

## Testing Matrix

Strata is tested on:

- Linux: Ubuntu 20.04, 22.04, Fedora 38, Arch Linux
- macOS: 12 (Monterey), 13 (Ventura), 14 (Sonoma)
- Python: 3.8, 3.9, 3.10, 3.11, 3.12
- PyTorch: 1.12, 1.13, 2.0, 2.1

## Known Issues

### Windows

- FUSE filesystem mounting not supported (no FUSE implementation)
- VM boot features not available (no KVM equivalent)
- CLI data packing works
- Python reading/writing works

### macOS

- macFUSE installation required for mount features
- VM boot requires QEMU (no native hypervisor integration)

### ARM64 Linux

- KVM acceleration supported on ARMv8
- Tested on AWS Graviton instances

## Reporting Compatibility Issues

If you encounter compatibility issues:

1. Check this document for known limitations
2. Verify versions: `strata --version`, `python --version`
3. Open issue with:
   - Strata version
   - Python version
   - Operating system and version
   - CPU architecture
   - Error message

## Future Compatibility

Planned support:

- **Python 3.13**: When released
- **PyTorch 2.2+**: Ongoing compatibility
- **Windows FUSE**: Investigating WinFsp integration
- **Format v2**: Backward-compatible reader

## See Also

- [How-To: Install Strata](../how-to/cli-usage/install-strata.md)
- [Reference: Python API](python-api.md)
- [Reference: CLI Commands](cli-reference.md)
