//! Zstandard block compression with optional dictionary support.
//!
//! Implements the `Compressor` trait using the `zstd` crate. Supports
//! trained dictionaries for higher ratios on structured data; the same
//! dictionary must be used for encode and decode.

use crate::algo::compression::Compressor;
use std::io::{Cursor, Read, Write};
use strata_common::{Result, StrataError};
use zstd::dict::{DecoderDictionary, EncoderDictionary};

/// Zstandard compressor with optional shared dictionary.
///
/// **Architectural intent:** Provides a higher-ratio compressor than LZ4 and
/// supports trained dictionaries to significantly improve compression on
/// structured data.
///
/// **Constraints:** The same dictionary bytes must be provided to both
/// encoder and decoder; the lifetime of the leaked dictionary slice is tied to
/// the process and should be considered global.
pub struct ZstdCompressor {
    level: i32,
    encoder_dict: Option<EncoderDictionary<'static>>,
    decoder_dict: Option<DecoderDictionary<'static>>,
}

impl std::fmt::Debug for ZstdCompressor {
    /// Renders a summary of the compressor configuration for diagnostics.
    ///
    /// **Architectural intent:** Exposes the compression level and presence of
    /// a dictionary without revealing the dictionary contents.
    ///
    /// **Constraints:** The format is intended for human consumption only and
    /// may change between versions.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ZstdCompressor")
            .field("level", &self.level)
            .field("has_dict", &self.encoder_dict.is_some())
            .finish()
    }
}

impl ZstdCompressor {
    /// Constructs a new Zstandard compressor with an optional training dictionary.
    ///
    /// **Architectural intent:** Prepares reusable encoder and decoder
    /// dictionaries so that blocks can be compressed and decompressed
    /// efficiently across the lifetime of the process.
    ///
    /// **Constraints:** When a dictionary is provided, its bytes are leaked to
    /// obtain a `'static` lifetime; this is acceptable because the compressor
    /// is expected to live for the duration of the application.
    ///
    /// **Side effects:** Allocates native zstd dictionary structures and
    /// permanently pins the underlying dictionary memory.
    pub fn new(level: i32, dict: Option<Vec<u8>>) -> Self {
        let (encoder_dict, decoder_dict) = if let Some(d) = &dict {
            let leaked_dict = Box::leak(d.clone().into_boxed_slice());
            (
                Some(EncoderDictionary::copy(leaked_dict, level)),
                Some(DecoderDictionary::copy(leaked_dict)),
            )
        } else {
            (None, None)
        };

        Self {
            level,
            encoder_dict,
            decoder_dict,
        }
    }

    /// Trains a Zstandard dictionary from representative sample blocks.
    ///
    /// **Architectural intent:** Uses zstd's built-in training facilities to
    /// build a dictionary that captures common patterns in the input, reducing
    /// average encoded size for similar data.
    ///
    /// **Constraints:** The provided `samples` must be representative of the
    /// workload; an unrepresentative or tiny sample set may yield a dictionary
    /// that hurts compression.
    pub fn train(samples: &[Vec<u8>], max_size: usize) -> Result<Vec<u8>> {
        zstd::dict::from_samples(samples, max_size)
            .map_err(|e| StrataError::Compression(format!("Failed to train dict: {}", e)))
    }

    /// Reads decompressed bytes from an arbitrary zstd decoder into `out`.
    ///
    /// **Architectural intent:** Normalizes the logic for draining different
    /// decoder types (with and without dictionaries) into a contiguous output
    /// buffer.
    ///
    /// **Constraints:** Stops early if the decoder reaches EOF before filling
    /// the buffer; callers must interpret the returned size appropriately.
    fn read_into_buf<R: Read>(reader: &mut R, out: &mut [u8]) -> Result<usize> {
        let mut total = 0;
        while total < out.len() {
            let n = reader
                .read(&mut out[total..])
                .map_err(|e| StrataError::Compression(e.to_string()))?;
            if n == 0 {
                break;
            }
            total += n;
        }
        Ok(total)
    }
}

impl Compressor for ZstdCompressor {
    /// Compresses a block of data using Zstandard, optionally with a dictionary.
    ///
    /// **Architectural intent:** Uses a streaming encoder when a prepared
    /// dictionary is available to avoid re-parsing and to maximize throughput.
    ///
    /// **Constraints:** All blocks compressed with a dictionary must be
    /// decompressed with a compatible dictionary instance; otherwise
    /// decompression will fail.
    fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        if let Some(dict) = &self.encoder_dict {
            let mut encoder = zstd::stream::write::Encoder::with_prepared_dictionary(
                Vec::with_capacity(data.len()),
                dict,
            )
            .map_err(|e| StrataError::Compression(e.to_string()))?;

            encoder
                .write_all(data)
                .map_err(|e| StrataError::Compression(e.to_string()))?;
            encoder
                .finish()
                .map_err(|e| StrataError::Compression(e.to_string()))
        } else {
            zstd::stream::encode_all(Cursor::new(data), self.level)
                .map_err(|e| StrataError::Compression(e.to_string()))
        }
    }

    /// Decompresses a Zstandard payload into an owned buffer.
    ///
    /// **Architectural intent:** Handles both dictionary and non-dictionary
    /// cases while providing a single entry point for callers.
    ///
    /// **Constraints:** When a dictionary is configured, the input must have
    /// been produced with a compatible dictionary; otherwise, the decoder will
    /// report a compression error.
    fn decompress(&self, data: &[u8]) -> Result<Vec<u8>> {
        if let Some(dict) = &self.decoder_dict {
            let mut decoder =
                zstd::stream::read::Decoder::with_prepared_dictionary(Cursor::new(data), dict)
                    .map_err(|e| StrataError::Compression(e.to_string()))?;

            let mut out = Vec::with_capacity(data.len() * 2);
            decoder
                .read_to_end(&mut out)
                .map_err(|e| StrataError::Compression(e.to_string()))?;
            Ok(out)
        } else {
            zstd::stream::decode_all(Cursor::new(data))
                .map_err(|e| StrataError::Compression(e.to_string()))
        }
    }

    /// Decompresses a Zstandard payload directly into a caller-provided buffer.
    ///
    /// **Architectural intent:** Avoids repeated allocations on hot paths by
    /// allowing callers to manage decode buffers explicitly.
    ///
    /// **Constraints:** The same dictionary choice as used during compression
    /// must be observed; internally we select different decoder types for
    /// dictionary vs non-dictionary cases.
    fn decompress_into(&self, data: &[u8], out: &mut [u8]) -> Result<usize> {
        if let Some(dict) = &self.decoder_dict {
            let mut decoder =
                zstd::stream::read::Decoder::with_prepared_dictionary(Cursor::new(data), dict)
                    .map_err(|e| StrataError::Compression(e.to_string()))?;

            Self::read_into_buf(&mut decoder, out)
        } else {
            let mut decoder = zstd::stream::read::Decoder::new(Cursor::new(data))
                .map_err(|e| StrataError::Compression(e.to_string()))?;

            Self::read_into_buf(&mut decoder, out)
        }
    }
}
