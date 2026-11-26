//! Job identifiers name workspace directories, so an accepted identifier must
//! never be able to change the directory a workspace is created in.

#![no_main]

use libfuzzer_sys::fuzz_target;
use paperwarden_core::JobId;
use std::path::{Component, Path};

fuzz_target!(|data: &[u8]| {
    let Ok(value) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(id) = JobId::new(value) else {
        return;
    };
    assert_eq!(id.as_str(), value);
    assert!(!value.is_empty());
    assert!(value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'));

    // Embedded in a workspace name, it stays one plain path component.
    let name = format!("pw-{}", id.as_str());
    let mut components = Path::new(&name).components();
    assert!(matches!(components.next(), Some(Component::Normal(_))));
    assert!(components.next().is_none());
});
