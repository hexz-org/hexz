//! Safetensors file format parser.
//!
//! Parses the safetensors wire format:
//! ```text
//! [8-byte LE u64: header_len] [header_len bytes: UTF-8 JSON] [raw tensor bytes...]
//! ```
//!
//! The JSON header schema:
//! ```json
//! {
//!   "tensor_name": {"dtype": "BF16", "shape": [32000, 4096], "data_offsets": [0, 262144000]},
//!   "__metadata__": {"model_type": "mistral"}
//! }
//! ```
//!
//! `data_offsets` are relative to the start of the data region (after the JSON header prefix).

use indexmap::IndexMap;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use hexz_common::Error;
use hexz_common::Result;

/// Parsed representation of a safetensors file.
pub struct SafetensorsFile {
    /// Tensors in original header order, keyed by name.
    pub tensors: IndexMap<String, TensorEntry>,
    /// Contents of the `__metadata__` key, if present.
    pub user_metadata: HashMap<String, String>,
    /// The original header JSON string (for lossless reconstruction).
    pub header_json: String,
    /// Absolute byte offset in the file where tensor data begins.
    pub data_start: u64,
}

/// Metadata for a single tensor within a safetensors file.
pub struct TensorEntry {
    pub dtype: String,
    pub shape: Vec<u64>,
    /// Absolute byte offset of tensor data in the source file.
    pub data_start: u64,
    /// Absolute byte offset of the first byte past the tensor data.
    pub data_end: u64,
}

/// Raw JSON shape for a single tensor as it appears in the header.
#[derive(Deserialize)]
struct RawTensorEntry {
    dtype: String,
    shape: Vec<u64>,
    data_offsets: [u64; 2],
}

impl SafetensorsFile {
    /// Open and parse a safetensors file.
    pub fn open(path: &Path) -> Result<Self> {
        let mut f = File::open(path)
            .map_err(|e| Error::Format(format!("failed to open {:?}: {}", path, e)))?;

        // Read 8-byte little-endian header length
        let mut len_buf = [0u8; 8];
        f.read_exact(&mut len_buf).map_err(|e| {
            Error::Format(format!("failed to read safetensors header length: {}", e))
        })?;
        let header_len = u64::from_le_bytes(len_buf);

        if header_len > 100 * 1024 * 1024 {
            return Err(Error::Format(format!(
                "safetensors header too large: {} bytes",
                header_len
            )));
        }

        // Read header JSON
        let mut header_bytes = vec![0u8; header_len as usize];
        f.read_exact(&mut header_bytes)
            .map_err(|e| Error::Format(format!("failed to read safetensors header: {}", e)))?;

        let header_json = String::from_utf8(header_bytes)
            .map_err(|e| Error::Format(format!("safetensors header is not valid UTF-8: {}", e)))?;

        // data begins immediately after [8-byte len] + [header_json]
        let data_start = 8 + header_len;

        // Parse the header JSON as an IndexMap to preserve insertion order
        let raw: IndexMap<String, serde_json::Value> =
            serde_json::from_str(&header_json).map_err(|e| {
                Error::Format(format!("failed to parse safetensors header JSON: {}", e))
            })?;

        let mut tensors: IndexMap<String, TensorEntry> = IndexMap::new();
        let mut user_metadata: HashMap<String, String> = HashMap::new();

        for (key, value) in raw {
            if key == "__metadata__" {
                if let Some(obj) = value.as_object() {
                    for (mk, mv) in obj {
                        if let Some(s) = mv.as_str() {
                            user_metadata.insert(mk.clone(), s.to_string());
                        }
                    }
                }
                continue;
            }

            let raw_entry: RawTensorEntry = serde_json::from_value(value).map_err(|e| {
                Error::Format(format!("failed to parse tensor entry {:?}: {}", key, e))
            })?;

            let tensor_data_start = data_start + raw_entry.data_offsets[0];
            let tensor_data_end = data_start + raw_entry.data_offsets[1];

            tensors.insert(
                key,
                TensorEntry {
                    dtype: raw_entry.dtype,
                    shape: raw_entry.shape,
                    data_start: tensor_data_start,
                    data_end: tensor_data_end,
                },
            );
        }

        Ok(SafetensorsFile {
            tensors,
            user_metadata,
            header_json,
            data_start,
        })
    }

    /// Compute the total size of all tensor data in the file.
    pub fn total_data_bytes(&self) -> u64 {
        self.tensors
            .values()
            .map(|t| t.data_end - t.data_start)
            .sum()
    }

    /// Serialize a new safetensors header with updated `data_offsets`.
    ///
    /// `tensor_offsets` maps tensor name to `[start, end]` relative to the
    /// new data region. Returns `[8-byte len][header JSON bytes]`.
    pub fn to_header_prefix(&self, tensor_offsets: &IndexMap<String, [u64; 2]>) -> Result<Vec<u8>> {
        let mut map: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();

        for (name, entry) in &self.tensors {
            let offsets = tensor_offsets
                .get(name)
                .ok_or_else(|| Error::Format(format!("missing offset for tensor {:?}", name)))?;

            let obj = serde_json::json!({
                "dtype": entry.dtype,
                "shape": entry.shape,
                "data_offsets": [offsets[0], offsets[1]],
            });
            map.insert(name.clone(), obj);
        }

        // Add __metadata__ if present
        if !self.user_metadata.is_empty() {
            let meta: serde_json::Value = self
                .user_metadata
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                .collect::<serde_json::Map<_, _>>()
                .into();
            map.insert("__metadata__".to_string(), meta);
        }

        let json_bytes = serde_json::to_vec(&serde_json::Value::Object(map))
            .map_err(|e| Error::Format(format!("failed to serialize safetensors header: {}", e)))?;

        let header_len = json_bytes.len() as u64;
        let mut out = Vec::with_capacity(8 + json_bytes.len());
        out.extend_from_slice(&header_len.to_le_bytes());
        out.extend_from_slice(&json_bytes);

        Ok(out)
    }
}

/// Read exactly `length` bytes from `file` at `offset`.
pub fn pread(file: &mut File, offset: u64, length: u64) -> Result<Vec<u8>> {
    file.seek(SeekFrom::Start(offset))?;
    let mut buf = vec![0u8; length as usize];
    file.read_exact(&mut buf)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn make_safetensors_file() -> NamedTempFile {
        let header = r#"{"weight":{"dtype":"F32","shape":[2,4],"data_offsets":[0,32]},"bias":{"dtype":"F32","shape":[4],"data_offsets":[32,48]}}"#;
        let header_bytes = header.as_bytes();
        let header_len = header_bytes.len() as u64;

        let mut f = NamedTempFile::new().unwrap();
        f.write_all(&header_len.to_le_bytes()).unwrap();
        f.write_all(header_bytes).unwrap();
        f.write_all(&[0u8; 48]).unwrap();
        f
    }

    #[test]
    fn test_open_parses_tensors() {
        let f = make_safetensors_file();
        let st = SafetensorsFile::open(f.path()).unwrap();
        assert_eq!(st.tensors.len(), 2);
        assert!(st.tensors.contains_key("weight"));
        assert!(st.tensors.contains_key("bias"));

        let weight = &st.tensors["weight"];
        assert_eq!(weight.dtype, "F32");
        assert_eq!(weight.shape, vec![2, 4]);
    }

    #[test]
    fn test_total_data_bytes() {
        let f = make_safetensors_file();
        let st = SafetensorsFile::open(f.path()).unwrap();
        assert_eq!(st.total_data_bytes(), 48);
    }

    #[test]
    fn test_to_header_prefix_roundtrip() {
        let f = make_safetensors_file();
        let st = SafetensorsFile::open(f.path()).unwrap();

        let mut offsets: IndexMap<String, [u64; 2]> = IndexMap::new();
        offsets.insert("weight".to_string(), [0, 32]);
        offsets.insert("bias".to_string(), [32, 48]);

        let prefix = st.to_header_prefix(&offsets).unwrap();
        assert!(prefix.len() > 8);
        let len = u64::from_le_bytes(prefix[..8].try_into().unwrap());
        assert_eq!(len as usize, prefix.len() - 8);
    }
}
