use std::{fs, process::Command};

#[test]
fn summarize_command_reconstructs_a_saved_bundle() {
    let directory = tempfile::tempdir().expect("temporary bundle");
    fs::write(
        directory.path().join("samples.ndjson"),
        concat!(
            "{\"record_type\":\"sample\",\"wall_time_utc\":\"2026-08-25T00:00:00Z\",\"monotonic_ns\":1,\"scheduled_monotonic_ns\":1,\"sampling_delay_ns\":0,\"processes\":[],\"probe\":{\"handle_count\":0}}\n",
            "{\"record_type\":\"sample\",\"wall_time_utc\":\"2026-08-25T00:00:00.500Z\",\"monotonic_ns\":500000001,\"scheduled_monotonic_ns\":500000001,\"sampling_delay_ns\":0,\"processes\":[],\"probe\":{\"handle_count\":0}}\n"
        ),
    )
    .expect("samples");

    let result = Command::new(env!("CARGO_BIN_EXE_perf-probe"))
        .args(["summarize", "--bundle"])
        .arg(directory.path())
        .output()
        .expect("run perf-probe summarize");

    assert!(
        result.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(directory.path().join("summary.json").is_file());
}

#[test]
fn help_exposes_run_attach_and_summarize() {
    let result = Command::new(env!("CARGO_BIN_EXE_perf-probe"))
        .arg("--help")
        .output()
        .expect("run perf-probe help");
    assert!(result.status.success());
    let stdout = String::from_utf8(result.stdout).expect("UTF-8 help");
    for command in ["run", "attach", "summarize"] {
        assert!(
            stdout.contains(command),
            "missing {command} command: {stdout}"
        );
    }
}
