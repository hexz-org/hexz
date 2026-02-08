# Strata File Format Specification

**Version**: 1
**Extension**: `.st`
**Endianness**: Little-endian
**Alignment**: No padding, densely packed

## Overview

Strata snapshots store compressed, block-indexed data with support for:
- Dual streams (disk + memory)
- Block-level deduplication
- Optional encryption (AES-256-GCM)
- Thin snapshots (parent references)
- Content-defined chunking

## File Structure

```
[Header: 512 bytes]
[Compressed Blocks: variable]
[Index Pages: variable]
[Master Index: variable, at header.index_offset]
```

## Header Format (512 bytes)

```rust
struct StrataHeader {
    magic: [u8; 8],              // "STRATA\0\0"
    version: u32,                // Format version (currently 1)
    block_size: u32,             // Uncompressed block size
    index_offset: u64,           // File offset of master index
    parent_path: Option<String>, // Path to parent (thin snapshots)
    dictionary_offset: Option<u64>,   // Zstd dictionary (future)
    dictionary_length: Option<u32>,
    metadata_offset: Option<u64>,     // JSON metadata (future)
    metadata_length: Option<u32>,
    signature_offset: Option<u64>,    // Ed25519 signature
    signature_length: Option<u32>,
    encryption: Option<EncryptionMetadata>,
    compression: CompressionType,     // Lz4 | Zstd
    features: FeatureFlags,
}

struct EncryptionMetadata {
    algorithm: u8,     // 1 = AES-256-GCM
    salt: [u8; 32],    // Argon2 salt
    nonce: [u8; 12],   // GCM nonce prefix
}

struct FeatureFlags {
    has_disk: bool,
    has_memory: bool,
    variable_blocks: bool,  // Future: non-uniform block sizes
}
```

Serialization: bincode with length-prefixed strings.

## Block Data

Compressed blocks are written sequentially after the header. Each block may be:

1. **Normal Block**: Compressed data at `BlockInfo.offset`
2. **Zero Block**: All zeros, `BlockInfo.offset = 0`, no data written
3. **Parent Block**: Thin snapshot reference, `BlockInfo.offset = BLOCK_OFFSET_PARENT`

**BLOCK_OFFSET_PARENT**: `0xFFFFFFFFFFFFFFFF` (u64::MAX)

### Compression

**LZ4**:
- Default, fast compression (~2GB/s)
- Uses `lz4_flex` crate
- No dictionary

**Zstandard**:
- Better compression (~500MB/s)
- Level 3 default
- Optional shared dictionary (future)

### Encryption

If `header.encryption.is_some()`:
1. Generate per-block key: `HKDF(password, block_idx, salt)`
2. Encrypt compressed block: `AES-256-GCM.encrypt(key, nonce||block_idx, compressed_data)`
3. Write ciphertext + auth tag

## Index Structures

### BlockInfo (20 bytes)

```rust
struct BlockInfo {
    offset: u64,      // File offset (0=zeros, MAX=parent)
    length: u32,      // Compressed size (0 if zero block)
    logical_len: u32, // Uncompressed size
    checksum: u32,    // CRC32 of compressed data
}
```

### IndexPage (variable)

```rust
struct IndexPage {
    blocks: Vec<BlockInfo>,  // Max 1024 entries
}
```

Stored as bincode-serialized blob at locations tracked by PageEntry.

### PageEntry (32 bytes)

```rust
struct PageEntry {
    offset: u64,       // File offset of serialized IndexPage
    length: u32,       // Size of serialized page
    start_block: u64,  // First block number in this page
    start_logical: u64,// Logical byte offset of first block
}
```

### MasterIndex (variable)

```rust
struct MasterIndex {
    disk_size: u64,            // Total uncompressed disk size
    memory_size: u64,          // Total uncompressed memory size
    disk_pages: Vec<PageEntry>,
    memory_pages: Vec<PageEntry>,
}
```

Stored at `header.index_offset`, bincode-serialized.

## Lookup Algorithm

To read bytes at logical offset `L` with length `N`:

1. Binary search `master.disk_pages` by `start_logical` to find containing pages
2. Load relevant `IndexPage` (from LRU cache or disk)
3. For each `BlockInfo` overlapping `[L, L+N)`:
   - If `offset == 0`: Return zeros
   - If `offset == MAX`: Read from parent snapshot at same logical position
   - Else: Read compressed block, decrypt (if encrypted), decompress, cache
4. Extract slice from each block, concatenate

## Thin Snapshots

When `header.parent_path.is_some()`, blocks with `offset = BLOCK_OFFSET_PARENT` are resolved by recursively reading the parent snapshot at the same logical offset.

**Parent Resolution**:
- Parent path is absolute
- Parent must be accessible when reading thin snapshot
- Circular references are detected (error)
- Maximum depth: 16 (configurable)

**Example**:
```
base.st:       [Block 0: data] [Block 1: data] [Block 2: data]
delta.st:      [Block 0: PARENT] [Block 1: modified] [Block 2: PARENT]
               ^-- references base.st

Reading delta.st block 0 → reads base.st block 0
Reading delta.st block 1 → reads local block
Reading delta.st block 2 → reads base.st block 2
```

## Content-Defined Chunking (CDC)

When `--cdc` is enabled during packing:

1. Apply FastCDC to input data with window size `avg_chunk`
2. Hash each chunk with BLAKE3
3. Store hash → offset mapping
4. If duplicate hash found, write `BlockInfo` referencing existing offset
5. Else, compress and write chunk as normal

**Chunk Boundaries**:
- Minimum: 16 KB (default)
- Average: 64 KB (default)
- Maximum: 128 KB (default)

**Deduplication Scope**: Within single snapshot only (cross-snapshot dedup is future work).

## Compatibility

**Forward Compatibility**:
- Readers must reject `version > 1`
- Unknown `compression` types: error
- Unknown `encryption` algorithms: error

**Backward Compatibility**:
- Version 1 readers handle version 1 files
- Future versions may add optional features detectable via `features` field

## Example

10 GB disk, 64 KB blocks, LZ4, no encryption:

```
Offset 0x0000: [Header: magic="STRATA\0\0", version=1, block_size=65536, ...]
Offset 0x0200: [Compressed Block 0: 32 KB LZ4 data]
Offset 0x8200: [Compressed Block 1: 28 KB LZ4 data]
...
Offset 0x15A3C800: [IndexPage 0: 1024 BlockInfo entries]
Offset 0x15A3E000: [IndexPage 1: 1024 BlockInfo entries]
...
Offset 0x15B9F800: [MasterIndex: disk_size=10737418240, disk_pages=[...]]
```

**Header.index_offset** = 0x15B9F800

## Verification

**Checksums**:
- Each block has CRC32 checksum of compressed (possibly encrypted) data
- Verified on decompression
- Mismatch → IOError

**Signature** (optional):
- Ed25519 signature over `MasterIndex` serialized bytes
- Stored at `header.signature_offset`
- Public key distribution: out-of-band

## Tools

**Inspect**:
```bash
strata data info snapshot.st
```

**Verify Integrity** (future):
```bash
strata data verify snapshot.st
```

**Dump Index** (debug):
```bash
# Future tool
strata-debug dump-index snapshot.st
```
