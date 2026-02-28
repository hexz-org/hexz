//! Safetensors → Hexz store and extract operations with XOR delta compression.
//!
//! ## Storage modes
//!
//! Each tensor in the manifest carries a `"storage"` field:
//!
//! - `"raw"` *(default)*: tensor bytes are stored verbatim in the hexz stream.
//! - `"xor_delta"`: the XOR of `(new_tensor ^ base_tensor)` is stored. On
//!   extraction, the base tensor is read from the parent hexz (whose path is
//!   recorded in the hexz header `parent_paths`) and XORed again to recover
//!   the original bytes.
//!
//! ## Why XOR delta?
//!
//! Fine-tuned weights differ from their base by small floating-point deltas.
//! XOR at the byte level produces output that is nearly all zeros for such
//! tensors. Zstd/LZ4 then compresses that sparse output to a tiny fraction of
//! the original size — typically 5–30× better than compressing the raw weights,
//! and 2–10× better than the block-level deduplication used in Phase 2.
//!
//! The computation is fully streaming: only one `block_size` chunk of each
//! tensor is in memory at a time, regardless of tensor size.

use hexz_common::Result;
use hexz_core::File as HexzFile;
use hexz_core::algo::compression::create_compressor_from_str;
use hexz_core::api::file::SnapshotStream;
use hexz_core::format::header::Header;
use hexz_core::format::safetensors::SafetensorsFile;
use hexz_store::local::MmapBackend;
use indexmap::IndexMap;

use crate::snapshot_writer::SnapshotWriter;

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

// ─────────────────────────────────────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for `store_safetensors`.
pub struct SafetensorsStoreConfig {
    pub input: PathBuf,
    pub output: PathBuf,
    /// Parent hexz archive for XOR delta and block-level deduplication.
    pub base: Option<PathBuf>,
    pub compression: String,
    pub block_size: u32,
    pub show_progress: bool,
}

/// Summary returned by `store_safetensors`.
pub struct SafetensorsStoreSummary {
    pub tensors: usize,
    pub total_bytes: u64,
    /// Physical bytes written to the output hexz file.
    pub stored_bytes: u64,
    /// Number of tensors stored as XOR deltas (new in Phase 3).
    pub xor_delta_tensors: usize,
    pub elapsed_secs: f64,
}

// ─────────────────────────────────────────────────────────────────────────────
// Store
// ─────────────────────────────────────────────────────────────────────────────

/// Store a `.safetensors` file as a hexz archive.
///
/// When `config.base` is provided:
/// - Tensors present in the base with the **same byte length** are stored as
///   XOR deltas: the bitwise XOR of `(new ^ base)` is written, which is nearly
///   all zeros for fine-tuned models and compresses extremely well.
/// - Tensors with a different size (e.g. added layers) fall back to raw storage.
/// - Tensors not present in the base are stored raw.
///
/// All I/O is streaming — only `block_size` bytes of each tensor are held in
/// memory at once.
pub fn store_safetensors(config: SafetensorsStoreConfig) -> Result<SafetensorsStoreSummary> {
    let started = Instant::now();

    let st = SafetensorsFile::open(&config.input)?;
    let total_bytes = st.total_data_bytes();

    let (compressor, comp_type) = create_compressor_from_str(&config.compression, None, None)?;

    let mut writer_builder = SnapshotWriter::builder(&config.output, compressor, comp_type)
        .block_size(config.block_size)
        .variable_blocks(false);

    // Open parent for block-level dedup (Phase 2) and XOR delta (Phase 3)
    let base_path_for_finalize = config.base.clone();
    let base_snap: Option<Arc<HexzFile>> = if let Some(ref base_path) = config.base {
        let backend = Arc::new(MmapBackend::new(base_path)?);
        let snap = HexzFile::open(backend, None)?;
        writer_builder = writer_builder.parent(Arc::clone(&snap));
        Some(snap)
    } else {
        None
    };

    // Read parent manifest to discover base tensor offsets for XOR delta
    let base_tensor_map: HashMap<String, (u64, u64)> = if let Some(ref base_path) = config.base {
        // If the parent has no manifest (e.g. was packed without safetensors),
        // just skip XOR delta and fall back to raw storage for all tensors.
        read_tensor_map(base_path).unwrap_or_default()
    } else {
        HashMap::new()
    };

    let mut writer = writer_builder.build()?;

    let mut src = File::open(&config.input)?;
    let mut tensors_manifest: IndexMap<String, serde_json::Value> = IndexMap::new();
    let mut logical_offset: u64 = 0;
    let block_size = config.block_size as usize;
    let mut xor_delta_tensors: usize = 0;

    // Compute total padded stream size for begin_stream
    let padded_total: u64 = st.tensors.values().fold(0u64, |acc, entry| {
        let sz = entry.data_end - entry.data_start;
        let pad = (-(sz as i64)).rem_euclid(block_size as i64) as u64;
        acc + sz + pad
    });

    writer.begin_stream(true, padded_total);

    for (name, entry) in &st.tensors {
        let tensor_len = entry.data_end - entry.data_start;

        // Attempt XOR delta if a base tensor of the same size exists
        let used_xor = if let Some(snap) = base_snap.as_ref() {
            if let Some(&(base_off, base_len)) = base_tensor_map.get(name.as_str()) {
                if base_len == tensor_len {
                    if config.show_progress {
                        eprintln!("  storing {} ({} bytes, xor_delta)", name, tensor_len);
                    }
                    write_xor_delta_blocks(
                        &mut src,
                        entry.data_start,
                        tensor_len,
                        snap,
                        base_off,
                        &mut writer,
                        block_size,
                    )?;

                    tensors_manifest.insert(
                        name.clone(),
                        serde_json::json!({
                            "dtype":       entry.dtype,
                            "shape":       entry.shape,
                            "offset":      logical_offset,
                            "length":      tensor_len,
                            "storage":     "xor_delta",
                            "base_offset": base_off,
                            "base_length": base_len,
                        }),
                    );
                    xor_delta_tensors += 1;
                    true
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };

        if !used_xor {
            if config.show_progress {
                eprintln!("  storing {} ({} bytes, raw)", name, tensor_len);
            }
            write_raw_blocks(
                &mut src,
                entry.data_start,
                tensor_len,
                &mut writer,
                block_size,
            )?;

            tensors_manifest.insert(
                name.clone(),
                serde_json::json!({
                    "dtype":   entry.dtype,
                    "shape":   entry.shape,
                    "offset":  logical_offset,
                    "length":  tensor_len,
                    "storage": "raw",
                }),
            );
        }

        logical_offset += tensor_len;

        // Pad to block boundary (zero blocks ≈ 8 bytes of metadata)
        let pad = (-(tensor_len as i64)).rem_euclid(block_size as i64) as u64;
        if pad > 0 {
            writer.write_data_block(&vec![0u8; pad as usize])?;
            logical_offset += pad;
        }
    }

    writer.end_stream()?;

    let manifest = serde_json::json!({
        "format":             "safetensors",
        "version":            "2",
        "safetensors_header": st.header_json,
        "tensors":            tensors_manifest,
    });
    let manifest_bytes = serde_json::to_vec(&manifest)
        .map_err(|e| hexz_common::Error::Format(format!("failed to serialize manifest: {}", e)))?;

    let parent_paths: Vec<String> = base_path_for_finalize
        .map(|p| vec![p.to_string_lossy().into_owned()])
        .unwrap_or_default();

    let stored_bytes = writer.current_offset();
    writer.finalize(parent_paths, Some(&manifest_bytes))?;

    Ok(SafetensorsStoreSummary {
        tensors: st.tensors.len(),
        total_bytes,
        stored_bytes,
        xor_delta_tensors,
        elapsed_secs: started.elapsed().as_secs_f64(),
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Extract
// ─────────────────────────────────────────────────────────────────────────────

/// Reconstruct a `.safetensors` file from a hexz archive.
///
/// Handles both raw and XOR-delta tensors transparently. For XOR-delta
/// tensors the parent hexz is opened from `header.parent_paths[0]`.
///
/// If `tensor_name` is `Some`, only the raw bytes for that tensor are written
/// (no safetensors file header). If `None`, a complete `.safetensors` file is
/// reconstructed.
pub fn extract_safetensors(input: &Path, output: &Path, tensor_name: Option<&str>) -> Result<()> {
    let meta_bytes = read_manifest(input)?;
    let manifest: serde_json::Value = serde_json::from_slice(&meta_bytes)
        .map_err(|e| hexz_common::Error::Format(format!("failed to parse manifest JSON: {}", e)))?;

    if manifest["format"].as_str() != Some("safetensors") {
        return Err(hexz_common::Error::Format(
            "hexz file is not a safetensors archive (wrong manifest format)".to_string(),
        ));
    }

    let tensors_obj = manifest["tensors"].as_object().ok_or_else(|| {
        hexz_common::Error::Format("manifest missing 'tensors' object".to_string())
    })?;

    // Open current hexz for tensor reads
    let snap = open_hexz(input)?;

    // Open parent hexz lazily (only if any XOR-delta tensor is present)
    let parent_snap: Option<Arc<HexzFile>> = {
        // Read header to find parent path
        let mut f = File::open(input)?;
        let header = Header::read_from(&mut f)?;
        if let Some(p) = header.parent_paths.into_iter().next() {
            let p = PathBuf::from(p);
            if p.exists() {
                Some(open_hexz(&p)?)
            } else {
                None
            }
        } else {
            None
        }
    };

    let out_file = File::create(output)?;
    let mut out = BufWriter::new(out_file);

    if let Some(name) = tensor_name {
        let t_info = tensors_obj.get(name).ok_or_else(|| {
            hexz_common::Error::Format(format!("tensor {:?} not found in manifest", name))
        })?;
        let bytes = reconstruct_tensor(t_info, &snap, parent_snap.as_ref(), name)?;
        out.write_all(&bytes)?;
    } else {
        let original_header = manifest["safetensors_header"].as_str().ok_or_else(|| {
            hexz_common::Error::Format("manifest missing 'safetensors_header'".to_string())
        })?;
        let orig_st = parse_stored_header(original_header)?;

        // Compute sequential data_offsets for the output file
        let mut new_offsets: IndexMap<String, [u64; 2]> = IndexMap::new();
        let mut cursor: u64 = 0;
        for name in orig_st.tensors.keys() {
            let t_info = tensors_obj.get(name.as_str()).ok_or_else(|| {
                hexz_common::Error::Format(format!(
                    "tensor {:?} in header but missing from manifest",
                    name
                ))
            })?;
            let length = t_info["length"].as_u64().ok_or_else(|| {
                hexz_common::Error::Format(format!("tensor {:?} missing 'length'", name))
            })?;
            new_offsets.insert(name.clone(), [cursor, cursor + length]);
            cursor += length;
        }

        out.write_all(&orig_st.to_header_prefix(&new_offsets)?)?;

        for name in orig_st.tensors.keys() {
            let t_info = tensors_obj.get(name.as_str()).ok_or_else(|| {
                hexz_common::Error::Format(format!("tensor {:?} not found in manifest", name))
            })?;
            let bytes = reconstruct_tensor(t_info, &snap, parent_snap.as_ref(), name)?;
            out.write_all(&bytes)?;
        }
    }

    out.flush()?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Private helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Write a tensor's raw bytes to the `SnapshotWriter` in `block_size` chunks.
fn write_raw_blocks(
    src: &mut File,
    src_start: u64,
    tensor_len: u64,
    writer: &mut SnapshotWriter,
    block_size: usize,
) -> Result<()> {
    src.seek(SeekFrom::Start(src_start))?;
    let mut remaining = tensor_len;
    while remaining > 0 {
        let chunk_len = remaining.min(block_size as u64) as usize;
        let mut buf = vec![0u8; chunk_len];
        src.read_exact(&mut buf)?;
        writer.write_data_block(&buf)?;
        remaining -= chunk_len as u64;
    }
    Ok(())
}

/// Compute `(src ^ base)` streaming and write the result to the `SnapshotWriter`.
///
/// Only one `block_size` chunk from each source is in memory at a time.
fn write_xor_delta_blocks(
    src: &mut File,
    src_start: u64,
    tensor_len: u64,
    base_snap: &Arc<HexzFile>,
    base_offset: u64,
    writer: &mut SnapshotWriter,
    block_size: usize,
) -> Result<()> {
    src.seek(SeekFrom::Start(src_start))?;
    let mut consumed: u64 = 0;

    while consumed < tensor_len {
        let chunk_len = (tensor_len - consumed).min(block_size as u64) as usize;

        // Read new tensor chunk
        let mut new_chunk = vec![0u8; chunk_len];
        src.read_exact(&mut new_chunk)?;

        // Read corresponding base tensor chunk from parent hexz
        let base_chunk =
            base_snap.read_at(SnapshotStream::Primary, base_offset + consumed, chunk_len)?;

        // XOR in-place; LLVM auto-vectorizes this loop
        for (n, b) in new_chunk.iter_mut().zip(base_chunk.iter()) {
            *n ^= b;
        }

        writer.write_data_block(&new_chunk)?;
        consumed += chunk_len as u64;
    }

    Ok(())
}

/// Reconstruct a single tensor's original bytes from the manifest entry.
///
/// Handles both `"raw"` and `"xor_delta"` storage modes.
fn reconstruct_tensor(
    t_info: &serde_json::Value,
    snap: &Arc<HexzFile>,
    parent_snap: Option<&Arc<HexzFile>>,
    name: &str,
) -> Result<Vec<u8>> {
    let offset = t_info["offset"]
        .as_u64()
        .ok_or_else(|| hexz_common::Error::Format(format!("tensor {:?} missing 'offset'", name)))?;
    let length = t_info["length"]
        .as_u64()
        .ok_or_else(|| hexz_common::Error::Format(format!("tensor {:?} missing 'length'", name)))?;

    let storage = t_info["storage"].as_str().unwrap_or("raw");

    if storage == "xor_delta" {
        let parent = parent_snap.ok_or_else(|| {
            hexz_common::Error::Format(format!(
                "tensor {:?} requires XOR delta reconstruction but parent hexz is unavailable; \
                 ensure the base archive is accessible at the path recorded in the hexz header",
                name
            ))
        })?;

        let base_offset = t_info["base_offset"].as_u64().ok_or_else(|| {
            hexz_common::Error::Format(format!("tensor {:?} missing 'base_offset'", name))
        })?;
        let base_length = t_info["base_length"].as_u64().ok_or_else(|| {
            hexz_common::Error::Format(format!("tensor {:?} missing 'base_length'", name))
        })?;

        if base_length != length {
            return Err(hexz_common::Error::Format(format!(
                "tensor {:?}: base_length ({}) != length ({}), corrupt manifest",
                name, base_length, length
            )));
        }

        // Read XOR delta and base tensor, then XOR to reconstruct
        let mut delta = snap.read_at(SnapshotStream::Primary, offset, length as usize)?;
        let base = parent.read_at(SnapshotStream::Primary, base_offset, base_length as usize)?;

        for (d, b) in delta.iter_mut().zip(base.iter()) {
            *d ^= b;
        }

        Ok(delta)
    } else {
        // "raw" or unrecognised → treat as raw
        snap.read_at(SnapshotStream::Primary, offset, length as usize)
    }
}

/// Open a hexz file and return an `Arc<HexzFile>`.
fn open_hexz(path: &Path) -> Result<Arc<HexzFile>> {
    let backend = Arc::new(MmapBackend::new(path)?);
    HexzFile::open(backend, None)
}

/// Read the raw manifest bytes from a hexz file's embedded metadata.
/// Read the embedded JSON manifest from a hexz file's metadata region.
pub fn read_manifest(path: &Path) -> Result<Vec<u8>> {
    let mut f = File::open(path)?;
    let header = Header::read_from(&mut f)?;
    let (meta_offset, meta_len) = match (header.metadata_offset, header.metadata_length) {
        (Some(o), Some(l)) => (o, l),
        _ => {
            return Err(hexz_common::Error::Format(
                "hexz file has no embedded manifest metadata".to_string(),
            ));
        }
    };
    let mut buf = vec![0u8; meta_len as usize];
    f.seek(SeekFrom::Start(meta_offset))?;
    f.read_exact(&mut buf)?;
    Ok(buf)
}

/// Parse the parent hexz manifest and return a map of `tensor_name → (offset, length)`.
///
/// These are the logical offsets within the parent hexz's primary stream.
fn read_tensor_map(path: &Path) -> Result<HashMap<String, (u64, u64)>> {
    let meta_bytes = read_manifest(path)?;
    let manifest: serde_json::Value = serde_json::from_slice(&meta_bytes).map_err(|e| {
        hexz_common::Error::Format(format!("failed to parse parent manifest: {}", e))
    })?;

    let tensors = manifest["tensors"].as_object().ok_or_else(|| {
        hexz_common::Error::Format("parent manifest missing 'tensors' object".to_string())
    })?;

    let mut map = HashMap::with_capacity(tensors.len());
    for (name, info) in tensors {
        if let (Some(off), Some(len)) = (info["offset"].as_u64(), info["length"].as_u64()) {
            map.insert(name.clone(), (off, len));
        }
    }
    Ok(map)
}

/// Reconstruct a `SafetensorsFile` stub from a stored header JSON string.
///
/// Used during extraction to restore original tensor ordering and metadata.
/// The `data_start`, `data_start`, and `data_end` fields are left at 0 since
/// they are not needed for header reconstruction.
fn parse_stored_header(header_json: &str) -> Result<SafetensorsFile> {
    let raw: IndexMap<String, serde_json::Value> =
        serde_json::from_str(header_json).map_err(|e| {
            hexz_common::Error::Format(format!("failed to parse stored safetensors header: {}", e))
        })?;

    let mut tensors: IndexMap<String, hexz_core::format::safetensors::TensorEntry> =
        IndexMap::new();
    let mut user_metadata = std::collections::HashMap::new();

    for (k, v) in raw {
        if k == "__metadata__" {
            if let Some(obj) = v.as_object() {
                for (mk, mv) in obj {
                    if let Some(s) = mv.as_str() {
                        user_metadata.insert(mk.clone(), s.to_string());
                    }
                }
            }
            continue;
        }
        let dtype = v["dtype"].as_str().unwrap_or("").to_string();
        let shape: Vec<u64> = v["shape"]
            .as_array()
            .map(|a| a.iter().filter_map(|x| x.as_u64()).collect())
            .unwrap_or_default();
        tensors.insert(
            k,
            hexz_core::format::safetensors::TensorEntry {
                dtype,
                shape,
                data_start: 0,
                data_end: 0,
            },
        );
    }

    Ok(SafetensorsFile {
        tensors,
        user_metadata,
        header_json: header_json.to_string(),
        data_start: 0,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use hexz_core::format::safetensors::pread;
    use std::io::Write;
    use tempfile::NamedTempFile;

    // Build a minimal safetensors file with one tensor of given bytes
    fn make_st_file(tensor_bytes: &[u8]) -> NamedTempFile {
        let len = tensor_bytes.len();
        let header = format!(
            r#"{{"weight":{{"dtype":"F32","shape":[{}],"data_offsets":[0,{}]}}}}"#,
            len / 4,
            len
        );
        let header_bytes = header.as_bytes();
        let header_len = header_bytes.len() as u64;

        let mut f = NamedTempFile::new().unwrap();
        f.write_all(&header_len.to_le_bytes()).unwrap();
        f.write_all(header_bytes).unwrap();
        f.write_all(tensor_bytes).unwrap();
        f
    }

    /// Compare two safetensors files semantically: same tensors, same data.
    ///
    /// The safetensors format does not guarantee JSON key ordering in the
    /// header, so we compare tensor metadata and raw bytes rather than the
    /// full file as a byte sequence.
    fn assert_tensors_equal(expected_path: &std::path::Path, actual_path: &std::path::Path) {
        use hexz_core::format::safetensors::SafetensorsFile;

        let exp = SafetensorsFile::open(expected_path).unwrap();
        let act = SafetensorsFile::open(actual_path).unwrap();

        assert_eq!(
            exp.tensors.len(),
            act.tensors.len(),
            "tensor count mismatch"
        );

        let mut exp_f = std::fs::File::open(expected_path).unwrap();
        let mut act_f = std::fs::File::open(actual_path).unwrap();

        for (name, exp_e) in &exp.tensors {
            let act_e = act
                .tensors
                .get(name)
                .unwrap_or_else(|| panic!("missing tensor {:?} in extracted file", name));
            assert_eq!(exp_e.dtype, act_e.dtype, "dtype mismatch for {}", name);
            assert_eq!(exp_e.shape, act_e.shape, "shape mismatch for {}", name);

            let exp_len = exp_e.data_end - exp_e.data_start;
            let act_len = act_e.data_end - act_e.data_start;
            assert_eq!(exp_len, act_len, "data length mismatch for {}", name);

            let exp_bytes = pread(&mut exp_f, exp_e.data_start, exp_len).unwrap();
            let act_bytes = pread(&mut act_f, act_e.data_start, act_len).unwrap();
            assert_eq!(exp_bytes, act_bytes, "tensor data mismatch for {:?}", name);
        }
    }

    #[test]
    fn test_raw_round_trip() {
        let data: Vec<u8> = (0..256u32).flat_map(|i| i.to_le_bytes()).collect();
        let src = make_st_file(&data);

        let out = NamedTempFile::new().unwrap();
        let result = NamedTempFile::new().unwrap();

        store_safetensors(SafetensorsStoreConfig {
            input: src.path().to_path_buf(),
            output: out.path().to_path_buf(),
            base: None,
            compression: "lz4".to_string(),
            block_size: 65536,
            show_progress: false,
        })
        .unwrap();

        extract_safetensors(out.path(), result.path(), None).unwrap();

        assert_tensors_equal(src.path(), result.path());
    }

    #[test]
    fn test_xor_delta_round_trip() {
        // base: all 0x11 bytes
        let base_data: Vec<u8> = vec![0x11u8; 1024];
        // new: base with small edits — simulates fine-tuning
        let mut new_data = base_data.clone();
        new_data[0] = 0x22;
        new_data[512] = 0x33;

        let base_src = make_st_file(&base_data);
        let new_src = make_st_file(&new_data);

        let base_hxz = NamedTempFile::new().unwrap();
        let new_hxz = NamedTempFile::new().unwrap();
        let extracted = NamedTempFile::new().unwrap();

        // Store base (no parent)
        store_safetensors(SafetensorsStoreConfig {
            input: base_src.path().to_path_buf(),
            output: base_hxz.path().to_path_buf(),
            base: None,
            compression: "lz4".to_string(),
            block_size: 65536,
            show_progress: false,
        })
        .unwrap();

        // Store new with XOR delta against base
        let summary = store_safetensors(SafetensorsStoreConfig {
            input: new_src.path().to_path_buf(),
            output: new_hxz.path().to_path_buf(),
            base: Some(base_hxz.path().to_path_buf()),
            compression: "lz4".to_string(),
            block_size: 65536,
            show_progress: false,
        })
        .unwrap();

        assert_eq!(
            summary.xor_delta_tensors, 1,
            "weight tensor should be XOR delta"
        );

        // Extract and verify tensor data is byte-identical to the original
        extract_safetensors(new_hxz.path(), extracted.path(), None).unwrap();

        assert_tensors_equal(new_src.path(), extracted.path());
    }

    #[test]
    fn test_xor_delta_single_tensor_extract() {
        let base_data: Vec<u8> = vec![0xAAu8; 256];
        let mut new_data = base_data.clone();
        new_data[10] = 0xFF;

        let base_src = make_st_file(&base_data);
        let new_src = make_st_file(&new_data);
        let base_hxz = NamedTempFile::new().unwrap();
        let new_hxz = NamedTempFile::new().unwrap();
        let out = NamedTempFile::new().unwrap();

        store_safetensors(SafetensorsStoreConfig {
            input: base_src.path().to_path_buf(),
            output: base_hxz.path().to_path_buf(),
            base: None,
            compression: "lz4".to_string(),
            block_size: 65536,
            show_progress: false,
        })
        .unwrap();

        store_safetensors(SafetensorsStoreConfig {
            input: new_src.path().to_path_buf(),
            output: new_hxz.path().to_path_buf(),
            base: Some(base_hxz.path().to_path_buf()),
            compression: "lz4".to_string(),
            block_size: 65536,
            show_progress: false,
        })
        .unwrap();

        // Single tensor extraction (raw bytes, no header)
        extract_safetensors(new_hxz.path(), out.path(), Some("weight")).unwrap();

        let recovered = std::fs::read(out.path()).unwrap();
        assert_eq!(
            new_data, recovered,
            "single-tensor XOR extract must be correct"
        );
    }
}
