# 5. PyO3 for Python Bindings

Date: Early development phase

## Status

Accepted

## Context

Hexz's primary use case is streaming data to PyTorch for ML training. The core engine is written in Rust (see ADR-0001), but Python is the dominant language in ML engineering. We need Python bindings that:

**Performance Requirements**:
- Zero-copy data transfer to NumPy/PyTorch tensors
- Release GIL during blocking I/O (enable true parallelism in DataLoader workers)
- Sub-millisecond overhead per `read()` call
- Support concurrent access from multiple Python threads

**API Requirements**:
- Pythonic interface (context managers, properties, exceptions)
- Type hints for IDE autocomplete
- Integration with `torch.utils.data.Dataset`
- Compatibility with Python 3.8+

**Operational Requirements**:
- Distribute binary wheels (no compilation required for users)
- Cross-platform (Linux, macOS, Windows)
- Minimal dependencies (avoid bloated C++ runtimes)

Alternatives considered:

1. **ctypes**: Pure Python FFI, but clunky API, no memory safety, manual reference counting
2. **cffi**: Better than ctypes, but still manual memory management
3. **pybind11**: Excellent C++ bindings, but requires C++ build system
4. **PyO3**: Rust-native Python bindings with memory safety

The constraint is maintaining Rust's memory safety guarantees while exposing a natural Python API.

## Decision

We will use **PyO3** for Python bindings, packaged with **Maturin** for distribution.

### Architecture

**Pure Rust Engine** (`crates/loader/src/engine/`):
- Core logic independent of Python
- Future-proof for other language bindings (C FFI, WASM)
- Testable without Python runtime

**PyO3 Wrapper** (`crates/loader/src/py_interface/`):
- `#[pyclass]` for Python-facing types
- `#[pyfunction]` for module functions
- Exception mapping from Rust `Result` to Python exceptions

**Python Layer** (`crates/loader/python/hexz/`):
- High-level wrappers for ergonomics
- PyTorch `Dataset` integration
- Type stubs (`.pyi`) for static analysis

### Key Design Patterns

**Zero-Copy Reads**:
```rust
fn read_into_buffer(&self, buffer: &PyAny) -> PyResult<usize> {
    let buf = buffer.extract::<PyBuffer<u8>>()?;
    // Write directly into NumPy array memory
    self.engine.read_range(offset, buf.as_mut_slice())
}
```

**GIL Management**:
```rust
fn read_range(&self, py: Python, offset: u64, length: usize) -> PyResult<Vec<u8>> {
    py.allow_threads(|| {
        // Blocking I/O happens with GIL released
        self.engine.read_range(offset, length)
    })
}
```

**Exception Mapping**:
```rust
impl From<Error> for PyErr {
    fn from(err: Error) -> Self {
        match err {
            Error::Io(e) => PyIOError::new_err(e.to_string()),
            Error::Corruption => PyValueError::new_err("Corrupted snapshot"),
            // ...
        }
    }
}
```

### Distribution Strategy

- **Maturin**: Build tool that creates PEP 517 compliant wheels
- **GitHub Actions**: Build wheels for `manylinux`, macOS (x86/ARM), Windows
- **PyPI Upload**: Users install via `pip install hexz`
- **Source Distribution**: Includes `pyproject.toml` for `pip install -e .`

## Consequences

### Positive

- **Memory Safety**: Rust ownership enforced at Python boundary (no segfaults from misuse)
- **Zero-Copy Performance**: Direct buffer protocol integration (no memcpy for NumPy arrays)
- **True Parallelism**: GIL released during I/O enables multi-worker DataLoader speedup
- **Type Safety**: PyO3 enforces type conversions at compile time
- **Binary Wheels**: Users don't need Rust toolchain, just `pip install`
- **Minimal Dependencies**: Rust statically links, wheel is self-contained
- **Cross-Platform**: Same codebase for Linux/macOS/Windows

### Negative

- **Wheel Size**: Binary wheels are larger than pure Python packages
- **Build Complexity**: Requires Rust toolchain + Maturin for development (documented in Makefile)
- **Debugging**: Stack traces cross Rust/Python boundary (harder to debug than pure Python)
- **PyO3 Version Coupling**: Tied to PyO3's Python version support lifecycle
- **Compile Time**: Rust compilation slower than pure Python edit-run cycle

### Neutral

- **Python Version Support**: 3.8+ (matches PyTorch minimum version)
- **ABI Compatibility**: Wheels target stable ABI (`abi3`) when possible
- **Development Mode**: `maturin develop` builds debug binary for fast iteration
- **Type Stubs**: Generated manually (future: auto-generate with `pyo3-stub-gen`)
- **Error Messages**: Rust panics converted to Python `RuntimeError` with full message

## Integration Examples

**Simple Read**:
```python
import hexz
with hexz.open("dataset.hxz") as reader:
    data = reader.read(4096)  # Returns bytes
```

**Zero-Copy to NumPy**:
```python
import numpy as np
buffer = np.zeros(4096, dtype=np.uint8)
reader.read(buffer=buffer)  # Fills buffer directly
```

**PyTorch Dataset**:
```python
class Dataset(torch.utils.data.Dataset):
    def __init__(self, path):
        self.reader = hexz.open(path)

    def __getitem__(self, idx):
        # GIL released during read
        data = self.reader.read(size, offset=idx*size)
        return torch.from_numpy(np.frombuffer(data))
```

## Related Decisions

- See ADR-0001 for Rust core engine rationale
- See how-to/ml-workflows/optimize-pytorch-dataloader.md for performance tuning
- See reference/python-api.md for complete API documentation
