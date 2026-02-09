# Strata Python API - Implementation Status

## Overview

This document tracks the implementation status of the clean, modern Python API for Strata.
All legacy code has been removed in favor of a pythonic, modular design.

## File Structure

```
strata/
├── __init__.py          ✅ Complete - Clean public API, no legacy
├── _core.pyi            ✅ Complete - Type stubs for Rust bindings
├── typing.py            ✅ Complete - All type aliases (BuildProfile, PackingMode, etc.)
├── exceptions.py        ✅ Complete - Full exception hierarchy
├── reader.py            ✅ Complete - Reader + AsyncReader with metadata, streaming
├── writer.py            ✅ 80% - Packing modes working, needs Rust support for:
│                             - Direct byte writing (currently uses temp files)
│                             - Metadata persistence (currently in-memory only)
│                             - Encryption (parameters accepted but not used)
├── array.py             ✅ Complete - NumPy integration with zero-copy
├── dataset.py           📝 Templates - PyTorch/TensorFlow dataset classes (TODO)
├── mount.py             ✅ Complete - FUSE mounting utilities
├── utils.py             ✅ Complete - inspect, analyze, diff, verify, info
└── profiles.py          📝 Templates - Build profiles (ml, eda, embedded, etc.)
```

## Module Status

### ✅ Complete Modules

#### `__init__.py`
- Clean public API with no legacy imports
- Exports: Reader, Writer, Dataset, mount, build, etc.
- Smart `open()` function for read/write modes

#### `typing.py`
- `PathLike` - Path type alias
- `Shape` - Array shape type
- `PackingMode` - "fast", "balanced", "tight"
- `BuildProfile` - "ml", "eda", "embedded", "generic", "archival"
- `DeduplicationMode` - "dcam", "full", "none"
- `CompressionAlgorithm` - "lz4", "zstd", "none"
- Protocol types for buffers and callbacks

#### `exceptions.py`
- `StrataError` - Base exception
- `IOError` - I/O errors (inherits from StrataError + OSError)
- `NetworkError` - S3/HTTP errors
- `FormatError` - Invalid file format
- `ValidationError` - Parameter validation
- `CompressionError` - Compression/decompression
- `EncryptionError` - Crypto errors
- `MountError` - Mount/unmount errors
- `CacheError` - Cache operations
- `VersionError` - Version incompatibility

#### `reader.py`
- `Reader` class:
  - ✅ File-like interface (read, seek, tell)
  - ✅ Random access (read_at, read_range)
  - ✅ Slice notation: `reader[0:1024]`
  - ✅ Metadata property
  - ✅ Streaming iteration: `iter_chunks()`
  - ✅ Context manager support
  - ✅ Pickle support
- `AsyncReader` class:
  - ✅ Async read operations
  - ✅ Async context manager
  - ⚠️  Needs: size() method, iter_chunks()

#### `array.py`
- `read_array()` - Read NumPy arrays with zero-copy option
- `write_array()` - Write NumPy arrays
- `ArrayView` - memmap-like lazy loading with slice notation

#### `mount.py`
- `MountPoint` - Context manager for mounting
- `mount()` - Function to create mount
- `unmount()` - Unmount function

#### `utils.py`
- `inspect()` - Returns Metadata object
- `analyze()` - Returns AnalysisReport with dedup stats
- `diff()` - Compare snapshots
- `verify()` - Verify signatures
- `info()` - Pretty-print metadata
- `Metadata` - Class with property access
- `AnalysisReport` - Class with dedup statistics

### 🔨 Partial Implementation

#### `writer.py` (80% Complete)
**Working:**
- Packing modes: "fast", "balanced", "tight"
- Compression algorithm selection
- Compression level mapping
- `add_file()` - Add files to snapshot
- `add_bytes()` - Add bytes (via temp file workaround)
- `add_array()` - Add NumPy arrays (via temp file)
- `add_metadata()` - Add metadata (in-memory only)
- Context manager with auto-finalization

**TODO (Needs Rust Changes):**
- Direct byte writing without temp files
- Metadata block persistence to file
- Dedup mode selection (DCAM vs full)
- Encryption implementation
- Progress callbacks

### 📝 Template/TODO Modules

#### `dataset.py` (Templates Created)
**Classes Defined:**
- `LRUCache` - TODO: Implement LRU eviction
  - `get()` - Lookup with access tracking
  - `put()` - Insert with eviction
  - `stats()` - Cache statistics

- `Prefetcher` - TODO: Implement background prefetching
  - Thread pool for prefetching
  - `hint()` - Prefetch hint
  - `get()` - Get prefetched data

- `Dataset` - TODO: Implement PyTorch dataset
  - Fixed and variable-length item support
  - Caching and prefetching integration
  - Shuffling with reproducible seeds
  - DDP-compatible epoch shuffling
  - Transform support
  - Zero-copy option

- `TFDataset` - TODO: Implement TensorFlow wrapper
  - `as_dataset()` - Convert to tf.data.Dataset

**Priority:** High - Critical for ML use cases

#### `profiles.py` (Templates Created)
**Defined:**
- `PROFILES` dict with configurations:
  - `ml` - Fast writes, sequential reads, LZ4, DCAM
  - `eda` - Balanced, good for notebooks
  - `embedded` - Max compression, small blocks
  - `generic` - Balanced defaults
  - `archival` - Ultra compression, full dedup

- `build()` function - TODO: Implement
  - Takes profile name
  - Applies overrides
  - Builds snapshot with Writer
  - Returns Metadata

**Priority:** Medium - Nice UX improvement

## API Usage Examples

### Reading (Complete ✅)
```python
import strata

# Modern API
with strata.open("data.st") as reader:
    # Property access
    size = reader.size
    meta = reader.metadata

    # Slice notation
    chunk = reader[0:1024]

    # Streaming
    for chunk in reader.iter_chunks(1024*1024):
        process(chunk)

# Array reading
import numpy as np
array = strata.read_array("data.st", offset=0, shape=(1000, 784), dtype='float32')

# Lazy loading
with strata.ArrayView("data.st", shape=(10000, 784)) as view:
    batch = view[0:100]  # Only loads 100 rows
```

### Writing (80% Complete ⚠️)
```python
import strata

# Packing modes
with strata.Writer("out.st", packing="tight", compression="zstd") as w:
    w.add_file("large_file.img")

# With overrides
with strata.Writer(
    "out.st",
    packing="balanced",
    dedup_mode="full",  # TODO: Wire to Rust
    encrypt=True,       # TODO: Implement
    password="secret"
) as w:
    w.add_file("data.img")
    w.add_metadata({"created": "2026-02-09"})  # TODO: Persist to file
```

### Build Profiles (Templates Only 📝)
```python
import strata

# TODO: Implement build() function
meta = strata.build("data/", "dataset.st", profile="ml")
print(f"Compressed to {meta.size_compressed / 1e9:.1f} GB")
```

### ML Datasets (Templates Only 📝)
```python
import strata
import torch

# TODO: Implement Dataset class
dataset = strata.Dataset(
    "imagenet.st",
    item_size=150528,
    cache_size_mb=2048,
    prefetch_factor=4,
    shuffle=True,
    seed=42,
)

loader = torch.utils.data.DataLoader(dataset, batch_size=32)

for epoch in range(10):
    dataset.set_epoch(epoch)  # DDP-compatible shuffling
    for batch in loader:
        train(batch)
```

## Code Quality Assessment

### ✅ Strengths (Already Pythonic & Clean)
1. **Modern Python patterns:**
   - Context managers (`with` statements)
   - Properties instead of getters/setters
   - Type hints throughout
   - Slice notation support
   - Async/await support

2. **Good separation of concerns:**
   - reader.py - Reading only
   - writer.py - Writing only
   - array.py - NumPy integration
   - dataset.py - ML integration
   - utils.py - Utilities

3. **Clean API surface:**
   - `strata.open()` - Smart dispatcher
   - `Reader[0:1024]` - Pythonic slicing
   - Fluent API: `writer.add(x).add(y).finalize()`

4. **Comprehensive exceptions:**
   - Clear hierarchy
   - Meaningful error messages
   - Proper inheritance (StrataError + OSError for IOError)

### ⚠️ Areas for Improvement

#### 1. Remove Runtime Warnings
**Current (writer.py:238):**
```python
warnings.warn(
    "Metadata storage is not yet implemented. "
    "Metadata will not be persisted to the snapshot file.",
    UserWarning,
)
```

**Problem:** Users don't like runtime warnings for missing features
**Fix:** Either implement or document clearly in docstrings

**Recommendation:**
```python
def add_metadata(self, metadata: Dict[str, Any]) -> "Writer":
    """Add custom metadata to the snapshot.

    Note:
        Metadata persistence is not yet implemented in v0.2.0.
        This method stores metadata in-memory but it will not be
        saved to the .st file. Coming in v0.3.0.
    """
    if not hasattr(self, "_metadata"):
        self._metadata = {}
    self._metadata.update(metadata)
    return self
```

#### 2. Error Codes for Programmatic Handling
**Add to exceptions.py:**
```python
class StrataError(Exception):
    """Base exception with error codes."""
    code: str = "UNKNOWN"

    def __init__(self, message: str, code: Optional[str] = None):
        super().__init__(message)
        if code:
            self.code = code

class VersionError(StrataError):
    code = "VERSION_MISMATCH"

class FormatError(StrataError):
    code = "INVALID_FORMAT"
```

**Usage:**
```python
try:
    reader = strata.open("file.st")
except strata.StrataError as e:
    if e.code == "VERSION_MISMATCH":
        # Handle version mismatch
    elif e.code == "INVALID_FORMAT":
        # Handle format error
```

#### 3. Input Validation Helpers
**Add to writer.py:**
```python
def _validate_packing_mode(mode: str) -> PackingMode:
    """Validate packing mode at runtime."""
    if mode not in get_args(PackingMode):
        valid = ", ".join(get_args(PackingMode))
        raise ValidationError(
            f"Invalid packing mode: {mode!r}. "
            f"Valid modes: {valid}"
        )
    return mode  # type: ignore
```

### 🔧 Scalability Issues & Fixes

#### Issue 1: Master Index Memory Usage
**Current State:**
- Full index loaded into RAM on file open
- For large files: 10TB / 64KB blocks = 160M blocks
- Each block: ~32 bytes metadata
- Total: **~5GB RAM** for 10TB file

**Impact:** Limits maximum file size

**Solution 1: Lazy Index Loading (2-3 hours)**
```rust
// crates/core/src/format/index/mod.rs

pub struct LazyMasterIndex {
    // Only load metadata, not all blocks
    disk_size: u64,
    memory_size: u64,
    page_offsets: Vec<u64>,  // Offsets to index pages
    page_cache: LruCache<usize, IndexPage>,  // LRU cache
    backend: Arc<dyn StorageBackend>,
}

impl LazyMasterIndex {
    pub fn load_page(&mut self, page_idx: usize) -> Result<&IndexPage> {
        if let Some(page) = self.page_cache.get(&page_idx) {
            return Ok(page);  // Cache hit
        }

        // Cache miss - load from disk
        let offset = self.page_offsets[page_idx];
        let page_data = self.backend.read(offset, PAGE_SIZE)?;
        let page: IndexPage = bincode::deserialize(&page_data)?;

        self.page_cache.put(page_idx, page);
        Ok(self.page_cache.get(&page_idx).unwrap())
    }
}
```

**Benefits:**
- Memory usage: ~1-10MB (only cached pages)
- Slight latency increase on random access
- Scales to 100TB+ files

**Implementation:** Medium priority (after Dataset)

#### Issue 2: No Block-Level Checksums
**Current:** Can only verify entire file signature
**Problem:** Can't detect partial corruption

**Solution: Add CRC32 per block (1-2 hours)**
```rust
// crates/core/src/format/index/mod.rs

pub struct BlockInfo {
    physical_offset: u64,
    logical_offset: u64,
    compressed_size: u32,
    uncompressed_size: u32,
    crc32: Option<u32>,  // NEW - checksum
}
```

**Enable with feature flag:**
```rust
pub struct FeatureFlags {
    // ...
    pub has_checksums: bool,  // NEW
}
```

**Usage:**
```python
# Verify block integrity
reader = strata.open("file.st")
if reader.metadata.features.has_checksums:
    reader.verify_block(block_idx=1000)
```

#### Issue 3: Single-threaded Compression
**Current:** Blocks compressed sequentially
**Speedup Potential:** 4-8x on multi-core systems

**Solution: Parallel Compression (2-3 hours)**
```rust
// crates/loader/src/py_interface/builder.rs
use rayon::prelude::*;

impl StrataBuilder {
    pub fn process_stream_parallel(&mut self, path: String) -> PyResult<()> {
        // Read blocks in chunks
        let blocks = read_blocks_from_file(path, self.block_size)?;

        // Compress in parallel
        let compressed: Vec<_> = blocks
            .par_iter()
            .map(|block| {
                let compressor = self.create_compressor();
                compressor.compress(block)
            })
            .collect();

        // Write sequentially (maintains order)
        for comp_block in compressed {
            self.write_block(comp_block)?;
        }

        Ok(())
    }
}
```

**Trade-off:**
- ✅ 4-8x faster compression
- ⚠️ Higher memory usage (parallel buffers)
- ⚠️ Slightly worse compression (no cross-block patterns)

**Recommendation:** Add as optional flag for large files

#### Issue 4: No Streaming Write API
**Current:** Must finalize() before reading
**Problem:** Can't stream-write and stream-read simultaneously

**Use case:** Real-time video capture
```python
# Would be nice to have:
with strata.Writer("video.st", streaming=True) as w:
    reader = strata.Reader("video.st", streaming=True)

    for frame in camera.capture():
        w.write(frame)
        # Reader can read already-written blocks
        latest = reader.read(w.tell() - FRAME_SIZE, FRAME_SIZE)
```

**Implementation:** Complex (requires partial index writes)
**Priority:** Low (niche use case)

---

## Implementation Priorities (Updated)

### 🚨 CRITICAL: Version Checking (1-2 hours)
**Status:** ⚠️ Will break on any format version change
**Effort:** 1-2 hours
**Impact:** Future-proof the format

**See FORMAT_VERSIONING_ANALYSIS.md for detailed implementation**

Files to modify:
1. `crates/core/src/format/version.rs` (NEW)
2. `crates/core/src/api/stratafile.rs`
3. `crates/loader/python/strata/utils.py`

---

### Priority 1: Dataset Module (6-8 hours)
**Status:** 📝 Templates exist, logic missing
**Effort:** 6-8 hours
**Impact:** 🔥 HIGH - Unlocks ML training use cases

#### Task 1.1: LRUCache Implementation (2 hours)
**File:** `crates/loader/python/strata/dataset.py`

**Implementation:**
```python
from collections import OrderedDict
from typing import Optional, Dict, List

class LRUCache:
    """Least-Recently-Used cache with byte-size tracking."""

    def __init__(self, max_size_mb: int):
        self.max_size_bytes = max_size_mb * 1024 * 1024
        self.current_size = 0
        self.cache: OrderedDict[int, bytes] = OrderedDict()
        self.hits = 0
        self.misses = 0

    def get(self, key: int) -> Optional[bytes]:
        """Get item and move to end (most recent)."""
        if key not in self.cache:
            self.misses += 1
            return None

        # Move to end (most recently used)
        self.cache.move_to_end(key)
        self.hits += 1
        return self.cache[key]

    def put(self, key: int, value: bytes) -> None:
        """Add item with LRU eviction."""
        # Remove old entry if exists
        if key in self.cache:
            old_value = self.cache.pop(key)
            self.current_size -= len(old_value)

        # Add new entry
        self.cache[key] = value
        self.current_size += len(value)

        # Evict until under limit
        while self.current_size > self.max_size_bytes and self.cache:
            # Remove least recently used (first item)
            oldest_key, oldest_value = self.cache.popitem(last=False)
            self.current_size -= len(oldest_value)
```

**Testing:**
```python
def test_lru_cache():
    cache = LRUCache(max_size_mb=1)  # 1MB

    # Add 2MB of data
    cache.put(0, b"x" * 600_000)  # 600KB
    cache.put(1, b"y" * 600_000)  # 600KB - evicts key 0

    assert cache.get(0) is None  # Evicted
    assert cache.get(1) is not None  # Still there
    assert cache.current_size <= 1_048_576
```

#### Task 1.2: Basic Dataset (3 hours)
**File:** `crates/loader/python/strata/dataset.py`

**Focus on core functionality:**
1. Fixed-size items (skip index file for now)
2. Caching with LRU
3. No prefetching yet (add later)
4. Basic shuffling

**Implementation:**
```python
def __getitem__(self, idx: int):
    """Get item at index with caching."""
    # Apply shuffle mapping
    if self._indices is not None:
        idx = self._indices[idx]

    # Check cache
    if self._cache:
        cached = self._cache.get(idx)
        if cached is not None:
            return self._decode_item(cached)

    # Calculate offset (fixed-size items only for now)
    offset = idx * self._item_size
    size = self._item_size

    # Read from file
    data = self._reader.read_at(offset, size)

    # Cache it
    if self._cache:
        self._cache.put(idx, data)

    return self._decode_item(data)
```

**Testing with PyTorch:**
```python
def test_dataset_pytorch():
    import torch
    from torch.utils.data import DataLoader

    # Create test file
    with strata.Writer("test.st") as w:
        for i in range(100):
            w.add_bytes(bytes([i] * 1024))  # 100 items, 1KB each

    # Load as dataset
    dataset = strata.Dataset(
        "test.st",
        item_size=1024,
        cache_size_mb=1,
        output_format="tensor",
    )

    loader = DataLoader(dataset, batch_size=4, num_workers=0)

    for batch in loader:
        assert batch.shape == (4, 1024)
        break  # Test first batch
```

#### Task 1.3: Prefetcher (3 hours)
**File:** `crates/loader/python/strata/dataset.py`

**Implementation:**
```python
from concurrent.futures import ThreadPoolExecutor
from threading import Lock
from queue import Queue

class Prefetcher:
    """Background prefetcher using thread pool."""

    def __init__(self, reader: Reader, prefetch_factor: int, num_workers: int):
        self.reader = reader
        self.prefetch_factor = prefetch_factor
        self.executor = ThreadPoolExecutor(max_workers=num_workers)
        self.prefetched: Dict[int, bytes] = {}
        self.lock = Lock()
        self.active = True

    def hint(self, index: int, offset: int, size: int) -> None:
        """Hint that index will be accessed soon."""
        if not self.active:
            return

        # Submit prefetch job
        future = self.executor.submit(self._prefetch_worker, index, offset, size)

    def _prefetch_worker(self, index: int, offset: int, size: int):
        """Worker function to prefetch data."""
        try:
            data = self.reader.read_at(offset, size)
            with self.lock:
                self.prefetched[index] = data
        except Exception as e:
            pass  # Ignore prefetch errors

    def get(self, index: int) -> Optional[bytes]:
        """Get prefetched item if available."""
        with self.lock:
            return self.prefetched.pop(index, None)

    def shutdown(self):
        """Shutdown prefetcher."""
        self.active = False
        self.executor.shutdown(wait=True)
```

**Testing:**
```python
def test_prefetcher():
    reader = strata.open("test.st")
    prefetcher = Prefetcher(reader, prefetch_factor=4, num_workers=2)

    # Hint next items
    for i in range(10):
        prefetcher.hint(i, i * 1024, 1024)

    time.sleep(0.1)  # Let prefetch happen

    # Should be prefetched
    assert prefetcher.get(5) is not None
```

#### Task 1.4: Index File Support (Optional - 2 hours)
For variable-length items.

**Format:**
```
[uint64 offset][uint64 size]  // Item 0
[uint64 offset][uint64 size]  // Item 1
...
```

**Implementation:** After basic Dataset works

---

### Priority 2: Build Profiles (2-3 hours)
**Status:** 📝 Function defined, directory walking missing
**Effort:** 2-3 hours
**Impact:** 🟢 MEDIUM - Great UX improvement

**See details in FORMAT_VERSIONING_ANALYSIS.md**

---

### Priority 3: Rust API Improvements (10-15 hours)
**Status:** ⚠️ Workarounds exist in Python
**Effort:** 10-15 hours
**Impact:** 🟡 MEDIUM - Removes technical debt

**Tasks:**
1. Direct byte writing (3-4 hours) - writer.py:163
2. Metadata persistence (4-6 hours) - writer.py:231
3. Dedup mode selection (2 hours)
4. Progress callbacks (2 hours)

**See details in FORMAT_VERSIONING_ANALYSIS.md**

## Testing Status

### Manual Testing
- ✅ Reader with metadata
- ✅ Reader with streaming
- ✅ Reader with slice notation
- ✅ Writer with packing modes
- ✅ Array reading/writing
- ✅ Mount/unmount

### Unit Tests
- ❌ No pytest suite yet
- **TODO:** Create comprehensive test suite

### Integration Tests
- ❌ No end-to-end tests yet
- **TODO:** Test full workflows

## Next Steps

1. **Implement Dataset module** (Priority 1)
   - Start with LRUCache implementation
   - Add threading for prefetcher
   - Wire up to PyTorch

2. **Implement build() function** (Priority 2)
   - Simple, high-impact feature
   - Great for demos and examples

3. **Create test suite** (Quality)
   - Unit tests for all modules
   - Integration tests for workflows
   - Performance benchmarks

4. **Rust improvements** (After Python layer stable)
   - Direct byte writing
   - Metadata persistence
   - Encryption

5. **Documentation** (Before 1.0)
   - User guide
   - API reference
   - Migration guide
   - Examples

## Removed Legacy Code

The following files were removed for a clean implementation:
- ~~`builder.py`~~ - Old SnapshotBuilder wrapper
- ~~`io.py`~~ - Legacy open() function
- ~~`torch.py`~~ - Old PyTorch integration

All functionality is now in the modern API with better names and patterns.

---

## Quick Reference: What to Do After Class

### ⚡ Immediate (1-2 hours) - CRITICAL
**Fix version checking to prevent future breakage**

Files to create/modify:
1. `crates/core/src/format/version.rs` (NEW)
2. `crates/core/src/api/stratafile.rs` (modify version check)
3. `crates/loader/python/strata/utils.py` (add version info)

See FORMAT_VERSIONING_ANALYSIS.md section "CRITICAL: Version Checking"

---

### 🔥 High Priority (6-8 hours) - HIGH VALUE
**Implement Dataset module for ML training**

**Phase 1: LRUCache (2 hours)**
- File: `crates/loader/python/strata/dataset.py:24-101`
- Replace `raise NotImplementedError` with OrderedDict implementation
- Test: Cache hit/miss, eviction, size tracking

**Phase 2: Basic Dataset (3 hours)**
- File: `crates/loader/python/strata/dataset.py:329-378`
- Implement `__getitem__()` with caching
- Implement `_create_indices()` for shuffling
- Test: PyTorch DataLoader integration

**Phase 3: Prefetcher (3 hours)**
- File: `crates/loader/python/strata/dataset.py:103-168`
- ThreadPoolExecutor implementation
- Background prefetching with hints
- Test: Prefetch accuracy

**Success Criteria:**
```python
dataset = strata.Dataset("data.st", item_size=1024, cache_size_mb=512)
loader = torch.utils.data.DataLoader(dataset, batch_size=32)
for batch in loader:
    train(batch)  # Should work!
```

---

### 🟢 Medium Priority (2-3 hours) - QUICK WIN
**Implement build() with directory walking**

File: `crates/loader/python/strata/profiles.py:60-107`
- Add `os.walk()` for directory recursion
- Handle both files and directories
- Apply profile configurations

**Test:**
```python
meta = strata.build("dataset/", "output.st", profile="ml")
print(f"Created {meta.size_compressed / 1e9:.1f} GB snapshot")
```

---

### 🔨 Long-term (10-15 hours) - REMOVES WORKAROUNDS
**Rust API improvements**

1. **Direct byte writing** (3-4 hours)
   - File: `crates/loader/src/py_interface/builder.rs`
   - Add `add_bytes()` method
   - Removes temp file workaround in writer.py:163

2. **Metadata persistence** (4-6 hours)
   - Add metadata block to format
   - Serialize to JSON/MessagePack
   - Write to dedicated section
   - Update header

3. **Parallel compression** (2-3 hours)
   - Use rayon for parallel block compression
   - 4-8x speedup on multi-core systems

4. **Lazy index loading** (2-3 hours)
   - LRU cache for index pages
   - Reduces memory usage for large files
   - Scales to 100TB+ files

---

## Success Metrics

### After Version Checking:
- ✅ Files with version 2+ load with warning (graceful degradation)
- ✅ Files with version 0 rejected with clear error
- ✅ `strata.inspect("file.st").is_compatible` works

### After Dataset Implementation:
- ✅ Can create PyTorch DataLoader from .st files
- ✅ Cache hit rate > 80% for sequential access
- ✅ Prefetcher loads next items in background
- ✅ Shuffling works with reproducible seeds

### After build() Implementation:
- ✅ `strata.build("dir/", "out.st", profile="ml")` works
- ✅ Recursively walks directories
- ✅ Applies profile configurations correctly

### After Rust Improvements:
- ✅ No temp files created during writing
- ✅ Metadata persisted to .st files
- ✅ 4-8x faster compression on multi-core systems
- ✅ Large files (10TB+) load without OOM

---

## Testing Checklist

### Unit Tests (TODO)
```bash
cd crates/loader/python
pytest tests/test_dataset.py -v
pytest tests/test_writer.py -v
pytest tests/test_profiles.py -v
```

### Integration Tests (TODO)
```python
# Test full workflow
strata.build("data/", "dataset.st", profile="ml", mode="fast")
dataset = strata.Dataset("dataset.st", item_size=1024, cache_size_mb=512)
loader = torch.utils.data.DataLoader(dataset, batch_size=32, num_workers=4)

for epoch in range(3):
    dataset.set_epoch(epoch)
    for batch in loader:
        assert batch.shape[0] <= 32
```

### Performance Benchmarks (TODO)
- Packing speed: > 500 MB/s (balanced mode)
- Reading speed: > 1 GB/s (cached)
- Cache hit rate: > 80% (sequential)
- Memory usage: < 100 MB (10GB file)

---

## Architecture Decisions

### Why LRU Cache?
- Simple, predictable eviction policy
- Good for sequential and random access
- O(1) lookup and insertion with OrderedDict

### Why ThreadPoolExecutor for Prefetching?
- Simple, built-in Python solution
- No external dependencies
- Good enough for 2-4 workers
- Alternative: asyncio (more complex, similar perf)

### Why Fixed-size Items First?
- 80% of ML datasets use fixed-size items
- Much simpler implementation
- Variable-length can be added later via index files

### Why Lazy Index Loading?
- Scalability bottleneck for large files
- Minimal latency impact (page cache)
- Enables 100TB+ files

### Why Parallel Compression Optional?
- Trade-off: speed vs compression ratio
- Not always beneficial (small files, I/O bound)
- Good for large files on multi-core systems

---

## Known Limitations

### Current (v0.2.0)
1. **Metadata not persisted** - In-memory only (writer.py:231)
2. **No index file support** - Variable-length items need workaround
3. **Single-threaded compression** - Slow for large files
4. **Full index in memory** - Limits max file size to ~10TB
5. **No block checksums** - Can't detect partial corruption

### Planned (v0.3.0)
1. ✅ Metadata persistence
2. ✅ Index file support for variable-length items
3. ✅ Optional parallel compression
4. ✅ Lazy index loading
5. ✅ Block-level CRC32 checksums

### Future (v1.0.0)
1. Streaming write API
2. Multi-level index (directory + leaf pages)
3. Brotli compression
4. ChaCha20-Poly1305 encryption
5. Incremental snapshots (rsync-like)

---

**Last Updated:** 2026-02-09
**Next Review:** After implementing Dataset module
