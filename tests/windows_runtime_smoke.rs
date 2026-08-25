#![cfg(windows)]

use std::{fs, process::Command};

use serde_json::Value;

fn probe() -> &'static str {
    env!("CARGO_BIN_EXE_perf-probe")
}

fn only_bundle(root: &std::path::Path) -> std::path::PathBuf {
    let mut entries = fs::read_dir(root)
        .expect("read output root")
        .map(|entry| entry.expect("directory entry").path())
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 1, "expected exactly one evidence bundle");
    entries.pop().expect("bundle path")
}

#[test]
fn launch_is_non_destructive_and_persists_reconstructable_raw_evidence() {
    let output = tempfile::tempdir().expect("temporary output");
    let result = Command::new(probe())
        .args(["run", "--output"])
        .arg(output.path())
        .args(["--", "cmd.exe", "/c", "exit", "0"])
        .output()
        .expect("run perf-probe");
    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let bundle = only_bundle(output.path());
    let events = fs::read_to_string(bundle.join("events.ndjson")).expect("events evidence");
    assert!(events.contains("launch_assigned_non_destructive_job"));
    assert!(events.contains("\"kill_on_job_close_enabled\":false"));
    assert!(bundle.join("processes.ndjson").exists());
    assert!(bundle.join("samples.ndjson").exists());
    let summary: Value =
        serde_json::from_slice(&fs::read(bundle.join("summary.json")).expect("summary"))
            .expect("summary JSON");
    assert_eq!(summary["exit_code"], 0);
    assert!(summary["sample_count"].as_u64().unwrap_or_default() >= 1);
}

#[test]
fn default_attach_persists_an_explicit_no_job_assignment_event() {
    let mut target = Command::new("cmd.exe")
        .args(["/c", "ping -n 3 127.0.0.1 > nul"])
        .spawn()
        .expect("start attach target");
    let output = tempfile::tempdir().expect("temporary output");
    let result = Command::new(probe())
        .args(["attach", "--pid"])
        .arg(target.id().to_string())
        .args(["--output"])
        .arg(output.path())
        .output()
        .expect("run attach");
    let _ = target.wait();
    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let bundle = only_bundle(output.path());
    let events = fs::read_to_string(bundle.join("events.ndjson")).expect("events evidence");
    assert!(events.contains("attach_observation_started"));
    assert!(events.contains("\"attached_to_probe_job\":false"));
}
