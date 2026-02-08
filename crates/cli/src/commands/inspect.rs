//! Introspection of existing Strata snapshot files.
//!
//! Reads the header and master index of a `.st` file and prints a
//! human-readable summary: format version, compression type, feature
//! flags, and storage statistics (disk/memory sizes, block counts).

use anyhow::{Context, Result};
use indicatif::HumanBytes;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use strata_core::format::header::StrataHeader;
use strata_core::format::index::MasterIndex;
use strata_core::format::magic::HEADER_SIZE;

/// Prints a human-readable summary of a Strata snapshot.
pub fn run(path: PathBuf) -> Result<()> {
    let mut f = File::open(&path).context("Failed to open snapshot file")?;
    let file_len = f.metadata()?.len();

    let mut header_bytes = [0u8; HEADER_SIZE];
    f.read_exact(&mut header_bytes)
        .context("Failed to read header")?;
    let header: StrataHeader =
        bincode::deserialize(&header_bytes).context("Invalid header format")?;

    f.seek(SeekFrom::Start(header.index_offset))
        .context("Failed to seek to index")?;
    let mut index_bytes = Vec::new();
    f.read_to_end(&mut index_bytes)
        .context("Failed to read master index")?;

    let master: MasterIndex =
        bincode::deserialize(&index_bytes).context("Invalid master index format")?;

    let total_uncompressed = master.disk_size + master.memory_size;
    let ratio = if file_len > 0 {
        total_uncompressed as f64 / file_len as f64
    } else {
        0.0
    };

    println!("Snapshot:       {:?}", path);
    println!("Format Version: {}", header.version);
    println!("Compression:    {:?}", header.compression);
    println!("Block Size:     {}", header.block_size);

    println!("\n--- Features ---");
    println!(
        "Encrypted:      {}",
        if header.encryption.is_some() {
            "Yes"
        } else {
            "No"
        }
    );
    println!(
        "Has Disk:       {}",
        if header.features.has_disk {
            "Yes"
        } else {
            "No"
        }
    );
    println!(
        "Has Memory:     {}",
        if header.features.has_memory {
            "Yes"
        } else {
            "No"
        }
    );
    println!(
        "Variable Blks:  {}",
        if header.features.variable_blocks {
            "Yes (CDC)"
        } else {
            "No"
        }
    );

    println!("\n--- Storage Statistics ---");
    println!("Original Size:  {}", HumanBytes(total_uncompressed));
    println!("Compressed:     {}", HumanBytes(file_len));
    println!("Ratio:          {:.2}x", ratio);

    println!("\n--- Index Details ---");
    println!("Index Offset:   {}", header.index_offset);
    println!("Disk Pages:     {}", master.disk_pages.len());
    println!("Memory Pages:   {}", master.memory_pages.len());

    Ok(())
}
