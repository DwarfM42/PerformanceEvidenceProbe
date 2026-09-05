//! Windows process launch/attach boundary.  Handles here are owned explicitly;
//! no Job policy can terminate a target when this process exits.

use std::{
    collections::{HashMap, HashSet},
    fs,
    mem::{size_of, zeroed},
    path::Path,
    ptr::null,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow, bail};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use windows_sys::Win32::{
    Foundation::{CloseHandle, FILETIME, GetLastError, HANDLE, INVALID_HANDLE_VALUE},
    Storage::FileSystem::GetDiskFreeSpaceExW,
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
            TH32CS_SNAPPROCESS,
        },
        IO::CreateIoCompletionPort,
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, IsProcessInJob,
            JOBOBJECT_ASSOCIATE_COMPLETION_PORT, JOBOBJECT_BASIC_AND_IO_ACCOUNTING_INFORMATION,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectAssociateCompletionPortInformation,
            JobObjectBasicAndIoAccountingInformation, JobObjectExtendedLimitInformation,
            QueryInformationJobObject, SetInformationJobObject,
        },
        ProcessStatus::{GetPerformanceInfo, PERFORMANCE_INFORMATION},
        SystemInformation::{
            GetLogicalProcessorInformationEx, GlobalMemoryStatusEx, MEMORYSTATUSEX, OSVERSIONINFOW,
            RelationProcessorCore, SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
        },
        Threading::{
            CREATE_SUSPENDED, CreateProcessW, GetCurrentProcess, GetExitCodeProcess,
            GetProcessHandleCount, GetProcessId, GetProcessIoCounters, GetProcessTimes,
            GetSystemTimes, IO_COUNTERS, OpenProcess, PROCESS_INFORMATION,
            PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_VM_READ, ResumeThread,
            STARTUPINFOW,
        },
    },
};

use crate::{
    contract::JobSafetyPolicy,
    evidence::{
        EvidenceEvent, EvidenceWriter, JobAccounting, Metric, ProbeSample, ProcessRecord,
        ProcessSample, SampleRecord, SubjectKind, SystemSample, UnavailableReason,
        write_completed_bundle_manifest,
    },
    summary::regenerate_summary,
};

const SAMPLE_INTERVAL: Duration = Duration::from_millis(500);
const HUNDRED_NS_PER_NS: u64 = 100;
const STILL_ACTIVE: u32 = 259;

struct OwnedHandle(HANDLE);
impl OwnedHandle {
    fn new(handle: HANDLE, name: &str) -> Result<Self> {
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            bail!("{name} failed with Win32 error {}", unsafe {
                GetLastError()
            });
        }
        Ok(Self(handle))
    }
    fn raw(&self) -> HANDLE {
        self.0
    }
    fn close(self) -> Result<()> {
        let handle = self.0;
        std::mem::forget(self);
        if unsafe { CloseHandle(handle) } == 0 {
            bail!("CloseHandle failed with Win32 error {}", unsafe {
                GetLastError()
            });
        }
        Ok(())
    }
}
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub fn run(
    output_root: &Path,
    max_retained_process_handles: usize,
    command: &[String],
) -> Result<()> {
    if command.is_empty() || max_retained_process_handles == 0 {
        bail!("run requires command and a positive handle limit");
    }
    let bundle = output_root.join(unique_bundle_name("run"));
    let writer = EvidenceWriter::start(&bundle, 256)?;
    let mut creation = build_windows_command_line(command)?;
    let mut startup: STARTUPINFOW = unsafe { zeroed() };
    startup.cb = size_of::<STARTUPINFOW>() as u32;
    let mut information: PROCESS_INFORMATION = unsafe { zeroed() };
    let created = unsafe {
        CreateProcessW(
            null(),
            creation.as_mut_ptr(),
            null(),
            null(),
            0,
            CREATE_SUSPENDED,
            null(),
            null(),
            &startup,
            &mut information,
        )
    };
    if created == 0 {
        bail!("CreateProcessW failed with Win32 error {}", unsafe {
            GetLastError()
        });
    }
    let process = OwnedHandle::new(information.hProcess, "CreateProcessW process handle")?;
    let thread_handle = OwnedHandle::new(information.hThread, "CreateProcessW thread handle")?;

    // The Job is accounting-only: explicit extended-limit info with zero flags.
    let job = OwnedHandle::new(
        unsafe { CreateJobObjectW(null(), null()) },
        "CreateJobObjectW",
    )?;
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JobSafetyPolicy::probe_default().limit_flags() as _;
    let set = unsafe {
        SetInformationJobObject(
            job.raw(),
            JobObjectExtendedLimitInformation,
            (&limits as *const _) as _,
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };
    if set == 0 {
        bail!(
            "SetInformationJobObject(non-destructive policy) failed with Win32 error {}",
            unsafe { GetLastError() }
        );
    }
    let completion_port = OwnedHandle::new(
        unsafe { CreateIoCompletionPort(INVALID_HANDLE_VALUE, std::ptr::null_mut(), 0, 1) },
        "CreateIoCompletionPort",
    )?;
    let association = JOBOBJECT_ASSOCIATE_COMPLETION_PORT {
        CompletionKey: job.raw() as *mut _,
        CompletionPort: completion_port.raw(),
    };
    if unsafe {
        SetInformationJobObject(
            job.raw(),
            JobObjectAssociateCompletionPortInformation,
            (&association as *const _) as _,
            size_of::<JOBOBJECT_ASSOCIATE_COMPLETION_PORT>() as u32,
        )
    } == 0
    {
        bail!(
            "SetInformationJobObject(completion port) failed with Win32 error {}",
            unsafe { GetLastError() }
        );
    }
    writer.event(
        EvidenceEvent::new("completion_port_prepared")
            .with_bool("job_completion_port_associated", true),
    )?;
    if unsafe { AssignProcessToJobObject(job.raw(), process.raw()) } == 0 {
        bail!(
            "AssignProcessToJobObject failed with Win32 error {}",
            unsafe { GetLastError() }
        );
    }
    let mut assigned = 0;
    if unsafe { IsProcessInJob(process.raw(), job.raw(), &mut assigned) } == 0 || assigned == 0 {
        bail!("Job membership verification failed");
    }

    let start_time = process_start_filetime(process.raw())?;
    let boot_identity = boot_identity()?;
    writer.process(ProcessRecord {
        process_local_id: 1,
        pid: information.dwProcessId,
        process_start_time: start_time,
        boot_identity: boot_identity.clone(),
        parent_local_id: None,
        discovery_source: "launch_root".into(),
        handle_acquisition_result: "retained_launch_handle".into(),
    })?;
    writer.event(
        EvidenceEvent::new("launch_assigned_non_destructive_job")
            .with_u64("pid", information.dwProcessId as u64)
            .with_bool("kill_on_job_close_enabled", false),
    )?;

    if unsafe { ResumeThread(thread_handle.raw()) } == u32::MAX {
        bail!("ResumeThread failed with Win32 error {}", unsafe {
            GetLastError()
        });
    }
    observe_until_exit(
        &writer,
        process.raw(),
        Some(job.raw()),
        information.dwProcessId,
        &boot_identity,
        max_retained_process_handles,
    )?;
    let mut exit_code = 0;
    if unsafe { GetExitCodeProcess(process.raw(), &mut exit_code) } == 0 {
        bail!("GetExitCodeProcess failed with Win32 error {}", unsafe {
            GetLastError()
        });
    }
    emit_terminal_event(&writer, process.raw(), 1, exit_code)?;
    process.close()?;
    writer.event(EvidenceEvent::new("handle_released").with_u64("process_local_id", 1))?;
    writer.finish()?;
    regenerate_summary(&bundle)?;
    write_bundle_metadata(
        &bundle,
        "launch",
        information.dwProcessId,
        start_time,
        &boot_identity,
        Some(exit_code),
        command.first().map(String::as_str),
        true,
        max_retained_process_handles,
    )?;
    println!("{}", bundle.display());
    Ok(())
}

pub fn attach(output_root: &Path, pid: u32, attach_job: bool) -> Result<()> {
    if attach_job {
        bail!("--attach-job is intentionally not implemented in Milestone 1");
    }
    let bundle = output_root.join(unique_bundle_name("attach"));
    let writer = EvidenceWriter::start(&bundle, 256)?;
    // Attach mode opens an observation handle only.  It intentionally does not
    // create or assign a Job, so collector lifetime cannot affect the target.
    let process = OwnedHandle::new(
        unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ | PROCESS_SYNCHRONIZE,
                0,
                pid,
            )
        },
        "OpenProcess(attach observation handle)",
    )?;
    let canonical_pid = unsafe { GetProcessId(process.raw()) };
    if canonical_pid != pid {
        bail!("OpenProcess returned mismatched PID {canonical_pid} for requested PID {pid}");
    }
    let start_time = process_start_filetime(process.raw())?;
    let attach_boot_identity = boot_identity()?;
    writer.process(ProcessRecord {
        process_local_id: 1,
        pid,
        process_start_time: start_time,
        boot_identity: attach_boot_identity.clone(),
        parent_local_id: None,
        discovery_source: "attach_root".into(),
        handle_acquisition_result: "retained_attach_observation_handle".into(),
    })?;
    writer.event(
        EvidenceEvent::new("attach_observation_started")
            .with_u64("pid", pid as u64)
            .with_bool("attached_to_probe_job", false),
    )?;
    observe_until_exit(&writer, process.raw(), None, pid, &attach_boot_identity, 1)?;
    let mut exit_code = 0;
    if unsafe { GetExitCodeProcess(process.raw(), &mut exit_code) } == 0 {
        bail!("GetExitCodeProcess failed with Win32 error {}", unsafe {
            GetLastError()
        });
    }
    emit_terminal_event(&writer, process.raw(), 1, exit_code)?;
    process.close()?;
    writer.event(EvidenceEvent::new("handle_released").with_u64("process_local_id", 1))?;
    writer.finish()?;
    regenerate_summary(&bundle)?;
    write_bundle_metadata(
        &bundle,
        "attach",
        pid,
        start_time,
        &attach_boot_identity,
        Some(exit_code),
        None,
        false,
        1,
    )?;
    println!("{}", bundle.display());
    Ok(())
}

fn observe_until_exit(
    writer: &EvidenceWriter,
    process: HANDLE,
    job: Option<HANDLE>,
    root_pid: u32,
    boot_identity: &str,
    max_retained_process_handles: usize,
) -> Result<()> {
    let origin = Instant::now();
    let mut index = 0_u64;
    let mut previous = None;
    let mut known_processes = HashMap::from([(root_pid, 1_u64)]);
    let mut retained_children = HashMap::<u32, (u64, OwnedHandle)>::new();
    let mut next_process_local_id = 2_u64;
    let mut retention_degraded = false;
    loop {
        let due = origin + SAMPLE_INTERVAL * index as u32;
        if let Some(remaining) = due.checked_duration_since(Instant::now()) {
            thread::sleep(remaining);
        }
        discover_children(
            writer,
            root_pid,
            boot_identity,
            max_retained_process_handles,
            &mut known_processes,
            &mut retained_children,
            &mut next_process_local_id,
            &mut retention_degraded,
        )?;
        finalize_exited_children(writer, &mut retained_children)?;
        let monotonic = origin.elapsed().as_nanos().min(u64::MAX as u128) as u64;
        let scheduled = SAMPLE_INTERVAL.as_nanos() as u64 * index;
        let mut sample = one_sample(
            process,
            job,
            1,
            &retained_children,
            monotonic,
            scheduled,
            previous,
        )?;
        let mut exit = 0;
        if unsafe { GetExitCodeProcess(process, &mut exit) } == 0 {
            bail!("GetExitCodeProcess failed with Win32 error {}", unsafe {
                GetLastError()
            });
        }
        sample.root_process_confirmed_live = exit == STILL_ACTIVE;
        for process_sample in &sample.processes {
            if process_sample.thread_count.is_none() {
                // A ToolHelp snapshot can lose a process while an already-open
                // observation handle still supplies the rest of this sample.
                // Preserve that distinction rather than serializing a fake zero
                // or leaving an unexplained V2 omission.
                writer.event(
                    EvidenceEvent::metric_unavailable(
                        Metric::ProcessThreadCount,
                        SubjectKind::ProcessSample,
                        UnavailableReason::SamplingDegraded,
                    )
                    .with_u64("process_local_id", process_sample.process_local_id)
                    .with_u64("sample_ordinal", index),
                )?;
            }
        }
        if sample.probe.thread_count.is_none() {
            writer.event(
                EvidenceEvent::metric_unavailable(
                    Metric::ProbeThreadCount,
                    SubjectKind::Sample,
                    UnavailableReason::SamplingDegraded,
                )
                .with_u64("sample_ordinal", index),
            )?;
        }
        previous = Some(monotonic);
        writer.sample(sample)?;
        if exit != STILL_ACTIVE {
            // A root may exit before an already retained descendant.  Continue
            // only the bounded terminal-finalization loop so every retained
            // child produces its release evidence before the writer closes.
            while !retained_children.is_empty() {
                thread::sleep(SAMPLE_INTERVAL);
                finalize_exited_children(writer, &mut retained_children)?;
            }
            return Ok(());
        }
        index = index.saturating_add(1);
    }
}

fn snapshot_processes() -> Result<Vec<(u32, u32)>> {
    let snapshot = OwnedHandle::new(
        unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) },
        "CreateToolhelp32Snapshot(process discovery)",
    )?;
    let mut entry = PROCESSENTRY32W::default();
    entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
    if unsafe { Process32FirstW(snapshot.raw(), &mut entry) } == 0 {
        bail!(
            "Process32FirstW(process discovery) failed with Win32 error {}",
            unsafe { GetLastError() }
        );
    }
    let mut records = Vec::new();
    loop {
        records.push((entry.th32ProcessID, entry.th32ParentProcessID));
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        if unsafe { Process32NextW(snapshot.raw(), &mut entry) } == 0 {
            break;
        }
    }
    Ok(records)
}

fn discover_children(
    writer: &EvidenceWriter,
    root_pid: u32,
    boot_identity: &str,
    max_retained_process_handles: usize,
    known_processes: &mut HashMap<u32, u64>,
    retained_children: &mut HashMap<u32, (u64, OwnedHandle)>,
    next_process_local_id: &mut u64,
    retention_degraded: &mut bool,
) -> Result<()> {
    let snapshot = snapshot_processes()?;
    let mut live_pids = snapshot.iter().map(|(pid, _)| *pid).collect::<HashSet<_>>();
    live_pids.insert(root_pid);
    // Discovery state is bounded by the currently observable live tree, rather
    // than accumulating a PID entry for every completed short-lived child.
    known_processes.retain(|pid, _| live_pids.contains(pid));
    let parent_by_pid = snapshot.iter().copied().collect::<HashMap<_, _>>();
    let mut descendants = HashSet::from([root_pid]);
    loop {
        let before = descendants.len();
        for (pid, parent_pid) in &snapshot {
            if descendants.contains(parent_pid) || known_processes.contains_key(parent_pid) {
                descendants.insert(*pid);
            }
        }
        if descendants.len() == before {
            break;
        }
    }
    for pid in descendants {
        if pid == root_pid || known_processes.contains_key(&pid) {
            continue;
        }
        let parent_pid = parent_by_pid.get(&pid).copied().unwrap_or_default();
        let parent_local_id = known_processes.get(&parent_pid).copied();
        let process_local_id = *next_process_local_id;
        *next_process_local_id = next_process_local_id.saturating_add(1);
        let handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ | PROCESS_SYNCHRONIZE,
                0,
                pid,
            )
        };
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            writer.process(ProcessRecord {
                process_local_id,
                pid,
                process_start_time: 0,
                boot_identity: boot_identity.to_owned(),
                parent_local_id,
                discovery_source: "toolhelp_descendant".into(),
                handle_acquisition_result: format!("failed_win32_{}", unsafe { GetLastError() }),
            })?;
            writer.event(
                EvidenceEvent::new("collector_degradation")
                    .with_u64("pid", pid as u64)
                    .with_string("reason", "child_handle_acquisition_failed"),
            )?;
            known_processes.insert(pid, process_local_id);
            continue;
        }
        let handle = OwnedHandle::new(handle, "OpenProcess(discovered child)")?;
        let start_time = match process_start_filetime(handle.raw()) {
            Ok(value) => value,
            Err(_) => 0,
        };
        let can_retain = retained_children.len().saturating_add(1) < max_retained_process_handles;
        writer.process(ProcessRecord {
            process_local_id,
            pid,
            process_start_time: start_time,
            boot_identity: boot_identity.to_owned(),
            parent_local_id,
            discovery_source: "toolhelp_descendant".into(),
            handle_acquisition_result: if can_retain {
                "retained_discovered_handle".into()
            } else {
                "not_retained_handle_limit".into()
            },
        })?;
        known_processes.insert(pid, process_local_id);
        if can_retain {
            retained_children.insert(pid, (process_local_id, handle));
            writer.event(
                EvidenceEvent::new("process_observed")
                    .with_u64("process_local_id", process_local_id)
                    .with_u64("pid", pid as u64),
            )?;
        } else if !*retention_degraded {
            *retention_degraded = true;
            writer.event(
                EvidenceEvent::new("collector_degradation")
                    .with_bool("handle_retention_degraded", true)
                    .with_string("reason", "max_retained_process_handles"),
            )?;
        }
    }
    Ok(())
}

fn finalize_exited_children(
    writer: &EvidenceWriter,
    retained_children: &mut HashMap<u32, (u64, OwnedHandle)>,
) -> Result<()> {
    let exited = retained_children
        .iter()
        .filter_map(|(pid, (_, handle))| {
            let mut code = 0_u32;
            let queried = unsafe { GetExitCodeProcess(handle.raw(), &mut code) } != 0;
            (queried && code != STILL_ACTIVE).then_some((*pid, code))
        })
        .collect::<Vec<_>>();
    for (pid, code) in exited {
        if let Some((process_local_id, handle)) = retained_children.remove(&pid) {
            emit_terminal_event(writer, handle.raw(), process_local_id, code)?;
            handle.close()?;
            writer.event(
                EvidenceEvent::new("handle_released")
                    .with_u64("process_local_id", process_local_id)
                    .with_u64("pid", pid as u64),
            )?;
        }
    }
    Ok(())
}

fn one_sample(
    process: HANDLE,
    job: Option<HANDLE>,
    process_local_id: u64,
    retained_children: &HashMap<u32, (u64, OwnedHandle)>,
    monotonic_ns: u64,
    scheduled_ns: u64,
    previous: Option<u64>,
) -> Result<SampleRecord> {
    let mut processes = vec![process_sample(process, process_local_id)?];
    // Only live handles are sampled. A process that races to exit remains
    // registered in processes.ndjson but is not given fabricated counters.
    for (child_local_id, handle) in retained_children.values() {
        if let Ok(sample) = process_sample(handle.raw(), *child_local_id) {
            processes.push(sample);
        }
    }
    let job = match job {
        Some(handle) => Some(job_accounting(handle)?),
        None => None,
    };
    let process_set_working_set_sum_bytes = Some(
        processes
            .iter()
            .map(|sample| {
                sample
                    .working_set_bytes
                    .expect("Windows working set bytes are present")
            })
            .sum(),
    );
    let process_set_private_bytes_sum = Some(
        processes
            .iter()
            .map(|sample| {
                sample
                    .private_bytes
                    .expect("Windows private bytes are present")
            })
            .sum(),
    );
    Ok(SampleRecord {
        schema_draft_version: "perf-evidence-v2-draft",
        record_type: "sample",
        wall_time_utc: utc_now()?,
        monotonic_ns,
        scheduled_monotonic_ns: scheduled_ns,
        sampling_delay_ns: monotonic_ns.saturating_sub(scheduled_ns),
        gap_from_previous_sample_ns: previous.map(|previous| monotonic_ns.saturating_sub(previous)),
        root_process_confirmed_live: false,
        process_set_working_set_sum_bytes,
        process_set_private_bytes_sum,
        processes,
        job,
        system: system_sample()?,
        probe: probe_sample()?,
    })
}

fn process_sample(process: HANDLE, process_local_id: u64) -> Result<ProcessSample> {
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX,
    };
    let mut memory = PROCESS_MEMORY_COUNTERS_EX::default();
    memory.cb = size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
    if unsafe { GetProcessMemoryInfo(process, (&mut memory as *mut _) as _, memory.cb) } == 0 {
        bail!("GetProcessMemoryInfo failed with Win32 error {}", unsafe {
            GetLastError()
        });
    }
    let mut creation: FILETIME = unsafe { zeroed() };
    let mut exit: FILETIME = unsafe { zeroed() };
    let mut kernel: FILETIME = unsafe { zeroed() };
    let mut user: FILETIME = unsafe { zeroed() };
    if unsafe { GetProcessTimes(process, &mut creation, &mut exit, &mut kernel, &mut user) } == 0 {
        bail!("GetProcessTimes failed with Win32 error {}", unsafe {
            GetLastError()
        });
    }
    let mut io = IO_COUNTERS::default();
    if unsafe { GetProcessIoCounters(process, &mut io) } == 0 {
        bail!("GetProcessIoCounters failed with Win32 error {}", unsafe {
            GetLastError()
        });
    }
    let mut handles = 0;
    if unsafe { GetProcessHandleCount(process, &mut handles) } == 0 {
        bail!("GetProcessHandleCount failed with Win32 error {}", unsafe {
            GetLastError()
        });
    }
    Ok(ProcessSample {
        process_local_id,
        working_set_bytes: Some(memory.WorkingSetSize as u64),
        private_bytes: Some(memory.PrivateUsage as u64),
        user_cpu_time_ns: filetime_to_ns(user),
        kernel_cpu_time_ns: filetime_to_ns(kernel),
        read_bytes: Some(io.ReadTransferCount),
        write_bytes: Some(io.WriteTransferCount),
        other_bytes: Some(io.OtherTransferCount),
        read_operations: Some(io.ReadOperationCount),
        write_operations: Some(io.WriteOperationCount),
        other_operations: Some(io.OtherOperationCount),
        thread_count: process_thread_count(unsafe { GetProcessId(process) })?,
        handle_count: Some(handles),
    })
}

fn system_sample() -> Result<SystemSample> {
    let mut idle: FILETIME = unsafe { zeroed() };
    let mut kernel: FILETIME = unsafe { zeroed() };
    let mut user: FILETIME = unsafe { zeroed() };
    if unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) } == 0 {
        bail!("GetSystemTimes failed with Win32 error {}", unsafe {
            GetLastError()
        });
    }
    let mut memory = MEMORYSTATUSEX::default();
    memory.dwLength = size_of::<MEMORYSTATUSEX>() as u32;
    if unsafe { GlobalMemoryStatusEx(&mut memory) } == 0 {
        bail!("GlobalMemoryStatusEx failed with Win32 error {}", unsafe {
            GetLastError()
        });
    }
    let mut performance = PERFORMANCE_INFORMATION::default();
    performance.cb = size_of::<PERFORMANCE_INFORMATION>() as u32;
    if unsafe { GetPerformanceInfo(&mut performance, performance.cb) } == 0 {
        bail!("GetPerformanceInfo failed with Win32 error {}", unsafe {
            GetLastError()
        });
    }
    let mut free = 0_u64;
    if unsafe {
        GetDiskFreeSpaceExW(
            // A null directory asks Windows for the current drive. Do not bind
            // collection to a developer-specific drive letter.
            null(),
            &mut free,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } == 0
    {
        bail!("GetDiskFreeSpaceExW failed with Win32 error {}", unsafe {
            GetLastError()
        });
    }
    let page_size = performance.PageSize as u64;
    Ok(SystemSample {
        system_user_cpu_time_ns: Some(filetime_to_ns(user)),
        system_kernel_cpu_time_ns: Some(filetime_to_ns(kernel)),
        system_idle_cpu_time_ns: Some(filetime_to_ns(idle)),
        available_physical_memory_bytes: Some(memory.ullAvailPhys),
        commit_current_bytes: Some((performance.CommitTotal as u64).saturating_mul(page_size)),
        commit_limit_bytes: Some((performance.CommitLimit as u64).saturating_mul(page_size)),
        disk_free_bytes: Some(free),
    })
}

fn probe_sample() -> Result<ProbeSample> {
    let sample = process_sample(unsafe { GetCurrentProcess() }, 0)?;
    Ok(ProbeSample {
        working_set_bytes: sample.working_set_bytes,
        private_bytes: sample.private_bytes,
        user_cpu_time_ns: sample.user_cpu_time_ns,
        kernel_cpu_time_ns: sample.kernel_cpu_time_ns,
        read_bytes: sample.read_bytes,
        write_bytes: sample.write_bytes,
        thread_count: sample.thread_count,
        handle_count: sample.handle_count,
    })
}

fn process_thread_count(pid: u32) -> Result<Option<u32>> {
    let snapshot = OwnedHandle::new(
        unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) },
        "CreateToolhelp32Snapshot",
    )?;
    let mut entry = PROCESSENTRY32W::default();
    entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
    if unsafe { Process32FirstW(snapshot.raw(), &mut entry) } == 0 {
        bail!("Process32FirstW failed with Win32 error {}", unsafe {
            GetLastError()
        });
    }
    loop {
        if entry.th32ProcessID == pid {
            return Ok(Some(entry.cntThreads));
        }
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        if unsafe { Process32NextW(snapshot.raw(), &mut entry) } == 0 {
            break;
        }
    }
    Ok(None)
}

fn job_accounting(job: HANDLE) -> Result<JobAccounting> {
    let mut accounting = JOBOBJECT_BASIC_AND_IO_ACCOUNTING_INFORMATION::default();
    if unsafe {
        QueryInformationJobObject(
            job,
            JobObjectBasicAndIoAccountingInformation,
            (&mut accounting as *mut _) as _,
            size_of::<JOBOBJECT_BASIC_AND_IO_ACCOUNTING_INFORMATION>() as u32,
            std::ptr::null_mut(),
        )
    } == 0
    {
        bail!(
            "QueryInformationJobObject(accounting) failed with Win32 error {}",
            unsafe { GetLastError() }
        );
    }
    Ok(JobAccounting {
        total_user_time_ns: (accounting.BasicInfo.TotalUserTime as u64)
            .saturating_mul(HUNDRED_NS_PER_NS),
        total_kernel_time_ns: (accounting.BasicInfo.TotalKernelTime as u64)
            .saturating_mul(HUNDRED_NS_PER_NS),
        read_operation_count: accounting.IoInfo.ReadOperationCount,
        write_operation_count: accounting.IoInfo.WriteOperationCount,
        other_operation_count: accounting.IoInfo.OtherOperationCount,
        read_transfer_bytes: accounting.IoInfo.ReadTransferCount,
        write_transfer_bytes: accounting.IoInfo.WriteTransferCount,
        other_transfer_bytes: accounting.IoInfo.OtherTransferCount,
        total_processes_os: accounting.BasicInfo.TotalProcesses as u64,
        active_processes_os: accounting.BasicInfo.ActiveProcesses as u64,
        total_terminated_by_limit_os: accounting.BasicInfo.TotalTerminatedProcesses as u64,
    })
}
fn emit_terminal_event(
    writer: &EvidenceWriter,
    process: HANDLE,
    process_local_id: u64,
    exit_code: u32,
) -> Result<()> {
    let sample = process_sample(process, 1)?;
    writer.event(
        EvidenceEvent::new("process_exit_observed")
            .with_u64("process_local_id", process_local_id)
            .with_u64("pid", unsafe { GetProcessId(process) } as u64)
            .with_u64("exit_code", exit_code as u64)
            .with_u64("terminal_user_cpu_time_ns", sample.user_cpu_time_ns)
            .with_u64("terminal_kernel_cpu_time_ns", sample.kernel_cpu_time_ns)
            .with_u64(
                "terminal_read_bytes",
                sample.read_bytes.expect("Windows read bytes are present"),
            )
            .with_u64(
                "terminal_write_bytes",
                sample.write_bytes.expect("Windows write bytes are present"),
            )
            .with_string("terminal_counter_fidelity", "attempted_after_exit"),
    )
}
fn process_start_filetime(process: HANDLE) -> Result<u64> {
    let mut c: FILETIME = unsafe { zeroed() };
    let mut e: FILETIME = unsafe { zeroed() };
    let mut k: FILETIME = unsafe { zeroed() };
    let mut u: FILETIME = unsafe { zeroed() };
    if unsafe { GetProcessTimes(process, &mut c, &mut e, &mut k, &mut u) } == 0 {
        return Err(anyhow!(
            "GetProcessTimes(start identity) failed with Win32 error {}",
            unsafe { GetLastError() }
        ));
    }
    Ok(filetime_u64(c))
}
fn filetime_u64(value: FILETIME) -> u64 {
    (u64::from(value.dwHighDateTime) << 32) | u64::from(value.dwLowDateTime)
}
fn filetime_to_ns(value: FILETIME) -> u64 {
    filetime_u64(value).saturating_mul(HUNDRED_NS_PER_NS)
}
fn utc_now() -> Result<String> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .context("format UTC timestamp")
}
#[repr(C)]
#[derive(Default)]
struct NativeSystemTimeOfDayInformation {
    boot_time: i64,
    current_time: i64,
    time_zone_bias: i64,
    current_time_zone_id: u32,
    reserved: u32,
    boot_time_bias: i64,
    sleep_time_bias: i64,
}

/// Obtains the OS-maintained boot-time FILETIME from
/// `NtQuerySystemInformation(SystemTimeOfDayInformation)`.  Unlike the old
/// collector wall-clock estimate, this is a stable boot-session value supplied
/// by Windows and remains part of every composite process identity.
fn boot_identity() -> Result<String> {
    use windows_sys::Wdk::System::SystemInformation::{
        NtQuerySystemInformation, SystemTimeOfDayInformation,
    };

    let mut information = NativeSystemTimeOfDayInformation::default();
    let status = unsafe {
        NtQuerySystemInformation(
            SystemTimeOfDayInformation,
            (&mut information as *mut NativeSystemTimeOfDayInformation).cast(),
            size_of::<NativeSystemTimeOfDayInformation>() as u32,
            std::ptr::null_mut(),
        )
    };
    if status < 0 || information.boot_time <= 0 {
        bail!(
            "NtQuerySystemInformation(SystemTimeOfDayInformation) failed with NTSTATUS {status:#x}"
        );
    }
    Ok(format!(
        "windows-boot-time-filetime-{:x}",
        information.boot_time as u64
    ))
}
fn unique_bundle_name(kind: &str) -> String {
    format!(
        "{}-{}",
        kind,
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    )
}
fn write_bundle_metadata(
    bundle: &Path,
    mode: &str,
    pid: u32,
    process_start_time: u64,
    boot_identity: &str,
    exit_code: Option<u32>,
    executable: Option<&str>,
    launched_in_job: bool,
    handle_limit: usize,
) -> Result<()> {
    use serde_json::json;

    let (os_major, os_minor, os_build) = windows_version()?;
    let host = json!({
        "os": "Windows",
        "os_version": format!("{os_major}.{os_minor}"),
        "os_build": os_build,
        "architecture": std::env::consts::ARCH,
        "cpu_model": std::env::var("PROCESSOR_IDENTIFIER").unwrap_or_else(|_| "unavailable".into()),
        "physical_core_count": physical_core_count()?,
        "logical_processor_count": std::thread::available_parallelism().map(|count| count.get()).unwrap_or(0),
        "installed_ram_bytes": installed_ram_bytes()?,
        "probe_version": env!("CARGO_PKG_VERSION"),
        "collector_version": env!("CARGO_PKG_VERSION"),
    });
    let target = json!({
        "mode": mode,
        "root_process_identity": {
            "process_local_id": 1,
            "pid": pid,
            "process_start_time": process_start_time,
            "boot_identity": boot_identity,
        },
        "normalized_executable_path_where_available": executable.and_then(|path| fs::canonicalize(path).ok()).map(|path| path.display().to_string()),
        "target_exit_code_where_available": exit_code,
        "launch_or_attach_metadata": {
            "launched_in_non_destructive_job": launched_in_job,
            "full_command_line_saved": false,
        },
    });
    let config = json!({
        "sampling_interval_ms": 500,
        "timer_backend": "std::time::Instant absolute deadline",
        "sampler_priority": "normal",
        "handle_retention_policy": "retain live observation handles; degrade rather than synthesize counters",
        "handle_retention_limit": handle_limit,
        "job_policy": {"limit_flags": 0, "kill_on_job_close_enabled": false, "performance_limits_applied": false},
        "output_policy": "bounded single-writer NDJSON",
        "flush_policy": "flush after every record; sync_data at writer finish",
    });
    let capabilities = json!({
        "windows.private_usage_bytes": "AVAILABLE",
        "windows.job_accounting": if launched_in_job { "AVAILABLE" } else { "NOT_APPLICABLE" },
        "windows.system_sampling": "AVAILABLE",
        "gpu.temperature": "UNSUPPORTED",
    });
    for (name, value) in [
        ("host.json", host),
        ("target.json", target),
        ("config.json", config),
        ("capabilities.json", capabilities),
    ] {
        fs::write(bundle.join(name), serde_json::to_vec_pretty(&value)?)?;
    }
    write_completed_bundle_manifest(
        bundle,
        if exit_code == Some(0) {
            "COMPLETE"
        } else {
            "TARGET_FAILED"
        },
        &[
            "host.json",
            "target.json",
            "config.json",
            "capabilities.json",
        ],
    )
}

fn windows_version() -> Result<(u32, u32, u32)> {
    use windows_sys::Wdk::System::SystemServices::RtlGetVersion;

    let mut version = OSVERSIONINFOW::default();
    version.dwOSVersionInfoSize = size_of::<OSVERSIONINFOW>() as u32;
    let status = unsafe { RtlGetVersion(&mut version) };
    if status < 0 {
        bail!("RtlGetVersion(host metadata) failed with NTSTATUS {status:#x}");
    }
    Ok((
        version.dwMajorVersion,
        version.dwMinorVersion,
        version.dwBuildNumber,
    ))
}
fn installed_ram_bytes() -> Result<u64> {
    let mut memory = MEMORYSTATUSEX::default();
    memory.dwLength = size_of::<MEMORYSTATUSEX>() as u32;
    if unsafe { GlobalMemoryStatusEx(&mut memory) } == 0 {
        bail!(
            "GlobalMemoryStatusEx(host metadata) failed with Win32 error {}",
            unsafe { GetLastError() }
        );
    }
    Ok(memory.ullTotalPhys)
}
fn physical_core_count() -> Result<u32> {
    let mut length = 0_u32;
    unsafe {
        GetLogicalProcessorInformationEx(RelationProcessorCore, std::ptr::null_mut(), &mut length)
    };
    if length == 0 {
        bail!(
            "GetLogicalProcessorInformationEx(core size) failed with Win32 error {}",
            unsafe { GetLastError() }
        );
    }
    let mut buffer = vec![0_u8; length as usize];
    if unsafe {
        GetLogicalProcessorInformationEx(
            RelationProcessorCore,
            buffer
                .as_mut_ptr()
                .cast::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>(),
            &mut length,
        )
    } == 0
    {
        bail!(
            "GetLogicalProcessorInformationEx(core data) failed with Win32 error {}",
            unsafe { GetLastError() }
        );
    }
    let mut offset = 0_usize;
    let mut count = 0_u32;
    while offset < length as usize {
        let record = unsafe {
            &*buffer
                .as_ptr()
                .add(offset)
                .cast::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>()
        };
        if record.Size == 0 {
            bail!("GetLogicalProcessorInformationEx returned zero-size record");
        }
        count = count.saturating_add(1);
        offset = offset.saturating_add(record.Size as usize);
    }
    if offset != length as usize || count == 0 {
        bail!("GetLogicalProcessorInformationEx returned malformed core records");
    }
    Ok(count)
}
fn build_windows_command_line(argv: &[String]) -> Result<Vec<u16>> {
    if argv.iter().any(|value| value.contains('\0')) {
        bail!("target argument contains NUL");
    }
    let text = argv
        .iter()
        .map(|argument| quote_windows_argument(argument))
        .collect::<Vec<_>>()
        .join(" ");
    Ok(text.encode_utf16().chain(std::iter::once(0)).collect())
}
fn quote_windows_argument(argument: &str) -> String {
    if !argument.is_empty() && !argument.chars().any(|c| c.is_whitespace() || c == '\"') {
        return argument.to_owned();
    }
    let mut out = String::from("\"");
    let mut slashes = 0;
    for c in argument.chars() {
        if c == '\\' {
            slashes += 1;
        } else if c == '\"' {
            out.push_str(&"\\".repeat(slashes * 2 + 1));
            out.push('\"');
            slashes = 0;
        } else {
            out.push_str(&"\\".repeat(slashes));
            slashes = 0;
            out.push(c);
        }
    }
    out.push_str(&"\\".repeat(slashes * 2));
    out.push('\"');
    out
}
