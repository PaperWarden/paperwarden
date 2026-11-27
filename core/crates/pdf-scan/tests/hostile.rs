//! Attacks beyond the corpus. A small builder writes well-formed PDFs with
//! correct cross-reference offsets, so each test differs from a valid file in
//! exactly one hostile way.

#![cfg(test)]

use miniz_oxide::deflate::compress_to_vec_zlib;
use paperwarden_pdf_scan::{scan, ActiveContent, Damage, Limit, Report, ScanError, ScanLimits};

/// Builds a PDF from object bodies; object N is `objects[N - 1]`.
fn pdf(objects: &[Vec<u8>], trailer_extra: &str) -> Vec<u8> {
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (index, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R {trailer_extra} >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

fn stream(dict: &str, data: &[u8]) -> Vec<u8> {
    let mut body = format!("<< {dict} /Length {} >>\nstream\n", data.len()).into_bytes();
    body.extend_from_slice(data);
    body.extend_from_slice(b"\nendstream");
    body
}

fn base() -> Vec<Vec<u8>> {
    vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] >>".to_vec(),
    ]
}

fn check(data: &[u8]) -> Result<Report, ScanError> {
    scan(data, &ScanLimits::default(), &mut || Ok(()))
}

#[test]
fn the_builder_makes_clean_pdfs() {
    let report = check(&pdf(&base(), "")).expect("clean");
    assert_eq!(
        report,
        Report {
            objects: 3,
            ..Report::default()
        }
    );
}

#[test]
fn an_image_whose_data_exceeds_its_dimensions_is_stopped() {
    // Declares 100 x 100 RGB (30 KB) but inflates to 8 MiB.
    let bomb = compress_to_vec_zlib(&vec![0u8; 8 << 20], 9);
    let mut objects = base();
    objects.push(stream(
        "/Type /XObject /Subtype /Image /Width 100 /Height 100 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode",
        &bomb,
    ));
    assert_eq!(
        check(&pdf(&objects, "")),
        Err(ScanError::Limit(Limit::ImageSize))
    );
}

#[test]
fn a_jpeg_larger_than_it_declares_is_stopped() {
    // Declares 10 x 10, but the JPEG frame header says 65535 x 65535.
    let jpeg = [
        0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x11, 0x08, 0xFF, 0xFF, 0xFF, 0xFF, 0x03, 1, 0x11, 0, 2,
        0x11, 0, 3, 0x11, 0, 0xFF, 0xD9,
    ];
    let mut objects = base();
    objects.push(stream(
        "/Type /XObject /Subtype /Image /Width 10 /Height 10 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode",
        &jpeg,
    ));
    assert_eq!(
        check(&pdf(&objects, "")),
        Err(ScanError::Limit(Limit::ImageSize))
    );
}

#[test]
fn declared_huge_images_are_stopped_without_decoding() {
    let mut objects = base();
    objects.push(stream(
        "/Type /XObject /Subtype /Image /Width 100000 /Height 100000 /ColorSpace /DeviceGray /BitsPerComponent 1 /Filter /CCITTFaxDecode",
        b"",
    ));
    assert_eq!(
        check(&pdf(&objects, "")),
        Err(ScanError::Limit(Limit::ImageSize))
    );
}

#[test]
fn a_stream_past_the_size_cap_is_stopped_even_at_a_modest_ratio() {
    // Random-looking data compresses badly: the ratio stays low, so the
    // per-stream cap is what stops it.
    let limits = ScanLimits {
        max_stream_bytes: 1 << 20,
        ..ScanLimits::default()
    };
    let data: Vec<u8> = (0u32..(2 << 20))
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
        .collect();
    let mut objects = base();
    objects.push(stream(
        "/Filter /FlateDecode",
        &compress_to_vec_zlib(&data, 6),
    ));
    assert_eq!(
        scan(&pdf(&objects, ""), &limits, &mut || Ok(())),
        Err(ScanError::Limit(Limit::StreamSize))
    );
}

#[test]
fn the_document_total_is_bounded() {
    let limits = ScanLimits {
        max_total_bytes: 1 << 20,
        ..ScanLimits::default()
    };
    let chunk = compress_to_vec_zlib(&b"0123456789abcdef".repeat(40_000), 6);
    let mut objects = base();
    for _ in 0..3 {
        objects.push(stream("/Filter /FlateDecode", &chunk));
    }
    assert_eq!(
        scan(&pdf(&objects, ""), &limits, &mut || Ok(())),
        Err(ScanError::Limit(Limit::TotalDecoded))
    );
}

#[test]
fn active_content_hidden_in_an_object_stream_is_flagged() {
    let inner = b"4 0 << /S /Launch /F (calc.exe) >>";
    let mut objects = base();
    objects.push(stream(
        "/Type /ObjStm /N 1 /First 4 /Filter /FlateDecode",
        &compress_to_vec_zlib(inner, 6),
    ));
    let report = check(&pdf(&objects, "")).expect("allowed");
    assert_eq!(report.active_content, vec![ActiveContent::Launch]);
    assert_eq!(report.objects, 5);
}

#[test]
fn nesting_hidden_in_an_object_stream_is_stopped() {
    let inner = [b"4 0 ".to_vec(), b"[".repeat(40), b"]".repeat(40)].concat();
    let mut objects = base();
    objects.push(stream(
        "/Type /ObjStm /N 1 /First 4 /Filter /FlateDecode",
        &compress_to_vec_zlib(&inner, 6),
    ));
    assert_eq!(
        check(&pdf(&objects, "")),
        Err(ScanError::Limit(Limit::Nesting))
    );
}

#[test]
fn obfuscated_names_do_not_hide_scripts() {
    let mut objects = base();
    objects[0] = b"<< /Type /Catalog /Pages 2 0 R /OpenAction << /#53 /Java#53cript /J#53 (app.alert(1)) >> >>".to_vec();
    let report = check(&pdf(&objects, "")).expect("allowed");
    assert_eq!(report.active_content, vec![ActiveContent::JavaScript]);
}

#[test]
fn every_kind_of_active_content_is_reported() {
    let mut objects = base();
    objects.push(b"<< /S /SubmitForm /F (https://paperwarden.invalid) >>".to_vec());
    objects.push(b"<< /S /ImportData /F (data.fdf) >>".to_vec());
    objects.push(b"<< /S /GoToR /F (other.pdf) >>".to_vec());
    objects.push(b"<< /Names << /EmbeddedFiles 9 0 R >> >>".to_vec());
    objects.push(b"<< /Type /Annot /Subtype /RichMedia >>".to_vec());
    objects.push(b"<< /Fields [] /XFA 10 0 R >>".to_vec());
    let report = check(&pdf(&objects, "")).expect("allowed");
    assert_eq!(
        report.active_content,
        vec![
            ActiveContent::SubmitForm,
            ActiveContent::ImportData,
            ActiveContent::RemoteGoTo,
            ActiveContent::EmbeddedFile,
            ActiveContent::RichMedia,
            ActiveContent::Xfa,
        ]
    );
}

#[test]
fn encrypted_documents_are_refused() {
    let mut objects = base();
    objects.push(b"<< /Filter /Standard /V 2 /R 3 /O (x) /U (y) /P -4 >>".to_vec());
    assert_eq!(
        check(&pdf(&objects, "/Encrypt 4 0 R")),
        Err(ScanError::Encrypted)
    );
}

#[test]
fn a_wrong_offset_in_the_table_is_damage() {
    let mut data = pdf(&base(), "");
    // Point object 2's entry somewhere else.
    let text = String::from_utf8(data.clone()).expect("ascii");
    let entry = text
        .lines()
        .filter(|line| line.ends_with(" n "))
        .nth(1)
        .expect("entry")
        .to_owned();
    let at = text.find(&entry).expect("entry");
    data.splice(at..at + 10, *b"0000000001");
    let report = check(&data).expect("allowed");
    assert_eq!(report.damage, vec![Damage::CrossReference]);
}

#[test]
fn a_long_revision_chain_is_stopped() {
    let limits = ScanLimits {
        max_sections: 4,
        ..ScanLimits::default()
    };
    let mut data = pdf(&base(), "");
    let mut previous = data.windows(4).position(|w| w == b"xref").expect("xref");
    for _ in 0..6 {
        let here = data.len();
        data.extend_from_slice(format!("xref\n0 1\n0000000000 65535 f \ntrailer\n<< /Size 4 /Root 1 0 R /Prev {previous} >>\n").as_bytes());
        previous = here;
    }
    data.extend_from_slice(format!("startxref\n{previous}\n%%EOF\n").as_bytes());
    assert_eq!(
        scan(&data, &limits, &mut || Ok(())),
        Err(ScanError::Limit(Limit::Sections))
    );
    assert!(check(&data)
        .expect("allowed at the default")
        .damage
        .is_empty());
}

#[test]
fn too_many_objects_are_stopped() {
    let limits = ScanLimits {
        max_objects: 10,
        ..ScanLimits::default()
    };
    let objects: Vec<Vec<u8>> = (0..20).map(|_| b"null".to_vec()).collect();
    assert_eq!(
        scan(&pdf(&objects, ""), &limits, &mut || Ok(())),
        Err(ScanError::Limit(Limit::Objects))
    );
}

#[test]
fn a_cross_reference_stream_is_checked() {
    // Objects 1-3, then an xref stream (object 4) with /W [1 4 1].
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (index, body) in base().iter().enumerate() {
        offsets.push(out.len() as u32);
        out.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref_at = out.len() as u32;
    let build = |offsets: &[u32]| {
        let mut rows = vec![0u8, 0, 0, 0, 0, 0xFF];
        for &offset in offsets.iter().chain([xref_at].iter()) {
            rows.push(1);
            rows.extend_from_slice(&offset.to_be_bytes());
            rows.push(0);
        }
        rows
    };
    for (rows, damaged) in [
        (build(&offsets), false),
        (build(&[offsets[0], 7, offsets[2]]), true),
    ] {
        let mut data = out.clone();
        data.extend_from_slice(b"4 0 obj\n");
        data.extend_from_slice(&stream("/Type /XRef /Size 5 /W [1 4 1] /Root 1 0 R", &rows));
        data.extend_from_slice(format!("\nendobj\nstartxref\n{xref_at}\n%%EOF\n").as_bytes());
        let report = check(&data).expect("allowed");
        assert_eq!(
            report.damage.contains(&Damage::CrossReference),
            damaged,
            "{:?}",
            report.damage
        );
    }
}

#[test]
fn garbage_never_panics_and_stays_bounded() {
    let mut state = 0x2545_F491_u32;
    for size in [0usize, 1, 5, 64, 1024, 65_536] {
        for _ in 0..20 {
            let mut data = b"%PDF-1.7\n".to_vec();
            data.extend((0..size).map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                b"<<>>[]() /obj stream endstream 0123456789R\n%"[(state % 44) as usize]
            }));
            let _ = check(&data);
        }
    }
}
