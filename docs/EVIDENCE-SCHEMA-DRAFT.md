# Evidence schema — draft (`perf-evidence-v1-draft`)

> **Status:** implementation draft, not a frozen interchange contract. The authoritative Milestone 1 contract is not present in this repository, so this document describes the implemented surface only.

## Bundle layout

A successful `run` or default `attach` creates a unique subdirectory beneath `--output` containing:

| File | Authority | Current semantics |
|---|---|---|
| `processes.ndjson` | raw evidence | One process identity record for the observed launch/attach root. |
| `samples.ndjson` | raw evidence | Periodic process, Job-when-applicable, system, and probe sample records. |
| `events.ndjson` | raw evidence | Lifecycle and terminal-counter events. |
| `summary.json` | derived | Deterministically regenerated from the persisted raw streams. |

The streams are newline-delimited UTF-8 JSON. A reader accepts completed JSON lines and may discard only an incomplete final EOF fragment. Malformed interior records are errors.

## Process record

`processes.ndjson` currently stores `ProcessRecord` with:

- `process_local_id`: collector-local numeric identity;
- `pid`: Windows PID, explicitly not independently sufficient as identity;
- `process_start_time`: Windows FILETIME tick identity component;
- `boot_identity`: collector boot-estimate identity component;
- `parent_local_id`: optional local parent reference;
- `discovery_source` and `handle_acquisition_result`.

The current runtime records the root process only. Child discovery is not implemented and absence of a child record must not be read as absence of the child.

## Sample record

`SampleRecord` contains wall and monotonic timestamps, scheduled deadline, delay, inter-sample gap, root-process samples, optional Job accounting, system fields, and probe fields.

Per-process raw cumulative fields include user/kernel CPU time, I/O operation and transfer counters, working set, private bytes, and handle count. CPU and I/O values are raw cumulative OS values; rates in `summary.json` are derived and are not collector authority.

Working-set set sums are sums of process working sets and can double count shared physical pages.

## Event record

Implemented event kinds include:

- `launch_assigned_non_destructive_job`: records `kill_on_job_close_enabled: false` after assignment and membership verification;
- `attach_observation_started`: records `attached_to_probe_job: false` for default attach;
- `process_exit_observed`: records exit code and terminal CPU/I/O capture attempt.

## Ordering, crash recovery, and boundedness

One writer thread owns all open streams. Producers send through a bounded channel; the writer serializes a complete JSON object followed by a newline and flushes it. The current implementation has not yet added a common sequence envelope or a finalized manifest/hash inventory. Consumers therefore must not treat this draft as a complete portable evidence format.

## Availability and fidelity

Not-yet-collected system and probe fields are currently represented by zero in the implementation. That is a known draft-schema defect: zero is ambiguous with unavailable. Future schema work must use an explicit availability/result union before any contract is frozen.
