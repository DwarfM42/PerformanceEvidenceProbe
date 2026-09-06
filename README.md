# PerformanceEvidenceProbe

[![Windows CI](https://github.com/DwarfM42/PerformanceEvidenceProbe/actions/workflows/windows-ci.yml/badge.svg?branch=main)](https://github.com/DwarfM42/PerformanceEvidenceProbe/actions/workflows/windows-ci.yml)
[![Linux CI](https://github.com/DwarfM42/PerformanceEvidenceProbe/actions/workflows/linux-ci.yml/badge.svg?branch=main)](https://github.com/DwarfM42/PerformanceEvidenceProbe/actions/workflows/linux-ci.yml)
[![macOS CI](https://github.com/DwarfM42/PerformanceEvidenceProbe/actions/workflows/macos-ci.yml/badge.svg?branch=main)](https://github.com/DwarfM42/PerformanceEvidenceProbe/actions/workflows/macos-ci.yml)

PerformanceEvidenceProbe lets you inspect runtime performance without instrumenting your application. It supports Windows, Linux, and macOS.

Run a command or attach to a process, keep the raw observations, and reconstruct a deterministic summary later. For counters covered by the availability contract, an observed zero is distinct from unavailable evidence, and semantically different platform measurements are not substituted. See the [platform limits](#cross-platform-behavior), including the current Linux/macOS timing limitations.

## What it does

- Creates a unique evidence bundle for a launched command or an observed PID.
- Preserves raw process identities, samples, lifecycle events, context metadata, and a derived `summary.json`.
- Uses a full-accounting Windows collector and intentionally narrower qualified collectors on Linux and macOS.
- Records process/run-level evidence to support performance analysis and benchmarking, without providing call-stack profiling, automatic diagnosis, or benchmark certification.

## Quick start

First [download and verify your release binary](#installation). Open a terminal in the directory containing it; these examples keep the original download filename and use harmless, short-lived commands.

### Windows PowerShell

```powershell
$probe = '.\perf-probe-windows-x86_64.exe'

# Run a command, then rebuild its summary from the saved observations.
$bundle = & $probe run --output .\evidence -- powershell.exe -NoProfile -Command "Start-Sleep -Seconds 2"
& $probe summarize --bundle $bundle

# Start a separate process so attach has a real, live PID to observe.
$target = Start-Process powershell.exe -ArgumentList '-NoProfile', '-Command', 'Start-Sleep -Seconds 5' -NoNewWindow -PassThru
$attachedBundle = & $probe attach --pid $target.Id --output .\evidence
& $probe summarize --bundle $attachedBundle
```

### Linux / macOS

```sh
# Select the downloaded binary for your OS.
case "$(uname -s)" in
  Linux)  probe=./perf-probe-linux-x86_64 ;;
  Darwin) probe=./perf-probe-macos-arm64 ;;
esac

bundle="$("$probe" run --output ./evidence -- sleep 2)"
"$probe" summarize --bundle "$bundle"

# Observe a separate, short-lived process.
sleep 5 &
target_pid=$!
attached_bundle="$("$probe" attach --pid "$target_pid" --output ./evidence)"
wait "$target_pid"
"$probe" summarize --bundle "$attached_bundle"
```

`run` and `attach` print the unique bundle directory they created on successful completion. These quiet targets make capturing that path straightforward; for your own command, target stdout may also appear, so use the printed bundle path rather than treating all stdout as a path. If collection fails, inspect the error before running `summarize`. See [platform details](#platform-details) for observation duration and coverage differences.

## Installation

Download the appropriate v0.2.0 asset from the [official release](https://github.com/DwarfM42/PerformanceEvidenceProbe/releases/tag/v0.2.0). Cloning this Rust repository is for building from source, not the primary installation path.

Also download [`SHA256SUMS.txt`](https://github.com/DwarfM42/PerformanceEvidenceProbe/releases/download/v0.2.0/SHA256SUMS.txt) into the same directory. **Before first launch**, calculate the binary's SHA-256 with the command for your OS below and compare it with the entire hash on that filename's line in `SHA256SUMS.txt` (hex letter case does not matter). Do not run it if the hashes differ. Verify before renaming the binary.

### Windows x86_64

Download [`perf-probe-windows-x86_64.exe`](https://github.com/DwarfM42/PerformanceEvidenceProbe/releases/download/v0.2.0/perf-probe-windows-x86_64.exe). In PowerShell:

```powershell
Get-FileHash -Algorithm SHA256 .\perf-probe-windows-x86_64.exe
Select-String -Path .\SHA256SUMS.txt -Pattern ' \*perf-probe-windows-x86_64\.exe$'
```

The executable is currently **unsigned**. SmartScreen may show "Windows protected your PC." After verifying the official release and checksum, inspect the prompt; use **More info → Run anyway** only if you intentionally downloaded and trust this official artifact. Do not disable SmartScreen globally. If policy prevents an exception, consult your administrator rather than bypassing it.

### Linux x86_64

Download [`perf-probe-linux-x86_64`](https://github.com/DwarfM42/PerformanceEvidenceProbe/releases/download/v0.2.0/perf-probe-linux-x86_64). Calculate its checksum:

```sh
sha256sum perf-probe-linux-x86_64
grep ' \*perf-probe-linux-x86_64$' SHA256SUMS.txt
```

After the hashes match, make the binary executable:

```sh
chmod +x perf-probe-linux-x86_64
```

### macOS Apple Silicon

Download [`perf-probe-macos-arm64`](https://github.com/DwarfM42/PerformanceEvidenceProbe/releases/download/v0.2.0/perf-probe-macos-arm64). Calculate its checksum:

```sh
shasum -a 256 perf-probe-macos-arm64
grep ' \*perf-probe-macos-arm64$' SHA256SUMS.txt
```

After the hashes match, make the binary executable:

```sh
chmod +x perf-probe-macos-arm64
```

This release is **not Developer ID-signed or notarized by Apple**. If Gatekeeper blocks first launch after you downloaded the official artifact and verified its checksum, follow [Apple's per-app approval guidance](https://support.apple.com/en-us/102445): **System Settings → Privacy & Security → Open Anyway**, then retry the command. Use an exception only if you trust the artifact; do not disable Gatekeeper globally or ignore a malware warning.

### Optional PATH setup and verification limits

After trying the examples, you may rename the binary to `perf-probe` (`perf-probe.exe` on Windows) and move it into a directory already on `PATH`; adjust the examples if you do.

The published checksums verify artifact bytes against the release inventory. They do **not** prove how the binary was built or that it is safe. Cryptographic build provenance / GitHub artifact attestation is not established for v0.2.0; it remains a future hardening item, not a current verification claim.

## Basic usage

`--output` names the parent directory; `run` and `attach` each create a unique bundle beneath it. `run` uses `--` to separate Probe options from the target command. Default `attach` observes the specified PID without taking ownership of it. `--attach-job` is intentionally unsupported in this release and fails closed; attach does not establish authority over, or isolation of, the target.

With `$probe` set as in Quick start, inspect the full options in PowerShell:

```powershell
& $probe --help
& $probe run --help
& $probe attach --help
& $probe summarize --help
```

Or in a POSIX shell:

```sh
"$probe" --help
"$probe" run --help
"$probe" attach --help
"$probe" summarize --help
```

## What you get

A completed bundle normally contains raw `processes.ndjson`, `samples.ndjson`, and `events.ndjson`; context files including `manifest.json`; and a derived `summary.json`.

- **Raw observations** are the primary record.
- **Events** explain lifecycle, retention, degradation, and terminal-observation outcomes.
- **Manifest and context** identify the run, host, target, configuration, capabilities, and completed artifacts.
- **Summary** is deterministic derived output that can be regenerated from the raw evidence.
- **Validity and completeness** distinguish trustworthy bounded evidence from degraded, declared-partial, invalid, or unfinished output.

## Cross-platform behavior

For optional process, collector, and system counters covered by the availability contract, observed zero remains zero, unavailable values are explicit, and semantically different measurements are not silently substituted. This guarantee does not cover every numeric field in the draft format.

**Qualified scope:** Windows x86_64 provides full-accounting `run` and observation-only `attach`. Linux x86_64 and macOS arm64 provide bounded direct-root `run` and observation-only single-root `attach`. Linux and macOS do not claim Job accounting, containment, descendant or process-tree closure, complete process-set totals, Windows-equivalent memory/handle/I/O/host metrics, or synthetic signal exit codes. Their omissions are typed evidence, not numeric zero.

**Linux/macOS timing limitation in v0.2.0:** raw monotonic/scheduled time and sampling-delay fields are fixed at `0`, and sample gaps are absent. The resulting summary `elapsed_ns` and `max_sample_gap_exact_ns` values are not measured duration or gap evidence. Do not interpret those zeros as observed zero time or use them for timing comparisons.

GitHub-hosted Windows, Linux, and macOS CI verifies canonical repository checks. A green hosted run is not itself a real-machine runtime qualification claim.

## Evidence semantics

The raw evidence streams are the primary record:

- `processes.ndjson` — observed process identities and acquisition outcomes.
- `samples.ndjson` — timestamped raw cumulative process counters, process-set sums, optional Job accounting, system samples, and collector samples.
- `events.ndjson` — lifecycle, retention/degradation, and terminal-counter events.

`summary.json` is derived output. `perf-probe summarize --bundle <bundle>` reconstructs it from the saved raw streams with deterministic serialization for identical complete input. It is a convenience summary, not a second measurement authority. See the [evidence schema](docs/EVIDENCE-SCHEMA-DRAFT.md) for bundle metadata, recovery rules, and field semantics.

Raw evidence records what the collector observed. It does **not** prove that the workload is correct, representative, complete, or suitable for a particular performance claim. A sampled peak is not an operating-system lifetime peak; a process-set working-set sum is not unique physical memory; and later analysis or qualification remains the consumer's responsibility.

## Dashboard-independent collection

The core collector is dashboard-independent: canonical raw evidence comes first, and collection does not require a visualization layer.

```text
observation → canonical machine-readable evidence → deterministic Probe-derived summary → optional downstream view
```

The raw streams and context metadata are canonical Probe evidence. `summary.json` is Probe-derived output reconstructed from those saved records. Scripts, `jq`, spreadsheets, data-analysis and visualization systems, or AI assistants can consume the bundle to make tables, charts, explanations, and diagnoses. Those outputs are downstream views: a chart, an AI interpretation, or a performance conclusion is not canonical Probe evidence.

This boundary permits an optional official frontend or third-party dashboard without changing the evidence authority model. No dashboard is included or promised by this release.

## Platform details

### Windows

Launch mode creates the target suspended, assigns it to a Windows Job with zero limit flags, verifies membership, then resumes it. The Job is used for containment observation and accounting, not performance control. In particular, the collector does not enable `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`; closing or crashing the collector is not configured to terminate the target.

On Windows, default attach retains an observation handle for the specified PID and waits for it to exit. It does not create a Probe Job or assign the target to one.

On Windows, `run --max-retained-process-handles` bounds process-handle retention (default `4096`, must be positive). If the bound prevents retention, the bundle records explicit degradation rather than inventing terminal measurements. This is not a cross-platform descendant-collection setting.

### Linux

The Linux x86_64 collector observes only a directly owned `run` root or one attached root. It does not claim a Job analogue, descendant accounting, process-tree closure, complete process-set totals, or Windows-equivalent memory, handle, I/O, or host metrics.

Linux `run` samples the root at nominal 500 ms intervals and waits for the directly owned root to exit. Linux `attach` captures a single bounded observation and returns without waiting for target exit; it is not a continuous monitor.

### macOS

The macOS arm64 collector has the same bounded direct-root and single-root observation boundary. It does not treat `phys_footprint`, file descriptors, or platform counters as substitutes for Windows private bytes, handles, I/O, or commit metrics.

In v0.2.0, macOS captures a single live sample rather than a periodic time series. `run` then waits for its directly owned root's terminal outcome; `attach` returns after the bounded observation and does not wait for target exit. Do not infer whole-run peaks or totals from that sample.

## Guarantees and non-goals

The collector has a bounded, single-owner NDJSON writer. Each completed record is a UTF-8 JSON line and is flushed after writing. Readers may discard only an incomplete final EOF fragment; malformed interior records make the stream invalid.

PerformanceEvidenceProbe is **not**:

- a correctness proof, benchmark certification authority, or workload-qualification framework;
- a sampling/call-stack profiler, debugger, performance-tuning tool, or automatic diagnosis engine;
- a claim that measured software is correct or a workload is representative;
- an OS lifetime-peak monitor;
- EvidenceRegistry or a completed EvidenceRegistry integration.

## Limitations and privacy

The schema is a draft, not a frozen interchange contract. Windows x86_64 has the documented full-accounting scope; **both Linux x86_64 and macOS arm64** have the narrower direct-root / observation-only scope above. Those are deliberate evidence boundaries, not equivalent full-accounting implementations. Other OS/architecture combinations are unqualified. Advanced sensors, calibration, and performance certification are outside the current scope. Windows process-tree observation can be incomplete because of races, access restrictions, and bounded retained handles; Linux and macOS do not claim descendant discovery. An interrupted collector can leave parseable raw streams without a completed summary or metadata set.

Evidence bundles can include timestamps, PIDs, executable paths where available, and host OS/hardware characteristics. Linux and macOS `run` also save launch arguments in `platform.json`; do not put secrets in those arguments. Review a bundle before sharing it. Full current limits are in [Known limitations](docs/KNOWN-LIMITATIONS.md).

## Build from source

The repository vendors its Rust dependencies in [`.vendor/`](.vendor/), configured by [`.cargo/config.toml`](.cargo/config.toml), and selects its toolchain through `rust-toolchain.toml`. Install Rust via [rustup](https://rustup.rs/) and the native build tools for your OS (Windows: Visual Studio Build Tools with Desktop development with C++; Linux: a C compiler/linker; macOS: Xcode Command Line Tools). On Windows, run the following in **Git Bash** with those build tools available. On Linux/macOS, use Bash. From the repository root:

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

PerformanceEvidenceProbe is licensed under either [Apache License, Version 2.0](LICENSE-APACHE) or [MIT License](LICENSE-MIT), at your option. Third-party dependencies retain their own licenses; see [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md) for notices and redistribution guidance. The [v0.2.0 release](https://github.com/DwarfM42/PerformanceEvidenceProbe/releases/tag/v0.2.0) includes both project license files and the third-party notices. See also the [contribution policy](CONTRIBUTING.md) and the **historical v0.1.0** [license/provenance audit](docs/LICENSE-AUDIT-2026-09-04.md).

## Sources

[1] https://raw.githubusercontent.com/HSF/prmon/main/README.md — HSF prmon README
[5] https://raw.githubusercontent.com/sloev/metrace/master/README.md — Metrace README
[7] https://raw.githubusercontent.com/sharkdp/hyperfine/master/README.md — hyperfine README
[10] https://raw.githubusercontent.com/phoronix-test-suite/phoronix-test-suite/master/README.md — Phoronix Test Suite README
[11] https://learn.microsoft.com/en-us/windows-hardware/test/wpt/windows-performance-recorder — Windows Performance Recorder documentation
[12] https://raw.githubusercontent.com/astrofrog/psrecord/main/README.rst — psrecord README
[13] https://raw.githubusercontent.com/smarr/ReBench/master/README.md — ReBench README
