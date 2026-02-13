# Feature Gates and Modular Builds

Strata is designed with modularity in mind, allowing you to install only the features you need. This reduces binary size, minimizes dependencies, and improves security by reducing attack surface.

## Available Features

### Rust Crate Features

#### strata-core

| Feature | Default | Description | Dependencies |
|---------|---------|-------------|--------------|
| `s3` | ✅ | AWS S3 storage backend | rust-s3, reqwest |
| `compression-zstd` | ✅ | Zstandard compression | zstd |
| `encryption` | ✅ | AES-GCM encryption | aes-gcm, pbkdf2, hmac |

**Note**: LZ4 compression is always available (no feature flag needed).

#### strata-common

| Feature | Default | Description | Dependencies |
|---------|---------|-------------|--------------|
| `signing` | ❌ | Ed25519 cryptographic signing | ed25519-dalek, sha2 |

#### strata-loader (Python bindings)

| Feature | Default | Description | Dependencies |
|---------|---------|-------------|--------------|
| `s3` | ✅ | S3 storage backend | strata-core/s3 |
| `compression-zstd` | ✅ | Zstandard compression | strata-core/compression-zstd |
| `encryption` | ❌ | AES-GCM encryption | strata-core/encryption |
| `signing` | ✅ | Ed25519 signing | strata-common/signing |
| `full` | ❌ | All features enabled | All of the above |

### Python Package Extras

| Extra | Includes | Use Case |
|-------|----------|----------|
| `[torch]` | PyTorch ≥2.0 | ML training with PyTorch DataLoader |
| `[tensorflow]` | TensorFlow ≥2.13 | ML training with TensorFlow Dataset |
| `[numpy]` | NumPy ≥1.20 | Scientific computing, array operations |
| `[ml]` | NumPy + PyTorch | Common ML stack |
| `[full]` | All ML frameworks | Everything for ML workflows |
| `[dev]` | Testing + linting tools | Development and contribution |

## Building with Custom Features

### Using Makefile

The Makefile supports a `FEATURES` parameter for build/develop targets:

```bash
# Default features (s3, zstd, signing)
make develop

# All features
make develop FEATURES=full

# Minimal build (no default features)
make develop FEATURES=minimal

# Custom feature list
make develop FEATURES="s3 signing"

# Build wheel with specific features
make python FEATURES="s3 compression-zstd encryption"
```

### Using maturin Directly

```bash
# Default features
maturin build --release

# All features
maturin build --release --features full

# Minimal (no defaults)
maturin build --release --no-default-features

# Custom features
maturin build --release --no-default-features --features "s3 signing"
```

### Using cargo

```bash
# Build loader with defaults
cargo build -p strata-loader --release

# Build with all features
cargo build -p strata-loader --release --features full

# Build minimal
cargo build -p strata-loader --release --no-default-features

# Build CLI with custom features
cargo build -p strata --release --no-default-features --features "s3 compression-zstd"
```

## Python Installation

```bash
# Minimal (core features only, ~5MB)
pip install strata

# With PyTorch support
pip install strata[torch]

# With all ML frameworks
pip install strata[full]

# Development installation
pip install strata[dev]
```

## Binary Size Comparison

Binary sizes for the Python loader (release build, stripped):

| Configuration | Size |
|--------------|------|
| Minimal (no default features) | 12MB |
| Default (s3, zstd, signing) | 12MB |
| Full (all features) | 12MB |

**Note**: Binary size is nearly identical across configurations because most size comes from core dependencies (PyO3 runtime, Tokio async runtime, LZ4 compression).

## Why Use Feature Gates?

While Rust feature gates provide minimal binary size savings in strata-loader, they offer other important benefits:

### 1. Python Dependency Management ⭐ (Primary Benefit)
The `[torch]`, `[tensorflow]`, and `[numpy]` extras are **essential**. These packages are hundreds of MB each:
```bash
pip install strata              # 0 Python dependencies
pip install strata[torch]       # Adds ~700MB of PyTorch
pip install strata[ml]          # Adds ~1GB (PyTorch + NumPy)
```

### 2. Dependency Tree Clarity
- Minimal: 467 dependencies
- Default: 485 dependencies
- Difference: 18 fewer crates to audit for security

### 3. Professional Architecture
Feature gates demonstrate:
- Modular design principles
- Thoughtful dependency management
- Enterprise-ready engineering

### 4. Future-Proofing
Infrastructure is ready for heavyweight features like:
- CUDA/GPU acceleration
- WASM builds
- Platform-specific optimizations
- Optional cloud provider SDKs

## CI/CD Testing Matrix

### GitHub Actions Example

```yaml
name: Feature Matrix CI

on: [push, pull_request]

jobs:
  test-features:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        features:
          - "default"
          - "minimal"
          - "full"
          - "s3"
          - "s3,compression-zstd"
          - "s3,signing"

    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Build with ${{ matrix.features }}
        run: |
          if [ "${{ matrix.features }}" = "default" ]; then
            cargo build -p strata-loader --release
          elif [ "${{ matrix.features }}" = "minimal" ]; then
            cargo build -p strata-loader --release --no-default-features
          elif [ "${{ matrix.features }}" = "full" ]; then
            cargo build -p strata-loader --release --features full
          else
            cargo build -p strata-loader --release --no-default-features --features "${{ matrix.features }}"
          fi

      - name: Run tests
        run: |
          if [ "${{ matrix.features }}" = "default" ]; then
            cargo test -p strata-loader
          elif [ "${{ matrix.features }}" = "minimal" ]; then
            cargo test -p strata-loader --no-default-features
          elif [ "${{ matrix.features }}" = "full" ]; then
            cargo test -p strata-loader --features full
          else
            cargo test -p strata-loader --no-default-features --features "${{ matrix.features }}"
          fi

  test-python-extras:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        extra:
          - ""
          - "[torch]"
          - "[numpy]"
          - "[ml]"

    steps:
      - uses: actions/checkout@v4

      - name: Setup Python
        uses: actions/setup-python@v5
        with:
          python-version: '3.10'

      - name: Install strata${{ matrix.extra }}
        run: |
          pip install -e .${{ matrix.extra }}

      - name: Run Python tests
        run: |
          pip install pytest
          pytest crates/loader/tests/

  binary-size-check:
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - name: Build and check sizes
        run: |
          # Minimal
          cargo build -p strata-loader --release --no-default-features
          MINIMAL_SIZE=$(stat -f%z target/release/libstrata_loader.so 2>/dev/null || stat -c%s target/release/libstrata_loader.so)

          # Default
          cargo build -p strata-loader --release
          DEFAULT_SIZE=$(stat -f%z target/release/libstrata_loader.so 2>/dev/null || stat -c%s target/release/libstrata_loader.so)

          # Full
          cargo build -p strata-loader --release --features full
          FULL_SIZE=$(stat -f%z target/release/libstrata_loader.so 2>/dev/null || stat -c%s target/release/libstrata_loader.so)

          echo "Binary sizes:"
          echo "  Minimal: $(($MINIMAL_SIZE / 1024 / 1024))MB"
          echo "  Default: $(($DEFAULT_SIZE / 1024 / 1024))MB"
          echo "  Full: $(($FULL_SIZE / 1024 / 1024))MB"
```

## Use Cases

### Edge Deployments

For IoT or edge devices where binary size matters:

```bash
# Disable S3 and compression-zstd
maturin build --release --no-default-features
```

### Air-Gapped Systems

For secure environments without network access:

```bash
# Build without S3 support
maturin build --release --no-default-features --features "compression-zstd signing"
```

### Size-Constrained Containers

For AWS Lambda or Cloud Run where cold start matters:

```bash
# Minimal build
make python FEATURES=minimal
```

### Full-Featured Development

For development environments:

```bash
# Everything
make develop FEATURES=full
pip install -e .[dev]
```

## Runtime Feature Detection

Python code can detect available features at runtime:

```python
import strata

# Check if crypto signing is available
if hasattr(strata, 'crypto') and strata.crypto is not None:
    try:
        strata.crypto.keygen('/tmp/test.key', '/tmp/test.pub')
        print("Crypto signing available")
    except AttributeError:
        print("Crypto signing not compiled in")
else:
    print("Crypto module not available")

# Check if Dataset is available (requires torch/numpy)
if hasattr(strata, 'Dataset'):
    print("Dataset class available")
else:
    print("Dataset requires: pip install strata[torch]")
```

## Best Practices

1. **Start minimal**: Only enable features you need
2. **Document requirements**: Specify required features in README
3. **Test feature combinations**: CI should test different feature sets
4. **Monitor binary size**: Track size changes in CI
5. **Use extras wisely**: Python extras keep base package small
