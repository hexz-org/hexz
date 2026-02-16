#![no_main]
use hexz_core::format::header::Header;
use hexz_core::format::magic::{FORMAT_VERSION, HEADER_SIZE, MAGIC_BYTES};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Test the full Header::read_from pipeline with a Cursor (simulating file I/O)
    let mut cursor = std::io::Cursor::new(data);
    let result = Header::read_from(&mut cursor);

    // If deserialization succeeds, validate the header fields
    if let Ok(header) = result {
        // Magic bytes check
        if header.magic != *MAGIC_BYTES {
            return; // Invalid magic — real code would reject
        }

        // Version check
        if header.version != FORMAT_VERSION {
            return; // Incompatible version — real code would reject
        }

        // Block size sanity (must be non-zero, reasonable power of 2)
        if header.block_size == 0 || header.block_size > 16 * 1024 * 1024 {
            return;
        }

        // Roundtrip: serialize back and check it doesn't panic
        let _ = bincode::serialize(&header);
    }

    // Also test with padded HEADER_SIZE input (what the real code path does)
    if data.len() >= HEADER_SIZE {
        let mut padded_cursor = std::io::Cursor::new(&data[..HEADER_SIZE]);
        let _ = Header::read_from(&mut padded_cursor);
    }
});
