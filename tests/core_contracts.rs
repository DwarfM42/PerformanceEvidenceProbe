use perf_evidence_probe::contract::{HandleRetention, JobSafetyPolicy, absolute_deadline_ns};

#[test]
fn handle_retention_is_bounded_and_records_degradation_without_synthesizing_terminal_data() {
    let mut retention = HandleRetention::new(2);

    assert!(retention.try_retain(10));
    assert!(retention.try_retain(11));
    assert!(!retention.try_retain(12));
    assert!(retention.degraded());
    assert_eq!(retention.retained_count(), 2);
    assert_eq!(retention.maximum_retained_count(), 2);

    retention.release(10);
    assert!(retention.try_retain(12));
    assert_eq!(retention.retained_count(), 2);
}

#[test]
fn job_safety_never_enables_kill_on_close() {
    let policy = JobSafetyPolicy::probe_default();

    assert_eq!(policy.limit_flags(), 0);
    assert!(!policy.kill_on_job_close_enabled());
}

#[test]
fn sampling_deadlines_are_absolute_not_relative_to_previous_completion() {
    assert_eq!(absolute_deadline_ns(1_000, 500, 0), 1_000);
    assert_eq!(absolute_deadline_ns(1_000, 500, 1), 1_500);
    assert_eq!(absolute_deadline_ns(1_000, 500, 9), 5_500);
}
