//! Bounded Linux attach evidence using a composite `/proc` process identity.

use anyhow::{Result, bail};
#[cfg(not(test))]
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};
use std::{
    io::{self, Read},
    path::Path,
};

#[cfg(not(test))]
use anyhow::{Context, anyhow};
#[cfg(not(test))]
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

#[cfg(not(test))]
use crate::{
    evidence::{
        EvidenceEvent, EvidenceWriter, Metric, ProbeSample, ProcessRecord, ProcessSample,
        SampleRecord, SubjectKind, SystemSample, UnavailableReason,
        write_completed_bundle_manifest,
    },
    summary::regenerate_summary,
};

const MAX_PROC_BYTES: u64 = 8 * 1024;
#[cfg(not(test))]
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

#[cfg(not(test))]
pub fn attach(output_root: &Path, pid: u32, attach_job: bool) -> Result<()> {
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

    let target = read_stat_for_identity(&initial)?;
    let probe_identity = read_identity(std::process::id()).context("establish probe identity")?;
    let probe = read_stat_for_identity(&probe_identity)?;
    if read_identity(pid).context("revalidate Linux attach identity")? != initial {
        bail!("attached process identity changed or disappeared during observation");
    }

    for metric in run_semantic_mismatches() {
        writer.event(EvidenceEvent::metric_unavailable(
            metric,
            SubjectKind::Run,
            UnavailableReason::SemanticMismatch,
        ))?;
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
            user_cpu_time_ns: ticks_to_ns(target.user_ticks)?,
            kernel_cpu_time_ns: ticks_to_ns(target.kernel_ticks)?,
            read_bytes: None,
            write_bytes: None,
            other_bytes: None,
            read_operations: None,
            write_operations: None,
            other_operations: None,
            thread_count: Some(target.thread_count),
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
            user_cpu_time_ns: ticks_to_ns(probe.user_ticks)?,
            kernel_cpu_time_ns: ticks_to_ns(probe.kernel_ticks)?,
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

#[cfg(test)]
#[allow(dead_code)]
pub fn attach(_output_root: &Path, _pid: u32, _attach_job: bool) -> Result<()> {
    bail!("Linux attach is not available in a path-included identity test")
}

#[cfg(not(test))]
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

#[cfg(not(test))]
fn read_stat_for_identity(identity: &ProcessIdentity) -> Result<StatSample> {
    let stat = read_limited(&format!("/proc/{}/stat", identity.pid))?;
    let sample = parse_stat_sample(identity.pid, &identity.boot_id, &stat)?;
    if sample.identity != *identity {
        bail!("process identity changed during stat observation");
    }
    Ok(sample)
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

#[cfg(not(test))]
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

#[cfg(not(test))]
fn utc_now() -> Result<String> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .context("format Linux attach UTC timestamp")
}

#[cfg(not(test))]
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
