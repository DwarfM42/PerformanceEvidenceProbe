use std::fs;

use perf_evidence_probe::{
    evidence::{ProcessSample, SystemSample},
    summary::regenerate_summary,
};
use serde_json::{Value, json};

fn sample() -> Value {
    json!({
        "schema_draft_version":"perf-evidence-v2-draft", "record_type":"sample",
        "wall_time_utc":"2026-09-05T00:00:00Z", "monotonic_ns":0,
        "scheduled_monotonic_ns":0, "sampling_delay_ns":0, "root_process_confirmed_live":true,
        "process_set_working_set_sum_bytes":10, "process_set_private_bytes_sum":8,
        "processes":[{"process_local_id":1,"working_set_bytes":10,"private_bytes":8,"user_cpu_time_ns":0,"kernel_cpu_time_ns":0,"read_bytes":0,"write_bytes":0,"other_bytes":0,"read_operations":0,"write_operations":0,"other_operations":0,"thread_count":1,"handle_count":0}],
        "probe":{"working_set_bytes":0,"private_bytes":0,"user_cpu_time_ns":0,"kernel_cpu_time_ns":0,"read_bytes":0,"write_bytes":0,"thread_count":1,"handle_count":0},
        "system":{"system_user_cpu_time_ns":0,"system_kernel_cpu_time_ns":0,"system_idle_cpu_time_ns":0,"available_physical_memory_bytes":0,"commit_current_bytes":0,"commit_limit_bytes":0,"disk_free_bytes":0}
    })
}
fn process() -> Value {
    json!({"process_local_id":1,"pid":42,"process_start_time":100,"boot_identity":"boot-a","discovery_source":"test","handle_acquisition_result":"retained"})
}
fn event(metric: &str, subject: &str, reason: &str) -> Value {
    json!({"record_type":"metric_unavailable","metric":metric,"subject_kind":subject,"reason":reason,"process_local_id":1,"sample_ordinal":0})
}
fn bundle(sample: Value, processes: Vec<Value>, events: Vec<Value>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("samples.ndjson"), format!("{}\n", sample)).unwrap();
    if !processes.is_empty() {
        fs::write(
            dir.path().join("processes.ndjson"),
            processes
                .into_iter()
                .map(|v| format!("{}\n", v))
                .collect::<String>(),
        )
        .unwrap();
    }
    if !events.is_empty() {
        fs::write(
            dir.path().join("events.ndjson"),
            events
                .into_iter()
                .map(|v| format!("{}\n", v))
                .collect::<String>(),
        )
        .unwrap();
    }
    dir
}
fn summary(dir: &tempfile::TempDir) -> Value {
    regenerate_summary(dir.path()).unwrap();
    serde_json::from_slice(&fs::read(dir.path().join("summary.json")).unwrap()).unwrap()
}

#[test]
fn present_zero_is_numeric_and_v2_raw_only_evidence_is_valid_complete() {
    let dir = bundle(sample(), vec![process()], vec![]);
    let result = summary(&dir);
    assert_eq!(result["measurement_validity"], "VALID");
    assert_eq!(result["measurement_completeness"], "COMPLETE");
    assert_eq!(result["peak_working_set_sampled_bytes"], 10);
    assert!(
        result.get("total_cpu_time_ns").is_none(),
        "a live final process set is not a run total"
    );
}

#[test]
fn typed_process_sample_omission_is_degraded_partial_and_transitively_hides_private_witness() {
    let mut record = sample();
    record["processes"][0]
        .as_object_mut()
        .unwrap()
        .remove("private_bytes");
    record
        .as_object_mut()
        .unwrap()
        .remove("process_set_private_bytes_sum");
    let result = summary(&bundle(
        record,
        vec![process()],
        vec![event(
            "process.private_bytes",
            "PROCESS_SAMPLE",
            "sampling_degraded",
        )],
    ));
    assert_eq!(result["measurement_validity"], "DEGRADED");
    assert_eq!(result["measurement_completeness"], "DECLARED_PARTIAL");
    assert!(result.get("peak_private_sampled_bytes").is_none());
}

#[test]
fn semantic_run_omission_is_valid_declared_partial() {
    let mut record = sample();
    record["system"]
        .as_object_mut()
        .unwrap()
        .remove("disk_free_bytes");
    let result = summary(&bundle(
        record,
        vec![process()],
        vec![
            json!({"record_type":"metric_unavailable","metric":"system.disk_free_bytes","subject_kind":"RUN","reason":"semantic_mismatch"}),
        ],
    ));
    assert_eq!(result["measurement_validity"], "VALID");
    assert_eq!(result["measurement_completeness"], "DECLARED_PARTIAL");
}

#[test]
fn malformed_present_null_or_unexplained_omission_fails_closed() {
    let mut null = sample();
    null["probe"]["handle_count"] = Value::Null;
    assert!(regenerate_summary(bundle(null, vec![process()], vec![]).path()).is_err());
    let mut omitted = sample();
    omitted["probe"]
        .as_object_mut()
        .unwrap()
        .remove("handle_count");
    assert!(regenerate_summary(bundle(omitted, vec![process()], vec![]).path()).is_err());
}

#[test]
fn rejects_wrong_domain_duplicate_and_present_metric_events() {
    let wrong = event("process.private_bytes", "SAMPLE", "sampling_degraded");
    assert!(regenerate_summary(bundle(sample(), vec![process()], vec![wrong]).path()).is_err());
    let present = event(
        "process.private_bytes",
        "PROCESS_SAMPLE",
        "sampling_degraded",
    );
    assert!(regenerate_summary(bundle(sample(), vec![process()], vec![present]).path()).is_err());
    let mut omitted = sample();
    omitted["processes"][0]
        .as_object_mut()
        .unwrap()
        .remove("private_bytes");
    omitted
        .as_object_mut()
        .unwrap()
        .remove("process_set_private_bytes_sum");
    let one = event(
        "process.private_bytes",
        "PROCESS_SAMPLE",
        "sampling_degraded",
    );
    assert!(
        regenerate_summary(bundle(omitted, vec![process()], vec![one.clone(), one]).path())
            .is_err()
    );
}

#[test]
fn process_scoped_event_requires_strong_unique_persisted_identity() {
    let mut record = sample();
    record["processes"][0]
        .as_object_mut()
        .unwrap()
        .remove("private_bytes");
    record
        .as_object_mut()
        .unwrap()
        .remove("process_set_private_bytes_sum");
    let e = event(
        "process.private_bytes",
        "PROCESS_SAMPLE",
        "sampling_degraded",
    );
    assert!(regenerate_summary(bundle(record.clone(), vec![], vec![e.clone()]).path()).is_err());
    let mut sentinel = process();
    sentinel["process_start_time"] = json!(0);
    assert!(
        regenerate_summary(bundle(record.clone(), vec![sentinel], vec![e.clone()]).path()).is_err()
    );
    assert!(
        regenerate_summary(bundle(record, vec![process(), process()], vec![e]).path()).is_err()
    );
}

#[test]
fn witnesses_are_exact_checked_and_not_availability_targets() {
    let mut wrong = sample();
    wrong["process_set_private_bytes_sum"] = json!(9);
    assert!(regenerate_summary(bundle(wrong, vec![process()], vec![]).path()).is_err());
    let mut overflow = sample();
    overflow["processes"] = json!([
        {"process_local_id":1,"working_set_bytes":18446744073709551615_u64,"private_bytes":1,"user_cpu_time_ns":0,"kernel_cpu_time_ns":0,"read_bytes":0,"write_bytes":0,"other_bytes":0,"read_operations":0,"write_operations":0,"other_operations":0,"thread_count":1,"handle_count":0},
        {"process_local_id":2,"working_set_bytes":1,"private_bytes":1,"user_cpu_time_ns":0,"kernel_cpu_time_ns":0,"read_bytes":0,"write_bytes":0,"other_bytes":0,"read_operations":0,"write_operations":0,"other_operations":0,"thread_count":1,"handle_count":0}
    ]);
    assert!(regenerate_summary(bundle(overflow, vec![process()], vec![]).path()).is_err());
    let derived = json!({"record_type":"metric_unavailable","metric":"process_set_private_bytes_sum","subject_kind":"RUN","reason":"unsupported"});
    assert!(regenerate_summary(bundle(sample(), vec![process()], vec![derived]).path()).is_err());
}

#[test]
fn manifest_disagreement_rejects_completed_bundle_and_historic_numeric_remains_accepted() {
    let dir = bundle(sample(), vec![process()], vec![]);
    fs::write(dir.path().join("manifest.json"), json!({"schema_draft_version":"perf-evidence-v2-draft","measurement_validity":"DEGRADED","measurement_completeness":"COMPLETE"}).to_string()).unwrap();
    assert!(regenerate_summary(dir.path()).is_err());
    let legacy = json!({"record_type":"sample","wall_time_utc":"2026-01-01T00:00:00Z","monotonic_ns":0,"scheduled_monotonic_ns":0,"sampling_delay_ns":0,"root_process_confirmed_live":false,"processes":[],"probe":{"handle_count":0}});
    let legacy_summary = summary(&bundle(legacy, vec![], vec![]));
    assert_eq!(
        legacy_summary["summary_schema_draft_version"],
        "perf-evidence-v1-draft"
    );
}

#[test]
fn omitted_system_metrics_do_not_serialize_as_zero() {
    let value = serde_json::to_value(SystemSample {
        system_user_cpu_time_ns: Some(0),
        system_kernel_cpu_time_ns: None,
        system_idle_cpu_time_ns: None,
        available_physical_memory_bytes: None,
        commit_current_bytes: None,
        commit_limit_bytes: None,
        disk_free_bytes: None,
    })
    .unwrap();
    assert_eq!(value["system_user_cpu_time_ns"], 0);
    assert!(value.get("commit_current_bytes").is_none());
}
#[test]
fn present_windows_shaped_metrics_serialize_as_numbers() {
    let value = serde_json::to_value(ProcessSample {
        process_local_id: 1,
        working_set_bytes: 0,
        private_bytes: Some(0),
        user_cpu_time_ns: 0,
        kernel_cpu_time_ns: 0,
        read_bytes: 0,
        write_bytes: 0,
        other_bytes: None,
        read_operations: Some(0),
        write_operations: Some(0),
        other_operations: None,
        thread_count: Some(1),
        handle_count: Some(0),
    })
    .unwrap();
    for key in [
        "private_bytes",
        "read_operations",
        "write_operations",
        "handle_count",
    ] {
        assert!(value[key].is_number(), "{key} must remain numeric");
    }
}
