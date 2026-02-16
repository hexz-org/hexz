#![no_main]
use hexz_core::format::index::MasterIndex;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Test MasterIndex::read_from with a Cursor (simulates reading from a file)
    // This exercises the MAX_INDEX_SIZE check and bincode deserialization
    let mut cursor = std::io::Cursor::new(data);
    let _ = MasterIndex::read_from(&mut cursor, 0);

    // Also test read_from_bounded with various lengths
    let mut cursor2 = std::io::Cursor::new(data);
    let _ = MasterIndex::read_from_bounded(&mut cursor2, 0, data.len() as u64);

    // Test with non-zero offset (exercises seek logic)
    if data.len() > 16 {
        let mut cursor3 = std::io::Cursor::new(data);
        let _ = MasterIndex::read_from(&mut cursor3, 8);
    }
});
