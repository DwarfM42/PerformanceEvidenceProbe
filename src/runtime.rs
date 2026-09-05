//! Runtime command boundary. Native collection remains isolated by platform.

use std::path::Path;

use anyhow::Result;
#[cfg(all(not(windows), not(target_os = "linux"), not(target_os = "macos")))]
use anyhow::bail;

#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
#[allow(dead_code)]
pub mod linux;

#[cfg(target_os = "macos")]
#[allow(dead_code)]
pub mod macos;

pub fn run(
    output_root: &Path,
    max_retained_process_handles: usize,
    command: &[String],
) -> Result<()> {
    #[cfg(windows)]
    {
        return windows::run(output_root, max_retained_process_handles, command);
    }
    #[cfg(target_os = "linux")]
    {
        return linux::run(output_root, max_retained_process_handles, command);
    }
    #[cfg(target_os = "macos")]
    {
        return macos::run(output_root, max_retained_process_handles, command);
    }
    #[cfg(all(not(windows), not(target_os = "linux"), not(target_os = "macos")))]
    {
        let _ = (output_root, max_retained_process_handles, command);
        bail!("perf-probe run requires Windows, Linux, or macOS")
    }
}

pub fn attach(output_root: &Path, pid: u32, attach_job: bool) -> Result<()> {
    #[cfg(windows)]
    {
        return windows::attach(output_root, pid, attach_job);
    }
    #[cfg(target_os = "linux")]
    {
        return linux::attach(output_root, pid, attach_job);
    }
    #[cfg(target_os = "macos")]
    {
        return macos::attach(output_root, pid, attach_job);
    }
    #[cfg(all(not(windows), not(target_os = "linux"), not(target_os = "macos")))]
    {
        let _ = (output_root, pid, attach_job);
        bail!("perf-probe attach requires Windows, Linux, or macOS")
    }
}
