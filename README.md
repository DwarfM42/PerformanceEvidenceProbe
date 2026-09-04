# PerformanceEvidenceProbe

**PerformanceEvidenceProbe is a Windows runtime performance evidence collector that observes a workload and preserves inspectable raw measurements for later analysis.**

Use it when you need a durable record of what a workload did while it ran: sampled process memory, CPU and I/O counters; process identities; Windows Job accounting for launched workloads; lifecycle events; and a derived summary. It is designed to preserve observations, not to decide what those observations mean.

**Current support:** Windows 10/11 x64. `run` and `attach` require Windows; on other platforms they fail explicitly. Build and use the current release with Rust and Git Bash.

## Quick start

Clone the repository, install a current stable Rust toolchain, and run these commands from **Git Bash** on Windows. Dependencies are vendored; `scripts/cargo-local.sh` keeps Cargo, build, and temporary state under the checkout.

```bash
bash scripts/cargo-local.sh build --release --locked

rm -rf ./tmp/quickstart-evidence
bundle="$(bash scripts/cargo-local.sh run --release --bin perf-probe -- run --output ./tmp/quickstart-evidence -- cmd.exe /c "ping -n 3 127.0.0.1 > nul")"
printf 'Evidence bundle: %s\n' "$bundle"

find "$bundle" -maxdepth 1 -type f -printf '%f\n' | sort
sed -n '1,80p' "$bundle/summary.json"

# Rebuild the derived summary from the saved raw evidence.
bash scripts/cargo-local.sh run --release --bin perf-probe -- summarize --bundle "$bundle"
```

The launch command prints the unique bundle path it created. The example runs only the local Windows `ping` command for a few seconds; it makes no network request. The expected completed bundle contains `manifest.json`, `host.json`, `target.json`, `config.json`, `capabilities.json`, three NDJSON streams, and `summary.json`.

## What it does

- Launches a command under an accounting-only, non-destructive Windows Job, or observes an existing PID without assigning it to a Job.
- Samples at a nominal 500 ms cadence and preserves scheduled and observed monotonic timing.
- Records raw process identity, process/system/probe samples, lifecycle events, and Job accounting when launch mode makes it applicable.
- Creates a deterministic `summary.json` from the persisted raw evidence after a completed run.

`--output` names the parent directory; the collector creates one unique bundle beneath it. Use `--` before the target executable:

```bash
bash scripts/cargo-local.sh run --release --bin perf-probe -- run --output ./evidence-output -- cmd.exe /c "your-command-here"
```

`--max-retained-process-handles` is bounded (default `4096`). If the bound prevents retention, the bundle records explicit degradation rather than inventing missing terminal measurements.

## Evidence and interpretation

The raw evidence streams are the primary record:

- `processes.ndjson` — observed process identities and acquisition outcomes.
- `samples.ndjson` — timestamped raw cumulative process counters, process-set sums, optional Job accounting, system samples, and collector samples.
- `events.ndjson` — lifecycle, retention/degradation, and terminal-counter events.

`summary.json` is derived output. `perf-probe summarize --bundle <bundle>` reconstructs it from the saved raw streams with deterministic serialization for identical complete input. It is a convenience summary, not a second measurement authority. See the [evidence schema](docs/EVIDENCE-SCHEMA-DRAFT.md) for bundle metadata, recovery rules, and field semantics.

Raw evidence records what the collector observed. It does **not** prove that the workload is correct, representative, complete, or suitable for a particular performance claim. A sampled peak is not an operating-system lifetime peak; a process-set working-set sum is not unique physical memory; and later analysis or qualification remains the consumer's responsibility.

## Runtime safety boundary

### Launch

Launch mode creates the target suspended, assigns it to a Windows Job with zero limit flags, verifies membership, then resumes it. The Job is used for containment observation and accounting, not performance control. In particular, the collector does not enable `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`; closing or crashing the collector is not configured to terminate the target.

### Attach

Default attach opens an observation handle for the specified PID and waits for it to exit. It does not create a Probe Job or assign the target to one:

```bash
bash scripts/cargo-local.sh run --release --bin perf-probe -- attach --pid 12345 --output ./evidence-output
```

`--attach-job` is intentionally unsupported in this release and fails closed. Do not use attach as a claim of authority over, or isolation of, the target process.

## Guarantees and non-goals

The collector has a bounded, single-owner NDJSON writer. Each completed record is a UTF-8 JSON line and is flushed after writing. Readers may discard only an incomplete final EOF fragment; malformed interior records make the stream invalid.

PerformanceEvidenceProbe is **not**:

- a correctness proof, benchmark certification authority, or workload-qualification framework;
- a profiler, debugger, performance-tuning tool, or automatic diagnosis engine;
- a claim that measured software is correct or a workload is representative;
- an OS lifetime-peak monitor;
- EvidenceRegistry or a completed EvidenceRegistry integration.

## Limitations and privacy

The schema is a draft, not a frozen interchange contract. Collection is currently Windows x64 only; advanced sensors, non-Windows collectors, calibration, and independent qualification are out of scope. Process-tree observation can be incomplete because of races, access restrictions, and bounded retained handles. An interrupted collector can leave parseable raw streams without a completed summary or metadata set.

Evidence bundles can include timestamps, PIDs, executable paths where available, and host OS/hardware characteristics. Review a bundle before sharing it. Full current limits are in [Known limitations](docs/KNOWN-LIMITATIONS.md).

## Build and test

```bash
bash scripts/cargo-local.sh fmt --check
bash scripts/cargo-local.sh test --all-targets --locked -- --nocapture
bash scripts/cargo-local.sh build --release --locked
```

The wrapper writes `cargo-home/`, `target/`, and `tmp/` beneath the repository; all are ignored by Git. The release executable is `target/release/perf-probe.exe`. Do not invoke Cargo directly from a shell whose Cargo or temporary-directory environment variables redirect state outside the checkout.

## Documentation

Start with the [documentation index](docs/README.md). It identifies the current user-facing schema and limitations documents, the retained technical design material, and historical closeout record.

## License

[MIT](LICENSE).
