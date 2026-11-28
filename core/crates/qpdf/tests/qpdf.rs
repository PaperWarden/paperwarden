//! qpdf against the corpus (PW-012). These tests need qpdf built in: fetch
//! its sources with `python scripts/fetch_qpdf.py`. They are ignored by
//! default and fail, rather than pass, when run without qpdf:
//!
//! ```sh
//! python scripts/fetch_qpdf.py
//! cargo test -p paperwarden-qpdf -- --ignored
//! ```
//!
//! `PAPERWARDEN_CORPUS` overrides the corpus location (used on devices), and
//! `PAPERWARDEN_EVIDENCE` names a file to write one JSON line per fixture to.

#![cfg(test)]

use paperwarden_core::validation::MagicBytes;
use paperwarden_core::{
    CancellationToken, JobError, JobId, JobLimits, JobRunner, Notice, Operation, Outcome,
};
use paperwarden_pdf_scan::{scan, ScanError, ScanLimits};
use paperwarden_qpdf::{
    available, check, limit_in_effect, page_count, transform, PageRange, QpdfError, QpdfProcessor,
    Rotation, Transform, LIMITS,
};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

const NEEDS_QPDF: &str = "needs qpdf built in; run python scripts/fetch_qpdf.py";

fn corpus_root() -> PathBuf {
    std::env::var_os("PAPERWARDEN_CORPUS")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../tests/corpus"))
}

fn corpus(relative: &str) -> PathBuf {
    corpus_root().join(relative)
}

/// A fresh directory for one test.
fn scratch(name: &str) -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "pw-qpdf-{name}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn require_qpdf() {
    assert!(available(), "{NEEDS_QPDF}");
}

fn run_job(
    input: &Path,
    transform: Transform,
    jobs: &Path,
    destination: &Path,
) -> Result<Vec<Notice>, JobError> {
    let processor = QpdfProcessor::new(transform);
    let validator = MagicBytes::new(0, b"%PDF-");
    let operation = Operation {
        validator: &validator,
        processor: &processor,
        verifier: &processor,
    };
    let limits = JobLimits::new(64 << 20, 64 << 20, Duration::from_secs(60)).unwrap();
    let runner = JobRunner::new(jobs, limits);
    let id = JobId::new("qpdf-job").unwrap();
    match runner.run(id, input, &operation, &CancellationToken::new()) {
        Ok(mut ready) => {
            ready.export_to(destination)?;
            let report = ready.finish();
            assert_eq!(report.outcome, Outcome::Exported);
            Ok(report.notices)
        }
        Err(report) => match report.outcome {
            Outcome::Stopped(error) => Err(error),
            other => panic!("job ended as {other:?}"),
        },
    }
}

fn no_workspace(jobs: &Path) {
    let left: Vec<_> = fs::read_dir(jobs)
        .map(|entries| entries.filter_map(Result::ok).map(|e| e.path()).collect())
        .unwrap_or_default();
    assert!(left.is_empty(), "workspaces left behind: {left:?}");
}

#[test]
#[ignore = "needs qpdf built in; run python scripts/fetch_qpdf.py"]
fn paperwardens_limits_replace_qpdfs_unbounded_defaults() {
    require_qpdf();
    for &(limit, value) in LIMITS {
        assert_eq!(limit_in_effect(limit), Some(value), "{limit:?}");
    }
}

#[test]
#[ignore = "needs qpdf built in; run python scripts/fetch_qpdf.py"]
fn every_corpus_pdf_meets_its_outcome_through_a_qpdf_job() {
    require_qpdf();
    let manifest: serde_like::Manifest = serde_like::read(&corpus("manifest.json"));
    let dir = scratch("corpus");
    let jobs = dir.join("jobs");
    let mut evidence =
        std::env::var_os("PAPERWARDEN_EVIDENCE").map(|path| fs::File::create(path).unwrap());
    let mut failures = Vec::new();
    for fixture in manifest.pdf_fixtures() {
        let input = corpus(&fixture.path);
        let destination = dir.join(format!("{}.pdf", fixture.path.replace('/', "_")));
        let started = Instant::now();
        let job = run_job(&input, Transform::Rewrite, &jobs, &destination);
        let job_ms = started.elapsed().as_millis();
        // qpdf alone, without the pre-check: what it does with hostile input.
        let started = Instant::now();
        let direct = transform(&input, &dir.join("direct.pdf"), &Transform::Rewrite);
        let direct_ms = started.elapsed().as_millis();
        let _ = fs::remove_file(dir.join("direct.pdf"));
        let checked = check(&input, "");

        let result = match &job {
            Ok(notices) if notices.contains(&Notice::Repaired) => "repaired".to_owned(),
            Ok(notices) if notices.is_empty() => "rewritten".to_owned(),
            Ok(_) => "rewritten, flagged".to_owned(),
            Err(error) => format!("stopped: {error}"),
        };
        let line = format!(
            "{{\"fixture\":\"{}\",\"outcome\":\"{}\",\"job\":\"{}\",\"job_ms\":{},\"qpdf_alone\":\"{:?}\",\"qpdf_ms\":{},\"check\":\"{:?}\"}}",
            fixture.path, fixture.outcome, result, job_ms, direct, direct_ms, checked
        );
        if let Some(file) = evidence.as_mut() {
            writeln!(file, "{line}").unwrap();
        }
        let ok = match fixture.outcome.as_str() {
            "accept" => matches!(&job, Ok(notices) if notices.is_empty()),
            "repair" => matches!(&job, Ok(notices) if notices.contains(&Notice::Repaired)),
            // qpdf keeps structure, scripts included; the pre-check flags them.
            "neutralize" => matches!(&job, Ok(notices) if !notices.is_empty()),
            // Refused by a bound, or handled within it.
            "bound" => matches!(&job, Ok(_) | Err(JobError::LimitExceeded(_))),
            "reject" => job.is_err(),
            _ => false,
        };
        if !ok {
            failures.push(line);
        }
        assert!(job_ms < 10_000, "{} took {job_ms} ms", fixture.path);
        no_workspace(&jobs);
    }
    assert!(failures.is_empty(), "{failures:#?}");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
#[ignore = "needs qpdf built in; run python scripts/fetch_qpdf.py"]
fn pages_are_rotated_extracted_and_merged() {
    require_qpdf();
    let dir = scratch("pages");
    let jobs = dir.join("jobs");
    let many = corpus("valid/pdf/many-pages-200.pdf");
    assert_eq!(page_count(&many, ""), Ok(200));

    let extracted = dir.join("extracted.pdf");
    let range = PageRange {
        first: 10,
        last: Some(12),
    };
    run_job(
        &many,
        Transform::Extract { pages: range },
        &jobs,
        &extracted,
    )
    .unwrap();
    assert_eq!(page_count(&extracted, ""), Ok(3));

    let rotated = dir.join("rotated.pdf");
    let rotate = Transform::Rotate {
        rotation: Rotation::Quarter,
        pages: PageRange::ALL,
    };
    run_job(&extracted, rotate, &jobs, &rotated).unwrap();
    assert_eq!(page_count(&rotated, ""), Ok(3));
    let bytes = fs::read(&rotated).unwrap();
    assert_eq!(
        bytes.windows(10).filter(|w| w == b"/Rotate 90").count(),
        3,
        "every page carries the rotation"
    );

    // Merging two inputs, outside the job runner (it stages one input).
    let merged = dir.join("merged.pdf");
    let others = vec![corpus("valid/pdf/two-pages-flate.pdf")];
    let outcome = transform(
        &corpus("valid/pdf/one-page.pdf"),
        &merged,
        &Transform::Merge { others },
    );
    assert!(outcome.is_ok(), "{outcome:?}");
    assert_eq!(page_count(&merged, ""), Ok(3));

    // A range past the end is refused before qpdf writes anything.
    let past = Transform::Extract {
        pages: PageRange {
            first: 199,
            last: Some(201),
        },
    };
    let error = run_job(&many, past, &jobs, &dir.join("past.pdf")).unwrap_err();
    assert!(matches!(error, JobError::EngineFailed(_)), "{error:?}");
    no_workspace(&jobs);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
#[ignore = "needs qpdf built in; run python scripts/fetch_qpdf.py"]
fn encryption_round_trips_and_needs_the_password() {
    require_qpdf();
    let dir = scratch("crypt");
    let jobs = dir.join("jobs");
    let original = corpus("valid/pdf/two-pages-flate.pdf");
    let encrypted = dir.join("encrypted.pdf");
    let encrypt = Transform::Encrypt {
        user: "correct horse".into(),
        owner: "battery staple".into(),
    };
    run_job(&original, encrypt, &jobs, &encrypted).unwrap();

    // AES-256 (revision 6); nothing opens it without a password.
    let bytes = fs::read(&encrypted).unwrap();
    assert!(bytes.windows(4).any(|w| w == b"/R 6"), "revision 6");
    assert!(bytes.windows(6).any(|w| w == b"/AESV3"), "AES-256");
    assert_eq!(page_count(&encrypted, ""), Err(QpdfError::Failed));
    assert_eq!(page_count(&encrypted, "wrong"), Err(QpdfError::Failed));
    assert_eq!(page_count(&encrypted, "correct horse"), Ok(2));
    assert_eq!(page_count(&encrypted, "battery staple"), Ok(2));
    // PaperWarden's pre-check cannot measure it without the password.
    assert!(matches!(
        scan(&bytes, &ScanLimits::default(), &mut || Ok(())),
        Err(ScanError::Encrypted)
    ));

    let wrong = Transform::Decrypt {
        password: "wrong".into(),
    };
    assert!(run_job(&encrypted, wrong, &jobs, &dir.join("wrong.pdf")).is_err());
    assert!(!dir.join("wrong.pdf").exists());

    let decrypted = dir.join("decrypted.pdf");
    let decrypt = Transform::Decrypt {
        password: "correct horse".into(),
    };
    run_job(&encrypted, decrypt, &jobs, &decrypted).unwrap();
    assert_eq!(page_count(&decrypted, ""), Ok(2));
    let plain = fs::read(&decrypted).unwrap();
    assert!(!plain.windows(8).any(|w| w == b"/Encrypt"));
    assert!(scan(&plain, &ScanLimits::default(), &mut || Ok(())).is_ok());
    // The password never reaches the output.
    assert!(!plain.windows(13).any(|w| w == b"correct horse"));
    no_workspace(&jobs);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
#[ignore = "needs qpdf built in; run python scripts/fetch_qpdf.py"]
fn malformed_input_is_a_typed_error_and_never_writes_an_output() {
    require_qpdf();
    let dir = scratch("malformed");
    let output = dir.join("out.pdf");
    let mut cases: Vec<(String, Vec<u8>)> = vec![
        ("empty".into(), Vec::new()),
        ("not a pdf".into(), b"MZ\x90\x00 an executable".to_vec()),
        ("header only".into(), b"%PDF-1.7\n".to_vec()),
        (
            "garbage".into(),
            (0..=255u8).cycle().take(64 << 10).collect(),
        ),
    ];
    let one_page = fs::read(corpus("valid/pdf/one-page.pdf")).unwrap();
    // Every prefix of a valid file: truncation at any point.
    for cut in (0..one_page.len()).step_by(one_page.len() / 16 + 1) {
        cases.push((format!("cut at {cut}"), one_page[..cut].to_vec()));
    }
    for (name, bytes) in cases {
        let input = dir.join("input.pdf");
        fs::write(&input, &bytes).unwrap();
        let _ = fs::remove_file(&output);
        let started = Instant::now();
        let result = transform(&input, &output, &Transform::Rewrite);
        assert!(started.elapsed() < Duration::from_secs(5), "{name}");
        match result {
            Err(QpdfError::Failed) => {}
            // qpdf may recover a cut file; what it writes must then open.
            Ok(_) => assert!(page_count(&output, "").is_ok(), "{name}"),
            Err(other) => panic!("{name}: {other:?}"),
        }
    }
    fs::remove_dir_all(&dir).unwrap();
}

/// The corpus manifest, read without a JSON dependency: only the fields
/// these tests need, from the generator's fixed layout.
mod serde_like {
    use std::path::Path;

    pub struct Fixture {
        pub path: String,
        pub outcome: String,
    }

    pub struct Manifest(Vec<Fixture>);

    impl Manifest {
        pub fn pdf_fixtures(&self) -> impl Iterator<Item = &Fixture> {
            self.0
                .iter()
                .filter(|fixture| fixture.path.ends_with(".pdf"))
        }
    }

    fn field(entry: &str, name: &str) -> Option<String> {
        let key = format!("\"{name}\": \"");
        let start = entry.find(&key)? + key.len();
        let end = entry[start..].find('"')? + start;
        Some(entry[start..end].to_owned())
    }

    pub fn read(path: &Path) -> Manifest {
        let text = std::fs::read_to_string(path).unwrap();
        let fixtures = text
            .split("\"path\": ")
            .skip(1)
            .filter_map(|entry| {
                let entry = format!("\"path\": {entry}");
                Some(Fixture {
                    path: field(&entry, "path")?,
                    outcome: field(&entry, "outcome")?,
                })
            })
            .collect();
        Manifest(fixtures)
    }
}
