#![no_main]

use http::{HeaderMap, HeaderName, HeaderValue};
use libfuzzer_sys::fuzz_target;
use relay::proxy::strip_hop_headers;

fuzz_target!(|input: &[u8]| {
    let mut headers = HeaderMap::new();
    for field in input.chunks(256).take(128) {
        let Some(separator) = field.iter().position(|byte| *byte == b':') else {
            continue;
        };
        let (name, value) = field.split_at(separator);
        let value = &value[1..];
        if let (Ok(name), Ok(value)) =
            (HeaderName::from_bytes(name), HeaderValue::from_bytes(value))
        {
            headers.append(name, value);
        }
    }
    strip_hop_headers(&mut headers);
});
