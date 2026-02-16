#![no_main]
use hexz_core::format::header::Header;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Attempt to deserialize a Header from arbitrary bytes — should not panic
    let _: Result<Header, _> = bincode::deserialize(data);
});
