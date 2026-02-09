# Strata File Format: Versioning & Compatibility Analysis

## Current Implementation

### File Format Version

**Location:** `crates/core/src/format/magic.rs`

```rust
/// Magic bytes identifying a Strata file: "STRT"
pub const MAGIC_BYTES: &[u8; 4] = b"STRT";

/// Current format version
pub const FORMAT_VERSION: u32 = 1;

/// Header size: 4096 bytes
pub const HEADER_SIZE: usize = 4096;
```

**✅ YES - Version numbers ARE stored in file headers**

---

### Header Structure

**Location:** `crates/core/src/format/header.rs`

```rust
pub struct StrataHeader {
    pub magic: [u8; 4],           // "STRT" - file type identifier
    pub version: u32,              // Format version (currently 1)
    pub block_size: u32,           // Block size in bytes
    pub index_offset: u64,         // Location of master index

    // Optional features
    pub parent_path: Option<String>,        // For thin snapshots
    pub dictionary_offset: Option<u64>,     // Zstd dictionary
    pub dictionary_length: Option<u32>,
    pub metadata_offset: Option<u64>,       // Custom metadata
    pub metadata_length: Option<u32>,
    pub signature_offset: Option<u64>,      // Ed25519 signature
    pub signature_length: Option<u32>,
    pub encryption: Option<KeyDerivationParams>,
    pub compression: CompressionType,       // Lz4 or Zstd
    pub features: FeatureFlags,             // Capability flags
}
```

**Key Points:**
1. **Every .st file starts with 4096-byte header**
2. **Version is checked on open** - see `StrataFile::new()` line 154
3. **Header is serialized with serde** (can add fields without breaking compat)
4. **Feature flags advertise capabilities**

---

## Version Checking

**Location:** `crates/core/src/api/stratafile.rs:154`

```rust
impl StrataFile {
    /// Opens a Strata snapshot with default cache settings.
    ///
    /// This is the primary constructor for `StrataFile`. It:
    /// 1. Reads and validates the snapshot header (magic bytes, version)
    /// 2. Deserializes the master index
    /// 3. Recursively loads parent snapshots (for thin snapshots)
    /// 4. Initializes block and page caches
    pub fn new(
        backend: Arc<dyn StorageBackend>,
        compressor: Box<dyn Compressor>,
        encryptor: Option<Box<dyn Encryptor>>,
    ) -> Result<Self> {
        // Reads header, validates magic bytes and version
        // Returns Err(StrataError::Format) if version is invalid
    }
}
```

**Version validation happens on every file open.**

---

## Backward Compatibility Strategy

### Current Approach (Version 1)

**Strengths:**
- ✅ Header contains version number
- ✅ Serde serialization allows adding optional fields
- ✅ Feature flags advertise capabilities
- ✅ Fixed 4096-byte header size allows format evolution

**Weaknesses:**
- ⚠️ No explicit backward/forward compatibility logic yet
- ⚠️ Version check is binary (accept v1, reject everything else)
- ⚠️ No migration code for future version upgrades

### Recommended Improvements

#### 1. **Version Range Checking**

**Current (too strict):**
```rust
if header.version != FORMAT_VERSION {
    return Err(StrataError::Format("Unsupported version"));
}
```

**Recommended (backward compatible):**
```rust
const MIN_SUPPORTED_VERSION: u32 = 1;
const MAX_SUPPORTED_VERSION: u32 = 1;  // Update as new versions are added

if header.version < MIN_SUPPORTED_VERSION {
    return Err(StrataError::Format(
        format!("Version {} is too old (min: {})",
                header.version, MIN_SUPPORTED_VERSION)
    ));
}

if header.version > MAX_SUPPORTED_VERSION {
    // Warning but can try to read
    eprintln!("Warning: File version {} is newer than supported {}. Some features may not work.",
              header.version, MAX_SUPPORTED_VERSION);
}

// Handle version-specific quirks
match header.version {
    1 => open_v1(header, backend),
    2 => open_v2(header, backend),  // Future
    _ => Err(StrataError::Format("Unknown version"))
}
```

#### 2. **Feature Flags for Capabilities**

**Already implemented:**
```rust
pub struct FeatureFlags {
    pub has_disk: bool,
    pub has_memory: bool,
    pub variable_blocks: bool,
}
```

**Recommendation: Expand feature flags**
```rust
pub struct FeatureFlags {
    // V1 flags
    pub has_disk: bool,
    pub has_memory: bool,
    pub variable_blocks: bool,

    // V2 flags (future)
    pub has_compression_v2: bool,  // New compression algorithm
    pub has_checksums: bool,        // Block-level checksums
    pub has_encryption_v2: bool,    // New encryption scheme

    // Reserved for future use
    pub reserved: [u8; 8],
}
```

**With feature flags, OLD readers can:**
- Open NEW files if they don't use unknown features
- Gracefully degrade (skip unknown features)
- Warn user about unsupported features

#### 3. **Migration API**

**Add to CLI and Python API:**
```rust
// Migrate old format to new format
strata migrate input_v1.st output_v2.st
```

```python
# Python API
strata.migrate("old_v1.st", "new_v2.st", target_version=2)
```

---

## Fast vs Tight Packing Modes

### Current Implementation

**Location:** `crates/common/src/config.rs`

```rust
pub enum BuildProfile {
    Generic,   // Balanced: 64KB blocks, LZ4/Zstd
    Eda,       // Small files: 16KB blocks, Zstd + dict
    Embedded,  // High compression: 4KB blocks, Zstd level 19
    Ml,        // ML datasets: 1MB blocks, LZ4
}
```

**CDC Parameters (Content-Defined Chunking):**
```rust
pub struct DedupeParams {
    pub min_chunk: u32,  // Default: 16KB
    pub avg_chunk: u32,  // Default: 64KB
    pub max_chunk: u32,  // Default: 128KB
}
```

### Proposed: Fast vs Tight Modes

#### **Fast Mode (Speed Priority)**

**Goal:** Fastest possible packing, minimal CPU overhead

```python
strata.pack(
    "data.img",
    "out.st",
    mode="fast",
    # Auto-configures:
    # - compression="lz4" (2 GB/s)
    # - compression_level=1
    # - dedup=False (no CDC)
    # - block_size=1MB (fewer blocks)
    # - train_dictionary=False
)
```

**Characteristics:**
- ⚡ **Speed:** ~2 GB/s packing
- 💾 **Compression ratio:** ~2-3x (LZ4)
- 🔍 **Dedup:** Disabled (no CDC overhead)
- 📦 **Use case:** Large files, fast iteration, temporary snapshots

#### **Balanced Mode (Default)**

**Goal:** Good compression with reasonable speed

```python
strata.pack(
    "data.img",
    "out.st",
    mode="balanced",
    # Auto-configures:
    # - compression="lz4" or "zstd" (adaptive)
    # - compression_level=3
    # - dedup=True
    # - CDC: min=16KB, avg=64KB, max=128KB
    # - block_size=64KB
)
```

**Characteristics:**
- ⚡ **Speed:** ~500 MB/s packing
- 💾 **Compression ratio:** ~3-5x
- 🔍 **Dedup:** FastCDC enabled
- 📦 **Use case:** General purpose, most ML datasets

#### **Tight Mode (Compression Priority)**

**Goal:** Maximum compression, CPU intensive

```python
strata.pack(
    "data.img",
    "out.st",
    mode="tight",
    # Auto-configures:
    # - compression="zstd"
    # - compression_level=19 (max)
    # - dedup=True
    # - CDC: min=4KB, avg=16KB, max=32KB (smaller chunks)
    # - block_size=4KB
    # - train_dictionary=True (20-40% better)
)
```

**Characteristics:**
- ⚡ **Speed:** ~50 MB/s packing (20x slower)
- 💾 **Compression ratio:** ~5-10x
- 🔍 **Dedup:** Aggressive CDC with small chunks
- 📦 **Use case:** Long-term archival, bandwidth-constrained

---

### Compatibility Between Modes

**✅ YES - All modes produce compatible .st files**

**Why compatible:**
1. **Same file format version** (FORMAT_VERSION = 1)
2. **Header stores compression type** (Lz4 or Zstd)
3. **Header stores block size** (variable per file)
4. **Index stores block locations** (works with any block size)

**Example:**
```python
# Pack with fast mode
strata.pack("data.img", "fast.st", mode="fast")

# Pack with tight mode
strata.pack("data.img", "tight.st", mode="tight")

# Both can be read the same way
with strata.open("fast.st") as f:
    data = f.read(4096)

with strata.open("tight.st") as f:
    data = f.read(4096)
```

**Differences are ONLY in:**
- Compression algorithm (stored in header)
- Block size (stored in header)
- Whether dedup was used (affects size only)

**The reader doesn't care HOW the file was created - it reads the header and adapts.**

---

## Recommended Python API

### Mode-Based Packing

```python
# Preset modes
strata.pack("data.img", "out.st", mode="fast")      # LZ4, no dedup
strata.pack("data.img", "out.st", mode="balanced")  # Default
strata.pack("data.img", "out.st", mode="tight")     # Zstd max, small chunks

# Or explicit control
strata.pack(
    "data.img",
    "out.st",
    compression="zstd",
    compression_level=9,
    dedup=True,
    min_chunk=8192,
    avg_chunk=32768,
    max_chunk=65536,
    block_size=16384
)

# Profile-based (as discussed)
strata.build("data/", "out.st", profile="ml")  # Auto-configures for ML
```

### Version Checking

```python
# Check file version
info = strata.inspect("file.st")
print(f"Format version: {info.version}")
print(f"Compatible: {info.is_compatible}")
print(f"Features: {info.features}")

# Migrate old versions
if not info.is_compatible:
    strata.migrate("old.st", "new.st", target_version=2)
```

---

## Summary & Recommendations

### Current State

✅ **Version numbers ARE stored** in every .st file header
✅ **Version checking happens** on file open
✅ **Format is extensible** via serde + feature flags
⚠️ **Version checking is too strict** (binary accept/reject)
⚠️ **No migration tools** for future version upgrades

### Recommendations

#### 1. **Versioning Strategy**

**Add to codebase:**
```rust
// crates/core/src/format/version.rs
pub const MIN_SUPPORTED_VERSION: u32 = 1;
pub const MAX_SUPPORTED_VERSION: u32 = 1;
pub const CURRENT_VERSION: u32 = 1;

pub fn is_compatible(file_version: u32) -> bool {
    file_version >= MIN_SUPPORTED_VERSION
    && file_version <= MAX_SUPPORTED_VERSION
}

pub fn can_read_with_degradation(file_version: u32) -> bool {
    // Can read newer versions, but some features may not work
    file_version >= MIN_SUPPORTED_VERSION
}
```

**Python API:**
```python
info = strata.inspect("file.st")
print(f"Version: {info.version}")
print(f"Current version: {strata.FORMAT_VERSION}")
print(f"Compatible: {info.is_compatible}")
if info.version > strata.FORMAT_VERSION:
    print("Warning: File was created with newer version")
```

#### 2. **Packing Modes**

**Add mode presets:**
```python
# Simple presets
strata.pack("data", "out.st", mode="fast")      # ~2 GB/s, 2-3x compression
strata.pack("data", "out.st", mode="balanced")  # ~500 MB/s, 3-5x compression
strata.pack("data", "out.st", mode="tight")     # ~50 MB/s, 5-10x compression

# All produce compatible .st files!
```

**Add to build profiles:**
```python
# Profile + mode combination
strata.build("data/", "out.st", profile="ml", mode="fast")
strata.build("data/", "out.st", profile="eda", mode="tight")
```

#### 3. **Feature Flags**

**Expand FeatureFlags:**
```rust
pub struct FeatureFlags {
    pub has_disk: bool,
    pub has_memory: bool,
    pub variable_blocks: bool,
    pub has_dictionary: bool,      // NEW
    pub has_metadata: bool,        // NEW
    pub has_encryption: bool,      // NEW
    pub has_signature: bool,       // NEW
    pub reserved: [u8; 8],         // For future use
}
```

**Python API:**
```python
info = strata.inspect("file.st")
if not info.features.has_encryption:
    print("Warning: File is not encrypted")
```

#### 4. **Migration Tools**

**Add migration command:**
```bash
# CLI
strata migrate input_v1.st output_v2.st --target-version=2

# Python
strata.migrate("old.st", "new.st", target_version=2)
```

---

## Format Evolution Example

### Version 1 → Version 2 (Hypothetical)

**New features in V2:**
- Block-level checksums
- New compression algorithm (Brotli)
- Improved encryption (ChaCha20-Poly1305)

**Backward compatibility:**
```rust
match header.version {
    1 => {
        // V1 files: No checksums, older compression
        open_v1(header, backend)
    }
    2 => {
        // V2 files: Check if we support new features
        if header.features.has_brotli && !SUPPORTS_BROTLI {
            return Err("This file requires Brotli support");
        }
        open_v2(header, backend)
    }
    _ => Err("Unsupported version")
}
```

**Forward compatibility:**
- V1 reader can open V2 file IF it doesn't use new features
- V2 reader can always open V1 files
- Migration tool can upgrade V1 → V2

---

## Answers to Your Questions

### 1. Are version numbers stored in .st file headers?

**✅ YES**
- Every .st file has a 4096-byte header
- Header contains `version: u32` field
- Currently FORMAT_VERSION = 1
- Version is checked on every file open

### 2. Will changes work retroactively?

**✅ YES, with proper version handling**
- Current code checks version on open
- Recommendation: Support range of versions (min-max)
- Use feature flags for optional features
- Provide migration tools for breaking changes

### 3. Should you have fast vs tight modes?

**✅ YES, great idea!**
- **Fast:** LZ4, no dedup, 1MB blocks (~2 GB/s)
- **Balanced:** LZ4/Zstd, dedup, 64KB blocks (~500 MB/s) [DEFAULT]
- **Tight:** Zstd max, dedup, 4KB blocks, dict training (~50 MB/s)

### 4. Would they be compatible .st files?

**✅ YES, 100% compatible**
- All modes use same format version
- Differences stored in header (compression, block_size)
- Reader adapts based on header
- No format incompatibility

**You can mix and match:**
- Create with fast mode, read anywhere
- Create with tight mode, read anywhere
- Combine in thin snapshot chains

---

## Implementation Priority

### CRITICAL: Version Checking (1-2 hours) ⚠️
**Why Critical:** Current version checking will reject any future file format versions, breaking forward compatibility.

**Files to modify:**
1. `crates/core/src/format/version.rs` (NEW FILE)
   ```rust
   //! Version checking and compatibility logic

   pub const MIN_SUPPORTED_VERSION: u32 = 1;
   pub const MAX_SUPPORTED_VERSION: u32 = 1;
   pub const CURRENT_VERSION: u32 = 1;

   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub enum VersionCompatibility {
       Full,      // Version fully supported
       Degraded,  // Can read but some features may not work
       Incompatible, // Too old to read
   }

   pub fn check_version(file_version: u32) -> Result<VersionCompatibility> {
       if file_version < MIN_SUPPORTED_VERSION {
           Err(StrataError::Version(format!(
               "File version {} is too old (minimum: {})",
               file_version, MIN_SUPPORTED_VERSION
           )))
       } else if file_version > MAX_SUPPORTED_VERSION {
           // Warn but allow - graceful degradation
           eprintln!(
               "Warning: File version {} is newer than supported {} - \
                some features may not work",
               file_version, MAX_SUPPORTED_VERSION
           );
           Ok(VersionCompatibility::Degraded)
       } else {
           Ok(VersionCompatibility::Full)
       }
   }
   ```

2. `crates/core/src/api/stratafile.rs` - Update version check in `StrataFile::new()`
   ```rust
   use crate::format::version::{check_version, VersionCompatibility};

   // In StrataFile::new():
   match check_version(header.version)? {
       VersionCompatibility::Full => {
           // All features available
       }
       VersionCompatibility::Degraded => {
           // Some features may not work - continue with caution
       }
   }
   ```

3. `crates/loader/python/strata/utils.py` - Add version info to Metadata
   ```python
   class Metadata:
       version: int
       is_compatible: bool
       compatibility_status: Literal["full", "degraded", "incompatible"]
   ```

**Testing:**
- Create test file with version 2
- Verify graceful degradation (warning but loads)
- Create test file with version 0
- Verify rejection with clear error

---

### Priority 1: Core Format Improvements (2-3 hours)

#### Task 1.1: Expand FeatureFlags (1 hour)
**File:** `crates/core/src/format/header.rs`

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct FeatureFlags {
    // V1 flags (existing)
    pub has_disk: bool,
    pub has_memory: bool,
    pub variable_blocks: bool,

    // V1 additional flags
    pub has_dictionary: bool,    // Zstd dictionary training
    pub has_metadata: bool,      // Custom metadata block
    pub has_encryption: bool,    // Encryption enabled
    pub has_signature: bool,     // Ed25519 signature
    pub has_checksums: bool,     // Per-block CRC32 checksums

    // V2 flags (future)
    pub has_compression_v2: bool, // Brotli, Zstd with patches
    pub has_encryption_v2: bool,  // ChaCha20-Poly1305

    // Reserved for future expansion
    pub reserved: [u8; 8],
}
```

**Update writers to set flags:**
- Set `has_dictionary` when dictionary training used
- Set `has_metadata` when metadata block written
- Set `has_encryption` when encryption enabled
- Set `has_signature` when signature added

#### Task 1.2: Python API for Version Checking (30 min)
**File:** `crates/loader/python/strata/utils.py`

```python
def inspect(path: PathLike) -> Metadata:
    """Enhanced with version checking."""
    meta = _core.inspect(path)
    meta.is_compatible = (
        meta.version >= MIN_SUPPORTED_VERSION
        and meta.version <= MAX_SUPPORTED_VERSION
    )
    return meta

# Add module constants
FORMAT_VERSION = 1
MIN_SUPPORTED_VERSION = 1
MAX_SUPPORTED_VERSION = 1
```

#### Task 1.3: Add Block Checksums (Optional - 2 hours)
**Files:**
- `crates/core/src/format/index/mod.rs` - Add CRC32 to BlockInfo
- Enable with feature flag

---

### Priority 2: Scalability Improvements (4-6 hours)

#### Issue 1: Master Index Memory Usage
**Current:** Full index loaded into memory on open
**Problem:** For 10TB file with 64KB blocks = 160M blocks * 32 bytes = 5GB RAM

**Solutions:**
1. **Lazy Index Loading (Medium effort):**
   - Load only active index pages
   - LRU cache for index pages
   - Prefetch adjacent pages

2. **Multi-level Index (High effort):**
   - Two-level B-tree: directory + leaf pages
   - Only load directory in memory
   - Load leaf pages on demand

**Recommendation:** Start with lazy loading (2-3 hours)

#### Issue 2: Single-threaded Compression
**Current:** Blocks compressed sequentially
**Fix:** Parallel block compression with rayon (2-3 hours)

```rust
use rayon::prelude::*;

// In StrataBuilder::process_stream:
let compressed_blocks: Vec<_> = uncompressed_blocks
    .par_iter()
    .map(|block| compressor.compress(block))
    .collect();
```

**Speedup:** 4-8x on multi-core systems

---

### Priority 3: Packing Modes & Profiles (3-4 hours)

#### Task 3.1: Wire Packing Modes to Rust (2 hours)
**File:** `crates/loader/src/py_interface/builder.rs`

Add mode parameter:
```rust
#[pyclass]
pub struct StrataBuilder {
    packing_mode: PackingMode,
    // ...
}

pub enum PackingMode {
    Fast,     // No dedup, LZ4, large blocks
    Balanced, // DCAM dedup, LZ4/Zstd, medium blocks
    Tight,    // Full dedup, Zstd max, small blocks
}

impl StrataBuilder {
    #[new]
    pub fn new(
        output_path: String,
        block_size: u32,
        compression: &str,
        compression_level: Option<i32>,
        packing_mode: &str,  // NEW
    ) -> PyResult<Self> {
        // Apply mode presets
        let (block_size, dedup, comp) = match packing_mode {
            "fast" => (1_048_576, false, "lz4"),
            "balanced" => (65536, true, compression),
            "tight" => (4096, true, "zstd"),
            _ => return Err(PyValueError::new_err("Invalid mode")),
        };
        // ...
    }
}
```

#### Task 3.2: Directory Walking in build() (1-2 hours)
**File:** `crates/loader/python/strata/profiles.py`

```python
import os
from pathlib import Path

def build(
    source: PathLike,
    output: PathLike,
    *,
    profile: BuildProfile = "generic",
    **overrides: Any,
) -> Metadata:
    """Build snapshot from file or directory."""
    config = PROFILES[profile].copy()
    config.update(overrides)

    with Writer(output, **config) as writer:
        source_path = Path(source)

        if source_path.is_file():
            writer.add_file(source)
        elif source_path.is_dir():
            # Walk directory recursively
            for root, dirs, files in os.walk(source):
                for file in files:
                    file_path = Path(root) / file
                    writer.add_file(file_path)
        else:
            raise ValidationError(f"Source not found: {source}")

    return inspect(output)
```

---

### Priority 4: ML Dataset Implementation (6-8 hours)

**See IMPLEMENTATION_STATUS.md for detailed breakdown**

This is the highest-value feature for ML users. Focus on:
1. LRUCache (2 hours)
2. Basic Dataset without prefetching (3 hours)
3. Prefetcher (3 hours)

---

### Priority 5: Rust API Improvements (10-15 hours)

#### Task 5.1: Direct Byte Writing (3-4 hours)
**File:** `crates/loader/src/py_interface/builder.rs`

```rust
impl StrataBuilder {
    pub fn add_bytes(&mut self, data: &[u8]) -> PyResult<()> {
        // Compress data
        let compressed = self.compressor.compress(data)?;

        // Write to file
        let offset = self.current_offset;
        self.writer.write_all(&compressed)?;

        // Update index
        self.master.add_block(BlockInfo {
            physical_offset: offset,
            compressed_size: compressed.len(),
            uncompressed_size: data.len(),
            // ...
        });

        self.current_offset += compressed.len() as u64;
        Ok(())
    }
}
```

**Impact:** Removes temp file workaround in writer.py

#### Task 5.2: Metadata Persistence (4-6 hours)
Add metadata block to file format:
- Serialize Python dict to JSON or MessagePack
- Write to dedicated section
- Update header with metadata_offset/length

#### Task 5.3: Progress Callbacks (2 hours)
Add callback parameter to builder:
```rust
pub fn set_progress_callback(&mut self, callback: PyObject) {
    self.progress_callback = Some(callback);
}
```

Call periodically during packing.

---

## Summary: What to Do After Class

### Immediate (1-2 hours):
✅ **Fix version checking** - Critical for future compatibility

### Short term (4-6 hours):
1. Implement Dataset.LRUCache
2. Implement basic Dataset.__getitem__()
3. Test with PyTorch DataLoader

### Medium term (8-12 hours):
1. Add direct byte writing to Rust
2. Implement Prefetcher
3. Add directory walking to build()
4. Expand FeatureFlags

### Long term (2-3 weeks):
1. Lazy index loading for scalability
2. Parallel compression
3. Metadata persistence
4. Block checksums
5. Migration tools
6. Comprehensive test suite

This prioritization maximizes impact while keeping effort reasonable! 🚀
