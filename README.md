# Performance Evidence Probe

Windows-first Rust collector for preserving raw performance evidence from a process. `perf-probe` can launch a target under an accounting-only Windows Job, observe an existing PID without assigning it to a Job, and deterministically rebuild a summary from the persisted raw streams.

## Scope

- **Platform:** Windows 10/11 x64 for `run` and `attach`.
- **Sampling:** 500 ms nominal interval with scheduled and observed monotonic timing retained in the raw evidence.
- **Evidence:** process identities, process and system samples, Job accounting when applicable, lifecycle events, and a derived `summary.json`.
- **Safety:** launch mode uses a non-destructive Job; default attach does not create or assign a Job to the target.

This is a measurement collector, not a profiler, debugger, or automatic performance-tuning tool.

## Build

Install a current Rust toolchain on Windows, then run:

```bash
cargo build --release --locked
```

The executable is written to `target\release\perf-probe.exe` unless `CARGO_TARGET_DIR` is set.

## Basic use

### Launch and observe a command

Use `--` before the target command. `--output` is the parent directory for a newly created evidence bundle.

```bash
cargo run --release -- run --output .\evidence -- cmd.exe /c "your-command-here"
```

You can set a bounded retained-process-handle limit when required:

```bash
cargo run --release -- run --output .\evidence --max-retained-process-handles 256 -- cmd.exe /c "your-command-here"
```

### Attach to an existing process

Default attach is observation-only and waits until the specified process exits.

```bash
cargo run --release -- attach --pid 12345 --output .\evidence
```

`--attach-job` is intentionally unsupported in the current CLI and fails closed.

### Rebuild a summary

Regenerate `summary.json` from a saved bundle's raw NDJSON streams:

```bash
cargo run --release -- summarize --bundle .\evidence\<bundle-directory>
```

The summary is reconstructed from persisted raw evidence and has deterministic serialization for identical input streams.

## Evidence bundle

Each successful `run` or default `attach` creates a unique directory below `--output` containing:

- `processes.ndjson` — observed process identity records.
- `samples.ndjson` — periodic process, Job-when-applicable, system, and Probe samples.
- `events.ndjson` — lifecycle, degradation, and terminal-counter events.
- `summary.json` — derived summary reconstructed from the raw streams.

The NDJSON streams contain one UTF-8 JSON record per line. Readers may discard only an incomplete final EOF fragment; malformed interior records are an evidence error.

## Verify a checkout

```bash
cargo fmt --check
cargo test --all-targets --locked -- --nocapture
cargo build --release --locked
```

## Documentation

- [Probe specification v0.2.1](docs/Performance%20Evidence%20Probe%20v0.2.md)
- [Milestone 1 implementation contract v0.1](docs/Performance%20Evidence%20Probe%20Milestone%201%20Implementation%20Contract%20v0.1.md)
- [Milestone 1 architecture](docs/MILESTONE-1-ARCHITECTURE.md)
- [Evidence schema draft](docs/EVIDENCE-SCHEMA-DRAFT.md)
- [Known limitations](docs/KNOWN-LIMITATIONS.md)
- [Repository closeout](docs/REPOSITORY-CLOSEOUT.md)

## License

MIT.
