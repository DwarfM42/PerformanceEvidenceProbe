use std::fs;

use perf_evidence_probe::evidence::{EvidenceEvent, EvidenceWriter, ProcessRecord};

#[test]
fn one_writer_serializes_complete_ndjson_records_to_each_stream() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let writer = EvidenceWriter::start(directory.path(), 2).expect("writer starts");
    writer
        .process(ProcessRecord::minimal(1, 100, 200, "boot-a"))
        .expect("process event");
    writer
        .event(EvidenceEvent::new("process_observed"))
        .expect("event");
    writer.finish().expect("writer joins");

    for name in ["processes.ndjson", "samples.ndjson", "events.ndjson"] {
        let content = fs::read_to_string(directory.path().join(name)).expect("stream exists");
        assert!(
            content.is_empty() || content.ends_with('\n'),
            "{name} has complete records"
        );
        for line in content.lines() {
            serde_json::from_str::<serde_json::Value>(line).expect("valid NDJSON record");
        }
    }
}
