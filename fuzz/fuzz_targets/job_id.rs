#![no_main]

use libfuzzer_sys::fuzz_target;
use paperwarden_core::JobId;

fuzz_target!(|data: &[u8]| {
    if let Ok(value) = std::str::from_utf8(data) {
        let _ = JobId::new(value);
    }
});
