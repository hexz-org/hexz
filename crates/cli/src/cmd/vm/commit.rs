//! Commit overlay and optional memory into a new snapshot.
//!
//! This command merges changes from a writable overlay (created during VM execution)
//! with a base snapshot to produce a new snapshot. It supports both "thick" snapshots
//! (standalone, contains all data) and "thin" snapshots (references parent for
//! unmodified blocks), providing flexibility for different storage and deployment scenarios.
//!
//! # Thin vs. Thick Snapshots
//!
//! ## Thick Snapshots (Default, `--thin=false`)
//!
//! **Characteristics:**
//! - Contains all disk data (modified + unmodified blocks)
//! - Standalone file that does not require parent snapshot
//! - Larger file size but completely self-contained
//! - Can be moved, copied, or distributed independently
//!
//! **Use Cases:**
//! - Production deployments where parent may not be available
//! - Archival and long-term storage
//! - Sharing snapshots with other users/systems
//! - Migrating VMs to different infrastructure
//!
//! **Storage Cost:**
//! - File size ≈ compressed size of entire disk
//! - Example: 10 GB disk → 3-5 GB thick snapshot (with compression)
//!
//! ## Thin Snapshots (`--thin=true`)
//!
//! **Characteristics:**
//! - Contains only modified blocks + metadata referencing parent
//! - Requires parent snapshot to be accessible at runtime
//! - Much smaller file size for incremental changes
//! - Parent path stored in snapshot header (absolute path)
//!
//! **Use Cases:**
//! - Incremental backups and version control
//! - Development and testing (quick snapshots of state)
//! - Space-efficient snapshot chains
//! - Checkpoint and rollback workflows
//!
//! **Storage Cost:**
//! - File size ≈ compressed size of modified blocks only
//! - Example: 10 GB disk with 500 MB changes → ~200 MB thin snapshot
//!
//! **Parent Reference Mechanism:**
//! - Parent path stored in `StrataHeader.parent_path` field (absolute path)
//! - Unmodified blocks marked with `BLOCK_OFFSET_PARENT` sentinel value
//! - Read path resolves parent recursively at runtime
//!
//! # Commit Workflow
//!
//! 1. **Read Base Snapshot**: Opens parent snapshot and extracts header
//! 2. **Load Overlay Metadata**: Reads `.meta` file to get modified block list
//! 3. **Process Disk Blocks**: For each block in final disk size:
//!    - If thin mode + unmodified: Write parent reference marker
//!    - If modified: Read base, apply overlay, compress, write
//!    - If thick mode + unmodified: Read from base, compress, write
//! 4. **Process Memory**: If memory dump provided, compress and append
//! 5. **Write Master Index**: Serialize index with all page entries
//! 6. **Update Header**: Write header with parent path (thin) or None (thick)
//! 7. **Clean Up**: Optionally delete overlay files
//!
//! # Block Processing Logic
//!
//! ```text
//! For each block in final disk:
//!   is_modified = block overlaps modified 4K chunks in .meta file
//!
//!   if thin AND not is_modified AND exists_in_base:
//!     write BLOCK_OFFSET_PARENT marker  // Reference parent
//!   else:
//!     if is_modified:
//!       data = base[block] + overlay[block]  // Merge changes
//!     else:  // Thick mode
//!       data = base[block]                   // Copy from base
//!     compress and write data
//! ```
//!
//! # Overlay Format
//!
//! **Overlay File (`.overlay`):**
//! - Sparse file containing modified 4 KiB chunks
//! - Chunks written at their logical disk offsets
//! - Unmodified regions remain as holes (zero-filled)
//!
//! **Metadata File (`.meta`):**
//! - Array of `u64` block indices (8 bytes each)
//! - Each entry is a 4 KiB block number that was written
//! - Sorted order for efficient lookup
//!
//! # Compression and Deduplication
//!
//! Committed blocks are:
//! 1. **Zero-Detected**: All-zero blocks stored as metadata only (no data written)
//! 2. **Compressed**: Non-zero blocks compressed with specified algorithm (LZ4 or Zstd)
//! 3. **Checksummed**: Each block includes CRC32 checksum for integrity verification
//!
//! # Common Usage Patterns
//!
//! ```bash
//! # Create thick (standalone) snapshot after VM modifications
//! strata vm commit \
//!   --base vm-base.st \
//!   --overlay vm-state.overlay \
//!   --output vm-updated.st
//!
//! # Create thin (incremental) snapshot for space efficiency
//! strata vm commit \
//!   --base vm-base.st \
//!   --overlay vm-state.overlay \
//!   --output vm-incremental.st \
//!   --thin
//!
//! # Commit with memory dump (from live snapshot)
//! strata vm commit \
//!   --base vm-base.st \
//!   --overlay vm-state.overlay \
//!   --memory vm-memory.dump \
//!   --output vm-checkpoint.st
//!
//! # Keep overlay after commit (for debugging)
//! strata vm commit \
//!   --base vm-base.st \
//!   --overlay vm-state.overlay \
//!   --output vm-new.st \
//!   --keep-overlay
//! ```

use anyhow::Result;
use indicatif::ProgressBar;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::Arc;
use strata_common::constants::{BLOCK_OFFSET_PARENT, DEFAULT_ZSTD_LEVEL};
use strata_core::StrataFile;
use strata_core::algo::compression::{Compressor, lz4::Lz4Compressor, zstd::ZstdCompressor};
use strata_core::api::stratafile::SnapshotStream;
use strata_core::format::{
    header::{CompressionType, FeatureFlags, StrataHeader},
    index::{BlockInfo, ENTRIES_PER_PAGE, IndexPage, MasterIndex, PageEntry},
    magic::{FORMAT_VERSION, HEADER_SIZE, MAGIC_BYTES},
};
use strata_core::store::StorageBackend;
use strata_core::store::local::FileBackend;

const OVERLAY_BLOCK_SIZE: u64 = 4096;
const META_ENTRY_SIZE: usize = 8;

/// Executes the commit command to merge overlay changes into a new snapshot.
///
/// Reads the base snapshot and overlay file to create a new snapshot containing
/// either all disk data (thick mode) or only modified blocks with parent references
/// (thin mode). Optionally includes a memory dump for full VM state preservation.
///
/// # Arguments
///
/// * `base_path` - Path to the base `.st` snapshot file
/// * `overlay_path` - Path to the overlay file containing modified blocks
/// * `memory_path` - Optional path to memory dump file (from live snapshot or migration)
/// * `output_path` - Path for the output snapshot file
/// * `algo` - Compression algorithm: "lz4" (fast) or "zstd" (higher ratio)
/// * `block_size` - Block size in bytes for chunking (typically 64 KiB)
/// * `keep_overlay` - If true, preserve overlay files after commit; otherwise delete
/// * `message` - Optional metadata message to embed in snapshot header
/// * `thin` - If true, create thin snapshot with parent references; otherwise thick
///
/// # Thin vs. Thick Behavior
///
/// **Thin Mode (`thin=true`):**
/// - Unmodified blocks: Write `BLOCK_OFFSET_PARENT` marker (no data)
/// - Modified blocks: Write compressed data as usual
/// - Header: Set `parent_path` to absolute path of `base_path`
/// - Result: Small snapshot dependent on parent
///
/// **Thick Mode (`thin=false`):**
/// - Unmodified blocks: Read from base, compress, write
/// - Modified blocks: Merge base + overlay, compress, write
/// - Header: Set `parent_path` to `None`
/// - Result: Large standalone snapshot
///
/// # Block Processing Algorithm
///
/// For each block in the final disk size:
/// 1. Determine if block is modified (check overlay metadata)
/// 2. Apply thin/thick logic:
///    - Thin + unmodified + in base → write parent reference
///    - Modified → read base, apply overlay patches, compress
///    - Thick + unmodified → read base, compress
/// 3. Write compressed data or reference marker to output
/// 4. Update index page with block metadata
///
/// # Memory Handling
///
/// Memory dumps are always stored in thick mode (no parent references) because:
/// - Memory state changes drastically between snapshots
/// - Parent memory state is rarely useful for incremental storage
/// - Simplifies implementation and reduces complexity
///
/// # Overlay Cleanup
///
/// If `keep_overlay=false` (default):
/// - Deletes overlay file after successful commit
/// - Deletes metadata file (`.meta`)
/// - Preserves original files if commit fails
///
/// # Errors
///
/// Returns an error if:
/// - Base snapshot cannot be opened or read
/// - Overlay or metadata files cannot be read
/// - Output file cannot be created or written
/// - Compression or serialization fails
/// - Disk I/O errors occur during processing
///
/// # Performance Characteristics
///
/// - **Thin commit**: Processes only modified blocks (~200-500 MB/s)
/// - **Thick commit**: Processes entire disk (~200-500 MB/s depending on compression)
/// - **With memory**: Adds memory size / compression throughput to total time
/// - **Progress bar**: Updates in real-time showing bytes processed
///
/// # Examples
///
/// ```no_run
/// use std::path::PathBuf;
/// use strata_cli::cmd::vm::commit;
///
/// // Create thick snapshot with LZ4 compression
/// commit::run(
///     PathBuf::from("base.st"),
///     PathBuf::from("changes.overlay"),
///     None,
///     PathBuf::from("updated.st"),
///     "lz4".to_string(),
///     65536,     // 64 KiB blocks
///     false,     // delete overlay
///     None,      // no message
///     false,     // thick mode
/// )?;
///
/// // Create thin snapshot with Zstd and memory
/// commit::run(
///     PathBuf::from("base.st"),
///     PathBuf::from("state.overlay"),
///     Some(PathBuf::from("memory.dump")),
///     PathBuf::from("checkpoint.st"),
///     "zstd".to_string(),
///     65536,
///     true,      // keep overlay
///     Some("Checkpoint before upgrade".to_string()),
///     true,      // thin mode
/// )?;
/// # Ok::<(), anyhow::Error>(())
/// ```
#[allow(clippy::too_many_arguments)]
pub fn run(
    base_path: PathBuf,
    overlay_path: PathBuf,
    memory_path: Option<PathBuf>,
    output_path: PathBuf,
    algo: String,
    block_size: u32,
    keep_overlay: bool,
    message: Option<String>,
    thin: bool, // NEW FLAG
) -> Result<()> {
    println!(
        "Committing changes from {:?} to {:?} (Thin: {})",
        overlay_path, output_path, thin
    );

    let backend = Arc::new(FileBackend::new(&base_path)?);
    let header_bytes = backend.read_exact(0, HEADER_SIZE)?;
    let header: StrataHeader = bincode::deserialize(&header_bytes)?;

    let read_compressor: Box<dyn Compressor> = match header.compression {
        CompressionType::Lz4 => Box::new(Lz4Compressor::new()),
        CompressionType::Zstd => {
            let dict = if let (Some(off), Some(len)) =
                (header.dictionary_offset, header.dictionary_length)
            {
                Some(backend.read_exact(off, len as usize)?.to_vec())
            } else {
                None
            };
            Box::new(ZstdCompressor::new(DEFAULT_ZSTD_LEVEL, dict))
        }
    };

    let base_snap = Arc::new(StrataFile::new(backend, read_compressor, None)?);

    let meta_path = overlay_path.with_extension("meta");
    let mut modified_blocks = std::collections::HashSet::new();

    if meta_path.exists() {
        let mut f = File::open(&meta_path)?;
        let mut buf = [0u8; META_ENTRY_SIZE];
        loop {
            match f.read_exact(&mut buf) {
                Ok(_) => {
                    modified_blocks.insert(u64::from_le_bytes(buf));
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            }
        }
    }

    let mut overlay_file = File::open(&overlay_path)?;

    let mut out = File::create(&output_path)?;
    out.write_all(&[0u8; HEADER_SIZE])?;
    let mut current_offset = HEADER_SIZE;

    let write_compressor: Box<dyn Compressor> = match algo.as_str() {
        "zstd" => Box::new(ZstdCompressor::new(DEFAULT_ZSTD_LEVEL, None)),
        _ => Box::new(Lz4Compressor::new()),
    };

    let mut master = MasterIndex::default();
    let mut global_block_idx = 0;

    let base_disk_size = base_snap.size(SnapshotStream::Disk);
    let overlay_len = overlay_file.metadata()?.len();
    let final_disk_size = std::cmp::max(base_disk_size, overlay_len);
    master.disk_size = final_disk_size;

    let pb = ProgressBar::new(final_disk_size);
    let mut page = IndexPage::default();
    let mut page_start_block = 0;
    let mut page_start_logical = 0u64;
    let mut current_logical_pos = 0u64;

    let bs = block_size as u64;
    let total_blocks = final_disk_size.div_ceil(bs);

    for i in 0..total_blocks {
        let block_start = i * bs;
        let mut block_len = bs;
        if block_start + block_len > final_disk_size {
            block_len = final_disk_size - block_start;
        }

        let mut is_modified = false;
        let start_ov_blk = block_start / OVERLAY_BLOCK_SIZE;
        let end_ov_blk = (block_start + block_len - 1) / OVERLAY_BLOCK_SIZE;

        for ov_blk in start_ov_blk..=end_ov_blk {
            if modified_blocks.contains(&ov_blk) {
                is_modified = true;
                break;
            }
        }

        // THIN PROVISIONING LOGIC
        if thin && !is_modified && block_start < base_disk_size {
            // If thin mode, not modified, and exists in base:
            // Write a marker pointing to parent.
            page.blocks.push(BlockInfo {
                offset: BLOCK_OFFSET_PARENT,
                length: 0,
                logical_len: block_len as u32,
                checksum: 0,
            });

            global_block_idx += 1;
            current_logical_pos += block_len;
            pb.inc(block_len);

            if page.blocks.len() >= ENTRIES_PER_PAGE {
                flush_page(
                    &mut page,
                    &mut out,
                    &mut current_offset,
                    &mut master.disk_pages,
                    page_start_block,
                    page_start_logical,
                )?;
                page_start_block = global_block_idx;
                page_start_logical = current_logical_pos;
            }
            continue;
        }

        // Standard Logic (Thick or Modified)
        let mut data = vec![0u8; block_len as usize];

        if is_modified {
            // Read base first for partial updates
            if block_start < base_disk_size {
                let base_data =
                    base_snap.read_at(SnapshotStream::Disk, block_start, block_len as usize)?;
                data[..base_data.len()].copy_from_slice(&base_data);
            }

            // Apply overlay
            for ov_blk in start_ov_blk..=end_ov_blk {
                if modified_blocks.contains(&ov_blk) {
                    let chunk_start = ov_blk * OVERLAY_BLOCK_SIZE;
                    let chunk_end =
                        std::cmp::min(chunk_start + OVERLAY_BLOCK_SIZE, final_disk_size);
                    let chunk_len = (chunk_end - chunk_start) as usize;
                    let rel_start = (chunk_start - block_start) as usize;

                    overlay_file.seek(SeekFrom::Start(chunk_start))?;
                    let _ = overlay_file.read(&mut data[rel_start..rel_start + chunk_len])?;
                }
            }
        } else if block_start < base_disk_size {
            // Not modified, but thick mode: Copy from base
            let base_data =
                base_snap.read_at(SnapshotStream::Disk, block_start, block_len as usize)?;
            data[..base_data.len()].copy_from_slice(&base_data);
        }

        if data.iter().all(|&b| b == 0) {
            page.blocks.push(BlockInfo {
                offset: 0,
                length: 0,
                logical_len: block_len as u32,
                checksum: 0,
            });
        } else {
            let compressed = write_compressor.compress(&data)?;
            let checksum = crc32fast::hash(&compressed);

            out.write_all(&compressed)?;
            page.blocks.push(BlockInfo {
                offset: current_offset as u64,
                length: compressed.len() as u32,
                logical_len: block_len as u32,
                checksum,
            });
            current_offset += compressed.len();
        }

        global_block_idx += 1;
        current_logical_pos += block_len;
        pb.inc(block_len);

        if page.blocks.len() >= ENTRIES_PER_PAGE {
            flush_page(
                &mut page,
                &mut out,
                &mut current_offset,
                &mut master.disk_pages,
                page_start_block,
                page_start_logical,
            )?;
            page_start_block = global_block_idx;
            page_start_logical = current_logical_pos;
        }
    }

    if !page.blocks.is_empty() {
        flush_page(
            &mut page,
            &mut out,
            &mut current_offset,
            &mut master.disk_pages,
            page_start_block,
            page_start_logical,
        )?;
    }

    // Handle Memory (Always thick for now, as memory changes drastically)
    if let Some(mem_path) = memory_path {
        println!("\nProcessing new memory state...");
        let mut mem_file = File::open(&mem_path)?;
        let mem_len = mem_file.metadata()?.len();
        master.memory_size = mem_len;

        let pb_mem = ProgressBar::new(mem_len);
        let mut page = IndexPage::default();
        let mut page_start_block = global_block_idx;
        let mut page_start_logical = 0u64;
        let mut current_mem_logical = 0u64;

        let mut buf = vec![0u8; block_size as usize];

        loop {
            let n = mem_file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            let chunk = &buf[..n];

            if chunk.iter().all(|&b| b == 0) {
                page.blocks.push(BlockInfo {
                    offset: 0,
                    length: 0,
                    logical_len: n as u32,
                    checksum: 0,
                });
            } else {
                let compressed = write_compressor.compress(chunk)?;
                let checksum = crc32fast::hash(&compressed);

                out.write_all(&compressed)?;
                page.blocks.push(BlockInfo {
                    offset: current_offset as u64,
                    length: compressed.len() as u32,
                    logical_len: n as u32,
                    checksum,
                });
                current_offset += compressed.len();
            }

            global_block_idx += 1;
            current_mem_logical += n as u64;
            pb_mem.inc(n as u64);

            if page.blocks.len() >= ENTRIES_PER_PAGE {
                flush_page(
                    &mut page,
                    &mut out,
                    &mut current_offset,
                    &mut master.memory_pages,
                    page_start_block,
                    page_start_logical,
                )?;
                page_start_block = global_block_idx;
                page_start_logical = current_mem_logical;
            }
        }

        if !page.blocks.is_empty() {
            flush_page(
                &mut page,
                &mut out,
                &mut current_offset,
                &mut master.memory_pages,
                page_start_block,
                page_start_logical,
            )?;
        }
    } else {
        master.memory_size = 0;
        master.memory_pages.clear();
    }

    let index_offset = current_offset as u64;
    let index_bytes = bincode::serialize(&master)?;
    out.write_all(&index_bytes)?;

    let (metadata_offset, metadata_length) = if let Some(msg) = message {
        let msg_bytes = msg.as_bytes();
        let off = current_offset as u64 + index_bytes.len() as u64;
        out.write_all(msg_bytes)?;
        (Some(off), Some(msg_bytes.len() as u32))
    } else {
        (None, None)
    };

    // Determine parent path for header
    let parent_path = if thin {
        // Store absolute path to base for v1 simplicity
        Some(
            std::fs::canonicalize(&base_path)?
                .to_string_lossy()
                .to_string(),
        )
    } else {
        None
    };

    let new_header = StrataHeader {
        magic: *MAGIC_BYTES,
        version: FORMAT_VERSION,
        block_size,
        index_offset,
        parent_path, // Set parent path
        dictionary_offset: None,
        dictionary_length: None,
        metadata_offset,
        metadata_length,
        signature_offset: None,
        signature_length: None,
        encryption: None,
        compression: if algo == "zstd" {
            CompressionType::Zstd
        } else {
            CompressionType::Lz4
        },
        features: FeatureFlags {
            has_disk: !master.disk_pages.is_empty(),
            has_memory: !master.memory_pages.is_empty(),
            variable_blocks: false,
        },
    };

    out.seek(SeekFrom::Start(0))?;
    out.write_all(&bincode::serialize(&new_header)?)?;

    if !keep_overlay {
        println!("Cleaning up overlay files...");
        let _ = std::fs::remove_file(&overlay_path);
        let _ = std::fs::remove_file(&meta_path);
    }

    println!("Commit complete: {:?}", output_path);
    Ok(())
}

fn flush_page(
    page: &mut IndexPage,
    out: &mut File,
    current_offset: &mut usize,
    pages_vec: &mut Vec<PageEntry>,
    start_block: u64,
    start_logical: u64,
) -> Result<()> {
    let bytes = bincode::serialize(&page)?;
    let p_off = *current_offset;
    out.write_all(&bytes)?;
    *current_offset += bytes.len();
    pages_vec.push(PageEntry {
        offset: p_off as u64,
        length: bytes.len() as u32,
        start_block,
        start_logical,
    });
    *page = IndexPage::default();
    Ok(())
}
