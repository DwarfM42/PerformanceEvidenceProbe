#![cfg(target_os = "linux")]

#[path = "../src/runtime/linux.rs"]
mod linux;

use std::io;

use linux::{
    IdentityComparison, ProcessIdentity, compare_identity, observe_with, parse_boot_id, parse_stat,
};

fn stat(pid: u32, comm: &str, starttime: u64) -> String {
    format!("{pid} ({comm}) S 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 {starttime} 0")
}

const BOOT_A: &str = "01234567-89ab-cdef-0123-456789abcdef";
const BOOT_B: &str = "fedcba98-7654-3210-fedc-ba9876543210";

#[test]
fn parses_pid_and_starttime_from_structurally_delimited_stat() {
    for comm in ["ordinary", "has spaces", "has ) and ( parentheses"] {
        assert_eq!(parse_stat(42, &stat(42, comm, 99)).unwrap(), 99, "{comm}");
    }
}

#[test]
fn rejects_invalid_or_incomplete_stat_records() {
    for record in [
        "",
        "42 (ordinary S 0 0",
        "41 (ordinary) S 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 99 0",
        "42 ordinary) S 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 99 0",
        "42 (ordinary) S 0 0 0",
        "42 (ordinary) S 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 nope 0",
    ] {
        assert!(parse_stat(42, record).is_err(), "{record:?}");
    }
}

#[test]
fn parses_only_canonical_nonempty_boot_ids() {
    assert_eq!(parse_boot_id(BOOT_A).unwrap(), BOOT_A);
    for boot in [
        "",
        "not-a-boot-id",
        "01234567-89ab-cdef-0123-456789abcdeg",
        "0123456789ab-cdef-0123-456789abcdef",
    ] {
        assert!(parse_boot_id(boot).is_err(), "{boot:?}");
    }
}

#[test]
fn compares_identity_without_collapsing_unavailable_into_different() {
    let same = ProcessIdentity::new(42, BOOT_A, 99).unwrap();
    assert_eq!(
        compare_identity(&same, Ok(same.clone())),
        IdentityComparison::SameInstance
    );
    assert_eq!(
        compare_identity(&same, ProcessIdentity::new(42, BOOT_A, 100)),
        IdentityComparison::DifferentInstance
    );
    assert_eq!(
        compare_identity(&same, ProcessIdentity::new(42, BOOT_B, 99)),
        IdentityComparison::DifferentInstance
    );
    assert_eq!(
        compare_identity(&same, Err(io::Error::new(io::ErrorKind::NotFound, "gone"))),
        IdentityComparison::Unavailable
    );
}

#[test]
fn lifecycle_reader_reports_disappearance_and_changed_starttime_deterministically() {
    let initial = observe_with(42, |_| Ok((BOOT_A.to_owned(), stat(42, "alive", 99)))).unwrap();
    let disappeared = observe_with(42, |_| Err(io::Error::new(io::ErrorKind::NotFound, "gone")));
    assert_eq!(
        compare_identity(&initial, disappeared),
        IdentityComparison::Unavailable
    );

    let changed = observe_with(42, |_| Ok((BOOT_A.to_owned(), stat(42, "reused", 100))));
    assert_eq!(
        compare_identity(&initial, changed),
        IdentityComparison::DifferentInstance
    );
}

#[test]
fn reads_current_process_identity_twice_while_alive() {
    let pid = std::process::id();
    let first = linux::read_identity(pid).unwrap();
    let second = linux::read_identity(pid).unwrap();

    println!(
        "boot_id={} pid={} starttime={}",
        first.boot_id, first.pid, first.starttime
    );
    assert_eq!(first, second);
}
