# PerformanceEvidenceProbe

[![Windows CI](https://github.com/DwarfM42/PerformanceEvidenceProbe/actions/workflows/windows-ci.yml/badge.svg?branch=main)](https://github.com/DwarfM42/PerformanceEvidenceProbe/actions/workflows/windows-ci.yml)
[![macOS CI](https://github.com/DwarfM42/PerformanceEvidenceProbe/actions/workflows/macos-ci.yml/badge.svg?branch=main)](https://github.com/DwarfM42/PerformanceEvidenceProbe/actions/workflows/macos-ci.yml)

**PerformanceEvidenceProbe is a runtime performance evidence collector with a full-accounting Windows collector and a deliberately narrower macOS collector. It preserves inspectable raw measurements for later analysis.**

Use it when you need a durable record of what a workload did while it ran. The Windows collector records sampled process memory, CPU and I/O counters, process identities, Windows Job accounting for launched workloads, lifecycle events, and a derived summary. macOS records only the narrower capabilities stated below. It is designed to preserve observations, not to decide what those observations mean.

**Current qualification:** Windows remains the qualified full-accounting collector. macOS has a bounded direct-root `run` and observation-only single-root `attach` collector. Deterministic macOS tests cover composite identity validation and loss/PID-reuse handling, raw-only attach-loss artifacts, per-sample target/probe metric-unavailable bindings, argv limits, and terminal encoding; host-runtime tests cover self-attach plus controlled direct-root zero, nonzero, and signal outcomes. Its new GitHub-hosted macOS CI workflow has not yet run. macOS does not claim Job accounting, containment, descendant/process-tree closure, complete process-set totals, canonical working/private memory, handles, Windows-shaped I/O, or canonical host/commit metrics. These surfaces are emitted as exact semantic omissions, never numeric zero.

GitHub Actions provides additional continuous verification on GitHub-hosted Windows runners. A green CI run verifies the repository's canonical checks there; it is not itself the Windows 10/11 x64 runtime qualification claim.

Linux has its own bounded collector. macOS evidence is intentionally narrower than Windows evidence; M3 runtime evidence is host-specific and is not implied by hosted CI.

That portability is intentional in the design. Platform-specific observation is kept separate from the Probe's evidence and interpretation boundaries: a collector observes the host and produces the raw process, sample, event, and context records; deterministic summarization operates on the persisted evidence; visualization and interpretation remain downstream. A Linux or macOS collector should therefore be able to add OS-specific observation without redefining what counts as Probe evidence. This is a design goal, not yet a cross-platform compatibility guarantee.

## Quick start

Clone the repository, use the Rust toolchain selected by `rust-toolchain.toml`, and run these commands from **Git Bash** on Windows. Git Bash is the verified shell for this Windows workflow, not a product runtime requirement. Dependencies are vendored.

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

## Why no built-in dashboard?

PerformanceEvidenceProbe intentionally owns observation and deterministic evidence production, not visualization or interpretation:

```text
observation → canonical machine-readable evidence → deterministic Probe-derived summary → optional downstream view
```

The raw streams and context metadata are canonical Probe evidence. `summary.json` is Probe-derived output reconstructed from those saved records. Scripts, `jq`, spreadsheets, data-analysis and visualization systems, or AI assistants can consume the bundle to make tables, charts, explanations, and diagnoses. Those outputs are downstream views: a chart, an AI interpretation, or a performance conclusion is not canonical Probe evidence.

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

The schema is a draft, not a frozen interchange contract. The Windows full-accounting collector is qualified on Windows x64; macOS support is limited to the narrower qualification stated above. Advanced sensors, other platform collectors, calibration, and independent qualification are out of scope. Process-tree observation can be incomplete because of races, access restrictions, and bounded retained handles. An interrupted collector can leave parseable raw streams without a completed summary or metadata set.

Evidence bundles can include timestamps, PIDs, executable paths where available, and host OS/hardware characteristics. Review a bundle before sharing it. Full current limits are in [Known limitations](docs/KNOWN-LIMITATIONS.md).

## Build and test

```bash
bash scripts/cargo-local.sh fmt --check
bash scripts/cargo-local.sh test --all-targets --locked -- --nocapture
bash scripts/cargo-local.sh build --release --locked
```

Windows CI runs the complete canonical test suite on every pull request and `main` push. The v0.1.0 preparation baseline enumerated 33 tests; see the [public-claim audit](docs/PUBLIC-CLAIM-AUDIT-2026-09-04.md) for that reconciled historical count. `scripts/cargo-local.sh` is the canonical repository-verification wrapper: it keeps `CARGO_HOME`, build artifacts, and temporary files in ignored directories beneath the checkout, so verification does not silently depend on or modify unrelated machine-global Cargo state. Cargo itself remains ordinary Rust tooling; the wrapper is a repository verification convention. The release executable is `target/release/perf-probe.exe`.

## Documentation

Start with the [documentation index](docs/README.md). It identifies the current user-facing schema and limitations documents, the retained technical design material, and historical closeout record.

## Related tools / how this differs

PerformanceEvidenceProbe does not invent process monitoring, sampling, performance counters, or benchmarking. [prmon](https://github.com/HSF/prmon) is one of the closest existing tools: it monitors resource consumption for a process and its children. [1] [psrecord](https://github.com/astrofrog/psrecord) records CPU and memory activity for a process, and [Metrace](https://github.com/sloev/metrace) collects CPU/memory metrics for process trees and produces plot-oriented output. [12] [5]

[Windows Performance Recorder](https://learn.microsoft.com/en-us/windows-hardware/test/wpt/windows-performance-recorder) is ETW-based recording infrastructure. [11] [hyperfine](https://github.com/sharkdp/hyperfine), [ReBench](https://github.com/smarr/ReBench), and the [Phoronix Test Suite](https://github.com/phoronix-test-suite/phoronix-test-suite) instead center repeated command timing, reproducible benchmark experiments, or test/benchmark execution and reporting. [7] [13] [10]

This collector's narrower responsibility is an inspectable, bounded evidence bundle: explicitly bound workload observation, lifecycle and degradation events, raw samples, host/target/config/capability metadata, and a deterministically derived summary. It does not replace a profiler, tracing system, or benchmark framework. These projects are comparison references only; no code or documentation from them is included here.

## Software and evidence licensing boundary

The project licenses govern PerformanceEvidenceProbe itself. Running the Probe against another program does not, merely by that act, apply these licenses to that program. Evidence artifacts can contain material supplied by or about the target program; users remain responsible for rights, privacy, and redistribution decisions for captured content and generated bundles.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT License](LICENSE-MIT), at your option. See the [contribution policy](CONTRIBUTING.md), [third-party notices](THIRD-PARTY-NOTICES.md), and [license/provenance audit](docs/LICENSE-AUDIT-2026-09-04.md).

## Sources

[1] https://raw.githubusercontent.com/HSF/prmon/main/README.md — HSF prmon README
[5] https://raw.githubusercontent.com/sloev/metrace/master/README.md — Metrace README
[7] https://raw.githubusercontent.com/sharkdp/hyperfine/master/README.md — hyperfine README
[10] https://raw.githubusercontent.com/phoronix-test-suite/phoronix-test-suite/master/README.md — Phoronix Test Suite README
[11] https://learn.microsoft.com/en-us/windows-hardware/test/wpt/windows-performance-recorder — Windows Performance Recorder documentation
[12] https://raw.githubusercontent.com/astrofrog/psrecord/main/README.rst — psrecord README
[13] https://raw.githubusercontent.com/smarr/ReBench/master/README.md — ReBench README
