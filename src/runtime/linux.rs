//! Bounded Linux attach evidence using a composite `/proc` process identity.

use anyhow::{Result, bail};
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};
use std::{
    io::{self, Read},
    path::Path,
};

use anyhow::{Context, anyhow};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    evidence::{
        EvidenceEvent, EvidenceWriter, Metric, ProbeSample, ProcessRecord, ProcessSample,
        SampleRecord, SubjectKind, SystemSample, UnavailableReason,
        write_completed_bundle_manifest,
    },
    summary::regenerate_summary,
};

const MAX_PROC_BYTES: u64 = 8 * 1024;
const PROCESS_LOCAL_ID: u64 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessIdentity {
    pub boot_id: String,
    pub pid: u32,
    pub starttime: u64,
}

impl ProcessIdentity {
    pub fn new(pid: u32, boot_id: &str, starttime: u64) -> io::Result<Self> {
        if pid == 0 || starttime == 0 {
            return Err(invalid("process identity contains a sentinel value"));
        }
        Ok(Self {
            boot_id: parse_boot_id(boot_id)?,
            pid,
            starttime,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityComparison {
    SameInstance,
    DifferentInstance,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StatSample {
    identity: ProcessIdentity,
    user_ticks: u64,
    kernel_ticks: u64,
    thread_count: u32,
}

pub fn parse_boot_id(input: &str) -> io::Result<String> {
    let boot_id = input.strip_suffix('\n').unwrap_or(input);
    if boot_id.len() != 36
        || [8, 13, 18, 23]
            .iter()
            .any(|&index| boot_id.as_bytes()[index] != b'-')
        || boot_id
            .bytes()
            .enumerate()
            .any(|(index, byte)| ![8, 13, 18, 23].contains(&index) && !byte.is_ascii_hexdigit())
    {
        return Err(invalid("boot ID is missing or malformed"));
    }
    Ok(boot_id.to_owned())
}

/// Parses the Linux `/proc/<pid>/stat` identity fields. `comm` may contain
/// spaces or parentheses, so the final structural `)` is the only delimiter.
pub fn parse_stat(expected_pid: u32, record: &str) -> io::Result<u64> {
    Ok(
        parse_stat_sample(expected_pid, "00000000-0000-0000-0000-000000000000", record)?
            .identity
            .starttime,
    )
}

fn parse_stat_sample(expected_pid: u32, boot_id: &str, record: &str) -> io::Result<StatSample> {
    if expected_pid == 0 {
        return Err(invalid("expected PID must be nonzero"));
    }
    let open = record
        .find('(')
        .ok_or_else(|| invalid("stat record is missing comm opener"))?;
    let close = record
        .rfind(')')
        .filter(|&close| close > open)
        .ok_or_else(|| invalid("stat record is missing structural comm closer"))?;
    let pid = record[..open]
        .trim()
        .parse::<u32>()
        .map_err(|_| invalid("stat PID is malformed"))?;
    if pid == 0 || pid != expected_pid {
        return Err(invalid("stat PID does not match expected PID"));
    }
    let fields = record[close + 1..].split_whitespace().collect::<Vec<_>>();
    let numeric = |index: usize, name: &str| {
        fields
            .get(index)
            .ok_or_else(|| invalid("stat record is incomplete"))?
            .parse::<u64>()
            .map_err(|_| invalid(name))
    };
    let user_ticks = numeric(11, "stat utime is malformed")?;
    let kernel_ticks = numeric(12, "stat stime is malformed")?;
    let thread_count = numeric(17, "stat num_threads is malformed")?
        .try_into()
        .map_err(|_| invalid("stat num_threads exceeds u32"))?;
    let starttime = numeric(19, "stat starttime is malformed")?;
    Ok(StatSample {
        identity: ProcessIdentity::new(pid, boot_id, starttime)?,
        user_ticks,
        kernel_ticks,
        thread_count,
    })
}

pub fn observe_with<F>(pid: u32, read: F) -> io::Result<ProcessIdentity>
where
    F: FnOnce(u32) -> io::Result<(String, String)>,
{
    let (boot_id, stat) = read(pid)?;
    ProcessIdentity::new(pid, &boot_id, parse_stat(pid, &stat)?)
}

pub fn read_identity(pid: u32) -> io::Result<ProcessIdentity> {
    let boot_id = read_limited("/proc/sys/kernel/random/boot_id")?;
    let stat = read_limited(&format!("/proc/{pid}/stat"))?;
    Ok(parse_stat_sample(pid, &boot_id, &stat)?.identity)
}

pub fn compare_identity(
    previous: &ProcessIdentity,
    current: io::Result<ProcessIdentity>,
) -> IdentityComparison {
    match current {
        Ok(current) if *previous == current => IdentityComparison::SameInstance,
        Ok(_) => IdentityComparison::DifferentInstance,
        Err(_) => IdentityComparison::Unavailable,
    }
}

pub fn attach(output_root: &Path, pid: u32, attach_job: bool) -> Result<()> {
    attach_with_observation(
        output_root,
        pid,
        attach_job,
        read_stat_for_identity,
        read_identity,
    )
}

fn attach_with_observation<F, R>(
    output_root: &Path,
    pid: u32,
    attach_job: bool,
    mut read_stat: F,
    mut revalidate: R,
) -> Result<()>
where
    F: FnMut(&ProcessIdentity) -> Result<StatSample>,
    R: FnMut(u32) -> io::Result<ProcessIdentity>,
{
    if attach_job {
        bail!("--attach-job has no Linux analogue");
    }
    let initial = read_identity(pid).context("establish Linux attach identity")?;
    let bundle = output_root.join(unique_bundle_name("attach"));
    let writer = EvidenceWriter::start(&bundle, 16)?;
    writer.process(ProcessRecord {
        process_local_id: PROCESS_LOCAL_ID,
        pid,
        process_start_time: initial.starttime,
        boot_identity: initial.boot_id.clone(),
        parent_local_id: None,
        discovery_source: "linux_attach_root".into(),
        handle_acquisition_result: "proc_identity_authoritative".into(),
    })?;

    let target = read_stat(&initial);
    let probe_identity = read_identity(std::process::id()).context("establish probe identity")?;
    let probe = read_stat(&probe_identity);
    if revalidate(pid).context("revalidate Linux attach identity")? != initial {
        bail!("attached process identity changed or disappeared during observation");
    }

    for metric in run_semantic_mismatches() {
        writer.event(EvidenceEvent::metric_unavailable(
            metric,
            SubjectKind::Run,
            UnavailableReason::SemanticMismatch,
        ))?;
    }
    if let Err(error) = &target {
        for metric in [
            Metric::ProcessUserCpuTimeNs,
            Metric::ProcessKernelCpuTimeNs,
            Metric::ProcessThreadCount,
        ] {
            writer.event(
                EvidenceEvent::metric_unavailable(
                    metric,
                    SubjectKind::ProcessSample,
                    operational_unavailable_reason(error),
                )
                .with_u64("process_local_id", PROCESS_LOCAL_ID)
                .with_u64("sample_ordinal", 0),
            )?;
        }
    }
    if let Err(error) = &probe {
        for metric in [Metric::ProbeUserCpuTimeNs, Metric::ProbeKernelCpuTimeNs] {
            writer.event(
                EvidenceEvent::metric_unavailable(
                    metric,
                    SubjectKind::Sample,
                    operational_unavailable_reason(error),
                )
                .with_u64("sample_ordinal", 0),
            )?;
        }
    }
    writer.sample(SampleRecord {
        schema_draft_version: "perf-evidence-v2-draft",
        record_type: "sample",
        wall_time_utc: utc_now()?,
        monotonic_ns: 0,
        scheduled_monotonic_ns: 0,
        sampling_delay_ns: 0,
        gap_from_previous_sample_ns: None,
        root_process_confirmed_live: true,
        process_set_working_set_sum_bytes: None,
        process_set_private_bytes_sum: None,
        processes: vec![ProcessSample {
            process_local_id: PROCESS_LOCAL_ID,
            working_set_bytes: None,
            private_bytes: None,
            user_cpu_time_ns: target
                .as_ref()
                .ok()
                .map(|value| ticks_to_ns(value.user_ticks))
                .transpose()?,
            kernel_cpu_time_ns: target
                .as_ref()
                .ok()
                .map(|value| ticks_to_ns(value.kernel_ticks))
                .transpose()?,
            read_bytes: None,
            write_bytes: None,
            other_bytes: None,
            read_operations: None,
            write_operations: None,
            other_operations: None,
            thread_count: target.as_ref().ok().map(|value| value.thread_count),
            handle_count: None,
        }],
        job: None,
        system: SystemSample {
            system_user_cpu_time_ns: None,
            system_kernel_cpu_time_ns: None,
            system_idle_cpu_time_ns: None,
            available_physical_memory_bytes: None,
            commit_current_bytes: None,
            commit_limit_bytes: None,
            disk_free_bytes: None,
        },
        probe: ProbeSample {
            working_set_bytes: None,
            private_bytes: None,
            user_cpu_time_ns: probe
                .as_ref()
                .ok()
                .map(|value| ticks_to_ns(value.user_ticks))
                .transpose()?,
            kernel_cpu_time_ns: probe
                .as_ref()
                .ok()
                .map(|value| ticks_to_ns(value.kernel_ticks))
                .transpose()?,
            read_bytes: None,
            write_bytes: None,
            thread_count: None,
            handle_count: None,
        },
    })?;
    writer.finish()?;
    regenerate_summary(&bundle)?;
    fs::write(
        bundle.join("platform.json"),
        b"{\"platform\":\"Linux\",\"mode\":\"attach\",\"full_command_line_saved\":false}\n",
    )?;
    write_completed_bundle_manifest(&bundle, "COMPLETE", &["platform.json"])?;
    println!("{}", bundle.display());
    Ok(())
}

fn run_semantic_mismatches() -> [Metric; 22] {
    [
        Metric::ProcessWorkingSetBytes,
        Metric::ProcessPrivateBytes,
        Metric::ProcessReadBytes,
        Metric::ProcessWriteBytes,
        Metric::ProcessOtherBytes,
        Metric::ProcessReadOperations,
        Metric::ProcessWriteOperations,
        Metric::ProcessOtherOperations,
        Metric::ProcessHandleCount,
        Metric::ProbeWorkingSetBytes,
        Metric::ProbePrivateBytes,
        Metric::ProbeReadBytes,
        Metric::ProbeWriteBytes,
        Metric::ProbeThreadCount,
        Metric::ProbeHandleCount,
        Metric::SystemUserCpuTimeNs,
        Metric::SystemKernelCpuTimeNs,
        Metric::SystemIdleCpuTimeNs,
        Metric::SystemAvailablePhysicalMemoryBytes,
        Metric::SystemCommitCurrentBytes,
        Metric::SystemCommitLimitBytes,
        Metric::SystemDiskFreeBytes,
    ]
}

fn read_stat_for_identity(identity: &ProcessIdentity) -> Result<StatSample> {
    let stat = read_limited(&format!("/proc/{}/stat", identity.pid))?;
    let sample = parse_stat_sample(identity.pid, &identity.boot_id, &stat)?;
    if sample.identity != *identity {
        bail!("process identity changed during stat observation");
    }
    Ok(sample)
}

fn operational_unavailable_reason(error: &anyhow::Error) -> UnavailableReason {
    if error
        .downcast_ref::<io::Error>()
        .is_some_and(|source| source.kind() == io::ErrorKind::PermissionDenied)
    {
        UnavailableReason::AuthorityUnavailable
    } else {
        UnavailableReason::SamplingDegraded
    }
}

fn read_limited(path: &str) -> io::Result<String> {
    let mut bytes = Vec::with_capacity(MAX_PROC_BYTES as usize + 1);
    std::fs::File::open(path)?
        .take(MAX_PROC_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_PROC_BYTES {
        return Err(invalid("proc record exceeds bounded input limit"));
    }
    String::from_utf8(bytes).map_err(|_| invalid("proc record is not UTF-8"))
}

fn ticks_to_ns(ticks: u64) -> Result<u64> {
    let ticks_per_second = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if ticks_per_second <= 0 {
        return Err(anyhow!(
            "sysconf(_SC_CLK_TCK) returned an invalid tick rate"
        ));
    }
    let ticks_per_second = ticks_per_second as u64;
    let seconds = ticks / ticks_per_second;
    let remainder = ticks % ticks_per_second;
    seconds
        .checked_mul(1_000_000_000)
        .and_then(|whole| {
            remainder
                .checked_mul(1_000_000_000)
                .and_then(|fraction| whole.checked_add(fraction / ticks_per_second))
        })
        .context("overflow converting Linux clock ticks to nanoseconds")
}

fn utc_now() -> Result<String> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .context("format Linux attach UTC timestamp")
}

fn unique_bundle_name(kind: &str) -> String {
    format!(
        "{kind}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    )
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_stat_loss_persists_exact_degraded_process_sample() {
        let output = tempfile::tempdir().unwrap();
        let pid = std::process::id();
        let mut target_calls = 0_u8;

        attach_with_observation(
            output.path(),
            pid,
            false,
            |identity| {
                if identity.pid == pid && target_calls == 0 {
                    target_calls += 1;
                    return Err(anyhow!("deterministic target stat loss"));
                }
                read_stat_for_identity(identity)
            },
            read_identity,
        )
        .unwrap();

        let bundle = std::fs::read_dir(output.path())
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let sample: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(bundle.join("samples.ndjson")).unwrap())
                .unwrap();
        let row = &sample["processes"][0];
        for field in ["thread_count", "user_cpu_time_ns", "kernel_cpu_time_ns"] {
            assert!(row.get(field).is_none(), "{field} must be absent, not zero");
        }
        let events = std::fs::read_to_string(bundle.join("events.ndjson")).unwrap();
        for metric in [
            "process.thread_count",
            "process.user_cpu_time_ns",
            "process.kernel_cpu_time_ns",
        ] {
            assert!(events.contains(metric));
        }
        assert_eq!(events.matches("sampling_degraded").count(), 3);
        assert!(events.matches("\"process_local_id\":1").count() >= 3);
        assert!(events.matches("\"sample_ordinal\":0").count() >= 3);
        assert!(sample["probe"]["user_cpu_time_ns"].is_number());
        assert!(sample["probe"]["kernel_cpu_time_ns"].is_number());
        assert!(!events.contains("probe.user_cpu_time_ns"));
        assert!(!events.contains("probe.kernel_cpu_time_ns"));
        let summary: serde_json::Value =
            serde_json::from_slice(&std::fs::read(bundle.join("summary.json")).unwrap()).unwrap();
        assert_eq!(summary["measurement_validity"], "DEGRADED");
    }

    #[test]
    fn target_stat_access_loss_is_exactly_authority_unavailable() {
        let output = tempfile::tempdir().unwrap();
        let pid = std::process::id();
        let mut target_calls = 0_u8;
        attach_with_observation(
            output.path(),
            pid,
            false,
            |identity| {
                if identity.pid == pid && target_calls == 0 {
                    target_calls += 1;
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "deterministic target stat access loss",
                    )
                    .into());
                }
                read_stat_for_identity(identity)
            },
            read_identity,
        )
        .unwrap();

        let bundle = std::fs::read_dir(output.path())
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let sample: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(bundle.join("samples.ndjson")).unwrap())
                .unwrap();
        let row = &sample["processes"][0];
        for field in ["thread_count", "user_cpu_time_ns", "kernel_cpu_time_ns"] {
            assert!(row.get(field).is_none(), "{field} must be absent");
        }
        assert!(sample["probe"]["user_cpu_time_ns"].is_number());
        assert!(sample["probe"]["kernel_cpu_time_ns"].is_number());

        let events = std::fs::read_to_string(bundle.join("events.ndjson")).unwrap();
        let unavailable = events
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter(|event| event["record_type"] == "metric_unavailable")
            .filter(|event| event["subject_kind"] == "PROCESS_SAMPLE")
            .collect::<Vec<_>>();
        assert_eq!(unavailable.len(), 3);
        for event in unavailable {
            assert_eq!(event["reason"], "authority_unavailable");
            assert_eq!(event["process_local_id"], PROCESS_LOCAL_ID);
            assert_eq!(event["sample_ordinal"], 0);
            assert!(matches!(
                event["metric"].as_str(),
                Some(
                    "process.thread_count"
                        | "process.user_cpu_time_ns"
                        | "process.kernel_cpu_time_ns"
                )
            ));
        }
        assert!(!events.contains("sampling_degraded"));
        assert!(!events.contains("probe.user_cpu_time_ns"));
        assert!(!events.contains("probe.kernel_cpu_time_ns"));
        let summary: serde_json::Value =
            serde_json::from_slice(&std::fs::read(bundle.join("summary.json")).unwrap()).unwrap();
        assert_eq!(summary["measurement_validity"], "DEGRADED");
    }

    #[test]
    fn probe_stat_loss_preserves_target_and_emits_only_probe_events() {
        let output = tempfile::tempdir().unwrap();
        let pid = std::process::id();
        let mut calls = 0_u8;
        attach_with_observation(
            output.path(),
            pid,
            false,
            |identity| {
                calls += 1;
                if calls == 2 {
                    return Err(anyhow!("deterministic probe stat loss"));
                }
                read_stat_for_identity(identity)
            },
            read_identity,
        )
        .unwrap();
        let bundle = std::fs::read_dir(output.path())
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let sample: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(bundle.join("samples.ndjson")).unwrap())
                .unwrap();
        assert!(sample["processes"][0]["user_cpu_time_ns"].is_number());
        assert!(sample["processes"][0]["kernel_cpu_time_ns"].is_number());
        assert!(sample["processes"][0]["thread_count"].is_number());
        assert!(sample["probe"].get("user_cpu_time_ns").is_none());
        assert!(sample["probe"].get("kernel_cpu_time_ns").is_none());
        let events = std::fs::read_to_string(bundle.join("events.ndjson")).unwrap();
        assert_eq!(events.matches("sampling_degraded").count(), 2);
        assert!(events.contains("probe.user_cpu_time_ns"));
        assert!(events.contains("probe.kernel_cpu_time_ns"));
        assert!(!events.contains("process.user_cpu_time_ns\",\"subject_kind\":\"PROCESS_SAMPLE"));
    }

    #[test]
    fn replacement_at_revalidation_leaves_no_completed_bundle() {
        let output = tempfile::tempdir().unwrap();
        let pid = std::process::id();
        let initial = read_identity(pid).unwrap();
        let replacement =
            ProcessIdentity::new(pid, &initial.boot_id, initial.starttime + 1).unwrap();
        assert!(
            attach_with_observation(
                output.path(),
                pid,
                false,
                read_stat_for_identity,
                move |_| Ok(replacement.clone()),
            )
            .is_err()
        );
        let bundle = std::fs::read_dir(output.path())
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert!(!bundle.join("manifest.json").exists());
        assert!(!bundle.join("summary.json").exists());
    }

    #[test]
    fn disappearance_at_revalidation_is_raw_only_without_final_live_sample() {
        let output = tempfile::tempdir().unwrap();
        let pid = std::process::id();
        assert!(
            attach_with_observation(output.path(), pid, false, read_stat_for_identity, |_| Err(
                io::Error::new(io::ErrorKind::NotFound, "target disappeared")
            ),)
            .is_err()
        );
        let bundle = std::fs::read_dir(output.path())
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert!(!bundle.join("summary.json").exists());
        assert!(!bundle.join("manifest.json").exists());
        assert!(
            std::fs::read_to_string(bundle.join("samples.ndjson"))
                .unwrap_or_default()
                .is_empty()
        );
    }

    #[test]
    fn producer_shaped_operational_event_mutations_fail_closed() {
        let output = tempfile::tempdir().unwrap();
        let pid = std::process::id();
        let mut target_calls = 0_u8;
        attach_with_observation(
            output.path(),
            pid,
            false,
            |identity| {
                if identity.pid == pid && target_calls == 0 {
                    target_calls += 1;
                    return Err(anyhow!("deterministic target stat loss"));
                }
                read_stat_for_identity(identity)
            },
            read_identity,
        )
        .unwrap();
        let bundle = std::fs::read_dir(output.path())
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let events_path = bundle.join("events.ndjson");
        let original = std::fs::read_to_string(&events_path).unwrap();
        for replacement in [
            ("\"process_local_id\":1", "\"process_local_id\":9"),
            ("\"sample_ordinal\":0", "\"sample_ordinal\":1"),
            ("process.user_cpu_time_ns", "process.private_bytes"),
            (
                "\"subject_kind\":\"PROCESS_SAMPLE\"",
                "\"subject_kind\":\"PROCESS\"",
            ),
        ] {
            std::fs::write(
                &events_path,
                original.replacen(replacement.0, replacement.1, 1),
            )
            .unwrap();
            assert!(regenerate_summary(&bundle).is_err(), "{:?}", replacement);
        }
        std::fs::write(
            &events_path,
            format!("{original}{}", original.lines().next().unwrap()),
        )
        .unwrap();
        assert!(regenerate_summary(&bundle).is_err());
    }

    #[test]
    fn observed_zero_cpu_is_numeric_without_unavailable_events() {
        let output = tempfile::tempdir().unwrap();
        let pid = std::process::id();
        attach_with_observation(
            output.path(),
            pid,
            false,
            |identity| {
                let mut observed = read_stat_for_identity(identity)?;
                observed.user_ticks = 0;
                observed.kernel_ticks = 0;
                assert_ne!(observed.thread_count, 0);
                Ok(observed)
            },
            read_identity,
        )
        .unwrap();
        let bundle = std::fs::read_dir(output.path())
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let sample: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(bundle.join("samples.ndjson")).unwrap())
                .unwrap();
        for value in [
            &sample["processes"][0]["user_cpu_time_ns"],
            &sample["processes"][0]["kernel_cpu_time_ns"],
            &sample["probe"]["user_cpu_time_ns"],
            &sample["probe"]["kernel_cpu_time_ns"],
        ] {
            assert_eq!(value, 0);
        }
        let events = std::fs::read_to_string(bundle.join("events.ndjson")).unwrap();
        for metric in [
            "process.user_cpu_time_ns",
            "process.kernel_cpu_time_ns",
            "probe.user_cpu_time_ns",
            "probe.kernel_cpu_time_ns",
        ] {
            assert!(!events.contains(metric));
        }
        let summary: serde_json::Value =
            serde_json::from_slice(&std::fs::read(bundle.join("summary.json")).unwrap()).unwrap();
        assert_eq!(summary["measurement_validity"], "VALID");
    }
}
