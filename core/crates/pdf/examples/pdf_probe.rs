//! Runs one PDF through the job runner with PDFium and prints one JSON line:
//! outcome, page count, timings and peak memory. Used to produce the
//! feasibility evidence in `docs/feasibility/evidence/PW-010/`.
//!
//! ```sh
//! pdf_probe <libpdfium> <pdf> <out-dir> [max-width max-height]
//! ```
//!
//! Run one process per file so peak memory (Linux and Android `VmHWM`) is per
//! file. The first page of an exported job is written to `<out-dir>` as PPM.

use paperwarden_core::validation::MagicBytes;
use paperwarden_core::{CancellationToken, JobId, JobLimits, JobRunner, Operation, Outcome};
use paperwarden_pdf::{PdfEngine, PpmVerifier, RenderProcessor};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn json_string(value: &str) -> String {
    let mut out = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            character if u32::from(character) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", u32::from(character)));
            }
            character => out.push(character),
        }
    }
    out.push('"');
    out
}

/// Peak resident memory of this process in KiB, where the OS reports it.
fn peak_rss_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|line| line.starts_with("VmHWM:"))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

#[allow(
    clippy::print_stdout,
    reason = "the probe's report is its standard output"
)]
fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let [library, pdf, out_dir, rest @ ..] = arguments.as_slice() else {
        return Err("usage: pdf_probe <libpdfium> <pdf> <out-dir> [max-width max-height]".into());
    };
    let (max_width, max_height) = match rest {
        [width, height] => (width.parse()?, height.parse()?),
        _ => (1080, 1920),
    };
    // The job runner requires an absolute workspace root.
    let out_dir = std::path::absolute(PathBuf::from(out_dir))?;
    std::fs::create_dir_all(&out_dir)?;

    let engine = PdfEngine::load(Path::new(library))?;
    let processor = RenderProcessor::new(engine, max_width, max_height);
    let validator = MagicBytes::new(0, b"%PDF-");
    let operation = Operation {
        validator: &validator,
        processor: &processor,
        verifier: &PpmVerifier,
    };
    let limits = JobLimits::new(64 << 20, 64 << 20, Duration::from_secs(30))?;
    let runner = JobRunner::new(out_dir.join("jobs"), limits);
    let stem = Path::new(pdf)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("document")
        .to_owned();

    let outcome = match runner.run(
        JobId::new("probe")?,
        Path::new(pdf),
        &operation,
        &CancellationToken::new(),
    ) {
        Ok(mut ready) => {
            let target = out_dir.join(format!("{stem}.ppm"));
            let _ = std::fs::remove_file(&target);
            ready.export_to(&target)?;
            match ready.finish().outcome {
                Outcome::Exported => "exported".to_owned(),
                other => format!("{other:?}"),
            }
        }
        Err(report) => match report.outcome {
            Outcome::Stopped(error) => format!("stopped: {error}"),
            other => format!("{other:?}"),
        },
    };
    let stats = processor.stats();
    println!(
        "{{\"file\":{},\"outcome\":{},\"pages\":{},\"rendered\":{},\"open_ms\":{:.1},\"render_ms\":{:.1},\"peak_rss_kib\":{},\"first_page_text\":{}}}",
        json_string(pdf),
        json_string(&outcome),
        stats.pages,
        stats.rendered,
        stats.open_time.as_secs_f64() * 1000.0,
        stats.render_time.as_secs_f64() * 1000.0,
        peak_rss_kib().map_or("null".to_owned(), |kib| kib.to_string()),
        stats
            .first_page_text
            .as_deref()
            .map_or("null".to_owned(), |text| json_string(text.trim())),
    );
    Ok(())
}
