use std::fs;

use perf_evidence_probe::summary::regenerate_summary;

#[test]
fn reconstruction_is_byte_deterministic_and_uses_raw_cumulative_counters() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let root = directory.path();
    fs::write(
        root.join("processes.ndjson"),
        concat!(
            "{\"process_local_id\":1,\"pid\":42,\"process_start_time\":100,\"boot_identity\":\"boot-a\",\"discovery_source\":\"launch_root\",\"handle_acquisition_result\":\"retained\"}\n",
            "{\"process_local_id\":2,\"pid\":43,\"process_start_time\":101,\"boot_identity\":\"boot-a\",\"parent_local_id\":1,\"discovery_source\":\"job_completion_port\",\"handle_acquisition_result\":\"retained\"}\n"
        ),
    ).expect("registry fixture");
    fs::write(
        root.join("samples.ndjson"),
        concat!(
            "{\"record_type\":\"sample\",\"wall_time_utc\":\"2026-08-25T00:00:00Z\",\"monotonic_ns\":1000000000,\"scheduled_monotonic_ns\":1000000000,\"sampling_delay_ns\":0,\"gap_from_previous_sample_ns\":null,\"processes\":[{\"process_local_id\":1,\"working_set_bytes\":100,\"private_bytes\":80,\"user_cpu_time_ns\":10,\"kernel_cpu_time_ns\":5,\"read_bytes\":1,\"write_bytes\":2,\"read_operations\":1,\"write_operations\":1,\"thread_count\":1,\"handle_count\":2}],\"probe\":{\"working_set_bytes\":10,\"private_bytes\":9,\"user_cpu_time_ns\":1,\"kernel_cpu_time_ns\":1,\"read_bytes\":0,\"write_bytes\":0,\"thread_count\":1,\"handle_count\":3}}\n",
            "{\"record_type\":\"sample\",\"wall_time_utc\":\"2026-08-25T00:00:00.500Z\",\"monotonic_ns\":1500000000,\"scheduled_monotonic_ns\":1500000000,\"sampling_delay_ns\":0,\"gap_from_previous_sample_ns\":500000000,\"processes\":[{\"process_local_id\":1,\"working_set_bytes\":120,\"private_bytes\":100,\"user_cpu_time_ns\":30,\"kernel_cpu_time_ns\":10,\"read_bytes\":5,\"write_bytes\":8,\"read_operations\":2,\"write_operations\":3,\"thread_count\":1,\"handle_count\":2},{\"process_local_id\":2,\"working_set_bytes\":40,\"private_bytes\":30,\"user_cpu_time_ns\":20,\"kernel_cpu_time_ns\":5,\"read_bytes\":4,\"write_bytes\":6,\"read_operations\":1,\"write_operations\":2,\"thread_count\":1,\"handle_count\":2}],\"job\":{\"total_user_time_ns\":50,\"total_kernel_time_ns\":15,\"read_operation_count\":3,\"write_operation_count\":5,\"other_operation_count\":0,\"read_transfer_bytes\":9,\"write_transfer_bytes\":14,\"other_transfer_bytes\":0,\"total_processes_os\":2,\"active_processes_os\":1,\"total_terminated_by_limit_os\":0},\"probe\":{\"working_set_bytes\":12,\"private_bytes\":10,\"user_cpu_time_ns\":2,\"kernel_cpu_time_ns\":1,\"read_bytes\":0,\"write_bytes\":0,\"thread_count\":1,\"handle_count\":4}}\n"
        ),
    ).expect("sample fixture");
    fs::write(
        root.join("events.ndjson"),
        "{\"record_type\":\"process_exit_observed\",\"process_local_id\":1,\"exit_code\":0,\"terminal_user_cpu_time_ns\":30,\"terminal_kernel_cpu_time_ns\":10,\"terminal_read_bytes\":5,\"terminal_write_bytes\":8,\"terminal_counter_fidelity\":\"COMPLETE\"}\n",
    ).expect("event fixture");

    regenerate_summary(root).expect("first summary reconstruction");
    let first = fs::read(root.join("summary.json")).expect("first summary");
    regenerate_summary(root).expect("second summary reconstruction");
    let second = fs::read(root.join("summary.json")).expect("second summary");

    assert_eq!(
        first, second,
        "summary must contain no regeneration-time data"
    );
    let summary: serde_json::Value = serde_json::from_slice(&second).expect("summary JSON");
    assert_eq!(summary["sample_count"], 2);
    assert_eq!(summary["max_sample_gap_exact_ns"], 500_000_000_u64);
    assert_eq!(summary["peak_working_set_sampled_bytes"], 160);
    assert_eq!(summary["total_cpu_time_ns"], 65);
    assert_eq!(summary["total_read_bytes"], 9);
    assert_eq!(summary["job_processes_without_observed_identity"], 0);
    assert_eq!(summary["maximum_probe_handle_count"], 4);
}

#[test]
fn last_live_memory_excludes_a_sample_not_confirmed_live_for_the_root() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let root = directory.path();
    fs::write(
        root.join("samples.ndjson"),
        concat!(
            "{\"record_type\":\"sample\",\"wall_time_utc\":\"2026-08-25T00:00:00Z\",\"monotonic_ns\":0,\"scheduled_monotonic_ns\":0,\"sampling_delay_ns\":0,\"root_process_confirmed_live\":true,\"processes\":[{\"process_local_id\":1,\"working_set_bytes\":100,\"private_bytes\":80,\"user_cpu_time_ns\":0,\"kernel_cpu_time_ns\":0,\"read_bytes\":0,\"write_bytes\":0,\"read_operations\":0,\"write_operations\":0,\"handle_count\":1}],\"probe\":{\"working_set_bytes\":1,\"private_bytes\":1,\"user_cpu_time_ns\":0,\"kernel_cpu_time_ns\":0,\"read_bytes\":0,\"write_bytes\":0,\"handle_count\":1}}\n",
            "{\"record_type\":\"sample\",\"wall_time_utc\":\"2026-08-25T00:00:00.500Z\",\"monotonic_ns\":500000000,\"scheduled_monotonic_ns\":500000000,\"sampling_delay_ns\":0,\"root_process_confirmed_live\":false,\"processes\":[{\"process_local_id\":1,\"working_set_bytes\":10,\"private_bytes\":8,\"user_cpu_time_ns\":1,\"kernel_cpu_time_ns\":0,\"read_bytes\":0,\"write_bytes\":0,\"read_operations\":0,\"write_operations\":0,\"handle_count\":1}],\"probe\":{\"working_set_bytes\":1,\"private_bytes\":1,\"user_cpu_time_ns\":0,\"kernel_cpu_time_ns\":0,\"read_bytes\":0,\"write_bytes\":0,\"handle_count\":1}}\n"
        ),
    )
    .expect("sample fixture");

    regenerate_summary(root).expect("summary reconstruction");
    let summary: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("summary.json")).expect("summary"))
            .expect("summary JSON");
    assert_eq!(summary["last_live_working_set_sample_bytes"], 100);
    assert_eq!(summary["last_live_private_sample_bytes"], 80);
    assert_eq!(
        summary["last_live_working_set_sample_time"],
        "2026-08-25T00:00:00Z"
    );
}
