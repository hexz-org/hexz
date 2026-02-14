//! Commit overlay and optional memory into a new snapshot.
//!
//! This command merges changes from a writable overlay (created during VM execution)
//! with a base snapshot to produce a new snapshot. It supports both "thick" snapshots
//! (standalone, contains all data) and "thin" snapshots (references parent for
//! unmodified blocks).

use anyhow::Result;
use hexz_common::constants::{META_ENTRY_SIZE, OVERLAY_BLOCK_SIZE};
use hexz_core::File as HexzFile;
use hexz_core::algo::compression::create_compressor_from_str;
use hexz_core::api::file::SnapshotStream;
use hexz_core::ops::snapshot_writer::SnapshotWriter;
use hexz_core::store::local::FileBackend;
use indicatif::ProgressBar;
use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::Arc;

/// Executes the commit command to merge overlay changes into a new snapshot.
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
    thin: bool,
) -> Result<()> {
    println!(
        "Committing changes from {:?} to {:?} (Thin: {})",
        overlay_path, output_path, thin
    );

    let backend = Arc::new(FileBackend::new(&base_path)?);
    let base_snap = HexzFile::open(backend, None)?;

    let meta_path = overlay_path.with_extension("meta");
    let mut modified_blocks = HashSet::new();

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

    let (write_compressor, compression_type) = create_compressor_from_str(&algo, None, None)?;

    let mut writer = SnapshotWriter::builder(&output_path, write_compressor, compression_type)
        .block_size(block_size)
        .variable_blocks(false)
        .build()?;

    let base_disk_size = base_snap.size(SnapshotStream::Disk);
    let overlay_len = overlay_file.metadata()?.len();
    let final_disk_size = std::cmp::max(base_disk_size, overlay_len);

    // --- Disk stream ---
    writer.begin_stream(true, final_disk_size);

    let pb = ProgressBar::new(final_disk_size);
    let bs = block_size as u64;
    let total_blocks = final_disk_size.div_ceil(bs);

    for i in 0..total_blocks {
        let block_start = i * bs;
        let mut block_len = bs;
        if block_start + block_len > final_disk_size {
            block_len = final_disk_size - block_start;
        }

        let start_ov_blk = block_start / OVERLAY_BLOCK_SIZE;
        let end_ov_blk = (block_start + block_len - 1) / OVERLAY_BLOCK_SIZE;
        let is_modified =
            (start_ov_blk..=end_ov_blk).any(|ov_blk| modified_blocks.contains(&ov_blk));

        // Thin: unmodified block in base → parent ref
        if thin && !is_modified && block_start < base_disk_size {
            writer.write_parent_ref(block_len as u32)?;
            pb.inc(block_len);
            continue;
        }

        // Build block data from base + overlay
        let mut data = vec![0u8; block_len as usize];

        if is_modified {
            if block_start < base_disk_size {
                let base_data =
                    base_snap.read_at(SnapshotStream::Disk, block_start, block_len as usize)?;
                data[..base_data.len()].copy_from_slice(&base_data);
            }

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
            let base_data =
                base_snap.read_at(SnapshotStream::Disk, block_start, block_len as usize)?;
            data[..base_data.len()].copy_from_slice(&base_data);
        }

        writer.write_data_block(&data)?;
        pb.inc(block_len);
    }

    writer.end_stream()?;

    // --- Memory stream ---
    if let Some(mem_path) = memory_path {
        println!("\nProcessing new memory state...");
        let mut mem_file = File::open(&mem_path)?;
        let mem_len = mem_file.metadata()?.len();

        writer.begin_stream(false, mem_len);

        let pb_mem = ProgressBar::new(mem_len);
        let mut buf = vec![0u8; block_size as usize];

        loop {
            let mut pos = 0;
            while pos < buf.len() {
                match mem_file.read(&mut buf[pos..]) {
                    Ok(0) => break,
                    Ok(n) => pos += n,
                    Err(e) => return Err(e.into()),
                }
            }
            if pos == 0 {
                break;
            }

            writer.write_data_block(&buf[..pos])?;
            pb_mem.inc(pos as u64);
        }

        writer.end_stream()?;
    }

    // --- Finalize ---
    let parent_path = if thin {
        Some(
            std::fs::canonicalize(&base_path)?
                .to_string_lossy()
                .to_string(),
        )
    } else {
        None
    };

    let meta_bytes = message.as_ref().map(|m| m.as_bytes());
    writer.finalize(parent_path, meta_bytes)?;

    if !keep_overlay {
        println!("Cleaning up overlay files...");
        let _ = std::fs::remove_file(&overlay_path);
        let _ = std::fs::remove_file(&meta_path);
    }

    println!("Commit complete: {:?}", output_path);
    Ok(())
}
