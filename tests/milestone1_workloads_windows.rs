#![cfg(windows)]

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::Duration,
};

use perf_evidence_probe::{ndjson::read_complete_records, summary::regenerate_summary};
use serde_json::Value;

fn probe() -> &'static str {
    env!("CARGO_BIN_EXE_perf-probe")
}

fn workload() -> PathBuf {
    Path::new(probe())
        .parent()
        .expect("probe parent")
        .join("perf-workload.exe")
}

fn only_bundle(root: &Path) -> PathBuf {
    let entries = fs::read_dir(root)
        .expect("read output root")
        .map(|entry| entry.expect("directory entry").path())
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 1, "exactly one evidence bundle");
    entries.into_iter().next().expect("bundle")
}

fn lines(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .expect("read NDJSON")
        .lines()
        .map(|line| serde_json::from_str(line).expect("valid NDJSON record"))
        .collect()
}

#[test]
fn w1_memory_ramp_reports_a_sustained_private_and_working_set_increase() {
    let output = tempfile::tempdir().expect("D-bound output root");
    let result = Command::new(probe())
        .args(["run", "--output"])
        .arg(output.path())
        .args(["--"])
        .arg(workload())
        .arg("memory-ramp")
        .output()
        .expect("launch probe over W1");
    assert!(
        result.status.success(),
        "W1 workload must complete: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let samples = lines(&only_bundle(output.path()).join("samples.ndjson"));
    assert!(samples.len() >= 4, "W1 must span multiple 500ms samples");
    let root_memory = samples
        .iter()
        .filter_map(|sample| {
            sample["processes"]
                .as_array()?
                .iter()
                .find(|row| row["process_local_id"] == 1)
                .map(|row| {
                    (
                        row["private_bytes"].as_u64().expect("private bytes"),
                        row["working_set_bytes"]
                            .as_u64()
                            .expect("working set bytes"),
                    )
                })
        })
        .collect::<Vec<_>>();
    let baseline = root_memory.first().expect("initial root sample");
    let peak = root_memory
        .iter()
        .max_by_key(|(private, _)| *private)
        .expect("peak root sample");
    assert!(
        peak.0 >= baseline.0.saturating_add(8 * 1024 * 1024),
        "W1 private bytes must ramp by at least 8 MiB: baseline={baseline:?}, peak={peak:?}"
    );
    assert!(
        peak.1 >= baseline.1.saturating_add(8 * 1024 * 1024),
        "W1 working set must ramp by at least 8 MiB: baseline={baseline:?}, peak={peak:?}"
    );
}

#[test]
fn w2_memory_spike_is_visible_in_raw_samples_without_claiming_os_peak_semantics() {
    let output = tempfile::tempdir().expect("D-bound output root");
    let result = Command::new(probe())
        .args(["run", "--output"])
        .arg(output.path())
        .args(["--"])
        .arg(workload())
        .arg("memory-spike")
        .output()
        .expect("launch probe over W2");
    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let samples = lines(&only_bundle(output.path()).join("samples.ndjson"));
    assert!(
        samples.len() >= 4,
        "W2 must span the 500ms sampling boundary"
    );
    let private_bytes = samples
        .iter()
        .filter_map(|sample| sample["processes"].as_array())
        .filter_map(|rows| rows.iter().find(|row| row["process_local_id"] == 1))
        .map(|row| row["private_bytes"].as_u64().expect("private bytes"))
        .collect::<Vec<_>>();
    let baseline = *private_bytes.first().expect("baseline private bytes");
    let (peak_index, peak) = private_bytes
        .iter()
        .enumerate()
        .max_by_key(|(_, value)| *value)
        .map(|(index, value)| (index, *value))
        .expect("peak private bytes");
    assert!(
        peak >= baseline.saturating_add(32 * 1024 * 1024),
        "W2 spike must be sampled"
    );
    assert!(
        peak_index + 1 < private_bytes.len(),
        "W2 must continue after the sampled spike"
    );
}

#[test]
fn w5_single_thread_cpu_load_increases_raw_cumulative_job_cpu() {
    let output = tempfile::tempdir().expect("D-bound output root");
    let result = Command::new(probe())
        .args(["run", "--output"])
        .arg(output.path())
        .args(["--"])
        .arg(workload())
        .arg("cpu-single")
        .output()
        .expect("launch probe over W5");
    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let cpu = lines(&only_bundle(output.path()).join("samples.ndjson"))
        .iter()
        .map(|sample| {
            sample["job"]["total_user_time_ns"]
                .as_u64()
                .expect("job user")
                + sample["job"]["total_kernel_time_ns"]
                    .as_u64()
                    .expect("job kernel")
        })
        .collect::<Vec<_>>();
    assert!(cpu.len() >= 3, "W5 needs repeated samples");
    assert!(
        cpu.windows(2).all(|pair| pair[1] >= pair[0]),
        "native cumulative CPU cannot go backward"
    );
    assert!(
        *cpu.last().expect("final CPU") >= 500_000_000,
        "W5 must produce sustained single-thread CPU work"
    );
}

#[test]
fn w6_multi_thread_cpu_load_exposes_multiple_threads_and_cumulative_job_cpu() {
    let output = tempfile::tempdir().expect("D-bound output root");
    let result = Command::new(probe())
        .args(["run", "--output"])
        .arg(output.path())
        .args(["--"])
        .arg(workload())
        .arg("cpu-multi")
        .output()
        .expect("launch probe over W6");
    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let samples = lines(&only_bundle(output.path()).join("samples.ndjson"));
    assert!(
        samples.iter().any(|sample| {
            sample["processes"]
                .as_array()
                .and_then(|rows| rows.iter().find(|row| row["process_local_id"] == 1))
                .and_then(|root| root["thread_count"].as_u64())
                .is_some_and(|count| count >= 2)
        }),
        "W6 must expose multi-thread target telemetry"
    );
    let final_cpu = samples.last().expect("final sample")["job"]["total_user_time_ns"]
        .as_u64()
        .expect("job user")
        + samples.last().expect("final sample")["job"]["total_kernel_time_ns"]
            .as_u64()
            .expect("job kernel");
    assert!(
        final_cpu >= 700_000_000,
        "W6 must produce sustained multi-thread CPU work"
    );
}

#[test]
fn w9_sequential_read_increases_raw_process_and_job_read_counters() {
    let output = tempfile::tempdir().expect("D-bound output root");
    let scratch = tempfile::tempdir().expect("D-bound scratch root");
    let source = scratch.path().join("w9-source.bin");
    fs::write(&source, vec![0x33_u8; 8 * 1024 * 1024]).expect("create W9 source data");
    let result = Command::new(probe())
        .args(["run", "--output"])
        .arg(output.path())
        .args(["--"])
        .arg(workload())
        .args(["sequential-read", source.to_string_lossy().as_ref()])
        .output()
        .expect("launch probe over W9");
    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let samples = lines(&only_bundle(output.path()).join("samples.ndjson"));
    let final_sample = samples.last().expect("final W9 sample");
    let root = final_sample["processes"]
        .as_array()
        .expect("process rows")
        .iter()
        .find(|row| row["process_local_id"] == 1)
        .expect("root row");
    assert!(root["read_bytes"].as_u64().expect("root read bytes") >= 8 * 1024 * 1024);
    assert!(
        final_sample["job"]["read_transfer_bytes"]
            .as_u64()
            .expect("job read bytes")
            >= 8 * 1024 * 1024
    );
    assert!(
        final_sample["job"]["read_operation_count"]
            .as_u64()
            .expect("job reads")
            > 0
    );
}

#[test]
fn w10_sequential_write_increases_raw_process_and_job_write_counters() {
    let output = tempfile::tempdir().expect("D-bound output root");
    let scratch = tempfile::tempdir().expect("D-bound scratch root");
    let destination = scratch.path().join("w10-destination.bin");
    let result = Command::new(probe())
        .args(["run", "--output"])
        .arg(output.path())
        .args(["--"])
        .arg(workload())
        .args(["sequential-write", destination.to_string_lossy().as_ref()])
        .output()
        .expect("launch probe over W10");
    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let samples = lines(&only_bundle(output.path()).join("samples.ndjson"));
    let final_sample = samples.last().expect("final W10 sample");
    let root = final_sample["processes"]
        .as_array()
        .expect("process rows")
        .iter()
        .find(|row| row["process_local_id"] == 1)
        .expect("root row");
    assert!(root["write_bytes"].as_u64().expect("root write bytes") >= 4 * 1024 * 1024);
    assert!(
        final_sample["job"]["write_transfer_bytes"]
            .as_u64()
            .expect("job write bytes")
            >= 4 * 1024 * 1024
    );
    assert!(
        final_sample["job"]["write_operation_count"]
            .as_u64()
            .expect("job writes")
            > 0
    );
}

#[test]
fn a6_target_abort_leaves_parseable_raw_evidence_and_reconstructable_summary() {
    let output = tempfile::tempdir().expect("D-bound output root");
    let result = Command::new(probe())
        .args(["run", "--output"])
        .arg(output.path())
        .args(["--"])
        .arg(workload())
        .arg("target-abort")
        .output()
        .expect("launch probe over A6 target abort");
    assert!(
        result.status.success(),
        "probe must preserve evidence after target abort: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let bundle = only_bundle(output.path());
    let manifest: Value =
        serde_json::from_slice(&fs::read(bundle.join("manifest.json")).expect("manifest"))
            .expect("manifest JSON");
    assert_eq!(manifest["run_state"], "TARGET_FAILED");
    let events = lines(&bundle.join("events.ndjson"));
    let terminal = events
        .iter()
        .find(|event| event["record_type"] == "process_exit_observed")
        .expect("target abort must produce terminal evidence");
    assert_ne!(
        terminal["exit_code"].as_u64(),
        Some(101),
        "A6 must exercise abort rather than an unknown-workload panic"
    );
    for name in ["processes.ndjson", "samples.ndjson", "events.ndjson"] {
        assert!(
            !read_complete_records(&bundle.join(name))
                .expect("parseable post-crash NDJSON")
                .is_empty(),
            "{name} must retain complete records"
        );
    }
    regenerate_summary(&bundle).expect("summary must reconstruct after target abort");
}

#[test]
fn a7_controlled_probe_interruption_leaves_flushed_parseable_ndjson() {
    let output = tempfile::tempdir().expect("D-bound output root");
    let mut collector = Command::new(probe())
        .args(["run", "--output"])
        .arg(output.path())
        .args(["--"])
        .arg(workload())
        // A long stable target ensures this always exercises collector
        // interruption, even when other workload tests consume all CPUs.
        .arg("long-hold")
        .spawn()
        .expect("launch interruptible probe");
    thread::sleep(Duration::from_millis(900));
    let kill = Command::new("taskkill")
        .args(["/PID", &collector.id().to_string(), "/F"])
        .output()
        .expect("issue controlled probe interruption");
    assert!(
        kill.status.success(),
        "taskkill stderr: {}",
        String::from_utf8_lossy(&kill.stderr)
    );
    let _ = collector.wait().expect("reap interrupted probe");
    thread::sleep(Duration::from_millis(1_200));

    let bundle = only_bundle(output.path());
    for name in ["processes.ndjson", "samples.ndjson", "events.ndjson"] {
        let records = read_complete_records(&bundle.join(name)).unwrap_or_else(|error| {
            panic!("{name} must remain parseable after interruption: {error}")
        });
        assert!(
            !records.is_empty(),
            "{name} must contain at least one flushed record"
        );
    }
    assert!(
        !bundle.join("summary.json").exists(),
        "probe interruption is raw-evidence-only and must not masquerade as completed summary generation"
    );
}

#[test]
fn a9_long_run_keeps_probe_private_memory_bounded_after_warmup() {
    let output = tempfile::tempdir().expect("D-bound output root");
    let result = Command::new(probe())
        .args(["run", "--output"])
        .arg(output.path())
        .args(["--"])
        .arg(workload())
        .arg("long-hold")
        .output()
        .expect("launch probe over long A9 workload");
    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let private = lines(&only_bundle(output.path()).join("samples.ndjson"))
        .iter()
        .map(|sample| {
            sample["probe"]["private_bytes"]
                .as_u64()
                .expect("probe private bytes")
        })
        .collect::<Vec<_>>();
    assert!(
        private.len() >= 12,
        "A9 must exercise a long multi-sample run"
    );
    let steady = &private[2..];
    let min = *steady.iter().min().expect("steady minimum");
    let max = *steady.iter().max().expect("steady maximum");
    assert!(
        max.saturating_sub(min) <= 32 * 1024 * 1024,
        "probe private bytes must not grow with run duration: min={min}, max={max}"
    );
}

#[test]
fn w7_child_cpu_remains_in_job_accounting_after_child_exit() {
    assert!(
        workload().is_file(),
        "W7 requires the dedicated synthetic perf-workload executable"
    );
    let output = tempfile::tempdir().expect("output root");
    let result = Command::new(probe())
        .args(["run", "--output"])
        .arg(output.path())
        .args(["--"])
        .arg(workload())
        .arg("child-cpu-then-exit")
        .output()
        .expect("launch probe over W7");
    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let samples = lines(&only_bundle(output.path()).join("samples.ndjson"));
    assert!(
        samples.len() >= 4,
        "W7 must provide post-child-exit samples"
    );
    let job_cpu = samples
        .iter()
        .map(|sample| {
            sample["job"]["total_user_time_ns"]
                .as_u64()
                .expect("Job user CPU")
                + sample["job"]["total_kernel_time_ns"]
                    .as_u64()
                    .expect("Job kernel CPU")
        })
        .collect::<Vec<_>>();
    assert!(
        job_cpu.windows(2).all(|pair| pair[1] >= pair[0]),
        "Job accounting must be cumulative"
    );
    assert!(
        *job_cpu.last().expect("last Job sample") >= 200_000_000,
        "exited child CPU must remain in the later Job total"
    );
}

#[test]
fn w8_child_io_remains_in_job_accounting_after_child_exit() {
    let output = tempfile::tempdir().expect("output root");
    let scratch = tempfile::tempdir().expect("scratch directory");
    let result = Command::new(probe())
        .args(["run", "--output"])
        .arg(output.path())
        .args(["--"])
        .arg(workload())
        .args([
            "child-io-then-exit",
            scratch.path().join("w8.bin").to_string_lossy().as_ref(),
        ])
        .output()
        .expect("launch probe over W8");
    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let samples = lines(&only_bundle(output.path()).join("samples.ndjson"));
    assert!(
        samples.len() >= 4,
        "W8 must provide post-child-exit samples"
    );
    let job_writes = samples
        .iter()
        .map(|sample| {
            sample["job"]["write_transfer_bytes"]
                .as_u64()
                .expect("Job write bytes")
        })
        .collect::<Vec<_>>();
    assert!(
        job_writes.windows(2).all(|pair| pair[1] >= pair[0]),
        "Job I/O accounting must be cumulative"
    );
    assert!(
        *job_writes.last().expect("last Job sample") >= 1_000_000,
        "exited child I/O must remain in the later Job total"
    );
}

#[test]
fn w3_child_tree_samples_each_live_observed_identity_as_a_process_set_sum() {
    let output = tempfile::tempdir().expect("output root");
    let result = Command::new(probe())
        .args(["run", "--output"])
        .arg(output.path())
        .args(["--"])
        .arg(workload())
        .arg("child-tree")
        .output()
        .expect("launch probe over W3");
    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let bundle = only_bundle(output.path());
    let identities = lines(&bundle.join("processes.ndjson"));
    assert!(
        identities.len() >= 3,
        "W3 requires root, child, and grandchild identities"
    );
    let samples = lines(&bundle.join("samples.ndjson"));
    let multi_process_sample = samples
        .iter()
        .find(|sample| {
            sample["processes"]
                .as_array()
                .is_some_and(|rows| rows.len() >= 3)
        })
        .expect("a sample must include every live retained identity");
    let samples_ids = multi_process_sample["processes"]
        .as_array()
        .expect("process sample rows")
        .iter()
        .filter_map(|row| row["process_local_id"].as_u64())
        .collect::<std::collections::BTreeSet<_>>();
    let registry_ids = identities
        .iter()
        .filter_map(|row| row["process_local_id"].as_u64())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(samples_ids.is_subset(&registry_ids));
    assert!(
        multi_process_sample.get("unique_memory_bytes").is_none(),
        "working-set aggregation must never be presented as unique physical memory"
    );
    let row_sum = multi_process_sample["processes"]
        .as_array()
        .expect("process sample rows")
        .iter()
        .map(|row| row["working_set_bytes"].as_u64().expect("working set"))
        .sum::<u64>();
    assert_eq!(
        multi_process_sample["process_set_working_set_sum_bytes"], row_sum,
        "aggregate must be an explicitly named process-set sum"
    );
}

#[test]
fn root_exit_drains_retained_child_lifecycle_before_bundle_completion() {
    let output = tempfile::tempdir().expect("output root");
    let result = Command::new(probe())
        .args(["run", "--output"])
        .arg(output.path())
        .args(["--"])
        .arg(workload())
        .arg("root-exit-child-hold")
        .output()
        .expect("launch probe over root-exit child workload");
    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let bundle = only_bundle(output.path());
    let processes = lines(&bundle.join("processes.ndjson"));
    let child_id = processes
        .iter()
        .find(|record| record["process_local_id"] != 1)
        .and_then(|record| record["process_local_id"].as_u64())
        .expect("retained child identity");
    let events = lines(&bundle.join("events.ndjson"));
    assert!(events.iter().any(|event| {
        event["record_type"] == "process_exit_observed" && event["pid"].as_u64().is_some()
    }));
    assert!(
        events.iter().any(|event| {
            event["record_type"] == "handle_released" && event["process_local_id"] == child_id
        }),
        "retained child handle must be released after its terminal finalization"
    );
}

#[test]
fn w4_short_lived_children_degrade_at_the_handle_cap_without_unbounded_retention() {
    let output = tempfile::tempdir().expect("output root");
    let result = Command::new(probe())
        .args(["run", "--max-retained-process-handles", "3", "--output"])
        .arg(output.path())
        .args(["--"])
        .arg(workload())
        .arg("short-lived-children")
        .output()
        .expect("launch probe over W4");
    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let bundle = only_bundle(output.path());
    let processes = lines(&bundle.join("processes.ndjson"));
    assert!(
        processes.len() >= 16,
        "W4 must expose many concurrent short-lived children"
    );
    let events = lines(&bundle.join("events.ndjson"));
    assert!(
        events.iter().any(|event| {
            event["record_type"] == "collector_degradation"
                && event["handle_retention_degraded"] == true
        }),
        "cap overflow must be explicit degradation, not hidden handle growth"
    );
    let releases = events
        .iter()
        .filter(|event| event["record_type"] == "handle_released")
        .count();
    assert!(
        releases >= 2,
        "retained short-lived handles must be finalized and released"
    );
    let manifest: Value =
        serde_json::from_slice(&fs::read(bundle.join("manifest.json")).expect("manifest"))
            .expect("manifest JSON");
    let summary: Value =
        serde_json::from_slice(&fs::read(bundle.join("summary.json")).expect("summary"))
            .expect("summary JSON");
    assert_eq!(manifest["measurement_validity"], "DEGRADED");
    assert_eq!(summary["measurement_validity"], "DEGRADED");
}
