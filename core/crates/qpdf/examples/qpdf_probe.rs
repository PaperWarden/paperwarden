//! Runs one qpdf transform from the command line (PW-012), for devices and
//! for measuring what qpdf adds to a binary:
//!
//! ```sh
//! qpdf_probe rewrite|rotate|encrypt|pages INPUT [OUTPUT]
//! ```
//!
//! Prints one JSON line. Built without qpdf (no fetched sources), every
//! operation reports `Unavailable`, which gives the size baseline.

#![allow(
    clippy::print_stdout,
    reason = "a command-line probe reports on stdout"
)]

use paperwarden_qpdf::{page_count, transform, PageRange, Rotation, Transform};
use std::path::Path;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(operation), Some(input)) = (args.get(1), args.get(2)) else {
        println!("{{\"error\":\"usage: qpdf_probe rewrite|rotate|encrypt|pages INPUT [OUTPUT]\"}}");
        return;
    };
    let input = Path::new(input);
    let output = args.get(3).map(Path::new);
    let started = Instant::now();
    let result = match (operation.as_str(), output) {
        ("pages", _) => format!("{:?}", page_count(input, "")),
        ("rewrite", Some(output)) => format!("{:?}", transform(input, output, &Transform::Rewrite)),
        ("rotate", Some(output)) => {
            let rotate = Transform::Rotate {
                rotation: Rotation::Quarter,
                pages: PageRange::ALL,
            };
            format!("{:?}", transform(input, output, &rotate))
        }
        ("encrypt", Some(output)) => {
            let encrypt = Transform::Encrypt {
                user: String::new(),
                owner: "probe-owner".to_owned(),
            };
            format!("{:?}", transform(input, output, &encrypt))
        }
        _ => "unknown operation or missing output".to_owned(),
    };
    println!(
        "{{\"operation\":\"{operation}\",\"result\":\"{result}\",\"ms\":{}}}",
        started.elapsed().as_millis()
    );
}
