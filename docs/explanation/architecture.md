# Hexz Architecture

This document covers the internal architecture of Hexz: format structure, read/write paths, deduplication, tensor manifest, and tensor-level chunking. The FUSE overlay (copy-on-write VM disk layer) is not covered here — that use case is deprioritized. See [ROADMAP.md](../project-docs/ROADMAP.md).

## System Overview

```mermaid
graph TD
    subgraph "User Interfaces"
        CLI["CLI Tool (hexz-cli)"]
        Py["Python API (hexz-loader)"]
    end

    subgraph "Core Engine (hexz-core)"
        API[API Layer]
        Ops[Operations]

        subgraph "Internal Modules"
            Format[Format Parsing\nsafetensors / GGUF / hxz]
            Cache[Block/Page Cache]
            Algo[Compression / Dedup / XOR delta]
            Store[Storage Backend]
        end
    end

    Common["hexz-common (Config, Errors, Crypto)"]

    CLI -->|Calls| Ops
    Py -->|Wraps| API
    API -->|High-level access| Ops
    Ops -->|Decodes| Format
    Ops -->|Reads/Writes via| Cache
    Cache -->|Fetches from| Store
    Cache -->|Processes via| Algo
    Format -.->|Uses| Common
    Store -.->|Uses| Common
    Algo -.->|Uses| Common
```

## File Format Structure

The `.hxz` file is a binary format designed for O(log N) random access. A fixed-size header points to a master index, which maps virtual offsets to page indices, which contain metadata for individual data blocks. An optional tensor manifest is stored at `metadata_offset` in the header.

```mermaid
classDiagram
    class File {
        +Header header
        +DataBlocks[] blocks
        +IndexPage[] pages
        +MasterIndex index
        +Metadata metadata
    }

    class Header {
        +Magic "HEXZ"
        +Version u16
        +BlockSize u32
        +IndexOffset u64
        +MetadataOffset u64
        +ParentPath Option~String~
    }

    class MasterIndex {
        +List~PageEntry~ pages
        +TotalSize u64
    }

    class PageEntry {
        +VirtualOffset u64
        +PhysicalOffset u64
        +Length u32
    }

    class IndexPage {
        +List~BlockInfo~ blocks
    }

    class BlockInfo {
        +VirtualOffset u64
        +PhysicalOffset u64
        +CompressedLength u32
        +LogicalLength u32
        +Hash Blake3Hash
        +StorageMode enum
    }

    class TensorManifest {
        +format: str
        +tensors: Map~String, TensorEntry~
    }

    class TensorEntry {
        +offset u64
        +length u64
        +dtype str
        +shape Vec~u64~
        +storage StorageMode
    }

    File *-- Header
    File *-- MasterIndex
    File *-- TensorManifest
    MasterIndex *-- PageEntry
    PageEntry --> IndexPage
    IndexPage *-- BlockInfo
    TensorManifest *-- TensorEntry
```

### Storage modes

`BlockInfo.StorageMode` (and `TensorEntry.storage`) encodes how a block was stored:

| Mode | Description |
|---|---|
| `Raw` | Block stored as-is (compressed with lz4/zstd) |
| `DedupRef` | Block content is identical to a block at `physical_offset` (may be in parent) |
| `Zero` | Block is all zeros — stored as 8 bytes of metadata, no data |
| `XorDelta` | Block is the XOR delta of this tensor against the parent's tensor (Phase 3) |

## Tensor Manifest

When packing a safetensors or GGUF file, Hexz writes a tensor manifest into the `metadata_offset` slot in the header. The manifest is a msgpack blob with this schema:

```json
{
  "format": "safetensors",
  "version": "1",
  "safetensors_header": "<original safetensors JSON header>",
  "tensors": {
    "embed_tokens.weight": {
      "offset": 0,
      "length": 268435456,
      "dtype": "BF16",
      "shape": [32000, 4096],
      "storage": "raw"
    },
    "lm_head.weight": {
      "offset": 268500992,
      "length": 268435456,
      "dtype": "BF16",
      "shape": [32000, 4096],
      "storage": "xor_delta"
    }
  }
}
```

`offset` and `length` are the logical byte range in the archive's virtual address space. The storage backend resolves logical offsets to physical block locations via the master index.

## Tensor-Level Chunking

For safetensors and GGUF files, Hexz does not use CDC (content-defined chunking). Instead:

1. Parse the file header to get a sorted list of `(tensor_name, data_start, data_end)`.
2. For each tensor, write its bytes in fixed `block_size` chunks. After the last chunk, pad to the next `block_size` boundary with zeros.
3. Record the logical offset and length of each tensor in the manifest.

This is simpler than CDC and avoids the rolling-hash scan overhead. Tensor boundaries never shift between a base model and a fine-tune (same architecture = same shapes), so tensor-boundary chunking provides stable block identities for dedup.

## Write Path (packing)

```mermaid
flowchart TD
    Start([Start]) --> OpenInput[Open source file\nsafetensors / GGUF / generic]
    OpenInput --> ParseHeader{Format?}

    ParseHeader -- safetensors/GGUF --> TensorList[Parse tensor manifest\nGet tensor name → byte range]
    ParseHeader -- generic --> CDC{CDC enabled?}

    TensorList --> TensorLoop{For each tensor}

    TensorLoop --> HasParent{Parent archive\nprovided?}
    HasParent -- Yes --> HashCheck{BLAKE3 match\nin parent?}
    HashCheck -- Yes --> DedupRef[Record DedupRef\nno bytes written]
    HashCheck -- No --> XorPhase3{XOR delta\nenabled? Phase 3}
    XorPhase3 -- Yes --> XorDelta[Read parent tensor\nXOR bytes\nCompress delta]
    XorPhase3 -- No --> WriteRaw[Compress tensor\nWrite blocks]
    HasParent -- No --> WriteRaw

    DedupRef --> Pad[Pad to block boundary]
    XorDelta --> Pad
    WriteRaw --> Pad

    Pad --> RecordManifest[Record tensor in manifest\noffset / length / storage_mode]
    RecordManifest --> TensorLoop

    TensorLoop -- Done --> WriteManifest[Serialize manifest → metadata_offset]
    WriteManifest --> WriteMasterIdx[Write master index]
    WriteMasterIdx --> UpdateHeader[Write final header]
    UpdateHeader --> End([Done])

    CDC -- Yes --> StreamChunker[FastCDC rolling hash]
    CDC -- No --> FixedChunker[Fixed block_size chunks]
    StreamChunker --> CompressBlock[Compress block]
    FixedChunker --> CompressBlock
    CompressBlock --> DedupLookup{BLAKE3 in\ndedup map?}
    DedupLookup -- Yes --> DedupRef2[Reference existing offset]
    DedupLookup -- No --> WriteBlock[Write block]
    DedupRef2 --> WriteManifest
    WriteBlock --> WriteManifest
```

## Read Path

```mermaid
flowchart TD
    Request["Read request\n(tensor name or offset+length)"] --> ManifestLookup[Lookup tensor manifest\nGet logical offset + length]
    ManifestLookup --> StorageCheck{Storage mode?}

    StorageCheck -- Raw / DedupRef --> MasterIndex[Binary search master index\nFind page entries]
    StorageCheck -- XorDelta --> MasterIndex

    MasterIndex --> PageCache{Page in cache?}
    PageCache -- Miss --> FetchPage[Read index page from backend]
    PageCache -- Hit --> BlockLoop
    FetchPage --> BlockLoop{For each block}

    BlockLoop --> BlockCache{Block in cache?}
    BlockCache -- Miss --> FetchBlock[Read compressed block from backend]
    FetchBlock --> Decompress[Decompress lz4/zstd]
    Decompress --> InsertCache[Insert in block cache]
    InsertCache --> Extract[Extract slice]
    BlockCache -- Hit --> Extract

    Extract --> MoreBlocks{More blocks?}
    MoreBlocks -- Yes --> BlockLoop
    MoreBlocks -- No --> AssembleTensor[Assemble tensor bytes]

    AssembleTensor --> XorCheck{Storage was\nXorDelta?}
    XorCheck -- Yes --> ReadParent[Read base tensor from parent archive]
    ReadParent --> XorReconstruct[XOR delta_bytes with base_bytes]
    XorReconstruct --> Return[Return tensor]
    XorCheck -- No --> Return
```

## Deduplication Logic

During packing, Hexz maintains an in-memory BLAKE3 hash → physical offset map. When a compressed block's hash matches an existing entry, the index records the existing physical offset and no bytes are written. For parent-chain dedup, the parent archive's index is loaded at pack time and blocks in the parent are added to the dedup map with their parent-relative offsets.

At read time, if a block's `physical_offset` refers to the parent archive, the storage backend transparently opens the parent file and reads from it.

## Storage Backends

The storage backend abstraction (`hexz-core::store`) provides byte-range reads from:

- `FileBackend` — local disk, using `pread` for concurrent access
- `MmapBackend` — memory-mapped file for zero-copy reads on warm pages
- `HttpBackend` — HTTP byte-range requests (`Range: bytes=start-end`)
- `S3Backend` — AWS S3 byte-range requests

All backends implement `read_at(offset: u64, length: u64) -> Bytes`. The caller does not need to know which backend is in use.
