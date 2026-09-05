#![cfg(target_os = "macos")]

use perf_evidence_probe::runtime::macos::{
    IdentityComparison, ProcessIdentity, checked_thread_count, compare_identity,
    parse_boot_identity, validate_command, validate_start_time,
};

const BOOT_A: &str = "macos-boot-time-unix-1a-2b";
const BOOT_B: &str = "macos-boot-time-unix-1a-2c";

#[test]
fn identity_requires_nonzero_pid_start_and_validated_boot_authority() {
    assert!(ProcessIdentity::new(0, BOOT_A, 1).is_err());
    assert!(ProcessIdentity::new(1, BOOT_A, 0).is_err());
    assert!(ProcessIdentity::new(1, "", 1).is_err());
    assert!(ProcessIdentity::new(1, "not-a-boot-authority", 1).is_err());
    assert_eq!(parse_boot_identity(BOOT_A).unwrap(), BOOT_A);
    assert_eq!(validate_start_time(1, 2).unwrap(), 1_000_002);
}

#[test]
fn identity_comparison_distinguishes_loss_from_pid_reuse() {
    let identity = ProcessIdentity::new(42, BOOT_A, 99).unwrap();
    assert_eq!(
        compare_identity(&identity, Ok(identity.clone())),
        IdentityComparison::SameInstance
    );
    assert_eq!(
        compare_identity(&identity, ProcessIdentity::new(42, BOOT_A, 100)),
        IdentityComparison::DifferentInstance
    );
    assert_eq!(
        compare_identity(&identity, ProcessIdentity::new(42, BOOT_B, 99)),
        IdentityComparison::DifferentInstance
    );
    assert_eq!(
        compare_identity(&identity, Err(std::io::Error::other("gone"))),
        IdentityComparison::Unavailable
    );
}

#[test]
fn rejects_malformed_start_authority_and_checked_thread_overflow() {
    assert!(validate_start_time(0, 1).is_err());
    assert!(validate_start_time(1, 1_000_000).is_err());
    assert!(validate_start_time(u64::MAX, 1).is_err());
    assert_eq!(checked_thread_count(0).unwrap(), 0);
    assert_eq!(checked_thread_count(i32::MAX).unwrap(), i32::MAX as u32);
    assert!(checked_thread_count(-1).is_err());
}

#[test]
fn direct_run_command_has_checked_count_and_utf8_byte_bounds() {
    assert!(validate_command(&["/bin/echo".into(), "ok".into()]).is_ok());
    assert!(validate_command(&[]).is_err());
    let too_many = vec!["x".to_owned(); 129];
    assert!(validate_command(&too_many).is_err());
    let too_large = vec!["x".repeat(32 * 1024 + 1)];
    assert!(validate_command(&too_large).is_err());
}
