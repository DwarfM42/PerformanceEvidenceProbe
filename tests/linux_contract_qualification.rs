use std::fs;

use perf_evidence_probe::summary::regenerate_summary;
use serde_json::{Value, json};

fn process() -> Value {
    json!({
        "process_local_id": 1,
        "pid": 42,
        "process_start_time": 100,
        "boot_identity": "boot-a",
        "discovery_source": "test",
        "handle_acquisition_result": "retained"
    })
}

fn sample(ordinal: u64, root_live: bool) -> Value {
    json!({
        "schema_draft_version": "perf-evidence-v2-draft",
        "record_type": "sample",
        "wall_time_utc": format!("2026-09-06T00:00:0{ordinal}Z"),
        "monotonic_ns": ordinal * 500_000_000,
        "scheduled_monotonic_ns": ordinal * 500_000_000,
        "sampling_delay_ns": 0,
        "gap_from_previous_sample_ns": if ordinal == 0 { Value::Null } else { json!(500_000_000_u64) },
        "root_process_confirmed_live": root_live,
        "process_set_working_set_sum_bytes": 10,
        "process_set_private_bytes_sum": 8,
        "processes": [{
            "process_local_id": 1,
            "working_set_bytes": 10,
            "private_bytes": 8,
            "user_cpu_time_ns": 3,
            "kernel_cpu_time_ns": 2,
            "read_bytes": 5,
            "write_bytes": 7,
            "other_bytes": 0,
            "read_operations": 1,
            "write_operations": 2,
            "other_operations": 0,
            "thread_count": 1,
            "handle_count": 0
        }],
        "probe": {
            "working_set_bytes": 1,
            "private_bytes": 1,
            "user_cpu_time_ns": 1,
            "kernel_cpu_time_ns": 1,
            "read_bytes": 0,
            "write_bytes": 0,
            "thread_count": 1,
            "handle_count": 0
        },
        "system": {
            "system_user_cpu_time_ns": 1,
            "system_kernel_cpu_time_ns": 1,
            "system_idle_cpu_time_ns": 1,
            "available_physical_memory_bytes": 1,
            "commit_current_bytes": 1,
            "commit_limit_bytes": 1,
            "disk_free_bytes": 1
        }
    })
}

fn unavailable(metric: &str, subject_kind: &str, reason: &str, ordinal: Option<u64>) -> Value {
    let mut event = json!({
        "record_type": "metric_unavailable",
        "metric": metric,
        "subject_kind": subject_kind,
        "reason": reason,
    });
    if matches!(subject_kind, "PROCESS" | "PROCESS_SAMPLE") {
        event["process_local_id"] = json!(1);
    }
    if let Some(ordinal) = ordinal {
        event["sample_ordinal"] = json!(ordinal);
    }
    event
}

fn bundle(samples: Vec<Value>, events: Vec<Value>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("processes.ndjson"),
        format!("{}\n", process()),
    )
    .unwrap();
    fs::write(
        dir.path().join("samples.ndjson"),
        samples
            .into_iter()
            .map(|record| format!("{record}\n"))
            .collect::<String>(),
    )
    .unwrap();
    if !events.is_empty() {
        fs::write(
            dir.path().join("events.ndjson"),
            events
                .into_iter()
                .map(|record| format!("{record}\n"))
                .collect::<String>(),
        )
        .unwrap();
    }
    dir
}

fn reconstruct(samples: Vec<Value>, events: Vec<Value>) -> Value {
    let dir = bundle(samples, events);
    regenerate_summary(dir.path()).unwrap();
    serde_json::from_slice(&fs::read(dir.path().join("summary.json")).unwrap()).unwrap()
}

fn process_row(sample: &mut Value) -> &mut serde_json::Map<String, Value> {
    sample["processes"][0].as_object_mut().unwrap()
}

#[test]
fn fully_observed_fixture_keeps_zero_numeric() {
    let mut observed = sample(0, true);
    let row = process_row(&mut observed);
    row.insert("user_cpu_time_ns".into(), json!(0));
    row.insert("kernel_cpu_time_ns".into(), json!(0));
    row.insert("thread_count".into(), json!(0));
    observed["probe"]["user_cpu_time_ns"] = json!(0);
    observed["probe"]["kernel_cpu_time_ns"] = json!(0);

    let summary = reconstruct(vec![observed], vec![]);
    assert_eq!(summary["measurement_validity"], "VALID");
    assert_eq!(summary["measurement_completeness"], "COMPLETE");
}

#[test]
fn one_proc_stat_authority_loss_explains_cpu_pair_and_thread_at_one_exact_sample() {
    let mut degraded = sample(0, true);
    for field in ["user_cpu_time_ns", "kernel_cpu_time_ns", "thread_count"] {
        process_row(&mut degraded).remove(field);
    }
    let summary = reconstruct(
        vec![degraded],
        vec![
            unavailable(
                "process.user_cpu_time_ns",
                "PROCESS_SAMPLE",
                "authority_unavailable",
                Some(0),
            ),
            unavailable(
                "process.kernel_cpu_time_ns",
                "PROCESS_SAMPLE",
                "authority_unavailable",
                Some(0),
            ),
            unavailable(
                "process.thread_count",
                "PROCESS_SAMPLE",
                "authority_unavailable",
                Some(0),
            ),
        ],
    );
    assert_eq!(summary["measurement_validity"], "DEGRADED");
    assert_eq!(summary["measurement_completeness"], "DECLARED_PARTIAL");
}

#[test]
fn semantic_and_operational_partial_are_independently_machine_explainable() {
    let mut mixed = sample(0, true);
    process_row(&mut mixed).remove("private_bytes");
    mixed
        .as_object_mut()
        .unwrap()
        .remove("process_set_private_bytes_sum");
    process_row(&mut mixed).remove("user_cpu_time_ns");
    let summary = reconstruct(
        vec![mixed],
        vec![
            unavailable("process.private_bytes", "RUN", "semantic_mismatch", None),
            unavailable(
                "process.user_cpu_time_ns",
                "PROCESS_SAMPLE",
                "sampling_degraded",
                Some(0),
            ),
        ],
    );
    assert_eq!(summary["measurement_validity"], "DEGRADED");
    assert_eq!(summary["measurement_completeness"], "DECLARED_PARTIAL");
}

#[test]
fn complete_system_source_loss_is_exactly_sample_bound() {
    let mut degraded = sample(0, true);
    for field in [
        "system_user_cpu_time_ns",
        "system_kernel_cpu_time_ns",
        "system_idle_cpu_time_ns",
        "available_physical_memory_bytes",
        "commit_current_bytes",
        "commit_limit_bytes",
        "disk_free_bytes",
    ] {
        degraded["system"].as_object_mut().unwrap().remove(field);
    }
    let events = [
        "system.system_user_cpu_time_ns",
        "system.system_kernel_cpu_time_ns",
        "system.system_idle_cpu_time_ns",
        "system.available_physical_memory_bytes",
        "system.commit_current_bytes",
        "system.commit_limit_bytes",
        "system.disk_free_bytes",
    ]
    .into_iter()
    .map(|metric| unavailable(metric, "SAMPLE", "authority_unavailable", Some(0)))
    .collect();
    let summary = reconstruct(vec![degraded], events);
    assert_eq!(summary["measurement_validity"], "DEGRADED");
    assert_eq!(summary["measurement_completeness"], "DECLARED_PARTIAL");
}

#[test]
fn probe_only_cpu_failure_does_not_erase_target_observation() {
    let mut degraded = sample(0, true);
    degraded["probe"]
        .as_object_mut()
        .unwrap()
        .remove("user_cpu_time_ns");
    degraded["probe"]
        .as_object_mut()
        .unwrap()
        .remove("kernel_cpu_time_ns");
    let summary = reconstruct(
        vec![degraded],
        vec![
            unavailable(
                "probe.user_cpu_time_ns",
                "SAMPLE",
                "sampling_degraded",
                Some(0),
            ),
            unavailable(
                "probe.kernel_cpu_time_ns",
                "SAMPLE",
                "sampling_degraded",
                Some(0),
            ),
        ],
    );
    assert_eq!(summary["measurement_validity"], "DEGRADED");
    assert_eq!(summary["measurement_completeness"], "DECLARED_PARTIAL");
}

#[test]
fn wrong_cpu_binding_and_run_operational_ambiguity_fail_closed() {
    let mut missing = sample(0, true);
    process_row(&mut missing).remove("user_cpu_time_ns");
    let wrong = unavailable(
        "process.user_cpu_time_ns",
        "PROCESS_SAMPLE",
        "sampling_degraded",
        Some(1),
    );
    assert!(regenerate_summary(bundle(vec![missing.clone()], vec![wrong]).path()).is_err());

    let run = unavailable("process.user_cpu_time_ns", "RUN", "semantic_mismatch", None);
    let operational = unavailable(
        "process.user_cpu_time_ns",
        "PROCESS_SAMPLE",
        "sampling_degraded",
        Some(0),
    );
    assert!(regenerate_summary(bundle(vec![missing], vec![run, operational]).path()).is_err());
}

#[test]
fn missing_memory_contributor_suppresses_derived_witness_instead_of_using_a_subset() {
    let mut degraded = sample(0, true);
    process_row(&mut degraded).remove("private_bytes");
    degraded
        .as_object_mut()
        .unwrap()
        .remove("process_set_private_bytes_sum");
    let summary = reconstruct(
        vec![degraded],
        vec![unavailable(
            "process.private_bytes",
            "PROCESS_SAMPLE",
            "sampling_degraded",
            Some(0),
        )],
    );
    assert!(summary.get("peak_private_sampled_bytes").is_none());
    assert!(summary.get("last_live_private_sample_bytes").is_none());
}

#[test]
fn target_disappearance_can_finalize_with_no_fabricated_last_live_process_sample() {
    let first = sample(0, true);
    let mut terminal = sample(1, false);
    terminal["processes"] = json!([]);
    terminal["process_set_working_set_sum_bytes"] = json!(0);
    terminal["process_set_private_bytes_sum"] = json!(0);

    let summary = reconstruct(vec![first, terminal], vec![]);
    assert_eq!(summary["last_live_working_set_sample_bytes"], 10);
    assert_eq!(summary["last_live_private_sample_bytes"], 8);
    assert!(summary.get("total_cpu_time_ns").is_none());
}

#[test]
fn thread_and_terminal_counter_width_overflow_are_invalid_not_silent_absence() {
    let mut oversized_thread = sample(0, true);
    process_row(&mut oversized_thread).insert("thread_count".into(), json!(u64::MAX));
    assert!(regenerate_summary(bundle(vec![oversized_thread], vec![]).path()).is_err());

    let terminal = json!({
        "record_type": "process_exit_observed",
        "process_local_id": 1,
        "exit_code": u64::MAX,
        "terminal_user_cpu_time_ns": "not-a-counter"
    });
    assert!(regenerate_summary(bundle(vec![sample(0, true)], vec![terminal]).path()).is_err());
}
