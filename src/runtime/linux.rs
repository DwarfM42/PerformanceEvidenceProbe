//! Linux process identity primitives. This module intentionally does not collect
//! metrics or produce evidence bundles.

use std::{fs, io};

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
    let starttime = fields
        .get(19)
        .ok_or_else(|| invalid("stat record is missing starttime"))?
        .parse::<u64>()
        .map_err(|_| invalid("stat starttime is malformed"))?;
    if starttime == 0 {
        return Err(invalid("stat starttime is a sentinel value"));
    }
    Ok(starttime)
}

pub fn observe_with<F>(pid: u32, read: F) -> io::Result<ProcessIdentity>
where
    F: FnOnce(u32) -> io::Result<(String, String)>,
{
    let (boot_id, stat) = read(pid)?;
    ProcessIdentity::new(pid, &boot_id, parse_stat(pid, &stat)?)
}

pub fn read_identity(pid: u32) -> io::Result<ProcessIdentity> {
    observe_with(pid, |pid| {
        Ok((
            fs::read_to_string("/proc/sys/kernel/random/boot_id")?,
            fs::read_to_string(format!("/proc/{pid}/stat"))?,
        ))
    })
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

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
