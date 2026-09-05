//! Truthful macOS direct-root and observation-only evidence through libproc.

use std::{
    ffi::{c_char, c_int, c_void},
    fs, io,
    mem::{size_of, zeroed},
    os::unix::process::ExitStatusExt,
    path::Path,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    evidence::{
        EvidenceEvent, EvidenceWriter, Metric, ProbeSample, ProcessRecord, ProcessSample,
        SampleRecord, SubjectKind, SystemSample, UnavailableReason,
        write_completed_bundle_manifest,
    },
    summary::regenerate_summary,
};

const PROC_PIDTBSDINFO: c_int = 3;
const PROC_PIDTASKINFO: c_int = 4;
const RUSAGE_INFO_V2: c_int = 2;
const PROCESS_LOCAL_ID: u64 = 1;
const WRITER_QUEUE_CAPACITY: usize = 16;
pub const MAX_COMMAND_ARGUMENTS: usize = 128;
pub const MAX_COMMAND_UTF8_BYTES: usize = 32 * 1024;

#[repr(C)]
#[derive(Clone, Copy)]
struct ProcBsdInfo {
    _flags: u32,
    _status: u32,
    _xstatus: u32,
    pid: u32,
    _ppid: u32,
    _uid: u32,
    _gid: u32,
    _ruid: u32,
    _rgid: u32,
    _svuid: u32,
    _svgid: u32,
    _reserved: u32,
    _comm: [c_char; 16],
    _name: [c_char; 32],
    _nfiles: u32,
    _pgid: u32,
    _pjobc: u32,
    _tdev: u32,
    _tpgid: u32,
    _nice: i32,
    start_sec: u64,
    start_usec: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ProcTaskInfo {
    _virtual_size: u64,
    _resident_size: u64,
    _total_user: u64,
    _total_system: u64,
    _threads_user: u64,
    _threads_system: u64,
    _policy: i32,
    _faults: i32,
    _pageins: i32,
    _cow_faults: i32,
    _messages_sent: i32,
    _messages_received: i32,
    _syscalls_mach: i32,
    _syscalls_unix: i32,
    _context_switches: i32,
    thread_count: i32,
    _running_threads: i32,
    _priority: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RusageInfoV2 {
    _uuid: [u8; 16],
    user_time_ns: u64,
    system_time_ns: u64,
    _pkg_idle_wakeups: u64,
    _interrupt_wakeups: u64,
    _pageins: u64,
    _wired_size: u64,
    _resident_size: u64,
    _phys_footprint: u64,
    _process_start_abstime: u64,
    _process_exit_abstime: u64,
    _child_user_time_ns: u64,
    _child_system_time_ns: u64,
    _child_pkg_idle_wakeups: u64,
    _child_interrupt_wakeups: u64,
    _child_pageins: u64,
    _child_elapsed_abstime: u64,
    _disk_read_bytes: u64,
    _disk_write_bytes: u64,
}

#[repr(C)]
struct Timeval {
    tv_sec: i64,
    tv_usec: i32,
}

unsafe extern "C" {
    fn proc_pidinfo(
        pid: c_int,
        flavor: c_int,
        arg: u64,
        buffer: *mut c_void,
        buffersize: c_int,
    ) -> c_int;
    fn proc_pid_rusage(pid: c_int, flavor: c_int, buffer: *mut RusageInfoV2) -> c_int;
    fn sysctlbyname(
        name: *const c_char,
        oldp: *mut c_void,
        oldlenp: *mut usize,
        newp: *mut c_void,
        newlen: usize,
    ) -> c_int;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub boot_identity: String,
    pub start_time: u64,
}

impl ProcessIdentity {
    pub fn new(pid: u32, boot_identity: &str, start_time: u64) -> io::Result<Self> {
        if pid == 0 || start_time == 0 {
            return Err(invalid("process identity contains a sentinel value"));
        }
        Ok(Self {
            pid,
            boot_identity: parse_boot_identity(boot_identity)?,
            start_time,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityComparison {
    SameInstance,
    DifferentInstance,
    Unavailable,
}

pub fn parse_boot_identity(value: &str) -> io::Result<String> {
    let parts = value
        .strip_prefix("macos-boot-time-unix-")
        .and_then(|v| v.split_once('-'));
    let Some((seconds, micros)) = parts else {
        return Err(invalid("boot authority is malformed"));
    };
    if seconds.is_empty()
        || micros.is_empty()
        || !seconds.bytes().all(|v| v.is_ascii_hexdigit())
        || !micros.bytes().all(|v| v.is_ascii_hexdigit())
    {
        return Err(invalid("boot authority is malformed"));
    }
    Ok(value.to_owned())
}

pub fn validate_start_time(seconds: u64, micros: u64) -> io::Result<u64> {
    if seconds == 0 || micros >= 1_000_000 {
        return Err(invalid("process start authority is malformed"));
    }
    seconds
        .checked_mul(1_000_000)
        .and_then(|v| v.checked_add(micros))
        .ok_or_else(|| invalid("process start authority overflows"))
}

pub fn checked_thread_count(value: i32) -> io::Result<u32> {
    u32::try_from(value).map_err(|_| invalid("macOS thread count is negative"))
}

pub fn validate_command(command: &[String]) -> Result<()> {
    if command.is_empty() || command.len() > MAX_COMMAND_ARGUMENTS {
        bail!("run command must contain 1..={MAX_COMMAND_ARGUMENTS} arguments");
    }
    let bytes = command.iter().try_fold(0_usize, |total, argument| {
        total
            .checked_add(argument.len())
            .and_then(|value| value.checked_add(1))
            .context("run command UTF-8 byte count overflows")
    })?;
    if bytes > MAX_COMMAND_UTF8_BYTES {
        bail!("run command exceeds {MAX_COMMAND_UTF8_BYTES} UTF-8 bytes");
    }
    Ok(())
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

pub fn run(
    output_root: &Path,
    max_retained_process_handles: usize,
    command: &[String],
) -> Result<()> {
    if max_retained_process_handles == 0 {
        bail!("run requires a positive handle limit");
    }
    validate_command(command)?;
    let bundle = output_root.join(unique_bundle_name("run"));
    let writer = EvidenceWriter::start(&bundle, WRITER_QUEUE_CAPACITY)?;
    let mut child = Command::new(&command[0])
        .args(&command[1..])
        .spawn()
        .context("spawn direct macOS run root")?;
    let identity = read_identity(child.id()).context("establish direct macOS run root identity")?;
    writer.process(record(
        &identity,
        "macos_run_direct_root",
        "owned_child_wait_handle",
    ))?;
    let sampled =
        observe_complete_sample(&writer, &identity, 0, || Ok(child.try_wait()?.is_none()))?;
    if !sampled {
        writer.finish()?;
        let _ = child.wait();
        bail!("no usable direct macOS run sample was observed");
    }
    let status = child.wait().context("wait direct macOS run root")?;
    let exit_code = status.code().map(|value| value as u32);
    let mut event = EvidenceEvent::new("process_exit_observed")
        .with_u64("process_local_id", PROCESS_LOCAL_ID)
        .with_u64("pid", identity.pid as u64);
    if let Some(code) = exit_code {
        event = event.with_u64("exit_code", code as u64);
    } else if let Some(signal) = status.signal() {
        event = event
            .with_string("terminal_kind", "signal")
            .with_u64("signal_number", signal as u64);
    }
    writer.event(event)?;
    writer.finish()?;
    regenerate_summary(&bundle)?;
    write_platform(
        &bundle,
        "run",
        &identity,
        Some(command),
        "directly_owned_child_wait",
    )?;
    write_completed_bundle_manifest(
        &bundle,
        if exit_code == Some(0) {
            "COMPLETE"
        } else {
            "TARGET_FAILED"
        },
        &["platform.json"],
    )?;
    println!("{}", bundle.display());
    Ok(())
}

pub fn attach(output_root: &Path, pid: u32, attach_job: bool) -> Result<()> {
    attach_with_observation(
        output_root,
        pid,
        attach_job,
        || Ok(true),
        read_identity,
        read_metrics,
    )
}

fn attach_with_observation<L, I, M>(
    output_root: &Path,
    pid: u32,
    attach_job: bool,
    root_live: L,
    mut read_identity: I,
    read_metrics: M,
) -> Result<()>
where
    L: FnMut() -> Result<bool>,
    I: FnMut(u32) -> io::Result<ProcessIdentity>,
    M: FnMut(&ProcessIdentity) -> Result<(u64, u64, u32)>,
{
    if attach_job {
        bail!("--attach-job has no macOS analogue");
    }
    let identity = read_identity(pid).context("establish macOS attach identity")?;
    let bundle = output_root.join(unique_bundle_name("attach"));
    let writer = EvidenceWriter::start(&bundle, WRITER_QUEUE_CAPACITY)?;
    writer.process(record(
        &identity,
        "macos_attach_root",
        "pid_start_boot_identity_verified",
    ))?;
    if !observe_complete_sample_with(
        &writer,
        &identity,
        0,
        root_live,
        read_identity,
        read_metrics,
    )? {
        writer.finish()?;
        bail!("no complete macOS attach sample was observed");
    }
    writer.finish()?;
    regenerate_summary(&bundle)?;
    write_platform(&bundle, "attach", &identity, None, "observation_only")?;
    write_completed_bundle_manifest(&bundle, "COMPLETE", &["platform.json"])?;
    println!("{}", bundle.display());
    Ok(())
}

fn observe_complete_sample<F>(
    writer: &EvidenceWriter,
    target: &ProcessIdentity,
    ordinal: u64,
    root_live: F,
) -> Result<bool>
where
    F: FnMut() -> Result<bool>,
{
    observe_complete_sample_with(
        writer,
        target,
        ordinal,
        root_live,
        read_identity,
        read_metrics,
    )
}

fn observe_complete_sample_with<L, I, M>(
    writer: &EvidenceWriter,
    target: &ProcessIdentity,
    ordinal: u64,
    mut root_live: L,
    mut read_identity: I,
    mut read_metrics: M,
) -> Result<bool>
where
    L: FnMut() -> Result<bool>,
    I: FnMut(u32) -> io::Result<ProcessIdentity>,
    M: FnMut(&ProcessIdentity) -> Result<(u64, u64, u32)>,
{
    if !root_live()?
        || compare_identity(target, read_identity(target.pid)) != IdentityComparison::SameInstance
    {
        writer.event(
            EvidenceEvent::new("process_observation_ended")
                .with_u64("process_local_id", PROCESS_LOCAL_ID)
                .with_u64("pid", target.pid as u64)
                .with_string("reason", "identity_or_liveness_lost"),
        )?;
        return Ok(false);
    }
    let target_sample = read_metrics(target);
    let probe_identity =
        read_identity(std::process::id()).context("establish macOS probe identity")?;
    let probe_sample = read_metrics(&probe_identity);
    if !root_live()?
        || compare_identity(target, read_identity(target.pid)) != IdentityComparison::SameInstance
    {
        writer.event(
            EvidenceEvent::new("process_observation_ended")
                .with_u64("process_local_id", PROCESS_LOCAL_ID)
                .with_u64("pid", target.pid as u64)
                .with_string("reason", "identity_or_liveness_lost"),
        )?;
        return Ok(false);
    }
    for metric in semantic_mismatches() {
        writer.event(EvidenceEvent::metric_unavailable(
            metric,
            SubjectKind::Run,
            UnavailableReason::SemanticMismatch,
        ))?;
    }
    write_operational_events(writer, ordinal, &target_sample, true)?;
    write_operational_events(writer, ordinal, &probe_sample, false)?;
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
            user_cpu_time_ns: target_sample.as_ref().ok().map(|v| v.0),
            kernel_cpu_time_ns: target_sample.as_ref().ok().map(|v| v.1),
            read_bytes: None,
            write_bytes: None,
            other_bytes: None,
            read_operations: None,
            write_operations: None,
            other_operations: None,
            thread_count: target_sample.as_ref().ok().map(|v| v.2),
            handle_count: None,
        }],
        job: None,
        system: empty_system(),
        probe: ProbeSample {
            working_set_bytes: None,
            private_bytes: None,
            user_cpu_time_ns: probe_sample.as_ref().ok().map(|v| v.0),
            kernel_cpu_time_ns: probe_sample.as_ref().ok().map(|v| v.1),
            read_bytes: None,
            write_bytes: None,
            thread_count: None,
            handle_count: None,
        },
    })?;
    Ok(true)
}

fn read_metrics(identity: &ProcessIdentity) -> Result<(u64, u64, u32)> {
    if compare_identity(identity, read_identity(identity.pid)) != IdentityComparison::SameInstance {
        bail!("process identity changed before metric attribution");
    }
    let mut usage: RusageInfoV2 = unsafe { zeroed() };
    if unsafe { proc_pid_rusage(identity.pid as c_int, RUSAGE_INFO_V2, &mut usage) } != 0 {
        bail!("proc_pid_rusage failed");
    }
    let mut task: ProcTaskInfo = unsafe { zeroed() };
    let returned = unsafe {
        proc_pidinfo(
            identity.pid as c_int,
            PROC_PIDTASKINFO,
            0,
            (&mut task as *mut ProcTaskInfo).cast(),
            size_of::<ProcTaskInfo>() as c_int,
        )
    };
    if returned != size_of::<ProcTaskInfo>() as c_int {
        bail!("proc_pidinfo(PROC_PIDTASKINFO) failed");
    }
    let threads = checked_thread_count(task.thread_count)?;
    if compare_identity(identity, read_identity(identity.pid)) != IdentityComparison::SameInstance {
        bail!("process identity changed during metric attribution");
    }
    Ok((usage.user_time_ns, usage.system_time_ns, threads))
}

fn read_identity(pid: u32) -> io::Result<ProcessIdentity> {
    if pid == 0 {
        return Err(invalid("PID must be nonzero"));
    }
    let mut info: ProcBsdInfo = unsafe { zeroed() };
    let returned = unsafe {
        proc_pidinfo(
            pid as c_int,
            PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut ProcBsdInfo).cast(),
            size_of::<ProcBsdInfo>() as c_int,
        )
    };
    if returned != size_of::<ProcBsdInfo>() as c_int || info.pid != pid {
        return Err(invalid("proc_pidinfo(PROC_PIDTBSDINFO) failed"));
    }
    ProcessIdentity::new(
        pid,
        &boot_identity()?,
        validate_start_time(info.start_sec, info.start_usec)?,
    )
}

fn boot_identity() -> io::Result<String> {
    let mut value: Timeval = unsafe { zeroed() };
    let mut length = size_of::<Timeval>();
    let name = b"kern.boottime\0";
    if unsafe {
        sysctlbyname(
            name.as_ptr().cast(),
            (&mut value as *mut Timeval).cast(),
            &mut length,
            std::ptr::null_mut(),
            0,
        )
    } != 0
        || length != size_of::<Timeval>()
        || value.tv_sec <= 0
        || value.tv_usec < 0
        || value.tv_usec >= 1_000_000
    {
        return Err(invalid("kern.boottime unavailable or malformed"));
    }
    Ok(format!(
        "macos-boot-time-unix-{:x}-{:x}",
        value.tv_sec, value.tv_usec
    ))
}

fn write_operational_events(
    writer: &EvidenceWriter,
    ordinal: u64,
    result: &Result<(u64, u64, u32)>,
    target: bool,
) -> Result<()> {
    if result.is_ok() {
        return Ok(());
    }
    let (metrics, subject) = if target {
        (
            &[
                Metric::ProcessUserCpuTimeNs,
                Metric::ProcessKernelCpuTimeNs,
                Metric::ProcessThreadCount,
            ][..],
            SubjectKind::ProcessSample,
        )
    } else {
        (
            &[Metric::ProbeUserCpuTimeNs, Metric::ProbeKernelCpuTimeNs][..],
            SubjectKind::Sample,
        )
    };
    let reason = if result.as_ref().err().is_some_and(|error| {
        error
            .downcast_ref::<io::Error>()
            .is_some_and(|source| source.kind() == io::ErrorKind::PermissionDenied)
    }) {
        UnavailableReason::AuthorityUnavailable
    } else {
        UnavailableReason::SamplingDegraded
    };
    for metric in metrics {
        let mut event = EvidenceEvent::metric_unavailable(*metric, subject, reason)
            .with_u64("sample_ordinal", ordinal);
        if target {
            event = event.with_u64("process_local_id", PROCESS_LOCAL_ID);
        }
        writer.event(event)?;
    }
    Ok(())
}

fn semantic_mismatches() -> [Metric; 22] {
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
fn empty_system() -> SystemSample {
    SystemSample {
        system_user_cpu_time_ns: None,
        system_kernel_cpu_time_ns: None,
        system_idle_cpu_time_ns: None,
        available_physical_memory_bytes: None,
        commit_current_bytes: None,
        commit_limit_bytes: None,
        disk_free_bytes: None,
    }
}
fn record(identity: &ProcessIdentity, discovery_source: &str, handle: &str) -> ProcessRecord {
    ProcessRecord {
        process_local_id: PROCESS_LOCAL_ID,
        pid: identity.pid,
        process_start_time: identity.start_time,
        boot_identity: identity.boot_identity.clone(),
        parent_local_id: None,
        discovery_source: discovery_source.into(),
        handle_acquisition_result: handle.into(),
    }
}
fn utc_now() -> Result<String> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .context("format macOS UTC timestamp")
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
    use serde_json::Value;

    const BOOT: &str = "macos-boot-time-unix-1-1";

    fn identity() -> ProcessIdentity {
        ProcessIdentity::new(42, BOOT, 9).unwrap()
    }

    fn raw_bundle() -> (tempfile::TempDir, std::path::PathBuf, EvidenceWriter) {
        let root = tempfile::tempdir().unwrap();
        let bundle = root.path().join("bundle");
        let writer = EvidenceWriter::start(&bundle, WRITER_QUEUE_CAPACITY).unwrap();
        (root, bundle, writer)
    }

    fn assert_raw_loss(bundle: &Path) {
        assert!(
            std::fs::read_to_string(bundle.join("samples.ndjson"))
                .unwrap()
                .is_empty()
        );
        let events = std::fs::read_to_string(bundle.join("events.ndjson")).unwrap();
        assert!(events.contains("process_observation_ended"));
        assert!(!events.contains("process_exit_observed"));
        assert!(!bundle.join("summary.json").exists());
        assert!(!bundle.join("manifest.json").exists());
    }

    #[test]
    fn liveness_loss_before_sample_is_raw_only_without_terminal() {
        let (_root, bundle, writer) = raw_bundle();
        let target = identity();
        writer
            .process(record(&target, "test", "identity_verified"))
            .unwrap();
        assert!(
            !observe_complete_sample_with(
                &writer,
                &target,
                0,
                || Ok(false),
                |_| Ok(target.clone()),
                |_| Ok((0, 0, 0)),
            )
            .unwrap()
        );
        writer.finish().unwrap();
        assert_raw_loss(&bundle);
    }

    #[test]
    fn reused_identity_before_sample_is_raw_only_without_terminal() {
        let (_root, bundle, writer) = raw_bundle();
        let target = identity();
        let reused = ProcessIdentity::new(42, BOOT, 10).unwrap();
        writer
            .process(record(&target, "test", "identity_verified"))
            .unwrap();
        assert!(
            !observe_complete_sample_with(
                &writer,
                &target,
                0,
                || Ok(true),
                |_| Ok(reused.clone()),
                |_| Ok((0, 0, 0)),
            )
            .unwrap()
        );
        writer.finish().unwrap();
        assert_raw_loss(&bundle);
    }

    #[test]
    fn unavailable_identity_before_sample_is_raw_only_without_terminal() {
        let (_root, bundle, writer) = raw_bundle();
        let target = identity();
        writer
            .process(record(&target, "test", "identity_verified"))
            .unwrap();
        assert!(
            !observe_complete_sample_with(
                &writer,
                &target,
                0,
                || Ok(true),
                |_| Err(io::Error::new(io::ErrorKind::NotFound, "gone")),
                |_| Ok((0, 0, 0)),
            )
            .unwrap()
        );
        writer.finish().unwrap();
        assert_raw_loss(&bundle);
    }

    #[test]
    fn attach_liveness_loss_before_sample_is_raw_only_without_terminal() {
        let root = tempfile::tempdir().unwrap();
        let error = attach_with_observation(
            root.path(),
            42,
            false,
            || Ok(false),
            |_| Ok(identity()),
            |_| Ok((0, 0, 0)),
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("no complete macOS attach sample was observed")
        );
        let bundle = std::fs::read_dir(root.path())
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert_raw_loss(&bundle);
    }

    #[test]
    fn attach_identity_loss_before_sample_is_raw_only_without_terminal() {
        let root = tempfile::tempdir().unwrap();
        let mut reads = 0_u8;
        let error = attach_with_observation(
            root.path(),
            42,
            false,
            || Ok(true),
            |_| {
                reads += 1;
                if reads == 1 {
                    Ok(identity())
                } else {
                    Err(io::Error::new(io::ErrorKind::NotFound, "gone"))
                }
            },
            |_| Ok((0, 0, 0)),
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("no complete macOS attach sample was observed")
        );
        let bundle = std::fs::read_dir(root.path())
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert_raw_loss(&bundle);
    }

    fn events(bundle: &Path) -> Vec<Value> {
        std::fs::read_to_string(bundle.join("events.ndjson"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    fn identity_reader(pid: u32) -> io::Result<ProcessIdentity> {
        ProcessIdentity::new(pid, BOOT, 9)
    }

    #[test]
    fn target_metric_failure_is_sample_bound_and_probe_remains_numeric() {
        let (_root, bundle, writer) = raw_bundle();
        let target = identity();
        assert!(
            observe_complete_sample_with(
                &writer,
                &target,
                0,
                || Ok(true),
                identity_reader,
                |observed| {
                    if observed.pid == target.pid {
                        bail!("target rusage unavailable");
                    }
                    Ok((0, 0, 0))
                },
            )
            .unwrap()
        );
        writer.finish().unwrap();

        let sample: Value =
            serde_json::from_str(&std::fs::read_to_string(bundle.join("samples.ndjson")).unwrap())
                .unwrap();
        assert!(sample["processes"][0].get("user_cpu_time_ns").is_none());
        assert_eq!(sample["probe"]["user_cpu_time_ns"], 0);
        assert_eq!(sample["probe"]["kernel_cpu_time_ns"], 0);

        let events = events(&bundle);
        let unavailable = events
            .iter()
            .filter(|event| event["record_type"] == "metric_unavailable")
            .filter(|event| event["reason"] == "sampling_degraded")
            .collect::<Vec<_>>();
        assert_eq!(unavailable.len(), 3);
        for (event, metric) in unavailable.into_iter().zip([
            "process.user_cpu_time_ns",
            "process.kernel_cpu_time_ns",
            "process.thread_count",
        ]) {
            assert_eq!(event["metric"], metric);
            assert_eq!(event["subject_kind"], "PROCESS_SAMPLE");
            assert_eq!(event["reason"], "sampling_degraded");
            assert_eq!(event["process_local_id"], PROCESS_LOCAL_ID);
            assert_eq!(event["sample_ordinal"], 0);
        }
    }

    #[test]
    fn probe_metric_failure_is_sample_bound_and_target_remains_numeric() {
        let (_root, bundle, writer) = raw_bundle();
        let target = identity();
        assert!(
            observe_complete_sample_with(
                &writer,
                &target,
                0,
                || Ok(true),
                identity_reader,
                |observed| {
                    if observed.pid == target.pid {
                        return Ok((0, 0, 0));
                    }
                    bail!("probe rusage unavailable");
                },
            )
            .unwrap()
        );
        writer.finish().unwrap();

        let sample: Value =
            serde_json::from_str(&std::fs::read_to_string(bundle.join("samples.ndjson")).unwrap())
                .unwrap();
        assert_eq!(sample["processes"][0]["user_cpu_time_ns"], 0);
        assert_eq!(sample["processes"][0]["kernel_cpu_time_ns"], 0);
        assert!(sample["probe"].get("user_cpu_time_ns").is_none());

        let events = events(&bundle);
        let unavailable = events
            .iter()
            .filter(|event| event["record_type"] == "metric_unavailable")
            .filter(|event| event["reason"] == "sampling_degraded")
            .collect::<Vec<_>>();
        assert_eq!(unavailable.len(), 2);
        for (event, metric) in unavailable
            .into_iter()
            .zip(["probe.user_cpu_time_ns", "probe.kernel_cpu_time_ns"])
        {
            assert_eq!(event["metric"], metric);
            assert_eq!(event["subject_kind"], "SAMPLE");
            assert_eq!(event["reason"], "sampling_degraded");
            assert!(event.get("process_local_id").is_none());
            assert_eq!(event["sample_ordinal"], 0);
        }
    }
}

fn write_platform(
    bundle: &Path,
    mode: &str,
    identity: &ProcessIdentity,
    command: Option<&[String]>,
    authority: &str,
) -> Result<()> {
    fs::write(
        bundle.join("platform.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"platform":"macOS","mode":mode,"root_process_identity":{"process_local_id":PROCESS_LOCAL_ID,"pid":identity.pid,"process_start_time":identity.start_time,"boot_identity":identity.boot_identity},"launched_command_argv":command,"root_observation_authority":authority,"represented_process_set":"direct_root_only","descendant_discovery":"not_attempted","process_tree_closure":"not_claimed","job_accounting":"not_claimed","process_group_session_cgroup_authority":"not_claimed","descendant_scope":"unknown_not_observed","full_command_line_saved":false}),
        )?,
    )?;
    Ok(())
}
