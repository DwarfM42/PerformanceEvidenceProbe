use std::fs;

use perf_evidence_probe::evidence::write_completed_bundle_manifest;
use serde_json::json;

#[test]
fn completed_manifest_is_platform_neutral_and_binds_reconstructed_measurement_state() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["processes.ndjson", "samples.ndjson", "events.ndjson"] {
        fs::write(dir.path().join(name), "").unwrap();
    }
    fs::write(
        dir.path().join("summary.json"),
        serde_json::to_vec(&json!({
            "measurement_validity":"VALID",
            "measurement_completeness":"DECLARED_PARTIAL"
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(dir.path().join("platform.json"), b"{}\n").unwrap();

    write_completed_bundle_manifest(dir.path(), "COMPLETE", &["platform.json"]).unwrap();

    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["schema_draft_version"], "perf-evidence-v2-draft");
    assert_eq!(manifest["run_state"], "COMPLETE");
    assert_eq!(manifest["measurement_validity"], "VALID");
    assert_eq!(manifest["measurement_completeness"], "DECLARED_PARTIAL");
    assert!(
        manifest["artifact_list"]
            .as_array()
            .unwrap()
            .iter()
            .any(|artifact| artifact["path"] == "platform.json")
    );
}
