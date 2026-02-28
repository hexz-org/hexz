//! Checkpoint save/load: tensor-aware I/O for hexz archives.
//!
//! Provides the pure-Rust implementation of `checkpoint.save()` and
//! `checkpoint.load()`. All I/O, compression, decompression, byte-shuffle,
//! and XOR delta encoding/reconstruction happen here so the Python wrapper
//! only needs to convert between raw bytes and torch tensors.

use hexz_common::{Error, Result};
use hexz_core::File as HexzFile;
use hexz_core::algo::compression::create_compressor_from_str;
use hexz_core::algo::transform::{byte_shuffle, byte_unshuffle, xor_in_place};
use hexz_core::api::file::{ParentLoader, SnapshotStream};
use hexz_core::format::header::Header;
use hexz_store::StorageBackend;
use rayon::prelude::*;
use serde::Deserialize;

use crate::snapshot_writer::SnapshotWriter;

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;

// ─────────────────────────────────────────────────────────────────────────────
// Manifest schema (must match Python checkpoint.save() output)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct CheckpointManifest {
    hexz_checkpoint: String,
    #[allow(dead_code)]
    tensor_count: Option<usize>,
    tensors: HashMap<String, TensorInfo>,
    #[serde(default)]
    scalars: HashMap<String, ScalarInfo>,
}

#[derive(Deserialize, Clone)]
struct TensorInfo {
    offset: u64,
    length: u64,
    dtype: String,
    shape: Vec<usize>,
    #[serde(default = "default_storage")]
    storage: String,
    base_offset: Option<u64>,
    #[allow(dead_code)]
    base_length: Option<u64>,
    #[serde(default = "default_element_size")]
    element_size: usize,
}

fn default_storage() -> String {
    "raw".to_string()
}

fn default_element_size() -> usize {
    1
}

#[derive(Deserialize, Clone)]
pub struct ScalarInfo {
    #[serde(rename = "type")]
    pub scalar_type: String,
    pub value: serde_json::Value,
}

// ─────────────────────────────────────────────────────────────────────────────
// Public output types
// ─────────────────────────────────────────────────────────────────────────────

/// A single loaded tensor: raw bytes + metadata.
pub struct TensorData {
    pub name: String,
    pub data: Vec<u8>,
    pub dtype: String,
    pub shape: Vec<usize>,
}

/// Complete checkpoint data ready for conversion to Python objects.
pub struct CheckpointData {
    pub tensors: Vec<TensorData>,
    pub scalars: HashMap<String, ScalarInfo>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Save types
// ─────────────────────────────────────────────────────────────────────────────

/// A single tensor to be saved: raw bytes + metadata.
pub struct TensorWriteSpec<'a> {
    pub name: String,
    pub data: &'a [u8],
    pub dtype: String,
    pub shape: Vec<usize>,
    pub element_size: usize,
}

/// Configuration for saving a checkpoint.
pub struct SaveCheckpointConfig<'a> {
    pub path: PathBuf,
    pub compression: String,
    pub compression_level: Option<i32>,
    pub block_size: u32,
    pub parent: Option<PathBuf>,
    pub message: Option<String>,
    pub num_workers: usize,
    /// Pre-reconstructed parent tensor bytes for chained XOR delta.
    /// When the parent itself stores XOR deltas, the caller can provide
    /// the already-reconstructed base bytes to avoid re-reading the chain.
    pub base_tensors: Option<HashMap<String, &'a [u8]>>,
}

/// Map dtype string to element size in bytes.
pub fn dtype_element_size(dtype: &str) -> usize {
    match dtype {
        "float16" | "bfloat16" | "int16" => 2,
        "float32" | "int32" => 4,
        "float64" | "int64" => 8,
        _ => 1, // int8, uint8, bool
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Save implementation
// ─────────────────────────────────────────────────────────────────────────────

/// Save tensors and scalars as a hexz checkpoint.
///
/// All I/O, compression, XOR delta encoding, and byte-shuffle are performed
/// here. The caller (Python wrapper) only needs to extract raw bytes from
/// torch tensors.
///
/// Tensors are written as a single contiguous stream with block-aligned
/// padding between them, enabling random access via the manifest.
pub fn save_checkpoint(
    tensors: &[TensorWriteSpec<'_>],
    scalars: &HashMap<String, ScalarInfo>,
    config: &SaveCheckpointConfig<'_>,
) -> Result<()> {
    let block_size = config.block_size as usize;

    // Set up compressor
    let packing_level = if config.parent.is_some() {
        // Delta saves: fast compression (saved frequently)
        Some(1)
    } else {
        // Baseline saves: use caller's level or default balanced
        config.compression_level.or(Some(3))
    };
    let (compressor, comp_type) =
        create_compressor_from_str(&config.compression, packing_level, None)?;

    // Open parent snapshot for block-level dedup + XOR delta
    let parent_snap: Option<Arc<HexzFile>> = if let Some(ref parent_path) = config.parent {
        let backend = Arc::new(hexz_store::local::MmapBackend::new(parent_path)?);
        Some(HexzFile::open(backend, None)?)
    } else {
        None
    };

    let mut writer_builder = SnapshotWriter::builder(&config.path, compressor, comp_type)
        .block_size(config.block_size)
        .variable_blocks(false);

    if let Some(ref snap) = parent_snap {
        writer_builder = writer_builder.parent(Arc::clone(snap));
    }

    let mut writer = writer_builder.build()?;

    // Read parent manifest for XOR delta metadata
    let parent_manifest: Option<CheckpointManifest> = if let Some(ref parent_path) = config.parent {
        match read_manifest_bytes(parent_path) {
            Ok(bytes) => parse_manifest(&bytes).ok(),
            Err(_) => None,
        }
    } else {
        None
    };

    // Build parent chain for reconstructing xor_delta parents
    let parent_chain: Vec<(Arc<HexzFile>, CheckpointManifest)> = if let Some(ref snap) = parent_snap
    {
        build_parent_chain(snap).unwrap_or_default()
    } else {
        Vec::new()
    };

    // Calculate total padded stream size
    let total_padded: u64 = tensors
        .iter()
        .map(|t| {
            let len = t.data.len() as u64;
            let pad = (-(len as i64)).rem_euclid(block_size as i64) as u64;
            len + pad
        })
        .sum();

    // Single stream for all tensors
    writer.begin_stream(true, total_padded);

    let mut tensors_manifest: HashMap<String, serde_json::Value> = HashMap::new();
    let mut offset: u64 = 0;
    let zero_pad = vec![0u8; block_size];

    for tensor in tensors {
        let length = tensor.data.len();
        let element_size = tensor.element_size;

        // Try XOR delta if parent has this tensor at the same size
        let used_xor = if let (Some(p_manifest), Some(p_snap)) = (&parent_manifest, &parent_snap) {
            if let Some(p_info) = p_manifest.tensors.get(&tensor.name) {
                if p_info.length as usize == length {
                    // Get the base bytes for XOR
                    let base_bytes = if p_info.storage == "xor_delta" {
                        // Parent tensor is itself a delta — get reconstructed bytes
                        if let Some(ref base_map) = config.base_tensors {
                            if let Some(base) = base_map.get(&tensor.name) {
                                Some((*base).to_vec())
                            } else {
                                // Reconstruct from parent chain
                                reconstruct_tensor(p_snap, p_info, &tensor.name, &parent_chain).ok()
                            }
                        } else {
                            // Reconstruct from parent chain
                            reconstruct_tensor(p_snap, p_info, &tensor.name, &parent_chain).ok()
                        }
                    } else {
                        // Raw parent tensor — read directly
                        p_snap
                            .read_at(SnapshotStream::Primary, p_info.offset, length)
                            .ok()
                    };

                    if let Some(base) = base_bytes {
                        // Compute XOR delta + byte shuffle
                        let mut xor_buf = vec![0u8; length];
                        xor_buf.copy_from_slice(tensor.data);
                        xor_in_place(&mut xor_buf, &base);

                        let mut scratch = Vec::new();
                        byte_shuffle(&mut xor_buf, element_size, &mut scratch);

                        // Write shuffled XOR delta
                        writer.write_blocks_parallel(&xor_buf, config.num_workers)?;

                        tensors_manifest.insert(
                            tensor.name.clone(),
                            serde_json::json!({
                                "offset": offset,
                                "length": length,
                                "dtype": tensor.dtype,
                                "shape": tensor.shape,
                                "storage": "xor_delta",
                                "base_offset": p_info.offset,
                                "base_length": p_info.length,
                                "element_size": element_size,
                            }),
                        );
                        true
                    } else {
                        false
                    }
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
            // Write raw
            writer.write_blocks_parallel(tensor.data, config.num_workers)?;

            tensors_manifest.insert(
                tensor.name.clone(),
                serde_json::json!({
                    "offset": offset,
                    "length": length,
                    "dtype": tensor.dtype,
                    "shape": tensor.shape,
                    "storage": "raw",
                }),
            );
        }

        offset += length as u64;

        // Pad to block boundary
        let pad = (-(length as i64)).rem_euclid(block_size as i64) as usize;
        if pad > 0 {
            writer.write_data_block(&zero_pad[..pad])?;
            offset += pad as u64;
        }
    }

    writer.end_stream()?;

    // Build scalars for manifest
    let scalars_json: HashMap<String, serde_json::Value> = scalars
        .iter()
        .map(|(name, info)| {
            (
                name.clone(),
                serde_json::json!({
                    "type": info.scalar_type,
                    "value": info.value,
                }),
            )
        })
        .collect();

    // Build and serialize manifest
    let manifest = serde_json::json!({
        "hexz_checkpoint": "1.0",
        "tensor_count": tensors.len(),
        "tensors": tensors_manifest,
        "scalars": scalars_json,
        "message": config.message,
    });
    let manifest_bytes = serde_json::to_vec(&manifest)
        .map_err(|e| Error::Format(format!("failed to serialize manifest: {}", e)))?;

    // Finalize with parent paths and manifest
    let parent_paths: Vec<String> = config
        .parent
        .as_ref()
        .map(|p| vec![p.to_string_lossy().into_owned()])
        .unwrap_or_default();

    writer.finalize(parent_paths, Some(&manifest_bytes))?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Load implementation
// ─────────────────────────────────────────────────────────────────────────────

/// Open a local snapshot with recursive parent chain resolution.
fn open_local(path: &Path) -> Result<Arc<HexzFile>> {
    let backend: Arc<dyn StorageBackend> = Arc::new(hexz_store::local::MmapBackend::new(path)?);
    let loader: ParentLoader = Box::new(|p: &str| {
        let backend: Arc<dyn StorageBackend> =
            Arc::new(hexz_store::local::MmapBackend::new(Path::new(p))?);
        let loader: ParentLoader = Box::new(open_local_parent);
        HexzFile::open_with_cache_and_loader(backend, None, None, None, Some(&loader))
    });
    HexzFile::open_with_cache_and_loader(backend, None, None, None, Some(&loader))
}

fn open_local_parent(parent_path: &str) -> Result<Arc<HexzFile>> {
    let backend: Arc<dyn StorageBackend> =
        Arc::new(hexz_store::local::MmapBackend::new(Path::new(parent_path))?);
    let loader: ParentLoader = Box::new(open_local_parent);
    HexzFile::open_with_cache_and_loader(backend, None, None, None, Some(&loader))
}

/// Read the JSON manifest embedded in a hexz file's metadata region.
fn read_manifest_bytes(path: &Path) -> Result<Vec<u8>> {
    let mut f = fs::File::open(path)?;
    let header = Header::read_from(&mut f)?;
    let (meta_offset, meta_len) = match (header.metadata_offset, header.metadata_length) {
        (Some(o), Some(l)) => (o, l),
        _ => {
            return Err(Error::Format(
                "hexz file has no embedded manifest metadata".to_string(),
            ));
        }
    };
    let mut buf = vec![0u8; meta_len as usize];
    f.seek(SeekFrom::Start(meta_offset))?;
    f.read_exact(&mut buf)?;
    Ok(buf)
}

/// Parse a checkpoint manifest from raw JSON bytes.
fn parse_manifest(raw: &[u8]) -> Result<CheckpointManifest> {
    serde_json::from_slice(raw)
        .map_err(|e| Error::Format(format!("failed to parse checkpoint manifest: {}", e)))
}

/// Reconstruct a single tensor's bytes, handling raw and XOR delta storage.
///
/// For XOR deltas, walks up the parent chain until it finds a "raw" ancestor,
/// then applies deltas forward to reconstruct the final bytes.
fn reconstruct_tensor(
    snap: &Arc<HexzFile>,
    info: &TensorInfo,
    name: &str,
    parent_chain: &[(Arc<HexzFile>, CheckpointManifest)],
) -> Result<Vec<u8>> {
    if info.storage == "raw" {
        // Simple case: read raw bytes directly
        return snap
            .read_at(SnapshotStream::Primary, info.offset, info.length as usize)
            .map_err(|e| {
                Error::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    e.to_string(),
                ))
            });
    }

    // XOR delta: need to find the base and reconstruct
    if info.storage != "xor_delta" {
        return Err(Error::Format(format!(
            "tensor {}: unknown storage type {:?}",
            name, info.storage
        )));
    }

    // Build the chain of deltas to apply (from current back to a raw ancestor).
    // Each entry is (snapshot, tensor_info) for the tensor at that level.
    let mut delta_chain: Vec<(&Arc<HexzFile>, &TensorInfo)> = vec![(snap, info)];

    for (parent_snap, parent_manifest) in parent_chain {
        if let Some(parent_info) = parent_manifest.tensors.get(name) {
            if parent_info.storage == "raw" {
                // Found the raw base — read it and apply deltas forward
                let mut data = parent_snap
                    .read_at(
                        SnapshotStream::Primary,
                        parent_info.offset,
                        parent_info.length as usize,
                    )
                    .map_err(|e| {
                        Error::Io(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            e.to_string(),
                        ))
                    })?;

                // Apply deltas in reverse order (oldest parent first → current)
                let mut scratch = Vec::new();
                for (delta_snap, delta_info) in delta_chain.iter().rev() {
                    let mut delta_bytes = delta_snap
                        .read_at(
                            SnapshotStream::Primary,
                            delta_info.offset,
                            delta_info.length as usize,
                        )
                        .map_err(|e| {
                            Error::Io(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                e.to_string(),
                            ))
                        })?;

                    byte_unshuffle(&mut delta_bytes, delta_info.element_size, &mut scratch);
                    xor_in_place(&mut delta_bytes, &data);
                    data = delta_bytes;
                }

                return Ok(data);
            } else if parent_info.storage == "xor_delta" {
                // Parent is also a delta — add to chain and keep walking
                delta_chain.push((parent_snap, parent_info));
            } else {
                return Err(Error::Format(format!(
                    "tensor {} in parent: unknown storage type {:?}",
                    name, parent_info.storage
                )));
            }
        } else {
            // Tensor not found in parent — shouldn't happen for a valid delta chain
            return Err(Error::Format(format!(
                "tensor {} referenced as xor_delta but not found in parent chain",
                name
            )));
        }
    }

    // If we exhausted the parent chain without finding a raw base, fall back to
    // reading this tensor's base from the parent snapshot directly using base_offset.
    // This handles the simple (non-chained) case where there's a single parent.
    if delta_chain.len() == 1 {
        if let Some((parent_snap, _)) = parent_chain.first() {
            if let Some(base_offset) = info.base_offset {
                let mut data = snap
                    .read_at(SnapshotStream::Primary, info.offset, info.length as usize)
                    .map_err(|e| {
                        Error::Io(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            e.to_string(),
                        ))
                    })?;

                let mut scratch = Vec::new();
                byte_unshuffle(&mut data, info.element_size, &mut scratch);

                let base_data = parent_snap
                    .read_at(SnapshotStream::Primary, base_offset, info.length as usize)
                    .map_err(|e| {
                        Error::Io(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            e.to_string(),
                        ))
                    })?;

                xor_in_place(&mut data, &base_data);
                return Ok(data);
            }
        }
    }

    Err(Error::Format(format!(
        "tensor {}: could not resolve XOR delta chain — no raw ancestor found",
        name
    )))
}

/// Load checkpoint tensors and scalars from a hexz file.
///
/// All I/O, decompression, byte-unshuffle, and XOR delta reconstruction are
/// performed here. The caller only needs to wrap the returned raw bytes as
/// torch tensors.
///
/// Tensors are loaded in parallel via rayon.
pub fn load_checkpoint(path: &Path, keys: Option<&[String]>) -> Result<CheckpointData> {
    // Read and parse manifest
    let manifest_bytes = read_manifest_bytes(path)?;
    let manifest = parse_manifest(&manifest_bytes)?;

    // Validate checkpoint marker
    if manifest.hexz_checkpoint.is_empty() {
        return Err(Error::Format(
            "not a hexz checkpoint (missing hexz_checkpoint marker)".to_string(),
        ));
    }

    // Determine which keys to load
    let all_tensor_keys: Vec<String> = manifest.tensors.keys().cloned().collect();
    let tensor_keys: Vec<String> = if let Some(filter) = keys {
        let filter_set: std::collections::HashSet<&str> =
            filter.iter().map(|s| s.as_str()).collect();
        // Validate all requested keys exist
        for k in &filter_set {
            if !manifest.tensors.contains_key(*k) && !manifest.scalars.contains_key(*k) {
                return Err(Error::Format(format!(
                    "key {:?} not found in checkpoint",
                    k
                )));
            }
        }
        all_tensor_keys
            .into_iter()
            .filter(|k| filter_set.contains(k.as_str()))
            .collect()
    } else {
        all_tensor_keys
    };

    // Open the snapshot with parent chain
    let snap = open_local(path)?;

    // Build parent chain for XOR delta resolution
    let parent_chain = build_parent_chain(&snap)?;

    // Load tensors in parallel
    let loaded: Vec<TensorData> = tensor_keys
        .par_iter()
        .map(|name| {
            let info = &manifest.tensors[name];
            let data = reconstruct_tensor(&snap, info, name, &parent_chain)?;
            Ok(TensorData {
                name: name.clone(),
                data,
                dtype: info.dtype.clone(),
                shape: info.shape.clone(),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    // Filter scalars
    let scalars = if let Some(filter) = keys {
        let filter_set: std::collections::HashSet<&str> =
            filter.iter().map(|s| s.as_str()).collect();
        manifest
            .scalars
            .into_iter()
            .filter(|(k, _)| filter_set.contains(k.as_str()))
            .collect()
    } else {
        manifest.scalars
    };

    Ok(CheckpointData {
        tensors: loaded,
        scalars,
    })
}

/// Walk the parent chain and collect (snapshot, manifest) pairs.
///
/// Returns parents ordered from immediate parent to oldest ancestor.
fn build_parent_chain(snap: &Arc<HexzFile>) -> Result<Vec<(Arc<HexzFile>, CheckpointManifest)>> {
    let mut chain = Vec::new();
    let mut parent_paths = snap.header.parent_paths.clone();

    while !parent_paths.is_empty() {
        let parent_path = PathBuf::from(&parent_paths[0]);
        if !parent_path.exists() {
            break;
        }

        // Read parent manifest
        let manifest_bytes = match read_manifest_bytes(&parent_path) {
            Ok(b) => b,
            Err(_) => break,
        };
        let manifest = match parse_manifest(&manifest_bytes) {
            Ok(m) => m,
            Err(_) => break,
        };

        // Open parent snapshot
        let parent_snap = open_local(&parent_path)?;

        // Get next level's parent paths
        parent_paths = parent_snap.header.parent_paths.clone();

        chain.push((parent_snap, manifest));
    }

    Ok(chain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_manifest_basic() {
        let json = r#"{
            "hexz_checkpoint": "1.0",
            "tensor_count": 2,
            "tensors": {
                "weight": {
                    "offset": 0,
                    "length": 1024,
                    "dtype": "float32",
                    "shape": [16, 16],
                    "storage": "raw"
                }
            },
            "scalars": {
                "step": {"type": "int", "value": 42}
            }
        }"#;

        let manifest = parse_manifest(json.as_bytes()).unwrap();
        assert_eq!(manifest.hexz_checkpoint, "1.0");
        assert_eq!(manifest.tensors.len(), 1);
        assert_eq!(manifest.scalars.len(), 1);

        let weight = &manifest.tensors["weight"];
        assert_eq!(weight.offset, 0);
        assert_eq!(weight.length, 1024);
        assert_eq!(weight.dtype, "float32");
        assert_eq!(weight.shape, vec![16, 16]);
        assert_eq!(weight.storage, "raw");
    }

    #[test]
    fn test_parse_manifest_xor_delta() {
        let json = r#"{
            "hexz_checkpoint": "1.0",
            "tensor_count": 1,
            "tensors": {
                "weight": {
                    "offset": 0,
                    "length": 1024,
                    "dtype": "float32",
                    "shape": [16, 16],
                    "storage": "xor_delta",
                    "base_offset": 0,
                    "base_length": 1024,
                    "element_size": 4
                }
            }
        }"#;

        let manifest = parse_manifest(json.as_bytes()).unwrap();
        let weight = &manifest.tensors["weight"];
        assert_eq!(weight.storage, "xor_delta");
        assert_eq!(weight.base_offset, Some(0));
        assert_eq!(weight.element_size, 4);
    }

    #[test]
    fn test_parse_manifest_defaults() {
        // Missing "storage" should default to "raw", "element_size" to 1
        let json = r#"{
            "hexz_checkpoint": "1.0",
            "tensors": {
                "bias": {
                    "offset": 1024,
                    "length": 64,
                    "dtype": "float32",
                    "shape": [16]
                }
            }
        }"#;

        let manifest = parse_manifest(json.as_bytes()).unwrap();
        let bias = &manifest.tensors["bias"];
        assert_eq!(bias.storage, "raw");
        assert_eq!(bias.element_size, 1);
    }
}
