//! Archive entry names are attacker-controlled (PRODUCT_INVARIANTS.md 11).
//! Any name `EntryPath` accepts, as UTF-8 or as a legacy CP437 name, must stay
//! inside the extraction directory, respect every bound, and be stable when
//! re-parsed.

#![no_main]

use libfuzzer_sys::fuzz_target;
use paperwarden_core::archive::{EntryPath, MAX_COMPONENT_BYTES, MAX_DEPTH, MAX_PATH_BYTES};
use std::path::{Component, Path};

fn check(path: &EntryPath) {
    let components = path.components();
    assert!(!components.is_empty());
    assert!(components.len() <= MAX_DEPTH);
    assert!(components.iter().map(String::len).sum::<usize>() <= MAX_PATH_BYTES);
    for component in components {
        assert!(!component.is_empty() && component.len() <= MAX_COMPONENT_BYTES);
        assert!(component != "." && component != "..");
        assert!(!component.contains(['/', '\\', ':']));
        assert!(!component.chars().any(char::is_control));
        assert!(!component.contains(['\u{202e}', '\u{2067}']));
    }

    // On this platform's path rules, every part is a plain name and the path
    // cannot leave the directory it is joined to.
    let relative = path.to_relative_path();
    assert!(relative.is_relative());
    assert!(relative
        .components()
        .all(|component| matches!(component, Component::Normal(_))));
    assert_eq!(relative.components().count(), components.len());
    let root = Path::new("extract-root");
    assert!(root.join(&relative).starts_with(root));

    // Parsing is idempotent on the canonical UTF-8 form.
    let mut canonical = components.join("/");
    if path.is_directory() {
        canonical.push('/');
    }
    assert_eq!(EntryPath::parse(canonical.as_bytes()).as_ref(), Ok(path));
}

fuzz_target!(|data: &[u8]| {
    let utf8 = EntryPath::parse(data);
    if let Ok(path) = &utf8 {
        assert!(data.len() <= MAX_PATH_BYTES);
        check(path);
    }
    let legacy = EntryPath::parse_cp437(data);
    if let Ok(path) = &legacy {
        check(path);
    }
    // ASCII names mean the same in both encodings.
    if data.is_ascii() {
        assert_eq!(utf8, legacy);
    }
});
