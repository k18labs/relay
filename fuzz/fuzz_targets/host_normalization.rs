#![no_main]

use libfuzzer_sys::fuzz_target;
use relay::config::normalize_host;

fuzz_target!(|input: &[u8]| {
    let host = String::from_utf8_lossy(input);
    let _ = normalize_host(&host);
});
