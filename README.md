# PerformanceEvidenceProbe

[![Windows CI](https://github.com/DwarfM42/PerformanceEvidenceProbe/actions/workflows/windows-ci.yml/badge.svg?branch=main)](https://github.com/DwarfM42/PerformanceEvidenceProbe/actions/workflows/windows-ci.yml)
[![Linux CI](https://github.com/DwarfM42/PerformanceEvidenceProbe/actions/workflows/linux-ci.yml/badge.svg?branch=main)](https://github.com/DwarfM42/PerformanceEvidenceProbe/actions/workflows/linux-ci.yml)
[![macOS CI](https://github.com/DwarfM42/PerformanceEvidenceProbe/actions/workflows/macos-ci.yml/badge.svg?branch=main)](https://github.com/DwarfM42/PerformanceEvidenceProbe/actions/workflows/macos-ci.yml)

PerformanceEvidenceProbe records inspectable runtime performance evidence for Windows, Linux, and macOS.

Run a command or attach to a process, keep the raw observations, and reconstruct a deterministic summary later. When a metric cannot be observed truthfully, it is recorded as unavailable rather than fabricated as zero or substituted with a different platform measurement.

## What it does

- Creates a unique evidence bundle for a launched command or an observed PID.
- Preserves raw process identities, samples, lifecycle events, context metadata, and a derived `summary.json`.
- Uses a full-accounting Windows collector and intentionally narrower qualified collectors on Linux and macOS.
- Keeps collection separate from profiling, diagnosis, benchmark certification, and interpretation.

## Quick start

Install a release binary, then run these commands from the directory containing it. On Windows use `./perf-probe.exe`; on Linux and macOS use `./perf-probe` after `chmod +x perf-probe`.

```bash
# Set this once for the platform-specific release binary.
probe=./perf-probe        # Linux or macOS
# probe=./perf-probe.exe  # Windows Git Bash

# Run a local command. Use -- before the target executable.
bundle="$("$probe" run --output ./evidence -- <command> [args...])"

# Observe an already-running process until it exits.
"$probe" attach --pid <PID> --output ./evidence

# Rebuild a deterministic summary from saved raw evidence.
"$probe" summarize --bundle "$bundle"
```

`run` prints the unique bundle directory it created. Replace `<command> [args...]` with a command available on your machine. The `attach` command also prints its own unique bundle path.

## Installation

Download the appropriate v0.2.0 asset from the [official release](https://github.com/DwarfM42/PerformanceEvidenceProbe/releases/tag/v0.2.0). Cloning this Rust repository is for building from source, not the primary installation path.

### Windows x86_64

Download [`perf-probe-windows-x86_64.exe`](https://github.com/DwarfM42/PerformanceEvidenceProbe/releases/download/v0.2.0/perf-probe-windows-x86_64.exe). Keep the `.exe` filename, or place it in a directory on `PATH` as `perf-probe.exe`.

### Linux x86_64

Download [`perf-probe-linux-x86_64`](https://github.com/DwarfM42/PerformanceEvidenceProbe/releases/download/v0.2.0/perf-probe-linux-x86_64), rename it to `perf-probe` if desired, and make it executable:

```bash
chmod +x perf-probe
```

Optionally move it to a directory already on `PATH`.

### macOS Apple Silicon

Download [`perf-probe-macos-arm64`](https://github.com/DwarfM42/PerformanceEvidenceProbe/releases/download/v0.2.0/perf-probe-macos-arm64), rename it to `perf-probe` if desired, and make it executable:

```bash
chmod +x perf-probe
```

Optionally move it to a directory already on `PATH`.

## Basic usage

`--output` names the parent directory; each command creates a unique bundle beneath it. `run` uses `--` to separate Probe options from the target command. Default `attach` is observation-only and never assigns a Job.

```bash
"$probe" run --output ./evidence -- <command> [args...]
"$probe" attach --pid <PID> --output ./evidence
"$probe" summarize --bundle ./evidence/<run-id>
```

`--max-retained-process-handles` is bounded (default `4096`). If that bound prevents retention, the bundle records explicit degradation rather than inventing terminal measurements.

## What you get

A completed bundle normally contains raw `processes.ndjson`, `samples.ndjson`, and `events.ndjson`; context files including `manifest.json`; and a derived `summary.json`.

- **Raw observations** are the primary record.
- **Events** explain lifecycle, retention, degradation, and terminal-observation outcomes.
- **Manifest and context** identify the run, host, target, configuration, capabilities, and completed artifacts.
- **Summary** is deterministic derived output that can be regenerated from the raw evidence.
- **Validity and completeness** distinguish trustworthy bounded evidence from degraded, declared-partial, invalid, or unfinished output.

## Cross-platform behavior

PerformanceEvidenceProbe does not invent equivalence across operating systems: observed zero remains zero; unavailable values are explicit; semantically different measurements are not silently substituted.

**Qualified scope:** Windows x86_64 provides full-accounting `run` and observation-only `attach`. Linux x86_64 and macOS arm64 provide bounded direct-root `run` and observation-only single-root `attach`. Linux and macOS do not claim Job accounting, containment, descendant or process-tree closure, complete process-set totals, Windows-equivalent memory/handle/I/O/host metrics, or synthetic signal exit codes. Their omissions are typed evidence, not numeric zero.

GitHub-hosted Windows, Linux, and macOS CI verifies canonical repository checks. A green hosted run is not itself a real-machine runtime qualification claim.

## Evidence semantics

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

## Platform details

### Windows

Launch mode creates the target suspended, assigns it to a Windows Job with zero limit flags, verifies membership, then resumes it. The Job is used for containment observation and accounting, not performance control. In particular, the collector does not enable `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`; closing or crashing the collector is not configured to terminate the target.

Default attach on every qualified platform opens an observation handle for the specified PID and waits for it to exit. It does not create a Probe Job or assign the target to one:

```bash
$probe attach --pid 12345 --output ./evidence-output
```

`--attach-job` is intentionally unsupported in this release and fails closed. Do not use attach as a claim of authority over, or isolation of, the target process.

### Linux

The Linux x86_64 collector observes only a directly owned `run` root or one attached root. It does not claim a Job analogue, descendant accounting, process-tree closure, complete process-set totals, or Windows-equivalent memory, handle, I/O, or host metrics.

### macOS

The macOS arm64 collector has the same bounded direct-root and single-root observation boundary. It does not treat `phys_footprint`, file descriptors, or platform counters as substitutes for Windows private bytes, handles, I/O, or commit metrics.

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

## Build from source

The repository vendors its Rust dependencies and selects its toolchain through `rust-toolchain.toml`. Build from a checkout only when you need a source build rather than a published v0.2.0 binary:

```bash
bash scripts/cargo-local.sh build --release --locked --bin perf-probe
```

## Testing / development

```bash
bash scripts/cargo-local.sh fmt --check
bash scripts/cargo-local.sh test --all-targets --locked -- --nocapture
bash scripts/cargo-local.sh check --all-targets --locked
```

Windows, Linux, and macOS CI run the complete canonical test suite on every pull request and `main` push; each also runs its platform-focused tests, checks all targets, builds `perf-probe` in release mode, and rejects whitespace errors. The v0.1.0 preparation baseline enumerated 33 tests; see the [public-claim audit](docs/PUBLIC-CLAIM-AUDIT-2026-09-04.md) for that reconciled historical count. `scripts/cargo-local.sh` is the canonical repository-verification wrapper: it keeps `CARGO_HOME`, build artifacts, and temporary files in ignored directories beneath the checkout, so verification does not silently depend on or modify unrelated machine-global Cargo state. Cargo itself remains ordinary Rust tooling; the wrapper is a repository verification convention. The release executable is `target/release/perf-probe` (`perf-probe.exe` on Windows).

## Detailed documentation

Start with the [documentation index](docs/README.md). It identifies the current user-facing schema and limitations documents, the retained technical design material, and historical closeout record.

## Related tools / how this differs

PerformanceEvidenceProbe does not invent process monitoring, sampling, performance counters, or benchmarking. [prmon](https://github.com/HSF/prmon) is one of the closest existing tools: it monitors resource consumption for a process and its children. [1] [psrecord](https://github.com/astrofrog/psrecord) records CPU and memory activity for a process, and [Metrace](https://github.com/sloev/metrace) collects CPU/memory metrics for process trees and produces plot-oriented output. [12] [5]

[Windows Performance Recorder](https://learn.microsoft.com/en-us/windows-hardware/test/wpt/windows-performance-recorder) is ETW-based recording infrastructure. [11] [hyperfine](https://github.com/sharkdp/hyperfine), [ReBench](https://github.com/smarr/ReBench), and the [Phoronix Test Suite](https://github.com/phoronix-test-suite/phoronix-test-suite) instead center repeated command timing, reproducible benchmark experiments, or test/benchmark execution and reporting. [7] [13] [10]

This collector's narrower responsibility is an inspectable, bounded evidence bundle: explicitly bound workload observation, lifecycle and degradation events, raw samples, host/target/config/capability metadata, and a deterministically derived summary. It does not replace a profiler, tracing system, or benchmark framework. These projects are comparison references only; no code or documentation from them is included here.

## Software and evidence licensing boundary

The project licenses govern PerformanceEvidenceProbe itself. Running the Probe against another program does not, merely by that act, apply these licenses to that program. Evidence artifacts can contain material supplied by or about the target program; users remain responsible for rights, privacy, and redistribution decisions for captured content and generated bundles.

## License / third-party notices

PerformanceEvidenceProbe is licensed under either [Apache License, Version 2.0](LICENSE-APACHE) or [MIT License](LICENSE-MIT), at your option. Third-party dependencies retain their own licenses; see [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md) for the relevant notices and redistribution guidance. See also the [contribution policy](CONTRIBUTING.md) and [license/provenance audit](docs/LICENSE-AUDIT-2026-09-04.md).

## Sources

[1] https://raw.githubusercontent.com/HSF/prmon/main/README.md — HSF prmon README
[5] https://raw.githubusercontent.com/sloev/metrace/master/README.md — Metrace README
[7] https://raw.githubusercontent.com/sharkdp/hyperfine/master/README.md — hyperfine README
[10] https://raw.githubusercontent.com/phoronix-test-suite/phoronix-test-suite/master/README.md — Phoronix Test Suite README
[11] https://learn.microsoft.com/en-us/windows-hardware/test/wpt/windows-performance-recorder — Windows Performance Recorder documentation
[12] https://raw.githubusercontent.com/astrofrog/psrecord/main/README.rst — psrecord README
[13] https://raw.githubusercontent.com/smarr/ReBench/master/README.md — ReBench README
