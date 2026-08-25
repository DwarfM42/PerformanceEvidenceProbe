use std::fs;

use perf_evidence_probe::ndjson::{NdjsonReadError, read_complete_records};

#[test]
fn accepts_only_an_incomplete_final_ndjson_line() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("samples.ndjson");
    fs::write(&path, b"{\"sequence\":1}\n{\"sequence\":2").expect("fixture written");

    let records = read_complete_records(&path).expect("final partial record is recoverable");

    assert_eq!(records, vec!["{\"sequence\":1}"]);
}

#[test]
fn rejects_corruption_before_the_final_ndjson_line() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("samples.ndjson");
    fs::write(&path, b"{\"sequence\":1}\nnot-json\n{\"sequence\":2}\n").expect("fixture written");

    let error = read_complete_records(&path).expect_err("interior corruption is evidence-invalid");

    assert!(matches!(
        error,
        NdjsonReadError::InteriorCorruption { line: 2, .. }
    ));
}
