//! The pre-check against every PDF in the corpus. Each fixture's expected
//! result follows from its corpus outcome (`docs/ci/corpus.md`):
//!
//! - `accept`: allowed, with no damage and no active content;
//! - `repair`: allowed and marked damaged, with the specific damage found;
//! - `neutralize`: allowed, with the specific active content flagged;
//! - `bound`: stopped with the specific limit, unless the bound belongs to the
//!   renderer (the page-size fixture);
//! - `reject`: refused, or marked damaged so the renderer's refusal stands.

#![cfg(test)]

use paperwarden_pdf_scan::{scan, ActiveContent, Damage, Limit, Report, ScanError, ScanLimits};
use std::path::Path;

fn corpus_root() -> std::path::PathBuf {
    std::env::var_os("PAPERWARDEN_CORPUS")
        .map(Into::into)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../tests/corpus"))
}

fn run(relative: &str) -> Result<Report, ScanError> {
    let bytes = std::fs::read(corpus_root().join(relative))
        .unwrap_or_else(|error| panic!("read {relative}: {error}"));
    scan(&bytes, &ScanLimits::default(), &mut || Ok(()))
}

enum Expect {
    Clean,
    Damaged(Damage),
    Active(&'static [ActiveContent]),
    Stopped(ScanError),
    /// Refused here, or damaged so that PDFium's refusal stands.
    Refused,
    /// The bound is the renderer's; the pre-check lets it through clean.
    RendererBound,
}

const EXPECTED: &[(&str, Expect)] = &[
    ("valid/pdf/one-page.pdf", Expect::Clean),
    ("valid/pdf/two-pages-flate.pdf", Expect::Clean),
    ("valid/pdf/incremental-update.pdf", Expect::Clean),
    ("valid/pdf/many-pages-200.pdf", Expect::Clean),
    ("valid/pdf/large-image.pdf", Expect::Clean),
    (
        "malformed/pdf/header-only.pdf",
        Expect::Stopped(ScanError::NoObjects),
    ),
    ("malformed/truncated.pdf", Expect::Refused),
    (
        "malformed/pdf/bad-xref-offsets.pdf",
        Expect::Damaged(Damage::CrossReference),
    ),
    (
        "malformed/pdf/no-eof-marker.pdf",
        Expect::Damaged(Damage::MissingEndOfFile),
    ),
    (
        "security/pdf/xref-prev-loop.pdf",
        Expect::Damaged(Damage::PrevCycle),
    ),
    (
        "security/pdf/stream-length-overflow.pdf",
        Expect::Damaged(Damage::StreamLength),
    ),
    (
        "security/pdf/javascript-openaction.pdf",
        Expect::Active(&[ActiveContent::JavaScript]),
    ),
    (
        "security/pdf/launch-and-uri-actions.pdf",
        Expect::Active(&[ActiveContent::Launch, ActiveContent::Uri]),
    ),
    (
        "security/pdf/deep-nesting.pdf",
        Expect::Stopped(ScanError::Limit(Limit::Nesting)),
    ),
    (
        "security/pdf/flate-bomb.pdf",
        Expect::Stopped(ScanError::Limit(Limit::CompressionRatio)),
    ),
    ("security/pdf/extreme-page-size.pdf", Expect::RendererBound),
    // Encrypted with pypdf (PW-042): measured after decryption.
    ("valid/pdf/encrypted-rc4-128.pdf", Expect::Clean),
    ("valid/pdf/encrypted-aes-128.pdf", Expect::Clean),
    ("valid/pdf/encrypted-aes-256.pdf", Expect::Clean),
    (
        "security/pdf/encrypted-flate-bomb.pdf",
        Expect::Stopped(ScanError::Limit(Limit::CompressionRatio)),
    ),
    (
        "security/pdf/encrypted-javascript.pdf",
        Expect::Active(&[ActiveContent::JavaScript]),
    ),
    (
        "security/pdf/encrypted-needs-password.pdf",
        Expect::Stopped(ScanError::PasswordRequired),
    ),
];

#[test]
fn every_corpus_pdf_meets_its_outcome() {
    for (fixture, expect) in EXPECTED {
        let result = run(fixture);
        match expect {
            Expect::Clean | Expect::RendererBound => {
                let report = result.unwrap_or_else(|error| panic!("{fixture}: {error:?}"));
                assert!(report.damage.is_empty(), "{fixture}: {:?}", report.damage);
                assert!(
                    report.active_content.is_empty(),
                    "{fixture}: {:?}",
                    report.active_content
                );
            }
            Expect::Damaged(damage) => {
                let report = result.unwrap_or_else(|error| panic!("{fixture}: {error:?}"));
                assert!(
                    report.damage.contains(damage),
                    "{fixture}: {:?}",
                    report.damage
                );
                assert!(
                    report.active_content.is_empty(),
                    "{fixture}: {:?}",
                    report.active_content
                );
            }
            Expect::Active(kinds) => {
                let report = result.unwrap_or_else(|error| panic!("{fixture}: {error:?}"));
                assert_eq!(report.active_content, *kinds, "{fixture}");
                assert!(report.damage.is_empty(), "{fixture}: {:?}", report.damage);
            }
            Expect::Stopped(error) => assert_eq!(result.as_ref().err(), Some(error), "{fixture}"),
            Expect::Refused => {
                if let Ok(report) = result {
                    assert!(report.is_damaged(), "{fixture} passed clean");
                }
            }
        }
    }
}

#[test]
fn every_corpus_pdf_has_an_expectation() {
    let manifest = std::fs::read_to_string(corpus_root().join("manifest.json")).expect("manifest");
    let listed: Vec<&str> = manifest
        .split('"')
        .filter(|item| item.ends_with(".pdf"))
        .collect();
    assert!(!listed.is_empty(), "no PDFs in the manifest");
    for path in listed {
        assert!(
            EXPECTED.iter().any(|(fixture, _)| *fixture == path),
            "{path} has no pre-check expectation"
        );
    }
}

#[test]
fn the_bomb_stops_after_a_bounded_amount_of_work() {
    let bytes = std::fs::read(corpus_root().join("security/pdf/flate-bomb.pdf")).expect("read");
    let mut checks = 0u32;
    let result = scan(&bytes, &ScanLimits::default(), &mut || {
        checks += 1;
        Ok(())
    });
    assert_eq!(result, Err(ScanError::Limit(Limit::CompressionRatio)));
    // Stopped near the ratio cap (16 KiB x 256), well before the full 16 MiB.
    assert!(checks < 16, "{checks} checkpoints");
}

#[test]
fn a_checkpoint_can_stop_the_scan() {
    let bytes = std::fs::read(corpus_root().join("valid/pdf/many-pages-200.pdf")).expect("read");
    let result = scan(&bytes, &ScanLimits::default(), &mut || {
        Err(paperwarden_pdf_scan::Interrupted)
    });
    assert_eq!(result, Err(ScanError::Interrupted));
}

#[test]
fn the_large_image_is_bounded_by_its_dimensions() {
    let bytes = std::fs::read(corpus_root().join("valid/pdf/large-image.pdf")).expect("read");
    let tight = ScanLimits {
        max_image_pixels: 4000 * 4000 - 1,
        ..ScanLimits::default()
    };
    assert_eq!(
        scan(&bytes, &tight, &mut || Ok(())),
        Err(ScanError::Limit(Limit::ImageSize))
    );
    let report = scan(&bytes, &ScanLimits::default(), &mut || Ok(())).expect("allowed");
    assert!(
        report.decoded_bytes >= 48_000_000,
        "{}",
        report.decoded_bytes
    );
}

#[test]
fn findings_become_the_job_runner_notices() {
    use paperwarden_core::{Inert, Notice};
    let cases: [(&str, &[Notice]); 7] = [
        ("valid/pdf/one-page.pdf", &[]),
        ("malformed/pdf/bad-xref-offsets.pdf", &[Notice::Repaired]),
        ("malformed/pdf/no-eof-marker.pdf", &[Notice::Repaired]),
        ("security/pdf/xref-prev-loop.pdf", &[Notice::Repaired]),
        (
            "security/pdf/stream-length-overflow.pdf",
            &[Notice::Repaired],
        ),
        (
            "security/pdf/javascript-openaction.pdf",
            &[Notice::NotRun(Inert::Scripts)],
        ),
        (
            "security/pdf/launch-and-uri-actions.pdf",
            &[
                Notice::NotRun(Inert::LaunchActions),
                Notice::NotRun(Inert::WebLinks),
            ],
        ),
    ];
    for (fixture, expected) in cases {
        let report = run(fixture).unwrap_or_else(|error| panic!("{fixture}: {error:?}"));
        assert_eq!(report.notices(), expected, "{fixture}");
    }
}

#[test]
fn encrypted_pdfs_are_decrypted_before_they_are_measured() {
    // The plain text is only in the decrypted content stream, so a scan that
    // measured ciphertext would decode almost nothing (or find it corrupt).
    for fixture in [
        "valid/pdf/encrypted-rc4-128.pdf",
        "valid/pdf/encrypted-aes-128.pdf",
        "valid/pdf/encrypted-aes-256.pdf",
    ] {
        let report = run(fixture).unwrap_or_else(|error| panic!("{fixture}: {error:?}"));
        assert!(
            report.decoded_bytes >= "BT /F1 18 Tf 20 40 Td (PaperWarden corpus) Tj ET".len() as u64,
            "{fixture} decoded {} bytes",
            report.decoded_bytes
        );
        assert!(report.damage.is_empty(), "{fixture}: {:?}", report.damage);
    }
}
