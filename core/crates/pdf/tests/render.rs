//! PDFium against the corpus. These tests need a PDFium library: fetch one
//! with `python scripts/fetch_pdfium.py <platform>` and set
//! `PAPERWARDEN_PDFIUM_LIB` to it. They are ignored by default and fail,
//! rather than pass, when run without the library:
//!
//! ```sh
//! PAPERWARDEN_PDFIUM_LIB=third_party/pdfium/linux-x64/lib/libpdfium.so \
//!   cargo test -p paperwarden-pdf -- --ignored
//! ```
//!
//! `PAPERWARDEN_CORPUS` overrides the corpus location (used on devices).

#![cfg(test)]

use paperwarden_core::validation::MagicBytes;
use paperwarden_core::{
    CancellationToken, JobError, JobId, JobLimits, JobRunner, Operation, Outcome,
};
use paperwarden_pdf::{PdfEngine, PdfError, PpmVerifier, RenderProcessor, MAX_RENDER_PIXELS};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// A phone-screen render must always fit the pixel cap.
const _: () = assert!(MAX_RENDER_PIXELS >= 1080 * 1920);

fn engine() -> &'static PdfEngine {
    let library = std::env::var_os("PAPERWARDEN_PDFIUM_LIB")
        .expect("set PAPERWARDEN_PDFIUM_LIB to a PDFium library (scripts/fetch_pdfium.py)");
    PdfEngine::load(Path::new(&library)).expect("load PDFium")
}

fn corpus(relative: &str) -> Vec<u8> {
    let root = std::env::var_os("PAPERWARDEN_CORPUS")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../tests/corpus"));
    std::fs::read(root.join(relative)).unwrap_or_else(|error| panic!("read {relative}: {error}"))
}

fn dark_pixels(rgba: &[u8]) -> usize {
    let (pixels, _) = rgba.as_chunks::<4>();
    pixels
        .iter()
        .filter(|[red, green, blue, _]| *red < 128 && *green < 128 && *blue < 128)
        .count()
}

#[test]
#[ignore = "needs PAPERWARDEN_PDFIUM_LIB; see the module docs"]
fn renders_and_extracts_the_one_page_fixture() {
    let bytes = corpus("valid/pdf/one-page.pdf");
    let document = engine().open(&bytes).expect("open");
    assert_eq!(document.page_count(), 1);
    assert_eq!(document.page_size(0).expect("size"), (300.0, 100.0));
    assert_eq!(document.text(0).expect("text").trim(), "PaperWarden corpus");

    let image = document.render(0, 600, 600).expect("render");
    assert_eq!((image.width, image.height), (600, 200));
    assert_eq!(image.rgba.len(), 600 * 200 * 4);
    let dark = dark_pixels(&image.rgba);
    assert!(dark > 100, "no text drawn ({dark} dark pixels)");
    assert!(
        dark < 600 * 200 / 4,
        "page not mostly white ({dark} dark pixels)"
    );
}

#[test]
#[ignore = "needs PAPERWARDEN_PDFIUM_LIB; see the module docs"]
fn reads_every_page_and_the_newest_revision() {
    let bytes = corpus("valid/pdf/two-pages-flate.pdf");
    let document = engine().open(&bytes).expect("open");
    assert_eq!(document.page_count(), 2);
    assert_eq!(document.text(0).expect("text").trim(), "Page one");
    assert_eq!(document.text(1).expect("text").trim(), "Page two");

    let bytes = corpus("valid/pdf/incremental-update.pdf");
    let document = engine().open(&bytes).expect("open");
    assert_eq!(
        document.text(0).expect("text").trim(),
        "PaperWarden corpus, revised"
    );

    let bytes = corpus("valid/pdf/many-pages-200.pdf");
    let document = engine().open(&bytes).expect("open");
    assert_eq!(document.page_count(), 200);
    assert_eq!(document.text(199).expect("text").trim(), "Page 200");
}

#[test]
#[ignore = "needs PAPERWARDEN_PDFIUM_LIB; see the module docs"]
fn renders_stay_inside_the_pixel_box() {
    let bytes = corpus("security/pdf/extreme-page-size.pdf");
    let document = engine().open(&bytes).expect("open");
    for page in 0..document.page_count() {
        match document.render(page, 1080, 1920) {
            Ok(image) => {
                assert!(
                    image.width <= 1080 && image.height <= 1920,
                    "{}x{}",
                    image.width,
                    image.height
                );
                assert_eq!(
                    image.rgba.len() as u64,
                    u64::from(image.width) * u64::from(image.height) * 4
                );
            }
            Err(PdfError::DegeneratePage | PdfError::Page(_)) => {}
            Err(other) => panic!("page {page}: unexpected {other:?}"),
        }
    }
    let bytes = corpus("valid/pdf/one-page.pdf");
    let document = engine().open(&bytes).expect("open");
    assert_eq!(
        document.render(0, 5000, 5000),
        Err(PdfError::RenderTooLarge)
    );
    assert_eq!(document.render(0, 0, 100), Err(PdfError::RenderTooLarge));
}

#[test]
#[ignore = "needs PAPERWARDEN_PDFIUM_LIB; see the module docs"]
fn malformed_and_hostile_files_finish_quickly() {
    for fixture in [
        "malformed/truncated.pdf",
        "malformed/pdf/bad-xref-offsets.pdf",
        "malformed/pdf/no-eof-marker.pdf",
        "malformed/pdf/header-only.pdf",
        "security/pdf/javascript-openaction.pdf",
        "security/pdf/launch-and-uri-actions.pdf",
        "security/pdf/xref-prev-loop.pdf",
        "security/pdf/deep-nesting.pdf",
        "security/pdf/flate-bomb.pdf",
        "security/pdf/stream-length-overflow.pdf",
    ] {
        let bytes = corpus(fixture);
        let started = Instant::now();
        if let Ok(document) = engine().open(&bytes) {
            for page in 0..document.page_count().min(5) {
                let _ = document.render(page, 512, 512);
                let _ = document.text(page);
            }
        }
        let elapsed = started.elapsed();
        assert!(
            elapsed < Duration::from_secs(10),
            "{fixture} took {elapsed:?}"
        );
    }
    let bytes = corpus("malformed/pdf/header-only.pdf");
    assert!(matches!(engine().open(&bytes), Err(PdfError::Open(_))));
}

fn job_root(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "paperwarden-pdf-{name}-{}-{nonce}",
        std::process::id()
    ))
}

#[test]
#[ignore = "needs PAPERWARDEN_PDFIUM_LIB; see the module docs"]
fn renders_through_the_isolated_job_runner() {
    let root = job_root("runner");
    std::fs::create_dir_all(&root).expect("root");
    let input = root.join("input.pdf");
    std::fs::write(&input, corpus("valid/pdf/one-page.pdf")).expect("write input");

    let processor = RenderProcessor::new(engine(), 600, 600);
    let validator = MagicBytes::new(0, b"%PDF-");
    let operation = Operation {
        validator: &validator,
        processor: &processor,
        verifier: &PpmVerifier,
    };
    let limits = JobLimits::new(64 << 20, 64 << 20, Duration::from_secs(30)).expect("limits");
    let runner = JobRunner::new(root.join("jobs"), limits);

    let mut ready = runner
        .run(
            JobId::new("pdf-1").expect("id"),
            &input,
            &operation,
            &CancellationToken::new(),
        )
        .expect("job ready");
    assert_eq!(
        ready.output_len(),
        ("P6\n600 200\n255\n".len() + 600 * 200 * 3) as u64
    );
    ready.export_to(&root.join("page.ppm")).expect("export");
    assert_eq!(ready.finish().outcome, Outcome::Exported);
    assert_eq!(processor.stats().pages, 1);

    std::fs::write(&input, corpus("malformed/pdf/header-only.pdf")).expect("write input");
    let report = runner
        .run(
            JobId::new("pdf-2").expect("id"),
            &input,
            &operation,
            &CancellationToken::new(),
        )
        .expect_err("malformed PDF must fail");
    assert_eq!(
        report.outcome,
        Outcome::Stopped(JobError::EngineFailed("the PDF could not be opened"))
    );
    assert_eq!(
        std::fs::read_dir(root.join("jobs")).expect("jobs").count(),
        0,
        "workspace left behind"
    );
    std::fs::remove_dir_all(&root).expect("cleanup");
}

#[test]
fn missing_library_is_reported_not_panicked() {
    // Only meaningful before any test bound PDFium in this process; binding
    // succeeds once and is then shared, so skip when already loaded.
    if std::env::var_os("PAPERWARDEN_PDFIUM_LIB").is_some() {
        return;
    }
    let missing = PdfEngine::load(Path::new("/nonexistent/libpdfium.so"));
    assert!(matches!(missing, Err(PdfError::Library(_))));
}
