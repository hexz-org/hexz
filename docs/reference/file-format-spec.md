# Strata File Format Specification

This document describes the binary format of `.st` (Strata) files.

## Overview

Strata files use a hierarchical index structure for O(log N) block lookup:

```
[Header] → [Master Index] → [Page Indices] → [Data Blocks]
```

## File Layout

```
Offset          | Content
----------------|----------------------------------
0x00            | File Header (64 bytes)
0x40            | Data Blocks (variable)
...             | ...
<index_offset>  | Page Index 0 (variable)
                | Page Index 1 (variable)
                | ...
                | Page Index N (variable)
<master_offset> | Master Index (variable)
EOF             | End of file
```

## File Header

Fixed 64-byte structure at offset 0:

| Offset | Size | Type | Field | Description |
|--------|------|------|-------|-------------|
| 0x00   | 8    | u64  | magic | Magic number: `0x53545241544120` ("STRATA ") |
| 0x08   | 2    | u16  | version | Format version (current: 1) |
| 0x0A   | 2    | u16  | flags | Feature flags (see below) |
| 0x0C   | 4    | u32  | block_size | Uncompressed block size |
| 0x10   | 1    | u8   | compression | Compression algorithm (see below) |
| 0x11   | 1    | u8   | compression_level | Compression level (0-22) |
| 0x12   | 6    | -    | reserved | Reserved for future use |
| 0x18   | 8    | u64  | uncompressed_size | Total uncompressed size |
| 0x20   | 8    | u64  | master_index_offset | Offset to master index |
| 0x28   | 4    | u32  | master_index_size | Size of master index |
| 0x2C   | 20   | -    | reserved | Reserved for future use |

**Compression Algorithms**:
- `0x00`: None (uncompressed)
- `0x01`: LZ4
- `0x02`: Zstandard

**Feature Flags** (bitfield):
- `0x0001`: CDC enabled (content-defined chunking)
- `0x0002`: Encrypted (AES-256-GCM)
- `0x0004`: Signed (Ed25519)
- `0x0008`: Thin snapshot (references parent)

All multi-byte integers are little-endian.

## Master Index

Located at `master_index_offset`, the master index maps virtual address space to page indices:

```
[PageEntry 0]
[PageEntry 1]
...
[PageEntry N]
```

**PageEntry** (32 bytes):

| Offset | Size | Type | Field | Description |
|--------|------|------|-------|-------------|
| 0x00   | 8    | u64  | virtual_offset | Starting virtual offset |
| 0x08   | 8    | u64  | physical_offset | Offset to page index |
| 0x10   | 8    | u64  | virtual_length | Virtual length covered |
| 0x18   | 4    | u32  | page_size | Size of page index |
| 0x1C   | 4    | u32  | block_count | Blocks in this page |

## Page Index

Each page index contains metadata for a contiguous range of blocks:

```
[BlockInfo 0]
[BlockInfo 1]
...
[BlockInfo M]
```

**BlockInfo** (48 bytes):

| Offset | Size | Type | Field | Description |
|--------|------|------|-------|-------------|
| 0x00   | 8    | u64  | virtual_offset | Virtual offset of block |
| 0x08   | 8    | u64  | physical_offset | Physical offset in file |
| 0x10   | 4    | u32  | compressed_size | Compressed size (0 = zero block) |
| 0x14   | 4    | u32  | uncompressed_size | Uncompressed size |
| 0x18   | 32   | u8[] | hash | BLAKE3 hash (if CDC enabled) |

**Special Cases**:
- `compressed_size = 0`: Zero block (all zeros), no physical storage
- `physical_offset = 0xFFFFFFFFFFFFFFFF`: Reference to parent snapshot (thin snapshots)

## Data Blocks

Compressed data stored sequentially. Format depends on compression algorithm:

**LZ4**:
- Raw LZ4-compressed data
- No additional framing

**Zstandard**:
- Zstd-compressed data with dictionaries (if used)
- Standard Zstd framing

**Encrypted Blocks**:
If encryption enabled:
- 12-byte nonce (prepended)
- AES-256-GCM encrypted compressed data
- 16-byte authentication tag (appended)

Total encrypted block size: `12 + compressed_size + 16`

## Lookup Algorithm

To read bytes at `offset` with `length`:

1. Binary search master index for `PageEntry` containing `offset`
2. Load page index from `PageEntry.physical_offset`
3. Binary search page index for `BlockInfo` entries overlapping range
4. For each block:
   - Read compressed data from `BlockInfo.physical_offset`
   - Decompress (and decrypt if needed)
   - Extract relevant slice
5. Concatenate slices and return

Time complexity: O(log P + log B) where P = pages, B = blocks per page

## Example

Minimal valid Strata file (1 block, LZ4-compressed):

```
Offset | Hex Data
-------|----------
0x00   | 53 54 52 41 54 41 20   Magic "STRATA "
0x08   | 01 00                   Version 1
0x0A   | 00 00                   No flags
0x0C   | 00 00 01 00             Block size 65536
0x10   | 01                      LZ4 compression
0x11   | 00                      Level 0
0x18   | 00 00 01 00 00 00 00 00 Uncompressed: 65536
0x20   | XX XX XX XX XX XX XX XX Master index offset
0x28   | 20 00 00 00             Master index size: 32
...    | <compressed block data>
...    | <page index: 1 BlockInfo>
...    | <master index: 1 PageEntry>
```

## Version History

- **Version 1** (current): Initial release

## See Also

- [Explanation: Architecture](../explanation/architecture.md)
- [ADR-0002: Block-Level Compression](../adr/0002-block-level-compression.md)
- [Reference: Compression Algorithms](compression-algorithms.md)
