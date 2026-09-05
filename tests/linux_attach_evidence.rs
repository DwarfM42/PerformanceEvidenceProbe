#![cfg(target_os = "linux")]

use std::{fs, process::Command};

use serde_json::Value;

fn only_bundle(root: &std::path::Path) -> std::path::PathBuf {
    let entries = fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 1);
    entries.into_iter().next().unwrap()
}

#[test]
fn attach_persists_a_v2_identity_bound_thread_and_cpu_sample() {
    let mut target = Command::new("sleep").arg("2").spawn().unwrap();
    let output = tempfile::tempdir().unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_perf-probe"))
        .args(["attach", "--pid", &target.id().to_string(), "--output"])
        .arg(output.path())
        .output()
        .unwrap();
    let _ = target.wait();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );

    let bundle = only_bundle(output.path());
    let process: Value =
        serde_json::from_str(&fs::read_to_string(bundle.join("processes.ndjson")).unwrap())
            .unwrap();
    let sample: Value =
        serde_json::from_str(&fs::read_to_string(bundle.join("samples.ndjson")).unwrap()).unwrap();
    let summary: Value =
        serde_json::from_slice(&fs::read(bundle.join("summary.json")).unwrap()).unwrap();
    let manifest: Value =
        serde_json::from_slice(&fs::read(bundle.join("manifest.json")).unwrap()).unwrap();

    assert_eq!(process["process_local_id"], 1);
    assert_eq!(process["pid"], target.id());
    assert!(process["process_start_time"].as_u64().unwrap() > 0);
    assert!(process["boot_identity"].as_str().unwrap().len() == 36);
    let row = &sample["processes"][0];
    assert!(row["thread_count"].is_number());
    assert!(row["user_cpu_time_ns"].is_number());
    assert!(row["kernel_cpu_time_ns"].is_number());
    assert_eq!(sample["schema_draft_version"], "perf-evidence-v2-draft");
    assert_eq!(
        summary["summary_schema_draft_version"],
        "perf-evidence-v2-draft"
    );
    assert_eq!(manifest["schema_draft_version"], "perf-evidence-v2-draft");
}
