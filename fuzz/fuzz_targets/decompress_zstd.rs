#![no_main]
use hexz_core::algo::compression::zstd::ZstdCompressor;
use hexz_core::algo::compression::Compressor;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let compressor = ZstdCompressor::new(3, None);

    // decompress arbitrary bytes — should not panic
    let _ = compressor.decompress(data);

    // decompress_into arbitrary bytes
    let mut out = vec![0u8; data.len().saturating_mul(4).max(1024)];
    let _ = compressor.decompress_into(data, &mut out);
});
