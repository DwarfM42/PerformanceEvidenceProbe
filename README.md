# Performance Evidence Probe

Windows-first Rust collector for preserving raw performance evidence from a process. `perf-probe` can launch a target under an accounting-only Windows Job, observe an existing PID without assigning it to a Job, and deterministically rebuild a summary from the persisted raw streams.

## Scope

- **Platform:** Windows 10/11 x64 for `run` and `attach`.
- **Sampling:** 500 ms nominal interval with scheduled and observed monotonic timing retained in the raw evidence.
- **Evidence:** process identities, process and system samples, Job accounting when applicable, lifecycle events, and a derived `summary.json`.
- **Safety:** launch mode uses a non-destructive Job; default attach does not create or assign a Job to the target.

This is a measurement collector, not a profiler, debugger, or automatic performance-tuning tool.

## Build

Install a current Rust toolchain and use the repository wrapper from Git Bash:

```bash
bash scripts/cargo-local.sh build --release --locked
```

The wrapper overrides inherited Cargo and temporary-directory variables. It keeps
the Cargo home in `cargo-home/`, build output in `target/`, and temporary files
in `tmp/`, all beneath this checkout. These local directories are ignored by Git.
Do not invoke Cargo directly for this repository when the surrounding shell may
set `CARGO_HOME`, `CARGO_TARGET_DIR`, `TEMP`, `TMP`, or `TMPDIR` elsewhere.

The release executable is `target/release/perf-probe.exe`.

## Basic use

### Launch and observe a command

Use `--` before the target command. `--output` is the parent directory for a newly created evidence bundle.

```bash
bash scripts/cargo-local.sh run --release -- run --output ./evidence-output -- cmd.exe /c "your-command-here"
```

You can set a bounded retained-process-handle limit when required:

```bash
bash scripts/cargo-local.sh run --release -- run --output ./evidence-output --max-retained-process-handles 256 -- cmd.exe /c "your-command-here"
```

### Attach to an existing process

Default attach is observation-only and waits until the specified process exits.

```bash
bash scripts/cargo-local.sh run --release -- attach --pid 12345 --output ./evidence-output
```

`--attach-job` is intentionally unsupported in the current CLI and fails closed.

### Rebuild a summary

Regenerate `summary.json` from a saved bundle's raw NDJSON streams:

```bash
bash scripts/cargo-local.sh run --release -- summarize --bundle ./evidence-output/<bundle-directory>
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
bash scripts/cargo-local.sh fmt --check
bash scripts/cargo-local.sh test --all-targets --locked -- --nocapture
bash scripts/cargo-local.sh build --release --locked
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
