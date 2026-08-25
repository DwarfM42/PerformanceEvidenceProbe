//! Runtime command boundary. Native Windows collection is deliberately isolated
//! behind this module so non-Windows binaries cannot pretend to collect data.

use std::path::Path;

use anyhow::Result;
#[cfg(not(windows))]
use anyhow::bail;

#[cfg(windows)]
mod windows;

pub fn run(
    output_root: &Path,
    max_retained_process_handles: usize,
    command: &[String],
) -> Result<()> {
    #[cfg(windows)]
    {
        return windows::run(output_root, max_retained_process_handles, command);
    }
    #[cfg(not(windows))]
    {
        let _ = (output_root, max_retained_process_handles, command);
        bail!("perf-probe run requires Windows")
    }
}

pub fn attach(output_root: &Path, pid: u32, attach_job: bool) -> Result<()> {
    #[cfg(windows)]
    {
        return windows::attach(output_root, pid, attach_job);
    }
    #[cfg(not(windows))]
    {
        let _ = (output_root, pid, attach_job);
        bail!("perf-probe attach requires Windows")
    }
}
