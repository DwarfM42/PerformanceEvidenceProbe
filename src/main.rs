use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use perf_evidence_probe::summary::regenerate_summary;

#[derive(Debug, Parser)]
#[command(
    name = "perf-probe",
    version,
    about = "Windows-first raw performance evidence collector"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Launch a target under a non-destructive Probe Job on Windows.
    Run {
        /// Parent directory into which a unique run bundle is created.
        #[arg(long, default_value = "perf-evidence")]
        output: PathBuf,
        /// Maximum process handles retained by the Probe.
        #[arg(long, default_value_t = 4096)]
        max_retained_process_handles: usize,
        /// Command argv; use `--` before the target executable.
        #[arg(required = true, trailing_var_arg = true)]
        command: Vec<String>,
    },
    /// Observe an existing process. Default attach never assigns a Job.
    Attach {
        #[arg(long)]
        pid: u32,
        #[arg(long, default_value = "perf-evidence")]
        output: PathBuf,
        /// Explicit opt-in for a future Job attachment implementation.
        #[arg(long)]
        attach_job: bool,
    },
    /// Regenerate deterministic summary.json from saved raw evidence.
    Summarize {
        #[arg(long)]
        bundle: PathBuf,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Summarize { bundle } => regenerate_summary(&bundle),
        Command::Run {
            output,
            max_retained_process_handles,
            command,
        } => {
            if max_retained_process_handles == 0 {
                bail!("--max-retained-process-handles must be positive");
            }
            perf_evidence_probe::runtime::run(&output, max_retained_process_handles, &command)
        }
        Command::Attach {
            pid,
            output,
            attach_job,
        } => perf_evidence_probe::runtime::attach(&output, pid, attach_job),
    }
}
