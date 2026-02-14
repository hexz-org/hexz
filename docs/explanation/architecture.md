# Hexz Data Flow and Architecture

This document details the internal architecture and data flow of the Hexz project, illustrating how various components interact to provide high-performance seekable access to compressed data.

## System Architecture

Hexz is built as a modular ecosystem. The user interacts through the CLI, Python API, or FUSE mount, all of which depend on the `hexz-core` engine. This engine orchestrates low-level modules like storage backends, compression algorithms, and caching layers, while `hexz-common` provides shared utilities.

```mermaid
graph TD
    subgraph "User Interfaces"
        CLI["CLI Tool<br/>(hexz-cli)"]
        Py["Python API<br/>(hexz-loader)"]
        Fuse["FUSE Mount<br/>(hexz-fuse)"]
    end

    subgraph "Core Engine (hexz-core)"
        API[API Layer]
        Ops[Operations]
        
        subgraph "Internal Modules"
            Format[Format Parsing]
            Cache[Block/Page Cache]
            Algo[Compression/Dedup]
            Store[Storage Backend]
        end
    end

    Common["hexz-common<br/>(Config, Errors, Crypto)"]

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

The Hexz (`.hxz`) file is a structured binary format designed for random access. It starts with a fixed-size header pointing to a Master Index. This index maps virtual offsets to Page Indices, which in turn contain metadata for individual compressed data blocks. This hierarchical lookup allows the engine to find any byte in the file with minimal I/O.

```mermaid
classDiagram
    class File {
        +Header header
        +MasterIndex index
        +Metadata metadata
    }

    class Header {
        +Magic "HEXZ"
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

    File *-- Header : Defines entry point
    File *-- MasterIndex : Maps logical space
    MasterIndex *-- PageEntry : Segments index
    PageEntry --> IndexPage : points to physical location
    IndexPage *-- BlockInfo : Maps blocks in page
    BlockInfo --> DataBlock : points to raw data
```

## Write Path (Packing)

During the packing process, input data is streamed and broken into chunks (fixed-size or content-defined). Each chunk is compressed and hashed. The system checks a deduplication map to see if the content already exists; if so, it references the existing offset. Otherwise, it writes the new block and updates the index. Finally, it writes the master index and updates the file header.

### Detailed Write Flow

```mermaid
flowchart TD
    Start([Start]) --> Open[Open input files]
    Open --> Header[Write placeholder header]

    Header --> Loop{More input blocks?}

    Loop -- Yes --> Zero{Block all zeros?}

    Zero -- Yes --> RecordZero[Record zero block reference]
    Zero -- No --> Compress[Compress block]

    Compress --> CDC{CDC enabled?}
    CDC -- Yes --> Hash[Hash compressed block]
    CDC -- No --> EncryptCheck{Encryption enabled?}

    Hash --> Dedup{Hash in dedup map?}
    Dedup -- Yes --> Ref[Reuse existing block offset]
    Dedup -- No --> EncryptCheck

    EncryptCheck -- Yes --> Encrypt[Encrypt block AES-256-GCM]
    EncryptCheck -- No --> CRC[Compute CRC32]

    Encrypt --> CRC
    CRC --> Write[Write block to file]

    RecordZero --> Next
    Ref --> Next
    Write --> RecordInfo[Add BlockInfo to index page]

    RecordInfo --> PageFull{Index page full?}
    PageFull -- Yes --> SerializePage[Serialize index page]
    SerializePage --> WritePage[Write page to file]
    WritePage --> RecordMaster[Add PageEntry to master index]
    RecordMaster --> NewPage[Start new index page]
    NewPage --> Next

    PageFull -- No --> Next

    Next --> Loop

    Loop -- No --> Final[Write final index page]
    Final --> Master[Write master index]
    Master --> Seek[Seek to file start]
    Seek --> UpdateHeader[Write final header]
    UpdateHeader --> Close[Close file]
    Close --> End([End])
```

```mermaid
flowchart TD
    subgraph "Ingestion & Chunking"
        Input["Input File/Stream"] --> Chunker
        Chunker -->|Fixed or CDC| Block["Raw Block"]
    end

    subgraph "Processing"
        Block --> Compressor["Compressor LZ4/Zstd"]
        Compressor --> Compressed["Compressed Block"]
        Compressed --> Hasher["Hasher SHA256"]
        Hasher --> Hash["Block Hash"]
    end

    subgraph "Persistence"
        Hash --> Dedup{"In Dedup Map?"}
        Dedup -- Yes --> IndexEntry["Create Index Entry<br/>(Reuse Offset)"]
        Dedup -- No --> Write["Write to .hxz File"]
        
        Write --> MapUpdate["Update Dedup Map"]
        MapUpdate --> IndexEntryNew["Create Index Entry<br/>(New Offset)"]
        
        IndexEntry --> IndexPage["Add to Index Page"]
        IndexEntryNew --> IndexPage
    end

    subgraph "Finalization"
        IndexPage --> MasterIdx["Generate Master Index"]
        MasterIdx --> Header["Write Header"]
    end
```

## Read Path

When a read request is made for a specific offset and length, the engine first consults the Master Index to find the relevant Index Pages. These pages are loaded (and cached) to identify the specific blocks containing the requested data. The engine then checks the block cache; on a miss, it fetches the compressed data from the storage backend, decompresses it, and serves the requested slice to the user.

### Detailed Read Flow

```mermaid
graph TD
    Start([Start]) --> Req[read 4KB, offset=1MB]
    Req --> FindBlocks[Find blocks containing 1MB, 1MB+4KB]
    FindBlocks --> SearchIndex[Binary search master index pages]
    SearchIndex --> PageCache{Check LRU<br/>page cache}
    
    PageCache -- Miss --> FetchPage[Read page from disk]
    PageCache -- Hit --> BlockLoop{For each block}
    FetchPage --> BlockLoop
    
    BlockLoop --> BlockCache{Check L1<br/>block cache}
    BlockCache -- Miss --> ReadStore[Read compressed data<br/>from backend]
    ReadStore --> Decrypt{Encrypted?}
    Decrypt -- Yes --> AES[Decrypt AES-256-GCM]
    Decrypt -- No --> Decompress[Decompress LZ4/Zstd]
    AES --> Decompress
    Decompress --> Verify[Verify checksum]
    Verify --> InsertCache[Insert into L1 cache]
    InsertCache --> Extract[Extract relevant slice]
    
    BlockCache -- Hit --> Extract
    Extract --> MoreBlocks{More blocks?}
    MoreBlocks -- Yes --> BlockLoop
    MoreBlocks -- No --> Concat[Concatenate slices]
    Concat --> Return[Return buffer]
    Return --> End([End])
```

```mermaid
flowchart TD
    Request["Read Request<br/>(Offset, Length)"] --> MasterIndex["Lookup Master Index"]
    
    subgraph "Index Resolution"
        MasterIndex --> PageCache{"Page in Cache?"}
        PageCache -- No --> FetchPage["Fetch Index Page from Store"]
        FetchPage --> ParsePage["Parse Block Metadata"]
        PageCache -- Yes --> ParsePage
    end
    
    subgraph "Data Retrieval"
        ParsePage --> BlockCache{"Block in Cache?"}
        BlockCache -- No --> FetchBlock["Read Compressed Block from Store"]
        FetchBlock --> Decompress["Decompress & Verify"]
        Decompress --> StoreCache["Store in Block Cache"]
        StoreCache --> Assemble
        BlockCache -- Yes --> Assemble["Assemble Data Slice"]
    end
    
    Assemble --> Return["Return Data to API/UI"]
```

## Deduplication Logic

Deduplication occurs during the write path and is transparent to the reader. By hashing compressed blocks and maintaining a mapping of hashes to file offsets, Hexz ensures that identical content is only stored once. Multiple index entries can point to the same physical data block, significantly reducing storage requirements for redundant data like OS images or repetitive logs.

```mermaid
sequenceDiagram
    participant Writer as Hexz Writer
    participant DedupMap as Hash-to-Offset Map
    participant File as .hxz Storage
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

## Chunking Strategy (Fixed vs. CDC)

Hexz supports both fixed-size chunking and Content-Defined Chunking (CDC). Fixed chunking splits data at regular intervals (e.g., every 64KB), while CDC uses a rolling hash to find "cut points" based on the data's content. CDC is superior for deduplication because small insertions shift boundaries in fixed chunking (breaking all subsequent blocks), whereas CDC resynchronizes, preserving matching blocks.

```mermaid
graph TD
    subgraph "Fixed Chunking"
        F_Data["Data Stream"]
        F_Data --> F_Split["Split at 64KB, 128KB..."]
        F_Split --> F_B1["Block 1"]
        F_Split --> F_B2["Block 2"]
        F_Split --> F_B3["Block 3"]
        
        F_Insert["Insertion at start"] -.->|Shifts all boundaries| F_B1_New["New Block 1"]
        F_Insert -.-> F_B2_New["New Block 2"]
        F_Insert -.-> F_B3_New["New Block 3"]
    end

    subgraph "Content-Defined Chunking (CDC)"
        C_Data["Data Stream"]
        C_Data --> C_Scan["Scan for Cut Hash"]
        C_Scan --> C_B1["Block A"]
        C_Scan --> C_B2["Block B"]
        C_Scan --> C_B3["Block C"]
        
        C_Insert["Insertion at start"] -.->|Only first block changes| C_B1_New["New Block A'"]
        C_Insert -.-> C_B2_Same["Block B (Same!)"]
        C_Insert -.-> C_B3_Same["Block C (Same!)"]
    end
```

## FUSE Overlay (Copy-On-Write)

When a Hexz archive is mounted with `--overlay`, it presents a writable filesystem. Since the `.hxz` file is immutable, writes are redirected to a temporary overlay file. A metadata map tracks which 4KB blocks have been modified. Read requests first check this map; if the block is modified, it is read from the overlay; otherwise, it is fetched from the base `.hxz` file.

```mermaid
flowchart TD
    Request["I/O Request<br/>(Offset, Length)"] --> Type{"Read or Write?"}
    
    Type -- Write --> WriteOverlay["Write to Overlay File"]
    WriteOverlay --> UpdateMeta["Update .meta Map"]
    
    Type -- Read --> CheckMeta{"Block in Overlay?"}
    
    CheckMeta -- Yes --> ReadOverlay["Read from Overlay File"]
    ReadOverlay --> Return
    
    CheckMeta -- No --> ReadBase["Read from Base .hxz File"]
    ReadBase --> Return["Return Data"]
```
