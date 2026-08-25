use std::{collections::BTreeSet, fs, path::Path};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::Value;

use crate::ndjson::read_complete_records;

#[derive(Debug, Serialize)]
struct Summary {
    summary_schema_draft_version: &'static str,
    elapsed_ns: u64,
    sample_count: u64,
    max_sample_gap_exact_ns: u64,
    peak_working_set_sampled_bytes: u64,
    peak_private_sampled_bytes: u64,
    last_live_working_set_sample_bytes: Option<u64>,
    last_live_working_set_sample_time: Option<String>,
    last_live_private_sample_bytes: Option<u64>,
    last_live_private_sample_time: Option<String>,
    total_cpu_time_ns: u64,
    average_cpu_utilization: Option<f64>,
    peak_cpu_utilization: Option<f64>,
    total_read_bytes: u64,
    total_write_bytes: u64,
    total_read_operations: u64,
    total_write_operations: u64,
    maximum_observed_process_count: u64,
    observed_distinct_process_count: u64,
    job_total_processes_os: Option<u64>,
    job_processes_without_observed_identity: Option<u64>,
    maximum_probe_handle_count: u64,
    handle_retention_degraded: bool,
    exit_code: Option<u32>,
    terminal_user_cpu_time_ns: Option<u64>,
    terminal_kernel_cpu_time_ns: Option<u64>,
    terminal_read_bytes: Option<u64>,
    terminal_write_bytes: Option<u64>,
    terminal_counter_fidelity: Option<String>,
    measurement_validity: &'static str,
}

/// Independently reconstructs the deterministic derived artifact strictly from
/// persisted raw evidence. This function does not receive sampler state.
pub fn regenerate_summary(bundle: &Path) -> Result<()> {
    let samples = read_values(bundle, "samples.ndjson")?;
    if samples.is_empty() {
        bail!("cannot reconstruct summary without complete sample records");
    }
    let processes = read_values_if_present(bundle, "processes.ndjson")?;
    let events = read_values_if_present(bundle, "events.ndjson")?;
    let mut observed_ids = BTreeSet::new();
    for record in &processes {
        if let Some(id) = u64_at(record, "process_local_id") {
            observed_ids.insert(id);
        }
    }

    let first_monotonic = required_u64(&samples[0], "monotonic_ns")?;
    let mut last_monotonic = first_monotonic;
    let mut max_gap = 0_u64;
    let mut peak_working_set = 0_u64;
    let mut peak_private = 0_u64;
    let mut maximum_observed_process_count = 0_u64;
    let mut maximum_probe_handle_count = 0_u64;
    let mut last_live_working_set = None;
    let mut last_live_private = None;
    let mut last_live_time = None;
    let mut final_job = None;
    let mut previous_total_cpu = None;
    let mut previous_monotonic = None;
    let mut total_cpu_from_samples = 0_u64;
    let mut cpu_utilizations = Vec::new();

    for sample in &samples {
        if sample.get("record_type").and_then(Value::as_str) != Some("sample") {
            bail!("samples.ndjson contains a non-sample record");
        }
        let monotonic = required_u64(sample, "monotonic_ns")?;
        last_monotonic = monotonic;
        let gap = u64_at(sample, "gap_from_previous_sample_ns").unwrap_or(0);
        max_gap = max_gap.max(gap);

        let process_samples = sample
            .get("processes")
            .and_then(Value::as_array)
            .context("sample missing processes array")?;
        maximum_observed_process_count =
            maximum_observed_process_count.max(process_samples.len() as u64);
        let working_set_sum = sum_process_metric(process_samples, "working_set_bytes")?;
        let private_sum = sum_process_metric(process_samples, "private_bytes")?;
        peak_working_set = peak_working_set.max(working_set_sum);
        peak_private = peak_private.max(private_sum);
        if sample
            .get("root_process_confirmed_live")
            .and_then(Value::as_bool)
            == Some(true)
        {
            last_live_working_set = Some(working_set_sum);
            last_live_private = Some(private_sum);
            last_live_time = sample
                .get("wall_time_utc")
                .and_then(Value::as_str)
                .map(str::to_owned);
        }

        let probe_handles = sample
            .get("probe")
            .and_then(|probe| u64_at(probe, "handle_count"))
            .unwrap_or(0);
        maximum_probe_handle_count = maximum_probe_handle_count.max(probe_handles);

        let sample_total_cpu = if let Some(job) = sample.get("job") {
            final_job = Some(job.clone());
            required_u64(job, "total_user_time_ns")?
                .saturating_add(required_u64(job, "total_kernel_time_ns")?)
        } else {
            process_samples.iter().try_fold(0_u64, |total, process| {
                Ok::<_, anyhow::Error>(
                    total
                        .saturating_add(required_u64(process, "user_cpu_time_ns")?)
                        .saturating_add(required_u64(process, "kernel_cpu_time_ns")?),
                )
            })?
        };
        if let (Some(previous_cpu), Some(previous_time)) = (previous_total_cpu, previous_monotonic)
        {
            let elapsed = monotonic.saturating_sub(previous_time);
            if elapsed > 0 {
                let delta = sample_total_cpu.saturating_sub(previous_cpu);
                cpu_utilizations.push(delta as f64 / elapsed as f64);
            }
        }
        previous_total_cpu = Some(sample_total_cpu);
        previous_monotonic = Some(monotonic);
        total_cpu_from_samples = sample_total_cpu;
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
            required_u64(job, "total_user_time_ns")?
                .saturating_add(required_u64(job, "total_kernel_time_ns")?),
            required_u64(job, "read_transfer_bytes")?,
            required_u64(job, "write_transfer_bytes")?,
            required_u64(job, "read_operation_count")?,
            required_u64(job, "write_operation_count")?,
            Some(required_u64(job, "total_processes_os")?),
        )
    } else {
        let final_processes = samples
            .last()
            .and_then(|sample| sample.get("processes"))
            .and_then(Value::as_array)
            .context("last sample missing processes")?;
        (
            total_cpu_from_samples,
            sum_process_metric(final_processes, "read_bytes")?,
            sum_process_metric(final_processes, "write_bytes")?,
            sum_process_metric(final_processes, "read_operations")?,
            sum_process_metric(final_processes, "write_operations")?,
            None,
        )
    };

    let terminal = events.iter().find(|event| {
        event.get("record_type").and_then(Value::as_str) == Some("process_exit_observed")
    });
    let handle_retention_degraded = events.iter().any(|event| {
        event.get("record_type").and_then(Value::as_str) == Some("collector_degradation")
            && event
                .get("handle_retention_degraded")
                .and_then(Value::as_bool)
                == Some(true)
    });

    let elapsed = last_monotonic.saturating_sub(first_monotonic);
    let average_cpu_utilization = if elapsed > 0 {
        Some(total_cpu as f64 / elapsed as f64)
    } else {
        None
    };
    let peak_cpu_utilization = cpu_utilizations.into_iter().reduce(f64::max);
    let observed_count = observed_ids.len() as u64;
    let summary = Summary {
        summary_schema_draft_version: "perf-evidence-v1-draft",
        elapsed_ns: elapsed,
        sample_count: samples.len() as u64,
        max_sample_gap_exact_ns: max_gap,
        peak_working_set_sampled_bytes: peak_working_set,
        peak_private_sampled_bytes: peak_private,
        last_live_working_set_sample_bytes: last_live_working_set,
        last_live_working_set_sample_time: last_live_time.clone(),
        last_live_private_sample_bytes: last_live_private,
        last_live_private_sample_time: last_live_time,
        total_cpu_time_ns: total_cpu,
        average_cpu_utilization,
        peak_cpu_utilization,
        total_read_bytes,
        total_write_bytes,
        total_read_operations,
        total_write_operations,
        maximum_observed_process_count,
        observed_distinct_process_count: observed_count,
        job_total_processes_os,
        job_processes_without_observed_identity: job_total_processes_os
            .map(|total| total.saturating_sub(observed_count)),
        maximum_probe_handle_count,
        handle_retention_degraded,
        exit_code: terminal
            .and_then(|event| u64_at(event, "exit_code"))
            .and_then(|value| u32::try_from(value).ok()),
        terminal_user_cpu_time_ns: terminal
            .and_then(|event| u64_at(event, "terminal_user_cpu_time_ns")),
        terminal_kernel_cpu_time_ns: terminal
            .and_then(|event| u64_at(event, "terminal_kernel_cpu_time_ns")),
        terminal_read_bytes: terminal.and_then(|event| u64_at(event, "terminal_read_bytes")),
        terminal_write_bytes: terminal.and_then(|event| u64_at(event, "terminal_write_bytes")),
        terminal_counter_fidelity: terminal
            .and_then(|event| event.get("terminal_counter_fidelity"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        measurement_validity: if handle_retention_degraded {
            "DEGRADED"
        } else {
            "VALID"
        },
    };
    let mut bytes = serde_json::to_vec_pretty(&summary)?;
    bytes.push(b'\n');
    fs::write(bundle.join("summary.json"), bytes).context("write deterministic summary")?;
    Ok(())
}

fn read_values(bundle: &Path, name: &str) -> Result<Vec<Value>> {
    let records =
        read_complete_records(&bundle.join(name)).with_context(|| format!("read {name}"))?;
    records
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            serde_json::from_str(&line)
                .with_context(|| format!("parse {name} record {}", index + 1))
        })
        .collect()
}

fn read_values_if_present(bundle: &Path, name: &str) -> Result<Vec<Value>> {
    let path = bundle.join(name);
    if path.exists() {
        read_values(bundle, name)
    } else {
        Ok(Vec::new())
    }
}

fn required_u64(value: &Value, key: &str) -> Result<u64> {
    u64_at(value, key).with_context(|| format!("missing required unsigned counter {key}"))
}

fn u64_at(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(Value::as_u64)
}

fn sum_process_metric(processes: &[Value], key: &str) -> Result<u64> {
    processes.iter().try_fold(0_u64, |sum, process| {
        Ok::<_, anyhow::Error>(sum.saturating_add(required_u64(process, key)?))
    })
}
