#![cfg(target_os = "macos")]

use std::{fs, process::Command};

use perf_evidence_probe::runtime::macos::{MAX_COMMAND_ARGUMENTS, MAX_COMMAND_UTF8_BYTES};
use serde_json::Value;

fn only_bundle(root: &std::path::Path) -> std::path::PathBuf {
    let entries = fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 1);
    entries.into_iter().next().unwrap()
}

fn run_command(output: &std::path::Path, command: &[String]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_perf-probe"))
        .args(["run", "--output"])
        .arg(output)
        .arg("--")
        .args(command)
        .output()
        .unwrap()
}

fn terminal_event(bundle: &std::path::Path) -> Value {
    fs::read_to_string(bundle.join("events.ndjson"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|event| event["record_type"] == "process_exit_observed")
        .unwrap()
}

fn assert_no_terminal_counters(event: &Value) {
    for field in [
        "terminal_user_cpu_time_ns",
        "terminal_kernel_cpu_time_ns",
        "terminal_read_bytes",
        "terminal_write_bytes",
        "terminal_counter_fidelity",
    ] {
        assert!(event.get(field).is_none(), "unexpected {field}");
    }
}

#[test]
fn direct_run_accepts_command_at_utf8_byte_limit() {
    let output = tempfile::tempdir().unwrap();
    let mut command = vec!["/bin/sh".to_owned(), "-c".to_owned(), "sleep 1".to_owned()];
    let fixed_bytes = command
        .iter()
        .map(|argument| argument.len() + 1)
        .sum::<usize>();
    command.push("x".repeat(MAX_COMMAND_UTF8_BYTES - fixed_bytes - 1));
    assert_eq!(
        command
            .iter()
            .map(|argument| argument.len() + 1)
            .sum::<usize>(),
        MAX_COMMAND_UTF8_BYTES
    );
    let result = run_command(output.path(), &command);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(only_bundle(output.path()).join("manifest.json").is_file());
}

#[test]
fn direct_run_rejects_utf8_byte_limit_plus_one_before_spawn() {
    let output = tempfile::tempdir().unwrap();
    let marker = output.path().join("spawned");
    let mut command = vec![
        "/bin/sh".to_owned(),
        "-c".to_owned(),
        format!("touch {}", marker.display()),
    ];
    let fixed_bytes = command
        .iter()
        .map(|argument| argument.len() + 1)
        .sum::<usize>();
    command.push("x".repeat(MAX_COMMAND_UTF8_BYTES - fixed_bytes));
    assert_eq!(
        command
            .iter()
            .map(|argument| argument.len() + 1)
            .sum::<usize>(),
        MAX_COMMAND_UTF8_BYTES + 1
    );
    let result = run_command(output.path(), &command);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("run command exceeds"));
    assert!(!marker.exists());
    assert_eq!(fs::read_dir(output.path()).unwrap().count(), 0);
}

#[test]
fn direct_run_accepts_command_at_argument_count_limit() {
    let output = tempfile::tempdir().unwrap();
    let mut command = vec!["/bin/sh".to_owned(), "-c".to_owned(), "sleep 1".to_owned()];
    command.extend(std::iter::repeat_n(
        "x".to_owned(),
        MAX_COMMAND_ARGUMENTS - command.len(),
    ));
    assert_eq!(command.len(), MAX_COMMAND_ARGUMENTS);
    let result = run_command(output.path(), &command);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(only_bundle(output.path()).join("manifest.json").is_file());
}

#[test]
fn direct_run_rejects_argument_count_limit_plus_one_before_spawn() {
    let output = tempfile::tempdir().unwrap();
    let marker = output.path().join("spawned");
    let mut command = vec![
        "/bin/sh".to_owned(),
        "-c".to_owned(),
        format!("touch {}", marker.display()),
    ];
    command.extend(std::iter::repeat_n(
        "x".to_owned(),
        MAX_COMMAND_ARGUMENTS + 1 - command.len(),
    ));
    assert_eq!(command.len(), MAX_COMMAND_ARGUMENTS + 1);
    let result = run_command(output.path(), &command);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("run command must contain"));
    assert!(!marker.exists());
    assert_eq!(fs::read_dir(output.path()).unwrap().count(), 0);
}

#[test]
fn attach_self_is_observation_only_with_semantic_omissions() {
    let output = tempfile::tempdir().unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_perf-probe"))
        .args([
            "attach",
            "--pid",
            &std::process::id().to_string(),
            "--output",
        ])
        .arg(output.path())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let bundle = only_bundle(output.path());
    let events = fs::read_to_string(bundle.join("events.ndjson")).unwrap();
    assert!(events.contains("semantic_mismatch"));
    assert!(!events.contains("process_exit_observed"));
    let platform: Value =
        serde_json::from_slice(&fs::read(bundle.join("platform.json")).unwrap()).unwrap();
    assert_eq!(platform["mode"], "attach");
    assert_eq!(platform["root_observation_authority"], "observation_only");
    assert_eq!(platform["process_tree_closure"], "not_claimed");
    assert!(bundle.join("summary.json").is_file());
    assert!(bundle.join("manifest.json").is_file());
}

#[test]
fn direct_root_zero_exit_writes_complete_bundle() {
    let output = tempfile::tempdir().unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_perf-probe"))
        .args(["run", "--output"])
        .arg(output.path())
        .args(["--", "/bin/sh", "-c", "sleep 1; exit 0"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let bundle = only_bundle(output.path());
    let manifest: Value =
        serde_json::from_slice(&fs::read(bundle.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["run_state"], "COMPLETE");
    assert!(bundle.join("summary.json").is_file());
    assert!(bundle.join("samples.ndjson").is_file());
    let terminal = terminal_event(&bundle);
    assert_eq!(terminal["process_local_id"], 1);
    assert_eq!(terminal["exit_code"], 0);
    assert!(terminal.get("terminal_kind").is_none());
    assert!(terminal.get("signal_number").is_none());
    assert_no_terminal_counters(&terminal);
}

#[test]
fn direct_root_nonzero_exit_has_complete_sample_and_no_fabricated_memory_metrics() {
    let output = tempfile::tempdir().unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_perf-probe"))
        .args(["run", "--output"])
        .arg(output.path())
        .args(["--", "/bin/sh", "-c", "sleep 1; exit 7"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let bundle = only_bundle(output.path());
    let sample: Value =
        serde_json::from_str(&fs::read_to_string(bundle.join("samples.ndjson")).unwrap()).unwrap();
    let row = &sample["processes"][0];
    assert!(row.get("working_set_bytes").is_none());
    assert!(row.get("private_bytes").is_none());
    assert!(row.get("handle_count").is_none());
    let terminal = terminal_event(&bundle);
    assert_eq!(terminal["process_local_id"], 1);
    assert_eq!(terminal["exit_code"], 7);
    assert!(terminal.get("terminal_kind").is_none());
    assert!(terminal.get("signal_number").is_none());
    assert_no_terminal_counters(&terminal);
    let manifest: Value =
        serde_json::from_slice(&fs::read(bundle.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["run_state"], "TARGET_FAILED");
}

#[test]
fn direct_root_signal_has_no_fabricated_exit_code() {
    let output = tempfile::tempdir().unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_perf-probe"))
        .args(["run", "--output"])
        .arg(output.path())
        .args(["--", "/bin/sh", "-c", "sleep 1; kill -TERM $$"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let bundle = only_bundle(output.path());
    let terminal = terminal_event(&bundle);
    assert_eq!(terminal["process_local_id"], 1);
    assert!(terminal.get("exit_code").is_none());
    assert_eq!(terminal["terminal_kind"], "signal");
    assert_eq!(terminal["signal_number"], libc::SIGTERM);
    assert_no_terminal_counters(&terminal);
    let manifest: Value =
        serde_json::from_slice(&fs::read(bundle.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["run_state"], "TARGET_FAILED");
}
