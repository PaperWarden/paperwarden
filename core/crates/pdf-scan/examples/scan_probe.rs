//! Runs the pre-check over PDFs and prints one line per file: the result,
//! damage, active content, objects, streams, decoded bytes and time.
//!
//! ```sh
//! scan_probe <pdf>...
//! ```

use paperwarden_pdf_scan::{scan, ScanLimits};
use std::time::Instant;

#[allow(
    clippy::print_stdout,
    reason = "the probe's report is its standard output"
)]
fn main() {
    for path in std::env::args().skip(1) {
        let Ok(bytes) = std::fs::read(&path) else {
            println!("{path}\tunreadable");
            continue;
        };
        let started = Instant::now();
        let result = scan(&bytes, &ScanLimits::default(), &mut || Ok(()));
        let millis = started.elapsed().as_secs_f64() * 1000.0;
        match result {
            Ok(report) => println!(
                "{path}\tallowed\tdamage={:?}\tactive={:?}\tobjects={}\tstreams={}\tdecoded={}\t{millis:.1} ms",
                report.damage, report.active_content, report.objects, report.streams, report.decoded_bytes
            ),
            Err(error) => println!("{path}\trefused\t{error:?}\t{millis:.1} ms"),
        }
    }
}
