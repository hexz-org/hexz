//! File signature, magic bytes, and header size constants for Hexz archives.
//!
//! This module defines the fundamental constants that identify and structure
//! Hexz archive files (`.hxz`). These values form the first line of defense
//! against file corruption and format misidentification, enabling readers to
//! quickly reject invalid files before attempting deserialization.
//!
//! # File Format Overview
//!
//! Every Hexz archive file has the following fixed structure:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │ Byte 0-3: Magic Bytes ("HEXZ")                              │
//! ├─────────────────────────────────────────────────────────────┤
//! │ Byte 4-4095: Header (bincode-serialized Header)             │
//! │   - version: u32                                             │
//! │   - block_size: u32                                          │
//! │   - index_offset: u64                                        │
//! │   - compression: CompressionType                             │
//! │   - features: FeatureFlags                                   │
//! │   - optional fields (encryption, parent, etc.)               │
//! ├─────────────────────────────────────────────────────────────┤
//! │ Byte 4096+: Compressed block data                            │
//! │ ...                                                          │
//! │ Index pages                                                  │
//! │ Master index (at header.index_offset)                        │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Magic Bytes Rationale
//!
//! The 4-byte signature `HEXZ` (ASCII: 0x48 0x45 0x58 0x5A) serves multiple purposes:
//!
//! ## Immediate Format Validation
//!
//! Readers can detect non-Hexz files with a single 4-byte read before
//! attempting any deserialization, preventing crashes or misinterpretation:
//!
//! ```rust,no_run
//! # use hexz_core::format::magic::MAGIC_BYTES;
//! # use hexz_common::Error;
//! # use std::io::Read;
//! # fn validate_magic<R: Read>(mut file: R) -> Result<(), Box<dyn std::error::Error>> {
//! let mut magic = [0u8; 4];
//! file.read_exact(&mut magic)?;
//! if &magic != MAGIC_BYTES {
//!     return Err(Error::Format("Invalid magic bytes".into()).into());
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Corruption Detection
//!
//! If the magic bytes are corrupted, the file is likely unrecoverable and
//! should be rejected immediately rather than attempting to parse garbage data.
//!
//! ## File Type Identification
//!
//! Operating systems and tools (e.g., `file(1)`) can identify `.hxz` files
//! by searching for the `HEXZ` signature, even if the file extension is wrong.
//!
//! ## Endianness Independence
//!
//! The ASCII signature avoids byte-order ambiguity. Unlike a numeric magic number
//! (e.g., `0x4845585A`), the byte sequence is identical on little-endian and
//! big-endian systems.
//!
//! # Header Size Calculation
//!
//! The header is fixed at 4096 bytes (4 KB) for several reasons:
//!
//! ## Alignment Benefits
//!
//! - **Page alignment**: Matches common OS page size (4096 bytes on x86/ARM)
//! - **Block alignment**: Compatible with 4KB block storage devices
//! - **DMA efficiency**: Hardware I/O transfers work optimally on page boundaries
//!
//! ## Padding Strategy
//!
//! The actual serialized [`Header`] is typically 200-500 bytes. The
//! remaining space is zero-padded, providing:
//!
//! - **Forward compatibility**: New header fields can be added without changing
//!   the header size, preserving alignment properties
//! - **Metadata expansion**: Optional fields (encryption params, signatures) fit
//!   within the fixed 4096-byte envelope
//!
//! ## Read Performance
//!
//! Fixed-size headers enable predictable I/O patterns:
//!
//! ```rust,no_run
//! # use hexz_core::format::magic::HEADER_SIZE;
//! # use hexz_core::format::header::Header;
//! # use std::io::Read;
//! # fn read_header<R: Read>(mut file: R) -> Result<(), Box<dyn std::error::Error>> {
//! // Single aligned read for header
//! let mut header_buf = vec![0u8; HEADER_SIZE];
//! file.read_exact(&mut header_buf)?;
//! let header: Header = bincode::deserialize(&header_buf[4..])?;
//! # Ok(())
//! # }
//! ```
//!
//! # Backward Compatibility Guarantee
//!
//! These constants are **immutable** across all Hexz versions:
//!
//! - `MAGIC_BYTES` must always be `b"HEXZ"` (changing this creates a new file format)
//! - `HEADER_SIZE` must always be `4096` (changing this breaks offset calculations)
//!
//! The [`FORMAT_VERSION`] constant, however, **can and will change** to indicate
//! format evolution. Version checking logic is in [`crate::format::version`].
//!
//! # Security Considerations
//!
//! ## Magic Byte Spoofing
//!
//! An attacker could create a malicious file with valid magic bytes but corrupted
//! or adversarial header data. Defenses include:
//!
//! - **Version checking**: Reject unknown versions (see [`crate::format::version::check_version`])
//! - **Checksum verification**: Validate block checksums before decompression
//! - **Bounds checking**: Ensure all offsets/lengths are within file size
//!
//! ## Header Parsing Robustness
//!
//! The bincode deserializer must handle truncated, oversized, or malformed headers
//! gracefully. Always deserialize with size limits:
//!
//! ```rust,no_run
//! # use hexz_core::format::magic::HEADER_SIZE;
//! # use hexz_core::format::header::Header;
//! # fn decode_header(header_buf: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
//! let header: Header = bincode::deserialize(header_buf)?;
//! # Ok(())
//! # }
//! ```
//!
//! # File Type Registration
//!
//! For integration with system file-type databases:
//!
//! ## MIME Type (Proposed)
//!
//! ```text
//! application/x-hexz-archive
//! ```
//!
//! ## Magic Database Entry (`/etc/magic`)
//!
//! ```text
//! 0       string  HEXZ            Hexz archive file
//! >4      ulelong x               \b, version %d
//! ```
//!
//! ## File Extension
//!
//! The conventional extension is `.hxz`, though the format does not require it.
//!
//! # Examples
//!
//! ## Validating Magic Bytes
//!
//! ```
//! use hexz_core::format::magic::MAGIC_BYTES;
//!
//! let file_header = b"HEXZ..."; // First bytes of a file
//! assert_eq!(&file_header[..4], MAGIC_BYTES);
//! ```
//!
//! ## Header Offset Calculation
//!
//! ```
//! use hexz_core::format::magic::HEADER_SIZE;
//!
//! // First compressed block starts immediately after header
//! let first_block_offset = HEADER_SIZE;
//! assert_eq!(first_block_offset, 4096);
//! ```
//!
//! ## File Format Detection
//!
//! ```rust,no_run
//! # use std::path::Path;
//! # use std::fs::File;
//! # use std::io::Read;
//! # use hexz_core::format::magic::MAGIC_BYTES;
//! fn is_hexz_file(path: &Path) -> std::io::Result<bool> {
//!     let mut file = File::open(path)?;
//!     let mut magic = [0u8; 4];
//!     file.read_exact(&mut magic)?;
//!     Ok(&magic == MAGIC_BYTES)
//! }
//! ```
//!
//! ## Reader Implementation
//!
//! ```rust,no_run
//! # use std::fs::File;
//! # use std::io::Read;
//! # use hexz_core::format::magic::{MAGIC_BYTES, HEADER_SIZE, FORMAT_VERSION};
//! # use hexz_core::format::header::Header;
//! # use hexz_common::Error;
//! fn read_header(mut file: &File) -> Result<Header, Error> {
//!     // Read full header region (magic + serialized header)
//!     let mut buf = vec![0u8; HEADER_SIZE];
//!     file.read_exact(&mut buf)?;
//!
//!     // Validate magic bytes
//!     if &buf[0..4] != MAGIC_BYTES {
//!         return Err(Error::Format("Invalid magic bytes".into()));
//!     }
//!
//!     // Deserialize header (bytes 4..4096)
//!     let header: Header = bincode::deserialize(&buf[4..])?;
//!
//!     // Validate version
//!     if header.version != FORMAT_VERSION {
//!         return Err(Error::Format(format!("Unsupported version: {}", header.version)));
//!     }
//!
//!     Ok(header)
//! }
//! ```
//!
//! [`Header`]: crate::format::header::Header

/// File signature identifying Hexz archive files.
///
/// This 4-byte constant (`HEXZ` in ASCII, 0x48 0x45 0x58 0x5A in hex) appears
/// at the beginning of every valid `.hxz` file. Readers must validate this
/// signature before attempting to parse the rest of the header.
///
/// # Rationale
///
/// - **ASCII-readable**: Easy to identify in hex dumps (`48 45 58 5A` = "HEXZ")
/// - **Low collision probability**: Unlikely to appear at offset 0 in non-Hexz files
/// - **Endian-neutral**: Byte sequence is identical regardless of CPU byte order
/// - **Mnemonic**: "HEXZ" identifies the project and format
///
/// # Validation Example
///
/// ```
/// use hexz_core::format::magic::MAGIC_BYTES;
///
/// let file_start = b"HEXZ\x01\x00\x00\x00..."; // First bytes of a file
/// if &file_start[..4] == MAGIC_BYTES {
///     println!("Valid Hexz file");
/// } else {
///     eprintln!("Not a Hexz file");
/// }
/// ```
///
/// # Error Handling
///
/// If magic bytes do not match, the file is either:
/// - Not a Hexz archive (e.g., wrong file type)
/// - Corrupted (e.g., truncated, damaged sectors)
/// - Generated by incompatible software (e.g., future format with different signature)
///
/// In all cases, reject the file immediately without attempting deserialization.
pub const MAGIC_BYTES: &[u8; 4] = b"HEXZ";

/// Format version number for archives written by this build.
///
/// This constant is written to the `version` field of new archive headers and
/// defines the on-disk format structure. When the format changes incompatibly
/// (e.g., new index layout, different serialization), this value is incremented.
///
/// # Current Version
///
/// **Version 1**: Initial Hexz format with:
/// - Two-level index (master index + paginated block metadata)
/// - bincode serialization for headers and indices
/// - LZ4 and Zstd compression support
/// - Optional AES-256-GCM encryption
/// - Thin provisioning via parent archive references
/// - Dual streams (main + auxiliary)
///
/// # Version History
///
/// | Version | Hexz Release | Key Changes |
/// |---------|--------------|-------------|
/// | 1       | 0.1.0        | Initial format |
///
/// # Version Checking
///
/// This constant defines what version is **written**. For logic on what versions
/// are **readable**, see [`crate::format::version::MIN_SUPPORTED_VERSION`] and
/// [`crate::format::version::MAX_SUPPORTED_VERSION`].
///
/// # Examples
///
/// ```
/// use hexz_core::format::magic::FORMAT_VERSION;
/// use hexz_core::format::header::Header;
///
/// let mut header = Header::default();
/// assert_eq!(header.version, FORMAT_VERSION);
/// assert_eq!(FORMAT_VERSION, 1);
/// ```
pub const FORMAT_VERSION: u32 = 1;

/// Size of the fixed header region at the start of archive files.
///
/// Every Hexz archive begins with a 4096-byte (4 KB) header containing:
/// - Magic bytes (4 bytes)
/// - Serialized [`Header`] structure (variable, typically 200-500 bytes)
/// - Zero-padding to fill remaining space
///
/// This size is **immutable** across all format versions. Changing it would
/// break offset calculations for existing readers.
///
/// # Alignment Properties
///
/// The 4096-byte size provides:
///
/// - **Page alignment**: Matches OS page size on x86, ARM, and most architectures
/// - **Block alignment**: Compatible with 4KB physical sector size (Advanced Format drives)
/// - **Cache efficiency**: Entire header fits in a single cache line or TLB entry
/// - **DMA optimization**: Hardware I/O controllers transfer page-aligned data efficiently
///
/// # Layout Within Header Region
///
/// ```text
/// Offset | Size  | Contents
/// -------|-------|--------------------------------------------------
/// 0      | 4     | Magic bytes (b"HEXZ")
/// 4      | ~400  | Serialized Header (bincode format)
/// ~404   | ~3692 | Zero-padding (reserved for future extensions)
/// 4096   | ...   | Start of compressed block data
/// ```
///
/// # Forward Compatibility
///
/// The large fixed size allows future format versions to add header fields
/// without changing the header size or breaking alignment. New fields are
/// serialized within the existing 4096-byte envelope.
///
/// # Performance Characteristics
///
/// Reading the header requires a single I/O operation:
///
/// - **SSD latency**: ~50-100 μs (single 4KB read)
/// - **HDD latency**: ~5-10 ms (seek + single 4KB read)
/// - **Network latency**: Depends on RTT, but single request avoids round-trips
///
/// # Size Rationale
///
/// Why not 512 bytes (classic sector size)?
/// - Modern serialized headers with encryption/compression metadata exceed 512 bytes
/// - Future extensions would require format version bump
///
/// Why not 8192 bytes (2 pages)?
/// - Wastes space for typical headers (~400 bytes serialized)
/// - Doubles read latency on high-latency backends (e.g., network storage)
///
/// 4096 bytes balances space efficiency and forward compatibility.
///
/// # Examples
///
/// ## Reading the Header
///
/// ```rust,no_run
/// # use std::fs::File;
/// # use std::io::Read;
/// # use hexz_core::format::magic::HEADER_SIZE;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut file = File::open("archive.hxz")?;
/// let mut header_buf = vec![0u8; HEADER_SIZE];
/// file.read_exact(&mut header_buf)?;
/// # Ok(())
/// # }
/// ```
///
/// ## Calculating Block Offsets
///
/// ```
/// use hexz_core::format::magic::HEADER_SIZE;
///
/// // First block is written immediately after header
/// let first_block_offset = HEADER_SIZE;
/// assert_eq!(first_block_offset, 4096);
///
/// // Second block offset (assuming first block is 2048 bytes compressed)
/// let second_block_offset = HEADER_SIZE + 2048;
/// assert_eq!(second_block_offset, 6144);
/// ```
///
/// [`Header`]: crate::format::header::Header
pub const HEADER_SIZE: usize = 4096;
