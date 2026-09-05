#![cfg(target_os = "linux")]

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    process::Command,
    thread,
    time::{Duration, Instant},
};

use serde_json::Value;

fn only_bundle(root: &std::path::Path) -> std::path::PathBuf {
    let entries = fs::read_dir(root)
        .expect("read output root")
        .map(|entry| entry.expect("directory entry").path())
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 1, "expected exactly one evidence bundle");
    entries.into_iter().next().expect("bundle")
}

fn assert_raw_only_prelaunch_failure(output: &std::path::Path) {
    let bundle = only_bundle(output);
    assert!(
        fs::read_to_string(bundle.join("processes.ndjson"))
            .expect("process stream")
            .is_empty(),
        "pre-launch failure must not fabricate a root identity"
    );
    assert!(
        fs::read_to_string(bundle.join("samples.ndjson"))
            .expect("sample stream")
            .is_empty(),
        "pre-launch failure must not fabricate a sample"
    );
    assert!(!bundle.join("summary.json").exists());
    assert!(!bundle.join("manifest.json").exists());
}

fn reconstruct_with_public_cli(bundle: &std::path::Path) {
    let result = Command::new(env!("CARGO_BIN_EXE_perf-probe"))
        .args(["summarize", "--bundle"])
        .arg(bundle)
        .output()
        .expect("reconstruct completed bundle");
    assert!(
        result.status.success(),
        "summary stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn run_missing_executable_leaves_only_raw_prelaunch_streams() {
    let output = tempfile::tempdir().expect("temporary output");
    let missing = output.path().join("missing executable with spaces");
    let result = Command::new(env!("CARGO_BIN_EXE_perf-probe"))
        .args(["run", "--output"])
        .arg(output.path())
        .arg("--")
        .arg(missing)
        .arg("argv boundary")
        .output()
        .expect("run perf-probe");

    assert!(!result.status.success());
    assert_raw_only_prelaunch_failure(output.path());
}

#[test]
fn run_non_executable_fixture_leaves_only_raw_prelaunch_streams() {
    let output = tempfile::tempdir().expect("temporary output");
    let fixture_root = tempfile::tempdir().expect("temporary fixture root");
    let fixture = fixture_root.path().join("non executable fixture");
    fs::write(&fixture, b"not executable\n").expect("fixture");
    fs::set_permissions(&fixture, fs::Permissions::from_mode(0o644)).expect("fixture mode");
    let result = Command::new(env!("CARGO_BIN_EXE_perf-probe"))
        .args(["run", "--output"])
        .arg(output.path())
        .arg("--")
        .arg(&fixture)
        .arg("argv boundary")
        .output()
        .expect("run perf-probe");

    assert!(!result.status.success());
    assert_raw_only_prelaunch_failure(output.path());
}

#[test]
fn run_nonzero_root_uses_the_canonical_observed_exit_code() {
    let output = tempfile::tempdir().expect("temporary output");
    let result = Command::new(env!("CARGO_BIN_EXE_perf-probe"))
        .args(["run", "--output"])
        .arg(output.path())
        .args([
            "--",
            "python3",
            "-c",
            "import time; time.sleep(1); raise SystemExit(7)",
            "argv boundary",
        ])
        .output()
        .expect("run perf-probe");
    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let bundle = only_bundle(output.path());
    let events = fs::read_to_string(bundle.join("events.ndjson")).expect("events evidence");
    let summary: Value =
        serde_json::from_slice(&fs::read(bundle.join("summary.json")).unwrap()).expect("summary");
    let manifest: Value =
        serde_json::from_slice(&fs::read(bundle.join("manifest.json")).unwrap()).expect("manifest");
    let platform: Value =
        serde_json::from_slice(&fs::read(bundle.join("platform.json")).unwrap()).expect("platform");

    assert!(events.contains("\"exit_code\":7"));
    assert_eq!(summary["exit_code"], 7);
    assert_eq!(manifest["run_state"], "TARGET_FAILED");
    assert_eq!(
        platform["launched_command_argv"],
        serde_json::json!([
            "python3",
            "-c",
            "import time; time.sleep(1); raise SystemExit(7)",
            "argv boundary",
        ])
    );
    assert_eq!(platform["descendant_scope"], "unknown_not_observed");
    reconstruct_with_public_cli(&bundle);
}

#[test]
fn run_signaled_root_writes_identity_bound_linux_terminal_metadata_without_exit_code() {
    let output = tempfile::tempdir().expect("temporary output");
    let mut collector = Command::new(env!("CARGO_BIN_EXE_perf-probe"))
        .args(["run", "--output"])
        .arg(output.path())
        .args(["--", "sleep", "10"])
        .spawn()
        .expect("start perf-probe");

    let deadline = Instant::now() + Duration::from_secs(5);
    let (bundle, process) = loop {
        if let Ok(bundle) = fs::read_dir(output.path()).and_then(|mut entries| {
            entries
                .next()
                .transpose()?
                .map(|entry| entry.path())
                .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))
        }) {
            if let Ok(raw) = fs::read_to_string(bundle.join("processes.ndjson")) {
                if let Ok(process) = serde_json::from_str::<Value>(&raw) {
                    break (bundle, process);
                }
            }
        }
        assert!(Instant::now() < deadline, "root identity was not persisted");
        thread::sleep(Duration::from_millis(10));
    };
    let pid = process["pid"].as_i64().expect("persisted root PID") as libc::pid_t;
    assert_eq!(unsafe { libc::kill(pid, libc::SIGABRT) }, 0, "signal root");

    let result = collector.wait().expect("wait perf-probe");
    assert!(result.success(), "collector status: {result}");

    let events = fs::read_to_string(bundle.join("events.ndjson")).expect("events evidence");
    let summary: Value =
        serde_json::from_slice(&fs::read(bundle.join("summary.json")).unwrap()).expect("summary");
    let manifest: Value =
        serde_json::from_slice(&fs::read(bundle.join("manifest.json")).unwrap()).expect("manifest");
    let platform: Value =
        serde_json::from_slice(&fs::read(bundle.join("platform.json")).unwrap()).expect("platform");
    let terminal: Value = serde_json::from_slice(
        &fs::read(bundle.join("linux_terminal.json")).expect("Linux terminal metadata"),
    )
    .expect("terminal metadata");

    assert!(!events.contains("\"exit_code\":"));
    assert!(
        summary["exit_code"].is_null(),
        "summary must not carry a synthetic signal exit code"
    );
    assert_eq!(manifest["run_state"], "TARGET_FAILED");
    assert_eq!(
        terminal["root_process_identity"],
        platform["root_process_identity"]
    );
    assert_eq!(terminal["terminal_outcome"]["kind"], "signal");
    assert_eq!(terminal["terminal_outcome"]["signal_number"], libc::SIGABRT);
    assert_eq!(terminal["terminal_outcome"]["signal_name"], "SIGABRT");
    assert!(terminal["terminal_outcome"]["core_dumped"].is_boolean());
    assert_eq!(
        platform["root_observation_authority"],
        "directly_owned_child_wait"
    );
    assert_eq!(platform["descendant_scope"], "unknown_not_observed");
    reconstruct_with_public_cli(&bundle);
}

#[test]
fn run_launches_a_direct_root_with_identity_samples_and_root_only_scope() {
    let output = tempfile::tempdir().expect("temporary output");
    let scratch = tempfile::tempdir().expect("temporary target scratch");
    let target = env!("CARGO_BIN_EXE_perf-workload");
    let argv_boundary_path = scratch.path().join("argv boundary file");
    let result = Command::new(env!("CARGO_BIN_EXE_perf-probe"))
        .args(["run", "--output"])
        .arg(output.path())
        .args(["--", target, "sequential-write"])
        .arg(&argv_boundary_path)
        .output()
        .expect("run perf-probe");
    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        argv_boundary_path.is_file(),
        "target did not receive argv path"
    );

    let bundle = only_bundle(output.path());
    let process: Value =
        serde_json::from_str(&fs::read_to_string(bundle.join("processes.ndjson")).unwrap())
            .expect("process record JSON");
    let samples = fs::read_to_string(bundle.join("samples.ndjson")).unwrap();
    let sample: Value =
        serde_json::from_str(samples.lines().next().expect("first sample")).expect("sample JSON");
    let events = fs::read_to_string(bundle.join("events.ndjson")).expect("events evidence");
    let summary: Value =
        serde_json::from_slice(&fs::read(bundle.join("summary.json")).unwrap()).expect("summary");
    let manifest: Value =
        serde_json::from_slice(&fs::read(bundle.join("manifest.json")).unwrap()).expect("manifest");
    let platform: Value =
        serde_json::from_slice(&fs::read(bundle.join("platform.json")).unwrap()).expect("platform");

    assert_eq!(process["process_local_id"], 1);
    assert!(process["pid"].as_u64().unwrap() > 0);
    assert!(process["process_start_time"].as_u64().unwrap() > 0);
    assert_eq!(process["boot_identity"].as_str().unwrap().len(), 36);
    assert_eq!(process["discovery_source"], "linux_run_direct_root");
    assert_eq!(
        process["handle_acquisition_result"],
        "owned_child_wait_handle"
    );

    let row = &sample["processes"][0];
    assert_eq!(sample["root_process_confirmed_live"], true);
    assert!(row["user_cpu_time_ns"].is_number());
    assert!(row["kernel_cpu_time_ns"].is_number());
    assert!(row["thread_count"].is_number());
    assert!(sample["probe"]["user_cpu_time_ns"].is_number());
    assert!(sample["probe"]["kernel_cpu_time_ns"].is_number());
    assert_eq!(events.matches("semantic_mismatch").count(), 22);

    assert_eq!(summary["exit_code"], 0);
    assert!(summary.get("total_cpu_time_ns").is_none());
    assert_eq!(manifest["run_state"], "COMPLETE");
    assert_eq!(manifest["measurement_validity"], "VALID");
    assert_eq!(manifest["measurement_completeness"], "DECLARED_PARTIAL");

    assert_eq!(platform["mode"], "run");
    assert_eq!(
        platform["launched_command_argv"],
        serde_json::json!([target, "sequential-write", argv_boundary_path])
    );
    assert_eq!(
        platform["root_observation_authority"],
        "directly_owned_child_wait"
    );
    assert_eq!(platform["process_tree_closure"], "not_claimed");
    assert_eq!(platform["job_accounting"], "not_claimed");
    assert_eq!(platform["descendant_scope"], "unknown_not_observed");
    reconstruct_with_public_cli(&bundle);
}

#[test]
fn run_cpu_workload_persists_increasing_root_cpu_across_live_samples() {
    let output = tempfile::tempdir().expect("temporary output");
    let result = Command::new(env!("CARGO_BIN_EXE_perf-probe"))
        .args(["run", "--output"])
        .arg(output.path())
        .args(["--", env!("CARGO_BIN_EXE_perf-workload"), "cpu-single"])
        .output()
        .expect("run perf-probe");
    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let bundle = only_bundle(output.path());
    let samples = fs::read_to_string(bundle.join("samples.ndjson"))
        .expect("sample evidence")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("sample JSON"))
        .collect::<Vec<_>>();
    assert!(
        samples.len() >= 3,
        "CPU fixture must span repeated live samples"
    );
    let cpu = samples
        .iter()
        .map(|sample| {
            let root = &sample["processes"][0];
            root["user_cpu_time_ns"].as_u64().expect("root user CPU")
                + root["kernel_cpu_time_ns"]
                    .as_u64()
                    .expect("root kernel CPU")
        })
        .collect::<Vec<_>>();
    assert!(cpu.windows(2).all(|pair| pair[1] >= pair[0]));
    assert!(
        cpu.last().unwrap() > cpu.first().unwrap(),
        "CPU fixture must increase an observed root counter"
    );
    for sample in &samples {
        assert_eq!(sample["root_process_confirmed_live"], true);
        assert_eq!(sample["processes"].as_array().unwrap().len(), 1);
        assert!(sample.get("job").is_none());
        assert!(sample.get("process_set_working_set_sum_bytes").is_none());
        assert!(sample.get("process_set_private_bytes_sum").is_none());
    }
    let summary: Value =
        serde_json::from_slice(&fs::read(bundle.join("summary.json")).unwrap()).unwrap();
    assert!(summary["total_cpu_time_ns"].is_null());
    reconstruct_with_public_cli(&bundle);
}

#[test]
fn run_already_exited_root_finalizes_only_the_pre_exit_live_sample() {
    let output = tempfile::tempdir().expect("temporary output");
    let result = Command::new(env!("CARGO_BIN_EXE_perf-probe"))
        .args(["run", "--output"])
        .arg(output.path())
        .args(["--", env!("CARGO_BIN_EXE_perf-workload"), "linux-exit"])
        .output()
        .expect("run perf-probe");

    assert!(
        result.status.success(),
        "already exited root status: {:?}",
        result.status
    );
    let bundle = only_bundle(output.path());
    let samples = fs::read_to_string(bundle.join("samples.ndjson"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(samples.len(), 1, "no post-exit live sample is fabricated");
    assert_eq!(samples[0]["root_process_confirmed_live"], true);
    let summary: Value =
        serde_json::from_slice(&fs::read(bundle.join("summary.json")).unwrap()).unwrap();
    assert_eq!(summary["exit_code"], 0);
    assert!(summary["total_cpu_time_ns"].is_null());
    reconstruct_with_public_cli(&bundle);
}

#[test]
fn collector_interruption_leaves_raw_only_evidence_without_terminal_fabrication() {
    let output = tempfile::tempdir().expect("temporary output");
    let mut collector = Command::new(env!("CARGO_BIN_EXE_perf-probe"))
        .args(["run", "--output"])
        .arg(output.path())
        .args(["--", env!("CARGO_BIN_EXE_perf-workload"), "linux-hold-long"])
        .spawn()
        .expect("start interruptible collector");

    let deadline = Instant::now() + Duration::from_secs(5);
    let (bundle, root_pid) = loop {
        if let Ok(bundle) = fs::read_dir(output.path()).and_then(|mut entries| {
            entries
                .next()
                .transpose()?
                .map(|entry| entry.path())
                .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))
        }) {
            if let Ok(raw) = fs::read_to_string(bundle.join("processes.ndjson")) {
                if let Ok(process) = serde_json::from_str::<Value>(&raw) {
                    break (
                        bundle,
                        process["pid"].as_i64().expect("root PID") as libc::pid_t,
                    );
                }
            }
        }
        assert!(Instant::now() < deadline, "root identity was not persisted");
        thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(
        unsafe { libc::kill(collector.id() as libc::pid_t, libc::SIGKILL) },
        0,
        "interrupt collector"
    );
    let _ = collector.wait().expect("reap interrupted collector");
    assert_eq!(
        unsafe { libc::kill(root_pid, libc::SIGKILL) },
        0,
        "clean known fixture root"
    );

    assert!(!bundle.join("summary.json").exists());
    assert!(!bundle.join("manifest.json").exists());
    let events = fs::read_to_string(bundle.join("events.ndjson")).unwrap_or_default();
    assert!(
        !events.contains("process_exit_observed"),
        "collector interruption must not fabricate observed root termination"
    );
}
