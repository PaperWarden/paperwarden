//! The PDF pre-check reads attacker-controlled files before PDFium does
//! (ADR-0007). For any input it must finish without panicking, give the same
//! answer twice, and stay inside the limits it reports against, even when
//! they are tight.

#![no_main]

use libfuzzer_sys::fuzz_target;
use paperwarden_pdf_scan::{scan, Interrupted, ScanError, ScanLimits};

/// Small limits, so the fuzzer reaches every bound with small inputs.
const TIGHT: ScanLimits = ScanLimits {
    max_nesting: 8,
    max_objects: 64,
    max_sections: 4,
    max_stream_bytes: 64 * 1024,
    max_ratio: 16,
    ratio_floor: 1024,
    max_total_bytes: 256 * 1024,
    max_image_pixels: 1_000_000,
};

fuzz_target!(|data: &[u8]| {
    // Most inputs should get past the signature check.
    let mut input = b"%PDF-1.7\n".to_vec();
    input.extend_from_slice(data);

    for limits in [ScanLimits::default(), TIGHT] {
        let mut checks = 0u32;
        let first = scan(&input, &limits, &mut || {
            checks += 1;
            Ok(())
        });
        let second = scan(&input, &limits, &mut || Ok(()));
        assert_eq!(first, second, "the pre-check is not deterministic");

        if let Ok(report) = &first {
            assert!(
                report.objects <= limits.max_objects,
                "object limit exceeded"
            );
            assert!(
                report.decoded_bytes <= limits.max_total_bytes,
                "decode limit exceeded"
            );
            assert!(
                report.streams <= report.objects,
                "a stream outside any object"
            );
        }

        // Stopping at the first checkpoint always stops the scan, unless it
        // finished or refused the file before reaching one.
        let stopped = scan(&input, &limits, &mut || Err(Interrupted));
        if checks > 0 {
            assert!(
                matches!(stopped, Err(ScanError::Interrupted | ScanError::NotPdf)),
                "a checkpoint was ignored: {stopped:?}"
            );
        }
    }
});
