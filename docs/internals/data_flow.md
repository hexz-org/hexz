# Strata Data Flow and Architecture

This document details the internal architecture and data flow of the Strata project, illustrating how various components interact to provide high-performance seekable access to compressed data.

## System Architecture

Strata is built as a modular ecosystem. The user interacts through the CLI, Python API, or FUSE mount, all of which depend on the `strata-core` engine. This engine orchestrates low-level modules like storage backends, compression algorithms, and caching layers, while `strata-common` provides shared utilities.

```mermaid
graph TD
    subgraph "User Interfaces"
        CLI[CLI Tool\n(strata-cli)]
        Py[Python API\n(strata-loader)]
        Fuse[FUSE Mount\n(strata-fuse)]
    end

    subgraph "Core Engine (strata-core)"
        API[API Layer]
        Ops[Operations]
        
        subgraph "Internal Modules"
            Format[Format Parsing]
            Cache[Block/Page Cache]
            Algo[Compression/Dedup]
            Store[Storage Backend]
        end
    end

    Common[strata-common\n(Config, Errors, Crypto)]

    CLI -->|Calls| Ops
    Py -->|Wraps| API
    Fuse -->|Mounts via| API

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

The Strata (`.st`) file is a structured binary format designed for random access. It starts with a fixed-size header pointing to a Master Index. This index maps virtual offsets to Page Indices, which in turn contain metadata for individual compressed data blocks. This hierarchical lookup allows the engine to find any byte in the file with minimal I/O.

```mermaid
classDiagram
    class StrataFile {
        +Header header
        +MasterIndex index
        +Metadata metadata
    }

    class Header {
        +Magic "STRATA"
        +Version
        +BlockSize
        +IndexOffset
    }

    class MasterIndex {
        +List~PageEntry~ pages
        +TotalSize
    }

    class PageEntry {
        +VirtualOffset
        +PhysicalOffset
        +Length
    }

    class IndexPage {
        +List~BlockInfo~ blocks
    }

    class BlockInfo {
        +Offset
        +CompressedLength
        +LogicalLength
        +Checksum
    }

    class DataBlock {
        +CompressedBytes
    }

    StrataFile *-- Header : Defines entry point
    StrataFile *-- MasterIndex : Maps logical space
    MasterIndex *-- PageEntry : Segments index
    PageEntry --> IndexPage : points to physical location
    IndexPage *-- BlockInfo : Maps blocks in page
    BlockInfo --> DataBlock : points to raw data
```

## Write Path (Packing)

During the packing process, input data is streamed and broken into chunks (fixed-size or content-defined). Each chunk is compressed and hashed. The system checks a deduplication map to see if the content already exists; if so, it references the existing offset. Otherwise, it writes the new block and updates the index. Finally, it writes the master index and updates the file header.

```mermaid
flowchart TD
    subgraph "Ingestion & Chunking"
        Input[Input File/Stream] --> Chunker
        Chunker -->|Fixed or CDC| Block[Raw Block]
    end

    subgraph "Processing"
        Block --> Compressor[Compressor LZ4/Zstd]
        Compressor --> Compressed[Compressed Block]
        Compressed --> Hasher[Hasher SHA256]
        Hasher --> Hash[Block Hash]
    end

    subgraph "Persistence"
        Hash --> Dedup{In Dedup Map?}
        Dedup -- Yes --> IndexEntry["Create Index Entry\n(Reuse Offset)"]
        Dedup -- No --> Write[Write to .st File]
        
        Write --> MapUpdate[Update Dedup Map]
        MapUpdate --> IndexEntryNew["Create Index Entry\n(New Offset)"]
        
        IndexEntry --> IndexPage[Add to Index Page]
        IndexEntryNew --> IndexPage
    end

    subgraph "Finalization"
        IndexPage --> MasterIdx[Generate Master Index]
        MasterIdx --> Header[Write Header]
    end
```

## Read Path

When a read request is made for a specific offset and length, the engine first consults the Master Index to find the relevant Index Pages. These pages are loaded (and cached) to identify the specific blocks containing the requested data. The engine then checks the block cache; on a miss, it fetches the compressed data from the storage backend, decompresses it, and serves the requested slice to the user.

```mermaid
flowchart TD
    Request["Read Request\n(Offset, Length)"] --> MasterIndex[Lookup Master Index]
    
    subgraph "Index Resolution"
        MasterIndex --> PageCache{Page in Cache?}
        PageCache -- No --> FetchPage[Fetch Index Page from Store]
        FetchPage --> ParsePage[Parse Block Metadata]
        PageCache -- Yes --> ParsePage
    end
    
    subgraph "Data Retrieval"
        ParsePage --> BlockCache{Block in Cache?}
        BlockCache -- No --> FetchBlock[Read Compressed Block from Store]
        FetchBlock --> Decompress[Decompress & Verify]
        Decompress --> StoreCache[Store in Block Cache]
        StoreCache --> Assemble
        BlockCache -- Yes --> Assemble[Assemble Data Slice]
    end
    
    Assemble --> Return[Return Data to API/UI]
```

## Deduplication Logic

Deduplication occurs during the write path and is transparent to the reader. By hashing compressed blocks and maintaining a mapping of hashes to file offsets, Strata ensures that identical content is only stored once. Multiple index entries can point to the same physical data block, significantly reducing storage requirements for redundant data like OS images or repetitive logs.

```mermaid
sequenceDiagram
    participant Writer as Strata Writer
    participant DedupMap as Hash-to-Offset Map
    participant File as .st Storage
    participant Index as Index Builder

    Note over Writer: Block A (Content X)
    Writer->>Writer: Hash(X) = H1
    Writer->>DedupMap: Check(H1)
    DedupMap-->>Writer: Not Found
    Writer->>File: Write Block A (Offset 100)
    Writer->>DedupMap: Store(H1 -> 100)
    Writer->>Index: Record(H1 at Offset 100)

    Note over Writer: Block B (Content Y)
    Writer->>Writer: Hash(Y) = H2
    Writer->>DedupMap: Check(H2)
    DedupMap-->>Writer: Not Found
    Writer->>File: Write Block B (Offset 200)
    Writer->>DedupMap: Store(H2 -> 200)
    Writer->>Index: Record(H2 at Offset 200)

    Note over Writer: Block C (Content X)
    Writer->>Writer: Hash(X) = H1
    Writer->>DedupMap: Check(H1)
    DedupMap-->>Writer: Found (Offset 100)
    Note right of Writer: Content exists! Skip writing.
    Writer->>Index: Record(H1 at Offset 100)
```
