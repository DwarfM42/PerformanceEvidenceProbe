//! Bounded, single-writer evidence transport and draft record types.

use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    thread::{self, JoinHandle},
    time::Duration,
};

use anyhow::{Context, Result, anyhow};
use crossbeam_channel::{Receiver, Sender, bounded};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ProcessRecord {
    pub process_local_id: u64,
    pub pid: u32,
    pub process_start_time: u64,
    pub boot_identity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_local_id: Option<u64>,
    pub discovery_source: String,
    pub handle_acquisition_result: String,
}

impl ProcessRecord {
    pub fn minimal(
        process_local_id: u64,
        pid: u32,
        process_start_time: u64,
        boot_identity: impl Into<String>,
    ) -> Self {
        Self {
            process_local_id,
            pid,
            process_start_time,
            boot_identity: boot_identity.into(),
            parent_local_id: None,
            discovery_source: "test".into(),
            handle_acquisition_result: "not_attempted".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ProcessSample {
    pub process_local_id: u64,
    pub working_set_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_bytes: Option<u64>,
    pub user_cpu_time_ns: u64,
    pub kernel_cpu_time_ns: u64,
    pub read_bytes: u64,
    pub write_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub other_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_operations: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub write_operations: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub other_operations: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobAccounting {
    pub total_user_time_ns: u64,
    pub total_kernel_time_ns: u64,
    pub read_operation_count: u64,
    pub write_operation_count: u64,
    pub other_operation_count: u64,
    pub read_transfer_bytes: u64,
    pub write_transfer_bytes: u64,
    pub other_transfer_bytes: u64,
    pub total_processes_os: u64,
    pub active_processes_os: u64,
    pub total_terminated_by_limit_os: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProbeSample {
    pub working_set_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_bytes: Option<u64>,
    pub user_cpu_time_ns: u64,
    pub kernel_cpu_time_ns: u64,
    pub read_bytes: u64,
    pub write_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SystemSample {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_user_cpu_time_ns: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_kernel_cpu_time_ns: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_idle_cpu_time_ns: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_physical_memory_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_current_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_limit_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_free_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SampleRecord {
    pub schema_draft_version: &'static str,
    pub record_type: &'static str,
    pub wall_time_utc: String,
    pub monotonic_ns: u64,
    pub scheduled_monotonic_ns: u64,
    pub sampling_delay_ns: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gap_from_previous_sample_ns: Option<u64>,
    pub root_process_confirmed_live: bool,
    pub process_set_working_set_sum_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_set_private_bytes_sum: Option<u64>,
    pub processes: Vec<ProcessSample>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job: Option<JobAccounting>,
    pub system: SystemSample,
    pub probe: ProbeSample,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvidenceEvent {
    pub record_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wall_time_utc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monotonic_ns: Option<u64>,
    #[serde(flatten)]
    pub detail: serde_json::Map<String, serde_json::Value>,
}

impl EvidenceEvent {
    pub fn new(record_type: impl Into<String>) -> Self {
        Self {
            record_type: record_type.into(),
            wall_time_utc: None,
            monotonic_ns: None,
            detail: serde_json::Map::new(),
        }
    }

    pub fn metric_unavailable(
        metric: Metric,
        subject_kind: SubjectKind,
        reason: UnavailableReason,
    ) -> Self {
        Self::new("metric_unavailable")
            .with_string("metric", metric.as_str())
            .with_string("subject_kind", subject_kind.as_str())
            .with_string("reason", reason.as_str())
    }

    pub fn with_u64(mut self, name: &str, value: u64) -> Self {
        self.detail.insert(name.into(), value.into());
        self
    }

    pub fn with_bool(mut self, name: &str, value: bool) -> Self {
        self.detail.insert(name.into(), value.into());
        self
    }

    pub fn with_string(mut self, name: &str, value: impl Into<String>) -> Self {
        self.detail.insert(name.into(), value.into().into());
        self
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Metric {
    ProcessPrivateBytes,
    ProcessOtherBytes,
    ProcessReadOperations,
    ProcessWriteOperations,
    ProcessOtherOperations,
    ProcessThreadCount,
    ProcessHandleCount,
    ProbePrivateBytes,
    ProbeThreadCount,
    ProbeHandleCount,
    SystemUserCpuTimeNs,
    SystemKernelCpuTimeNs,
    SystemIdleCpuTimeNs,
    SystemAvailablePhysicalMemoryBytes,
    SystemCommitCurrentBytes,
    SystemCommitLimitBytes,
    SystemDiskFreeBytes,
}
impl Metric {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProcessPrivateBytes => "process.private_bytes",
            Self::ProcessOtherBytes => "process.other_bytes",
            Self::ProcessReadOperations => "process.read_operations",
            Self::ProcessWriteOperations => "process.write_operations",
            Self::ProcessOtherOperations => "process.other_operations",
            Self::ProcessThreadCount => "process.thread_count",
            Self::ProcessHandleCount => "process.handle_count",
            Self::ProbePrivateBytes => "probe.private_bytes",
            Self::ProbeThreadCount => "probe.thread_count",
            Self::ProbeHandleCount => "probe.handle_count",
            Self::SystemUserCpuTimeNs => "system.system_user_cpu_time_ns",
            Self::SystemKernelCpuTimeNs => "system.system_kernel_cpu_time_ns",
            Self::SystemIdleCpuTimeNs => "system.system_idle_cpu_time_ns",
            Self::SystemAvailablePhysicalMemoryBytes => "system.available_physical_memory_bytes",
            Self::SystemCommitCurrentBytes => "system.commit_current_bytes",
            Self::SystemCommitLimitBytes => "system.commit_limit_bytes",
            Self::SystemDiskFreeBytes => "system.disk_free_bytes",
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub enum SubjectKind {
    Run,
    Process,
    Sample,
    ProcessSample,
}
impl SubjectKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Run => "RUN",
            Self::Process => "PROCESS",
            Self::Sample => "SAMPLE",
            Self::ProcessSample => "PROCESS_SAMPLE",
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub enum UnavailableReason {
    Unsupported,
    NotApplicable,
    SemanticMismatch,
    AuthorityUnavailable,
    SamplingDegraded,
}
impl UnavailableReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unsupported => "unsupported",
            Self::NotApplicable => "not_applicable",
            Self::SemanticMismatch => "semantic_mismatch",
            Self::AuthorityUnavailable => "authority_unavailable",
            Self::SamplingDegraded => "sampling_degraded",
        }
    }
}

enum WriterMessage {
    Process(ProcessRecord),
    Sample(SampleRecord),
    Event(EvidenceEvent),
}

/// All NDJSON streams are exclusively owned by this background thread. Its
/// bounded queue makes writer backlog visible instead of growing RAM forever.
pub struct EvidenceWriter {
    sender: Sender<WriterMessage>,
    thread: JoinHandle<Result<()>>,
}

impl EvidenceWriter {
    pub fn start(bundle: &Path, queue_capacity: usize) -> Result<Self> {
        if queue_capacity == 0 {
            return Err(anyhow!("writer queue capacity must be positive"));
        }
        fs::create_dir_all(bundle).context("create evidence bundle directory")?;
        let (sender, receiver) = bounded(queue_capacity);
        let bundle = bundle.to_path_buf();
        let thread = thread::Builder::new()
            .name("perf-probe-evidence-writer".into())
            .spawn(move || writer_loop(bundle, receiver))
            .context("start evidence writer")?;
        Ok(Self { sender, thread })
    }

    pub fn process(&self, record: ProcessRecord) -> Result<()> {
        self.send(WriterMessage::Process(record))
    }
    pub fn sample(&self, record: SampleRecord) -> Result<()> {
        self.send(WriterMessage::Sample(record))
    }
    pub fn event(&self, record: EvidenceEvent) -> Result<()> {
        self.send(WriterMessage::Event(record))
    }

    fn send(&self, message: WriterMessage) -> Result<()> {
        self.sender
            .send_timeout(message, Duration::from_millis(250))
            .map_err(|error| anyhow!("bounded evidence writer unavailable: {error}"))
    }

    pub fn finish(self) -> Result<()> {
        drop(self.sender);
        self.thread
            .join()
            .map_err(|_| anyhow!("evidence writer panicked"))??;
        Ok(())
    }
}

fn writer_loop(bundle: PathBuf, receiver: Receiver<WriterMessage>) -> Result<()> {
    let mut processes = open_stream(&bundle, "processes.ndjson")?;
    let mut samples = open_stream(&bundle, "samples.ndjson")?;
    let mut events = open_stream(&bundle, "events.ndjson")?;
    for message in receiver {
        match message {
            WriterMessage::Process(record) => write_record(&mut processes, &record)?,
            WriterMessage::Sample(record) => write_record(&mut samples, &record)?,
            WriterMessage::Event(record) => write_record(&mut events, &record)?,
        }
    }
    for stream in [&mut processes, &mut samples, &mut events] {
        stream.flush()?;
        stream.get_ref().sync_data()?;
    }
    Ok(())
}

fn open_stream(bundle: &Path, name: &str) -> Result<BufWriter<File>> {
    File::create(bundle.join(name))
        .map(BufWriter::new)
        .with_context(|| format!("create {name}"))
}

fn write_record<T: Serialize>(stream: &mut BufWriter<File>, record: &T) -> Result<()> {
    serde_json::to_writer(&mut *stream, record)?;
    stream.write_all(b"\n")?;
    // A flushed line is independently parseable should the probe later crash.
    stream.flush()?;
    Ok(())
}
