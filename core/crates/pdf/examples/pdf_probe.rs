//! Runs one PDF through the job runner with PDFium and prints one JSON line:
//! outcome, page count, timings and peak memory. Used to produce the
//! feasibility evidence in `docs/feasibility/evidence/PW-010/`.
//!
//! ```sh
//! pdf_probe <libpdfium> <pdf> <out-dir> [max-width max-height]
//! ```
//!
//! Run one process per file so peak memory (`VmHWM` on Linux and Android,
//! `ru_maxrss` on iOS and macOS) is per file. The first page of an exported
//! job is written to `<out-dir>` as PPM.

use paperwarden_core::validation::MagicBytes;
use paperwarden_core::{
    CancellationToken, JobError, JobId, JobLimits, JobRunner, Operation, Outcome,
};
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

/// A JSON list of the items' names, or `null` when the pre-check did not run.
fn json_list<T: std::fmt::Debug>(items: Option<&Vec<T>>) -> String {
    items.map_or("null".to_owned(), |items| {
        let names: Vec<String> = items
            .iter()
            .map(|item| json_string(&format!("{item:?}")))
            .collect();
        format!("[{}]", names.join(","))
    })
}

/// Peak resident memory of this process in KiB, where the OS reports it.
#[cfg(not(target_vendor = "apple"))]
fn peak_rss_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|line| line.starts_with("VmHWM:"))?;
    line.split_whitespace().nth(1)?.parse().ok()
}

/// Peak resident memory of this process in KiB. Apple platforms have no
/// `/proc`; `getrusage` reports `ru_maxrss` there in bytes.
#[cfg(target_vendor = "apple")]
fn peak_rss_kib() -> Option<u64> {
    use nix::sys::resource::{getrusage, UsageWho};
    let bytes = getrusage(UsageWho::RUSAGE_SELF).ok()?.max_rss();
    u64::try_from(bytes).ok().map(|bytes| bytes / 1024)
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
            Outcome::Stopped(JobError::LimitExceeded(reason)) => format!("limit: {reason}"),
            Outcome::Stopped(error) => format!("stopped: {error}"),
            other => format!("{other:?}"),
        },
    };
    let stats = processor.stats();
    println!(
        "{{\"file\":{},\"outcome\":{},\"damage\":{},\"active_content\":{},\"precheck_ms\":{:.1},\"pages\":{},\"rendered\":{},\"open_ms\":{:.1},\"render_ms\":{:.1},\"peak_rss_kib\":{},\"first_page_text\":{}}}",
        json_string(pdf),
        json_string(&outcome),
        json_list(stats.precheck.as_ref().map(|report| &report.damage)),
        json_list(stats.precheck.as_ref().map(|report| &report.active_content)),
        stats.precheck_time.as_secs_f64() * 1000.0,
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
