#![no_main]
use hexz_core::format::index::{BlockInfo, IndexPage, MasterIndex, PageEntry};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // MasterIndex deserialization — should not panic
    let _: Result<MasterIndex, _> = bincode::deserialize(data);

    // IndexPage deserialization
    let _: Result<IndexPage, _> = bincode::deserialize(data);

    // Individual structs
    let _: Result<BlockInfo, _> = bincode::deserialize(data);
    let _: Result<PageEntry, _> = bincode::deserialize(data);
});
