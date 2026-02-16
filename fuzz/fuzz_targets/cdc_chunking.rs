#![no_main]
use hexz_core::algo::dedup::cdc::StreamChunker;
use hexz_core::algo::dedup::dcam::DedupeParams;
use libfuzzer_sys::fuzz_target;
use std::io::Cursor;

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }

    let params = DedupeParams {
        f: 14,    // 2^14 = 16KB average
        m: 2048,  // 2KB minimum
        z: 65536, // 64KB maximum
        w: 48,
        v: 16,
    };

    let chunker = StreamChunker::new(Cursor::new(data), params);
    for chunk_result in chunker {
        // Each chunk should either succeed or return an error, never panic
        let _ = chunk_result;
    }
});
