# Zero-Copy I/O Architecture

How Hexz achieves zero-copy data transfer to NumPy and PyTorch.

## The Problem: Memory Copies

Traditional Python I/O involves multiple copies:

```
Disk → Kernel buffer → Python bytes → NumPy array → PyTorch tensor
      (copy 1)         (copy 2)       (copy 3)
```

Each copy:
- Takes CPU time
- Uses memory bandwidth
- Adds latency

For I/O-bound ML training workloads, reducing copies can reduce memory bandwidth and latency.

## Zero-Copy Approach

Hexz minimizes copies using:

1. **Memory mapping** (local files)
2. **Buffer protocol** (Python to NumPy)
3. **Direct writes** (Rust to Python buffers)

### Memory Mapping (mmap)

For local files, Hexz uses memory-mapped I/O:

```
File on disk ← Mapped to virtual memory ← Read directly
```

**Benefits**:
- No explicit read() syscall
- OS handles paging
- Multiple processes can share same mapping
- Zero copy from disk to process

**Limitation**: Only works for local files, not S3/HTTP

### Buffer Protocol

Python's buffer protocol allows direct memory access:

```python
import numpy as np
buffer = np.zeros(4096, dtype=np.uint8)
reader.read(buffer=buffer)  # Write directly into buffer
```

**What happens**:
1. NumPy allocates memory
2. Hexz gets pointer to NumPy memory
3. Rust reads/decompresses directly into NumPy memory
4. No intermediate Python bytes object

**Result**: One less copy

### GIL Release

Python's Global Interpreter Lock (GIL) prevents parallelism.

Hexz releases GIL during I/O:

```rust
fn read_range(&self, py: Python, offset: u64, length: usize) -> PyResult<Vec<u8>> {
    py.allow_threads(|| {
        // Blocking I/O with GIL released
        // Other Python threads can run
        self.engine.read_range(offset, length)
    })
}
```

**Benefit**: Multi-worker DataLoader achieves true parallelism

## Implementation Details

### Local File Path

```
Disk file
    ↓
mmap() → Virtual memory mapping
    ↓
Decompress into target buffer (if compressed)
    ↓
Return slice of decompressed data
```

**Zero copies**: mmap is zero-copy, decompression writes directly to destination

### S3/HTTP Path

```
S3 range request
    ↓
Compressed block → Buffer in Rust
    ↓
Decompress → Target buffer (Python/NumPy)
```

**One copy**: Network to Rust buffer (unavoidable), then decompress to destination

### To NumPy Array

```python
# Traditional (2 copies)
data = reader.read(4096)  # Copy 1: Rust → Python bytes
array = np.frombuffer(data, dtype=np.uint8)  # Copy 2: bytes → NumPy

# Zero-copy (0 copies)
array = np.zeros(4096, dtype=np.uint8)
reader.read(buffer=array)  # Direct write to NumPy memory
```

### To PyTorch Tensor

```python
import torch
import numpy as np

# Create NumPy array (backed by tensor memory)
tensor = torch.zeros(4096, dtype=torch.uint8)
array = tensor.numpy()  # Zero-copy view

# Read directly into tensor memory
reader.read(buffer=array)  # Writes to tensor

# tensor now contains data, no copies
```

## Performance Impact

> **Not yet benchmarked end-to-end.** The analysis below is based on the implementation, not measured numbers.

**Traditional approach** — multiple copies:
- Disk → Rust buffer → Python `bytes` → NumPy array

**With buffer protocol** — fewer copies:
- Disk → decompress directly into caller-provided NumPy buffer

The practical speedup depends on workload characteristics (block size, cache hit rate, compression ratio) and has not yet been measured in a full Python DataLoader loop.

## Limitations

### S3/HTTP Not Truly Zero-Copy

Network data must be received into a buffer (copy unavoidable).

Mitigation: Decompress directly to target buffer (one copy, not two).

### Alignment Requirements

Some operations require aligned memory. NumPy/PyTorch handle this, but custom buffers may need care.

### Buffer Lifetime

Zero-copy requires buffer to outlive the read operation. Hexz doesn't hold references, so this is user's responsibility.

## Best Practices

### Use Buffer Protocol

```python
# Good: zero-copy
buffer = np.zeros(size, dtype=np.uint8)
reader.read(buffer=buffer)

# Avoid: creates intermediate bytes
data = reader.read(size)
buffer = np.frombuffer(data, dtype=np.uint8)
```

### Pre-allocate Buffers

```python
# Allocate once per epoch
batch_buffer = np.zeros((batch_size, channels, height, width), dtype=np.uint8)

for batch_idx in range(num_batches):
    # Reuse buffer
    reader.read(buffer=batch_buffer.ravel())
    # Process batch_buffer
```

### Let PyTorch Own Memory

```python
# Tensor owns memory, NumPy is view
tensor = torch.zeros(size, dtype=torch.uint8)
buffer = tensor.numpy()  # Zero-copy view

# Read into tensor memory
reader.read(buffer=buffer)

# Use tensor directly
model(tensor)
```

## See Also

- [ADR-0005: PyO3 for Python Bindings](../adr/0005-pyo3-python-bindings.md) - Architecture decision
- [How-To: Optimize PyTorch DataLoader](../how-to/ml-workflows/optimize-pytorch-dataloader.md) - Performance tuning
- [Reference: Python API](../reference/python-api.md) - Buffer protocol usage
