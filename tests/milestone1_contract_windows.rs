#![cfg(windows)]

use std::{fs, mem::size_of, process::Command};

use serde_json::Value;
use windows_sys::{
    Wdk::System::SystemServices::RtlGetVersion, Win32::System::SystemInformation::OSVERSIONINFOW,
};

fn probe() -> &'static str {
    env!("CARGO_BIN_EXE_perf-probe")
}

fn only_bundle(root: &std::path::Path) -> std::path::PathBuf {
    let entries = fs::read_dir(root)
        .expect("read output root")
        .map(|entry| entry.expect("directory entry").path())
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 1, "exactly one evidence bundle");
    entries.into_iter().next().expect("bundle")
}

fn lines(path: &std::path::Path) -> Vec<Value> {
    fs::read_to_string(path)
        .expect("read NDJSON")
        .lines()
        .map(|line| serde_json::from_str(line).expect("valid NDJSON record"))
        .collect()
}

#[test]
fn launch_bundle_records_required_metadata_real_metrics_and_release_lifecycle() {
    let output = tempfile::tempdir().expect("output root");
    let result = Command::new(probe())
        .args(["run", "--output"])
        .arg(output.path())
        .args(["--", "cmd.exe", "/c", "ping -n 3 127.0.0.1 > nul"])
        .output()
        .expect("launch probe");
    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let bundle = only_bundle(output.path());
    for name in [
        "manifest.json",
        "host.json",
        "target.json",
        "config.json",
        "capabilities.json",
        "processes.ndjson",
        "samples.ndjson",
        "events.ndjson",
        "summary.json",
    ] {
        assert!(
            bundle.join(name).is_file(),
            "required artifact missing: {name}"
        );
    }

    let manifest: Value =
        serde_json::from_slice(&fs::read(bundle.join("manifest.json")).expect("manifest"))
            .expect("manifest JSON");
    assert_eq!(manifest["schema_draft_version"], "perf-evidence-v1-draft");
    assert_eq!(manifest["run_state"], "COMPLETE");
    assert_eq!(manifest["measurement_validity"], "VALID");
    assert!(manifest["artifact_list"].as_array().unwrap().len() >= 8);
    let host: Value = serde_json::from_slice(&fs::read(bundle.join("host.json")).expect("host"))
        .expect("host JSON");
    assert!(
        host["os_version"]
            .as_str()
            .is_some_and(|value| value.contains('.')),
        "host must record a numeric Windows version rather than an environment label"
    );
    assert!(
        host["os_build"].as_u64().is_some(),
        "host must record Windows build number"
    );
    assert!(
        host["physical_core_count"]
            .as_u64()
            .is_some_and(|value| value > 0),
        "host must record actual physical core count"
    );
    assert!(
        host["installed_ram_bytes"]
            .as_u64()
            .is_some_and(|value| value > 0),
        "host must record installed physical RAM"
    );

    let processes = lines(&bundle.join("processes.ndjson"));
    assert!(
        processes
            .iter()
            .any(|process| process["process_local_id"] == 1 && process["pid"].as_u64().is_some()),
        "root identity must be registered"
    );
    assert!(
        processes
            .iter()
            .all(|process| process["boot_identity"].as_str().is_some())
    );

    let samples = lines(&bundle.join("samples.ndjson"));
    assert!(
        samples.len() >= 2,
        "long workload should produce repeated samples"
    );
    assert!(samples.iter().all(|sample| {
        sample["system"]["available_physical_memory_bytes"]
            .as_u64()
            .unwrap_or_default()
            > 0
    }));
    assert!(samples.iter().all(|sample| {
        sample["probe"]["working_set_bytes"]
            .as_u64()
            .unwrap_or_default()
            > 0
    }));
    assert!(samples.iter().any(|sample| {
        sample["processes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|process| process["thread_count"].as_u64().unwrap_or_default() > 0)
    }));
    assert!(samples.iter().any(|sample| {
        sample["job"]["total_processes_os"]
            .as_u64()
            .unwrap_or_default()
            >= 2
    }));

    let events = lines(&bundle.join("events.ndjson"));
    assert!(
        events
            .iter()
            .any(|event| event["record_type"] == "completion_port_prepared")
    );
    assert!(
        events
            .iter()
            .any(|event| event["record_type"] == "process_exit_observed")
    );
    assert!(
        events
            .iter()
            .any(|event| event["record_type"] == "handle_released")
    );

    let summary: Value =
        serde_json::from_slice(&fs::read(bundle.join("summary.json")).expect("summary"))
            .expect("summary JSON");
    assert!(
        summary["observed_distinct_process_count"]
            .as_u64()
            .unwrap_or_default()
            >= 1
    );
    assert!(
        summary["job_processes_without_observed_identity"]
            .as_u64()
            .is_some()
    );
    assert!(
        summary["max_sample_gap_exact_ns"]
            .as_u64()
            .unwrap_or_default()
            > 0
    );
}

#[test]
fn host_metadata_matches_the_unmanifested_ntdll_os_version_source() {
    // GetVersionExW reports 6.2 without a Windows 8.1/10 manifest. Host
    // metadata must not serialize that compatibility value as actual facts.
    let mut expected = OSVERSIONINFOW::default();
    expected.dwOSVersionInfoSize = size_of::<OSVERSIONINFOW>() as u32;
    let status = unsafe { RtlGetVersion(&mut expected) };
    assert_eq!(status, 0, "RtlGetVersion must return STATUS_SUCCESS");

    let output = tempfile::tempdir().expect("output root");
    let result = Command::new(probe())
        .args(["run", "--output"])
        .arg(output.path())
        .args(["--", "cmd.exe", "/c", "ping -n 3 127.0.0.1 > nul"])
        .output()
        .expect("launch probe");
    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let host: Value = serde_json::from_slice(
        &fs::read(only_bundle(output.path()).join("host.json")).expect("host"),
    )
    .expect("host JSON");
    assert_eq!(
        host["os_version"],
        format!("{}.{}", expected.dwMajorVersion, expected.dwMinorVersion),
        "host OS version must match RtlGetVersion rather than GetVersionExW compatibility reporting"
    );
    assert_eq!(
        host["os_build"], expected.dwBuildNumber,
        "host OS build must match RtlGetVersion"
    );
}

#[test]
fn root_identity_uses_an_authoritative_windows_boot_session_value() {
    let output = tempfile::tempdir().expect("output root");
    let result = Command::new(probe())
        .args(["run", "--output"])
        .arg(output.path())
        .args(["--", "cmd.exe", "/c", "ping -n 2 127.0.0.1 > nul"])
        .output()
        .expect("launch probe");
    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let processes = lines(&only_bundle(output.path()).join("processes.ndjson"));
    let root = processes
        .iter()
        .find(|record| record["process_local_id"] == 1)
        .expect("root identity record");
    let boot_identity = root["boot_identity"]
        .as_str()
        .expect("root boot identity string");
    assert!(
        boot_identity.starts_with("windows-boot-time-filetime-"),
        "identity must come from the OS boot-time query, not a collector wall-clock estimate: {boot_identity}"
    );
    let encoded = boot_identity
        .strip_prefix("windows-boot-time-filetime-")
        .expect("known prefix");
    assert!(
        u64::from_str_radix(encoded, 16).is_ok(),
        "boot-time identity must contain the native FILETIME value: {boot_identity}"
    );
}

#[test]
fn child_tree_is_registered_individually_without_fabricating_job_identity_coverage() {
    let output = tempfile::tempdir().expect("output root");
    let result = Command::new(probe())
        .args(["run", "--output"])
        .arg(output.path())
        .args([
            "--",
            "cmd.exe",
            "/c",
            "cmd.exe /c \"ping -n 4 127.0.0.1 > nul\" & ping -n 4 127.0.0.1 > nul",
        ])
        .output()
        .expect("launch probe");
    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let bundle = only_bundle(output.path());
    let processes = lines(&bundle.join("processes.ndjson"));
    assert!(
        processes.len() >= 2,
        "observable child must be registered separately"
    );
    assert!(
        processes
            .iter()
            .any(|record| record["parent_local_id"].as_u64().is_some())
    );
    let summary: Value =
        serde_json::from_slice(&fs::read(bundle.join("summary.json")).expect("summary"))
            .expect("summary JSON");
    let observed = summary["observed_distinct_process_count"]
        .as_u64()
        .expect("observed count");
    let job_total = summary["job_total_processes_os"]
        .as_u64()
        .expect("job total");
    assert!(observed >= 2);
    assert!(
        job_total >= observed,
        "Job count is OS authority; no fake identities may close its gap"
    );
    assert_eq!(
        summary["job_processes_without_observed_identity"],
        job_total.saturating_sub(observed),
        "unobserved count must be derived rather than filled with invented identities",
    );
}

#[test]
fn job_terminated_count_is_mapped_from_native_accounting_not_synthesized() {
    // This is a source-level regression guard only. The W7/W8 dynamic workload
    // evidence remains the Contract-grade accounting proof.
    let runtime_source = include_str!("../src/runtime/windows.rs");
    assert!(
        runtime_source.contains("accounting.BasicInfo.TotalTerminatedProcesses"),
        "Job accounting must map the native TotalTerminatedProcesses field"
    );
    assert!(
        !runtime_source.contains("total_terminated_by_limit_os: 0"),
        "a fixed zero makes unavailable/native accounting indistinguishable"
    );
}

#[test]
fn a12_native_job_accounting_preserves_raw_counter_semantics_without_zero_substitution() {
    let runtime_source = include_str!("../src/runtime/windows.rs");
    for required_mapping in [
        "total_user_time_ns: (accounting.BasicInfo.TotalUserTime as u64)",
        "total_kernel_time_ns: (accounting.BasicInfo.TotalKernelTime as u64)",
        "read_operation_count: accounting.IoInfo.ReadOperationCount",
        "write_operation_count: accounting.IoInfo.WriteOperationCount",
        "other_operation_count: accounting.IoInfo.OtherOperationCount",
        "read_transfer_bytes: accounting.IoInfo.ReadTransferCount",
        "write_transfer_bytes: accounting.IoInfo.WriteTransferCount",
        "other_transfer_bytes: accounting.IoInfo.OtherTransferCount",
        "total_processes_os: accounting.BasicInfo.TotalProcesses as u64",
        "active_processes_os: accounting.BasicInfo.ActiveProcesses as u64",
        "total_terminated_by_limit_os: accounting.BasicInfo.TotalTerminatedProcesses as u64",
    ] {
        assert!(
            runtime_source.contains(required_mapping),
            "missing native accounting mapping: {required_mapping}"
        );
    }

    let output = tempfile::tempdir().expect("D-bound output root");
    let scratch = tempfile::tempdir().expect("D-bound scratch root");
    let result = Command::new(probe())
        .args(["run", "--output"])
        .arg(output.path())
        .args([
            "--",
            env!("CARGO_BIN_EXE_perf-workload"),
            "sequential-write",
        ])
        .arg(scratch.path().join("a12.bin"))
        .output()
        .expect("launch raw native-accounting workload");
    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let sample = lines(&only_bundle(output.path()).join("samples.ndjson"))
        .pop()
        .expect("final sample");
    let job = sample["job"]
        .as_object()
        .expect("native Job accounting object");
    for key in [
        "total_user_time_ns",
        "total_kernel_time_ns",
        "read_operation_count",
        "write_operation_count",
        "other_operation_count",
        "read_transfer_bytes",
        "write_transfer_bytes",
        "other_transfer_bytes",
        "total_processes_os",
        "active_processes_os",
        "total_terminated_by_limit_os",
    ] {
        assert!(
            job.get(key).and_then(Value::as_u64).is_some(),
            "missing native counter {key}"
        );
    }
    assert!(
        job["write_transfer_bytes"]
            .as_u64()
            .expect("write transfer")
            >= 4 * 1024 * 1024
    );
    assert!(
        job["total_processes_os"]
            .as_u64()
            .expect("total process count")
            >= job["active_processes_os"]
                .as_u64()
                .expect("active process count")
    );
}

#[test]
fn discovery_registry_is_pruned_to_live_processes_between_snapshots() {
    // Source-level guard; W4 separately supplies the dynamic child-churn proof.
    let runtime_source = include_str!("../src/runtime/windows.rs");
    assert!(
        runtime_source.contains("known_processes.retain(|pid, _| live_pids.contains(pid))"),
        "completed child PIDs must not remain in the in-memory discovery registry for the entire run"
    );
}
