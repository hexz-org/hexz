#![no_main]
use hexz_core::algo::compression::zstd::ZstdCompressor;
use hexz_core::algo::compression::Compressor;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Clamp to avoid excessive allocation during dictionary parsing
    let dict_data = &data[..data.len().min(256 * 1024)];

    // Constructing a ZstdCompressor with arbitrary dictionary bytes — should not panic
    let compressor = ZstdCompressor::new(3, Some(dict_data.to_vec()));

    // Attempt to decompress garbage with this dictionary-loaded compressor
    let _ = compressor.decompress(data);

    // Also test decompress_into
    let mut out = vec![0u8; data.len().saturating_mul(4).max(1024)];
    let _ = compressor.decompress_into(data, &mut out);
});
