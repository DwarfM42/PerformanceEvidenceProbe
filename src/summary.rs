use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs,
    path::Path,
};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::Value;

use crate::ndjson::read_complete_records;

const V2: &str = "perf-evidence-v2-draft";

#[derive(Debug, Serialize)]
struct Summary {
    summary_schema_draft_version: &'static str,
    elapsed_ns: u64,
    sample_count: u64,
    max_sample_gap_exact_ns: u64,
    peak_working_set_sampled_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    peak_private_sampled_bytes: Option<u64>,
    last_live_working_set_sample_bytes: Option<u64>,
    last_live_working_set_sample_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_live_private_sample_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_live_private_sample_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    total_cpu_time_ns: Option<u64>,
    average_cpu_utilization: Option<f64>,
    peak_cpu_utilization: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    total_read_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    total_write_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    total_read_operations: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    total_write_operations: Option<u64>,
    maximum_observed_process_count: u64,
    observed_distinct_process_count: u64,
    job_total_processes_os: Option<u64>,
    job_processes_without_observed_identity: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    maximum_probe_handle_count: Option<u64>,
    handle_retention_degraded: bool,
    exit_code: Option<u32>,
    terminal_user_cpu_time_ns: Option<u64>,
    terminal_kernel_cpu_time_ns: Option<u64>,
    terminal_read_bytes: Option<u64>,
    terminal_write_bytes: Option<u64>,
    terminal_counter_fidelity: Option<String>,
    measurement_validity: &'static str,
    measurement_completeness: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Domain {
    Process,
    Probe,
    System,
}
#[derive(Debug, Clone, Copy)]
struct Metric {
    name: &'static str,
    domain: Domain,
    field: &'static str,
}
const METRICS: &[Metric] = &[
    Metric {
        name: "process.private_bytes",
        domain: Domain::Process,
        field: "private_bytes",
    },
    Metric {
        name: "process.other_bytes",
        domain: Domain::Process,
        field: "other_bytes",
    },
    Metric {
        name: "process.read_operations",
        domain: Domain::Process,
        field: "read_operations",
    },
    Metric {
        name: "process.write_operations",
        domain: Domain::Process,
        field: "write_operations",
    },
    Metric {
        name: "process.other_operations",
        domain: Domain::Process,
        field: "other_operations",
    },
    Metric {
        name: "process.thread_count",
        domain: Domain::Process,
        field: "thread_count",
    },
    Metric {
        name: "process.handle_count",
        domain: Domain::Process,
        field: "handle_count",
    },
    Metric {
        name: "probe.private_bytes",
        domain: Domain::Probe,
        field: "private_bytes",
    },
    Metric {
        name: "probe.thread_count",
        domain: Domain::Probe,
        field: "thread_count",
    },
    Metric {
        name: "probe.handle_count",
        domain: Domain::Probe,
        field: "handle_count",
    },
    Metric {
        name: "system.system_user_cpu_time_ns",
        domain: Domain::System,
        field: "system_user_cpu_time_ns",
    },
    Metric {
        name: "system.system_kernel_cpu_time_ns",
        domain: Domain::System,
        field: "system_kernel_cpu_time_ns",
    },
    Metric {
        name: "system.system_idle_cpu_time_ns",
        domain: Domain::System,
        field: "system_idle_cpu_time_ns",
    },
    Metric {
        name: "system.available_physical_memory_bytes",
        domain: Domain::System,
        field: "available_physical_memory_bytes",
    },
    Metric {
        name: "system.commit_current_bytes",
        domain: Domain::System,
        field: "commit_current_bytes",
    },
    Metric {
        name: "system.commit_limit_bytes",
        domain: Domain::System,
        field: "commit_limit_bytes",
    },
    Metric {
        name: "system.disk_free_bytes",
        domain: Domain::System,
        field: "disk_free_bytes",
    },
];

#[derive(Debug)]
struct Unavailable<'a> {
    metric: &'a Metric,
    subject: &'a str,
    reason: &'a str,
    process: Option<u64>,
    sample: Option<usize>,
}

/// Reconstructs the deterministic v2 summary from persisted raw evidence. Raw
/// streams plus typed availability events are authoritative; a manifest only
/// constrains acceptance of a completed producer bundle.
pub fn regenerate_summary(bundle: &Path) -> Result<()> {
    let samples = read_values(bundle, "samples.ndjson")?;
    if samples.is_empty() {
        bail!("cannot reconstruct summary without complete sample records");
    }
    let processes = read_values_if_present(bundle, "processes.ndjson")?;
    let events = read_values_if_present(bundle, "events.ndjson")?;
    let v2 = samples
        .iter()
        .any(|s| s.get("schema_draft_version").and_then(Value::as_str) == Some(V2));
    if v2
        && !samples
            .iter()
            .all(|s| s.get("schema_draft_version").and_then(Value::as_str) == Some(V2))
    {
        bail!("mixed or missing sample schema identity in v2 evidence");
    }

    let authority = process_authority(&processes)?;
    let unavailable = if v2 {
        parse_unavailable(&events, &samples, &authority)?
    } else {
        Vec::new()
    };
    let mut observed_ids = BTreeSet::new();
    let mut first_monotonic = None;
    let mut last_monotonic = 0;
    let mut max_gap = 0_u64;
    let mut peak_working_set = 0_u64;
    let mut peak_private = Some(0_u64);
    let mut maximum_observed_process_count = 0_u64;
    let mut maximum_probe_handle_count = Some(0_u64);
    let mut last_live_working_set = None;
    let mut last_live_private = None;
    let mut last_live_time = None;
    let mut last_live_private_time = None;
    let mut final_job = None;
    let mut cpu_utilizations = Vec::new();
    let mut previous_job_cpu = None;
    let mut previous_monotonic = None;
    let mut operational_omission = false;
    let mut any_omission = false;

    for (ordinal, sample) in samples.iter().enumerate() {
        if sample.get("record_type").and_then(Value::as_str) != Some("sample") {
            bail!("samples.ndjson contains a non-sample record");
        }
        let monotonic = required_u64(sample, "monotonic_ns")?;
        first_monotonic.get_or_insert(monotonic);
        last_monotonic = monotonic;
        max_gap = max_gap.max(nullable_gap_u64(sample)?.unwrap_or(0));
        let rows = sample
            .get("processes")
            .and_then(Value::as_array)
            .context("sample missing processes array")?;
        let mut ids = HashSet::new();
        for row in rows {
            let id = required_u64(row, "process_local_id")?;
            if !ids.insert(id) {
                bail!("duplicate process_local_id {id} inside sample {ordinal}");
            }
            observed_ids.insert(id);
        }
        maximum_observed_process_count = maximum_observed_process_count.max(rows.len() as u64);

        if v2 {
            for metric in METRICS {
                let observations: Vec<(&Value, Option<u64>, Option<u64>)> = match metric.domain {
                    Domain::Process => rows
                        .iter()
                        .map(|row| {
                            Ok((
                                row,
                                optional_u64(row, metric.field)?,
                                Some(required_u64(row, "process_local_id")?),
                            ))
                        })
                        .collect::<Result<_>>()?,
                    Domain::Probe => vec![(
                        sample.get("probe").context("sample missing probe")?,
                        optional_u64(sample.get("probe").unwrap(), metric.field)?,
                        None,
                    )],
                    Domain::System => vec![(
                        sample.get("system").context("sample missing system")?,
                        optional_u64(sample.get("system").unwrap(), metric.field)?,
                        None,
                    )],
                };
                for (_object, value, process) in observations {
                    let matching: Vec<_> = unavailable
                        .iter()
                        .filter(|event| applies(event, metric, ordinal, process))
                        .collect();
                    if value.is_some() && !matching.is_empty() {
                        bail!("numeric {} contradicts metric_unavailable", metric.name);
                    }
                    if value.is_none() {
                        if matching.len() != 1 {
                            bail!(
                                "{} omission at sample {ordinal} has {} explanations",
                                metric.name,
                                matching.len()
                            );
                        }
                        any_omission = true;
                        if matches!(
                            matching[0].reason,
                            "authority_unavailable" | "sampling_degraded"
                        ) {
                            operational_omission = true;
                        }
                    }
                }
            }
        } else {
            // Historic readers accept absent optional leaves, but never reinterpret
            // malformed-present known numeric fields as absence.
            validate_present_optional_metrics(sample, rows)?;
        }

        let working_set_sum = sum_required(rows, "working_set_bytes")?;
        let private_sum = sum_optional(rows, "private_bytes")?;
        if v2 {
            witness(
                sample,
                "process_set_working_set_sum_bytes",
                Some(working_set_sum),
            )?;
            witness(sample, "process_set_private_bytes_sum", private_sum)?;
        }
        peak_working_set = peak_working_set.max(working_set_sum);
        peak_private = match (peak_private, private_sum) {
            (Some(a), Some(b)) => Some(a.max(b)),
            _ => None,
        };
        if sample
            .get("root_process_confirmed_live")
            .and_then(Value::as_bool)
            == Some(true)
        {
            last_live_working_set = Some(working_set_sum);
            last_live_private = private_sum;
            last_live_time = sample
                .get("wall_time_utc")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if private_sum.is_some() {
                last_live_private_time = last_live_time.clone();
            }
        }
        let handles = sample
            .get("probe")
            .map(|p| optional_u64(p, "handle_count"))
            .transpose()?
            .flatten();
        maximum_probe_handle_count = match (maximum_probe_handle_count, handles) {
            (Some(a), Some(b)) => Some(a.max(b)),
            _ => None,
        };
        if let Some(job) = sample.get("job") {
            let total = checked_add(
                required_u64(job, "total_user_time_ns")?,
                required_u64(job, "total_kernel_time_ns")?,
                "job cpu",
            )?;
            if let (Some(previous), Some(previous_time)) = (previous_job_cpu, previous_monotonic) {
                let elapsed = monotonic
                    .checked_sub(previous_time)
                    .context("non-monotonic sample time")?;
                if elapsed > 0 {
                    cpu_utilizations.push(
                        total.checked_sub(previous).context("decreasing job CPU")? as f64
                            / elapsed as f64,
                    );
                }
            }
            previous_job_cpu = Some(total);
            previous_monotonic = Some(monotonic);
            final_job = Some(job.clone());
        }
    }
    if v2 {
        for event in &unavailable {
            if !event_explains_any(event, &samples) {
                bail!("metric_unavailable explains no omission");
            }
        }
    }

    let (
        total_cpu,
        total_read_bytes,
        total_write_bytes,
        total_read_operations,
        total_write_operations,
        job_total_processes_os,
    ) = if let Some(job) = final_job.as_ref() {
        (
            Some(checked_add(
                required_u64(job, "total_user_time_ns")?,
                required_u64(job, "total_kernel_time_ns")?,
                "run total cpu",
            )?),
            Some(required_u64(job, "read_transfer_bytes")?),
            Some(required_u64(job, "write_transfer_bytes")?),
            Some(required_u64(job, "read_operation_count")?),
            Some(required_u64(job, "write_operation_count")?),
            Some(required_u64(job, "total_processes_os")?),
        )
    } else {
        (None, None, None, None, None, None)
    };

    let terminal = root_terminal(&events, &authority, v2)?;
    let handle_retention_degraded = events.iter().any(|event| {
        event.get("record_type").and_then(Value::as_str) == Some("collector_degradation")
            && event
                .get("handle_retention_degraded")
                .and_then(Value::as_bool)
                == Some(true)
    });
    let validity = if handle_retention_degraded || operational_omission {
        "DEGRADED"
    } else {
        "VALID"
    };
    let completeness = if any_omission {
        "DECLARED_PARTIAL"
    } else {
        "COMPLETE"
    };
    let elapsed = last_monotonic
        .checked_sub(first_monotonic.unwrap())
        .context("non-monotonic sample time")?;
    let observed_count = observed_ids.len() as u64;
    let summary = Summary {
        summary_schema_draft_version: if v2 { V2 } else { "perf-evidence-v1-draft" },
        elapsed_ns: elapsed,
        sample_count: samples.len() as u64,
        max_sample_gap_exact_ns: max_gap,
        peak_working_set_sampled_bytes: peak_working_set,
        peak_private_sampled_bytes: peak_private,
        last_live_working_set_sample_bytes: last_live_working_set,
        last_live_working_set_sample_time: last_live_time,
        last_live_private_sample_bytes: last_live_private,
        last_live_private_sample_time: last_live_private_time,
        total_cpu_time_ns: total_cpu,
        average_cpu_utilization: total_cpu
            .filter(|_| elapsed > 0)
            .map(|cpu| cpu as f64 / elapsed as f64),
        peak_cpu_utilization: cpu_utilizations.into_iter().reduce(f64::max),
        total_read_bytes,
        total_write_bytes,
        total_read_operations,
        total_write_operations,
        maximum_observed_process_count,
        observed_distinct_process_count: observed_count,
        job_total_processes_os,
        job_processes_without_observed_identity: job_total_processes_os
            .map(|n| n.saturating_sub(observed_count)),
        maximum_probe_handle_count,
        handle_retention_degraded,
        exit_code: terminal
            .and_then(|e| optional_u64(e, "exit_code").ok().flatten())
            .and_then(|n| u32::try_from(n).ok()),
        terminal_user_cpu_time_ns: terminal
            .and_then(|e| optional_u64(e, "terminal_user_cpu_time_ns").ok().flatten()),
        terminal_kernel_cpu_time_ns: terminal.and_then(|e| {
            optional_u64(e, "terminal_kernel_cpu_time_ns")
                .ok()
                .flatten()
        }),
        terminal_read_bytes: terminal
            .and_then(|e| optional_u64(e, "terminal_read_bytes").ok().flatten()),
        terminal_write_bytes: terminal
            .and_then(|e| optional_u64(e, "terminal_write_bytes").ok().flatten()),
        terminal_counter_fidelity: terminal
            .and_then(|e| e.get("terminal_counter_fidelity"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        measurement_validity: validity,
        measurement_completeness: completeness,
    };
    validate_manifest(bundle, &summary, v2)?;
    let mut bytes = serde_json::to_vec_pretty(&summary)?;
    bytes.push(b'\n');
    fs::write(bundle.join("summary.json"), bytes).context("write deterministic summary")?;
    Ok(())
}

fn parse_unavailable<'a>(
    events: &'a [Value],
    samples: &[Value],
    authority: &HashMap<u64, bool>,
) -> Result<Vec<Unavailable<'a>>> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for event in events
        .iter()
        .filter(|e| e.get("record_type").and_then(Value::as_str) == Some("metric_unavailable"))
    {
        let metric_name = required_str(event, "metric")?;
        let metric = METRICS
            .iter()
            .find(|m| m.name == metric_name)
            .context("unknown or derived metric_unavailable target")?;
        let subject = required_str(event, "subject_kind")?;
        let reason = required_str(event, "reason")?;
        if !matches!(
            reason,
            "unsupported"
                | "not_applicable"
                | "semantic_mismatch"
                | "authority_unavailable"
                | "sampling_degraded"
        ) {
            bail!("unknown unavailable reason");
        }
        let process = match subject {
            "PROCESS" | "PROCESS_SAMPLE" => Some(required_u64(event, "process_local_id")?),
            _ => None,
        };
        let sample = match subject {
            "SAMPLE" | "PROCESS_SAMPLE" => Some(required_u64(event, "sample_ordinal")? as usize),
            _ => None,
        };
        if !matches!(subject, "RUN" | "PROCESS" | "SAMPLE" | "PROCESS_SAMPLE")
            || (subject == "RUN" && (process.is_some() || sample.is_some()))
        {
            bail!("invalid metric_unavailable subject");
        }
        if sample.is_some_and(|i| i >= samples.len()) {
            bail!("metric_unavailable references nonexistent sample");
        }
        match metric.domain {
            Domain::Process if !matches!(subject, "RUN" | "PROCESS" | "PROCESS_SAMPLE") => {
                bail!("invalid process metric subject")
            }
            Domain::Probe | Domain::System if !matches!(subject, "RUN" | "SAMPLE") => {
                bail!("invalid singleton metric subject")
            }
            _ => {}
        }
        if matches!(
            reason,
            "unsupported" | "not_applicable" | "semantic_mismatch"
        ) && subject != "RUN"
        {
            bail!("semantic unavailable reason requires RUN");
        }
        if reason == "authority_unavailable"
            && !matches!(subject, "PROCESS" | "PROCESS_SAMPLE" | "SAMPLE")
        {
            bail!("authority_unavailable requires operational subject");
        }
        if reason == "sampling_degraded" && !matches!(subject, "PROCESS_SAMPLE" | "SAMPLE") {
            bail!("sampling_degraded requires sample subject");
        }
        if let Some(id) = process {
            if authority.get(&id) != Some(&true) {
                bail!("process-scoped omission lacks valid persisted identity authority");
            }
            if let Some(sample_index) = sample {
                if !samples[sample_index]
                    .get("processes")
                    .and_then(Value::as_array)
                    .is_some_and(|rows| {
                        rows.iter()
                            .any(|p| p.get("process_local_id").and_then(Value::as_u64) == Some(id))
                    })
                {
                    bail!("PROCESS_SAMPLE omission process is not in sample");
                }
            }
        }
        let key = format!("{metric_name}|{subject}|{:?}|{:?}", process, sample);
        if !seen.insert(key) {
            bail!("duplicate metric_unavailable declaration");
        }
        result.push(Unavailable {
            metric,
            subject,
            reason,
            process,
            sample,
        });
    }
    Ok(result)
}

fn applies(event: &Unavailable<'_>, metric: &Metric, ordinal: usize, process: Option<u64>) -> bool {
    event.metric.name == metric.name
        && match event.subject {
            "RUN" => true,
            "PROCESS" => process == event.process,
            "SAMPLE" => event.sample == Some(ordinal),
            "PROCESS_SAMPLE" => process == event.process && event.sample == Some(ordinal),
            _ => false,
        }
}
fn event_explains_any(event: &Unavailable<'_>, samples: &[Value]) -> bool {
    samples
        .iter()
        .enumerate()
        .any(|(ordinal, sample)| match event.metric.domain {
            Domain::Process => sample
                .get("processes")
                .and_then(Value::as_array)
                .is_some_and(|rows| {
                    rows.iter().any(|row| {
                        row.get(event.metric.field).is_none()
                            && applies(
                                event,
                                event.metric,
                                ordinal,
                                row.get("process_local_id").and_then(Value::as_u64),
                            )
                    })
                }),
            Domain::Probe => sample.get("probe").is_some_and(|p| {
                p.get(event.metric.field).is_none() && applies(event, event.metric, ordinal, None)
            }),
            Domain::System => sample.get("system").is_some_and(|s| {
                s.get(event.metric.field).is_none() && applies(event, event.metric, ordinal, None)
            }),
        })
}
fn process_authority(records: &[Value]) -> Result<HashMap<u64, bool>> {
    let mut authority = HashMap::new();
    for record in records {
        let id = required_u64(record, "process_local_id")?;
        let valid = required_u64(record, "pid").unwrap_or(0) != 0
            && required_u64(record, "process_start_time").unwrap_or(0) != 0
            && required_str(record, "boot_identity").is_ok_and(|boot| !boot.is_empty());
        authority
            .entry(id)
            .and_modify(|known| *known = false)
            .or_insert(valid);
    }
    Ok(authority)
}
fn root_terminal<'a>(
    events: &'a [Value],
    authority: &HashMap<u64, bool>,
    v2: bool,
) -> Result<Option<&'a Value>> {
    let terminal: Vec<_> = events
        .iter()
        .filter(|e| {
            e.get("record_type").and_then(Value::as_str) == Some("process_exit_observed")
                && e.get("process_local_id").and_then(Value::as_u64) == Some(1)
        })
        .collect();
    if v2 && terminal.len() > 1 {
        bail!("duplicate root terminal events");
    }
    if v2 && !terminal.is_empty() && authority.get(&1) != Some(&true) {
        bail!("root terminal lacks persisted root identity authority");
    }
    Ok(terminal.into_iter().next().or_else(|| {
        if v2 {
            None
        } else {
            events.iter().find(|e| {
                e.get("record_type").and_then(Value::as_str) == Some("process_exit_observed")
            })
        }
    }))
}
fn validate_manifest(bundle: &Path, summary: &Summary, v2: bool) -> Result<()> {
    let path = bundle.join("manifest.json");
    if !path.exists() {
        return Ok(());
    }
    let manifest: Value = serde_json::from_slice(&fs::read(path)?).context("parse manifest")?;
    if manifest.get("schema_draft_version").and_then(Value::as_str) == Some(V2) {
        if !v2
            || manifest.get("measurement_validity").and_then(Value::as_str)
                != Some(summary.measurement_validity)
            || manifest
                .get("measurement_completeness")
                .and_then(Value::as_str)
                != Some(summary.measurement_completeness)
        {
            bail!("completed v2 manifest disagrees with reconstructed measurement state");
        }
    }
    Ok(())
}
fn validate_present_optional_metrics(sample: &Value, rows: &[Value]) -> Result<()> {
    for metric in METRICS {
        match metric.domain {
            Domain::Process => {
                for row in rows {
                    optional_u64(row, metric.field)?;
                }
            }
            Domain::Probe => {
                if let Some(p) = sample.get("probe") {
                    optional_u64(p, metric.field)?;
                }
            }
            Domain::System => {
                if let Some(s) = sample.get("system") {
                    optional_u64(s, metric.field)?;
                }
            }
        }
    }
    Ok(())
}
fn witness(sample: &Value, field: &str, expected: Option<u64>) -> Result<()> {
    match (sample.get(field), expected) {
        (Some(value), Some(expected)) if strict_u64(value).ok() == Some(expected) => Ok(()),
        (None, None) => Ok(()),
        _ => bail!("derived witness {field} is missing, malformed, or inconsistent"),
    }
}
fn sum_required(rows: &[Value], field: &str) -> Result<u64> {
    rows.iter().try_fold(0, |sum, row| {
        checked_add(sum, required_u64(row, field)?, field)
    })
}
fn sum_optional(rows: &[Value], field: &str) -> Result<Option<u64>> {
    rows.iter()
        .try_fold(Some(0), |sum, row| match (sum, optional_u64(row, field)?) {
            (Some(sum), Some(value)) => Ok(Some(checked_add(sum, value, field)?)),
            _ => Ok(None),
        })
}
fn checked_add(a: u64, b: u64, field: &str) -> Result<u64> {
    a.checked_add(b)
        .with_context(|| format!("overflow computing {field}"))
}
fn required_u64(value: &Value, key: &str) -> Result<u64> {
    value
        .get(key)
        .context(format!("missing required unsigned counter {key}"))
        .and_then(strict_u64)
}
fn optional_u64(value: &Value, key: &str) -> Result<Option<u64>> {
    value.get(key).map(strict_u64).transpose()
}
/// This historic timing field is independently nullable; it is not an optional
/// canonical raw metric and therefore does not use V2 availability semantics.
fn nullable_gap_u64(value: &Value) -> Result<Option<u64>> {
    match value.get("gap_from_previous_sample_ns") {
        None | Some(Value::Null) => Ok(None),
        Some(value) => strict_u64(value).map(Some),
    }
}
fn strict_u64(value: &Value) -> Result<u64> {
    value.as_u64().context(
        "numeric field must be a non-negative integral u64, never null or another JSON type",
    )
}
fn required_str<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .with_context(|| format!("missing string field {key}"))
}
fn read_values(bundle: &Path, name: &str) -> Result<Vec<Value>> {
    read_complete_records(&bundle.join(name))
        .with_context(|| format!("read {name}"))?
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            serde_json::from_str(&line)
                .with_context(|| format!("parse {name} record {}", index + 1))
        })
        .collect()
}
fn read_values_if_present(bundle: &Path, name: &str) -> Result<Vec<Value>> {
    if bundle.join(name).exists() {
        read_values(bundle, name)
    } else {
        Ok(Vec::new())
    }
}
