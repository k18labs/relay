#![no_main]

use libfuzzer_sys::fuzz_target;
use relay::config::Config;

fuzz_target!(|input: &str| {
    if let Ok(config) = Config::parse(input) {
        for route in &config.routes {
            let _ = config.route(&route.host, &route.path);
            let _ = config.route(&route.host, "/fuzz-probe");
        }
    }
});
