//! The content validator must never panic on hostile headers or extreme
//! offsets, and must accept exactly the inputs carrying its signature
//! (PRODUCT_INVARIANTS.md 10).

#![no_main]

use libfuzzer_sys::arbitrary::{Result, Unstructured};
use libfuzzer_sys::fuzz_target;
use paperwarden_core::validation::{InputValidator, MagicBytes, StagedInput, HEADER_LEN};

/// Real signatures plus edge cases: empty, one byte, longer than most headers.
const SIGNATURES: &[&[u8]] = &[
    b"%PDF-",
    b"PK\x03\x04",
    b"\x89PNG\r\n\x1a\n",
    b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1",
    b"",
    b"A",
    &[0x55; 64],
];

fn check(data: &[u8]) -> Result<()> {
    let mut input = Unstructured::new(data);
    let signature = *input.choose(SIGNATURES)?;
    let offset = if input.ratio(1, 16)? {
        usize::MAX - input.int_in_range(0..=8)?
    } else {
        input.int_in_range(0..=128)?
    };
    let claimed_extra: u32 = input.arbitrary()?;
    let rest = input.take_rest();
    let header = rest.get(..HEADER_LEN.min(rest.len())).unwrap_or(rest);
    let len = header.len() as u64 + u64::from(claimed_extra);

    let accepted = MagicBytes::new(offset, signature)
        .validate(&StagedInput::new(header, len))
        .is_ok();
    let expected = !signature.is_empty()
        && offset
            .checked_add(signature.len())
            .and_then(|end| header.get(offset..end))
            == Some(signature);
    assert_eq!(accepted, expected);
    Ok(())
}

fuzz_target!(|data: &[u8]| {
    let _ = check(data);
});
