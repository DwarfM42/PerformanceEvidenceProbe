#![cfg(target_os = "linux")]

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use serde_json::Value;

struct KnownFixtureChildren(Vec<libc::pid_t>);

impl Drop for KnownFixtureChildren {
    fn drop(&mut self) {
        for &pid in &self.0 {
            unsafe {
                let _ = libc::kill(pid, libc::SIGKILL);
            }
        }
    }
}

fn only_bundle(root: &Path) -> PathBuf {
    let entries = fs::read_dir(root)
        .expect("read output root")
        .map(|entry| entry.expect("directory entry").path())
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 1, "expected exactly one evidence bundle");
    entries.into_iter().next().expect("bundle")
}

fn run_fixture(mode: &str, child_must_remain_live: bool, escaped_session: bool) {
    let output = tempfile::tempdir().expect("temporary output");
    let fixture = tempfile::tempdir().expect("temporary fixture state");
    let report = fixture.path().join("known children");
    let mut collector = Command::new(env!("CARGO_BIN_EXE_perf-probe"));
    collector
        .args(["run", "--output"])
        .arg(output.path())
        .args(["--", env!("CARGO_BIN_EXE_perf-workload"), mode])
        .arg(&report);
    if child_must_remain_live {
        let status = collector
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("run perf-probe");
        assert!(status.success(), "collector status: {status}");
    } else {
        let result = collector.output().expect("run perf-probe");
        assert!(
            result.status.success(),
            "collector stderr: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }

    let bundle = only_bundle(output.path());
    let process: Value = fs::read_to_string(bundle.join("processes.ndjson"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).expect("root process JSON"))
        .next()
        .expect("root process evidence");
    let sample: Value = fs::read_to_string(bundle.join("samples.ndjson"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).expect("root sample JSON"))
        .next()
        .expect("root sample evidence");
    let summary: Value =
        serde_json::from_slice(&fs::read(bundle.join("summary.json")).unwrap()).expect("summary");
    let manifest: Value =
        serde_json::from_slice(&fs::read(bundle.join("manifest.json")).unwrap()).expect("manifest");
    let platform: Value =
        serde_json::from_slice(&fs::read(bundle.join("platform.json")).unwrap()).expect("platform");
    let report = fs::read_to_string(report).expect("fixture child report");
    let rows = report.lines().collect::<Vec<_>>();
    assert!(
        !rows.is_empty(),
        "fixture must report purpose-built children"
    );
    let children = KnownFixtureChildren(
        rows.iter()
            .map(|row| row.split_whitespace().next().unwrap().parse().unwrap())
            .collect(),
    );

    assert_eq!(process["process_local_id"], 1);
    assert_eq!(platform["root_process_identity"]["pid"], process["pid"]);
    assert_eq!(
        platform["root_process_identity"]["process_start_time"],
        process["process_start_time"]
    );
    assert_eq!(
        platform["root_process_identity"]["boot_identity"],
        process["boot_identity"]
    );
    assert_eq!(sample["processes"].as_array().unwrap().len(), 1);
    assert_eq!(sample["processes"][0]["process_local_id"], 1);
    assert_eq!(
        sample["processes"][0]["process_local_id"],
        process["process_local_id"]
    );
    assert_eq!(sample["processes"][0]["process_local_id"], 1);
    assert!(sample.get("process_set_working_set_sum_bytes").is_none());
    assert!(sample.get("process_set_private_bytes_sum").is_none());
    assert!(sample.get("job").is_none());
    assert!(summary["total_cpu_time_ns"].is_null());
    assert_eq!(manifest["run_state"], "COMPLETE");
    assert_eq!(platform["represented_process_set"], "direct_root_only");
    assert_eq!(platform["descendant_discovery"], "not_attempted");
    assert_eq!(platform["descendant_scope"], "unknown_not_observed");
    assert_eq!(
        platform["root_exits_before_descendants_scope"],
        "unknown_not_observed"
    );
    assert_eq!(platform["process_tree_closure"], "not_claimed");
    assert_eq!(platform["job_accounting"], "not_claimed");
    assert_eq!(
        platform["process_group_session_cgroup_authority"],
        "not_claimed"
    );
    assert!(
        !fs::read_to_string(bundle.join("events.ndjson"))
            .unwrap()
            .contains("child"),
        "no child lifecycle evidence belongs in the root-only producer"
    );

    if child_must_remain_live {
        let child = children.0[0];
        assert_eq!(
            unsafe { libc::kill(child, 0) },
            0,
            "known child remains live after root exit"
        );
        assert_eq!(summary["sample_count"], 1);
        assert_eq!(sample["root_process_confirmed_live"], true);
    }
    if escaped_session {
        let fields = rows[0].split_whitespace().collect::<Vec<_>>();
        assert_eq!(fields.len(), 3, "escaped fixture reports pid/pgid/sid");
        assert_eq!(fields[0], fields[1], "child owns its process group");
        assert_eq!(fields[0], fields[2], "child owns its session");
    }
}

#[test]
fn run_with_ordinary_child_keeps_only_the_direct_root_represented() {
    run_fixture("linux-ordinary-child", false, false);
}

#[test]
fn run_with_grandchild_keeps_only_the_direct_root_represented() {
    run_fixture("linux-grandchild", false, false);
}

#[test]
fn run_after_child_exit_keeps_only_the_direct_root_represented() {
    run_fixture("linux-child-exits-first", false, false);
}

#[test]
fn run_finishes_on_root_exit_while_known_child_remains_live() {
    run_fixture("linux-root-exits-child-alive", true, false);
}

#[test]
fn run_does_not_treat_a_new_child_session_as_process_set_authority() {
    run_fixture("linux-child-new-session", false, true);
}
