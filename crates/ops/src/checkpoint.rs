//! Checkpoint loading: read tensor bytes from hexz archives.
//!
//! Provides the pure-Rust implementation of `checkpoint.load()`. All I/O,
//! decompression, byte-unshuffle, and XOR delta reconstruction happen here
//! so the Python wrapper only needs to convert raw bytes into torch tensors.

use hexz_common::{Error, Result};
use hexz_core::File as HexzFile;
use hexz_core::algo::transform::{byte_unshuffle, xor_in_place};
use hexz_core::api::file::{ParentLoader, SnapshotStream};
use hexz_core::format::header::Header;
use hexz_store::StorageBackend;
use rayon::prelude::*;
use serde::Deserialize;

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
// Implementation
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
