//! Algorithms for compression, encryption, hashing, and deduplication.
//!
//! This module provides the algorithmic foundation for Hexz's snapshot format,
//! organizing all cryptographic and data reduction primitives behind pluggable
//! traits. Each submodule defines an abstract interface and concrete implementations
//! that can be swapped independently without affecting higher layers.
//!
//! # Architecture
//!
//! The algorithm layer follows a **trait-based plugin architecture**:
//!
//! ```text
//! ┌─────────────┐
//! │ Format Layer│ Uses traits, not concrete types
//! └──────┬──────┘
//!        │
//! ┌──────┴──────────────────────────────────┐
//! │  Algorithm Traits (this module)         │
//! │  - Compressor  - Encryptor              │
//! │  - ContentHasher  - StreamChunker       │
//! └──────┬──────────────────────────────────┘
//!        │
//! ┌──────┴───────────────────────────────────┐
//! │  Concrete Implementations (submodules)   │
//! │  - LZ4, Zstandard                        │
//! │  - AES-256-GCM                           │
//! │  - BLAKE3                                │
//! │  - FastCDC                               │
//! └──────────────────────────────────────────┘
//! ```
//!
//! # Available Algorithms
//!
//! ## Compression ([`compression`])
//! - **LZ4**: Fast compression, ~500 MB/s compress, ~2 GB/s decompress
//! - **Zstandard**: Balanced compression, better ratios, dictionary support
//!
//! ## Encryption ([`encryption`])
//! - **AES-256-GCM**: Authenticated encryption with per-block nonces
//!
//! ## Hashing ([`hashing`])
//! - **BLAKE3**: Fast cryptographic hash for content addressing
//!
//! ## Deduplication ([`dedup`])
//! - **FastCDC**: Content-defined chunking for variable-sized blocks
//! - **DCAM**: Analytical model for optimizing CDC parameters
//!
//! # Design Principles
//!
//! 1. **Trait Abstraction**: Format code depends only on traits, not implementations
//! 2. **Thread Safety**: All algorithms are `Send + Sync` for parallel access
//! 3. **Stateless**: Algorithms are pure functions (or use internal synchronization)
//! 4. **Composability**: Algorithms can be combined independently
//!
//! # Usage Example
//!
//! ```no_run
//! # use hexz_core::algo::compression::{Compressor, lz4::Lz4Compressor};
//! # use hexz_core::algo::encryption::{Encryptor, aes_gcm::AesGcmEncryptor};
//! // Compress and encrypt a block
//! let compressor = Lz4Compressor::new();
//! let encryptor = AesGcmEncryptor::new(b"password", b"salt12345678salt", 600_000);
//!
//! let data = b"Block data to compress and encrypt";
//! let compressed = compressor.compress(data)?;
//! let encrypted = encryptor.encrypt(&compressed, 0)?;
//! # Ok::<(), hexz_common::Error>(())
//! ```

/// Block compression codecs (LZ4, Zstd).
pub mod compression;

/// Per-block authenticated encryption (AES-256-GCM).
pub mod encryption;

/// Content-addressing hash functions.
pub mod hashing;

/// Deduplication algorithms (CDC, DCAM).
pub mod dedup;
