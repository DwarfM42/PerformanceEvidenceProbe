# Evidence schema — draft (`perf-evidence-v2-draft`)

> **Status:** implemented draft, not a frozen interchange contract. This
> document describes the current public output surface; the root
> [README](../README.md) and [known limitations](KNOWN-LIMITATIONS.md) state
> its supported use and boundaries.

## Bundle lifecycle and layout

A normally completed `run` or default `attach` creates a unique subdirectory
beneath `--output`. Its files depend on the platform:

| File | Kind | Current purpose |
|---|---|---|
| `processes.ndjson` | Raw evidence | Process identity records and platform-specific observation-acquisition outcomes. |
| `samples.ndjson` | Raw evidence | Timestamped cumulative process counters, process-set sums, optional Job accounting, system samples, and collector samples. |
| `events.ndjson` | Raw evidence | Lifecycle, retention/degradation, and terminal-counter events. |
| `summary.json` | Derived | Deterministic summary reconstructed from saved raw evidence. |
| `host.json` | Windows context metadata | Windows version/build, architecture, CPU/RAM details, and collector version. |
| `target.json` | Windows context metadata | Mode, root identity, executable path where available, exit code, and Job/command-line handling. |
| `config.json` | Windows context metadata | Sampling, handle-retention, Job, writer, and flush settings. |
| `capabilities.json` | Windows context metadata | Current Windows collection capability status. |
| `manifest.json` | Context metadata | Run ID, schema/version identity, run state, artifact sizes, and measurement validity. |
| `platform.json` | Context metadata | Linux/macOS-specific scope, identity, or launch details, depending on mode. |
| `linux_terminal.json` | Linux context metadata | Signal terminal outcome for a directly owned `run` root, written only when it terminates by signal. |

The final JSON metadata is written only after the raw streams are finalized and
the summary has been generated. Windows writes `host.json`, `target.json`,
`config.json`, and `capabilities.json`; Linux/macOS write `platform.json` instead.
The common completion helper then builds the manifest from the runtime's supplied
metadata files plus the raw streams and summary. If the
collector is interrupted, the bundle may contain only raw NDJSON. That is
intentionally not presented as a completed bundle.

## V2 availability contract

V2 identifies each newly written sample with `schema_draft_version` and uses
`perf-evidence-v2-draft` in its summary and completed manifest. A raw-only V2
bundle is still reconstructable; it is not thereby a finalized producer bundle.

Optional canonical raw numeric observations are represented either by a JSON
number (including truthful `0`) or by an omitted key plus exactly one applicable
`metric_unavailable` event. `null`, strings, booleans, negative values,
fractional values, and out-of-range values for known unsigned numeric fields
are invalid evidence, not absence. Derived witnesses and summary fields are not
availability-event targets.

The closed raw metric vocabulary is domain-qualified: `process.working_set_bytes`,
`process.private_bytes`, `process.user_cpu_time_ns`,
`process.kernel_cpu_time_ns`, `process.read_bytes`, `process.write_bytes`,
`process.other_bytes`, `process.read_operations`, `process.write_operations`,
`process.other_operations`, `process.thread_count`, `process.handle_count`,
`probe.working_set_bytes`, `probe.private_bytes`, `probe.user_cpu_time_ns`,
`probe.kernel_cpu_time_ns`, `probe.read_bytes`, `probe.write_bytes`,
`probe.thread_count`, `probe.handle_count`, and
`system.{system_user_cpu_time_ns,system_kernel_cpu_time_ns,system_idle_cpu_time_ns,available_physical_memory_bytes,commit_current_bytes,commit_limit_bytes,disk_free_bytes}`.

`metric_unavailable` has closed `reason` values: `unsupported`,
`not_applicable`, `semantic_mismatch`, `authority_unavailable`, and
`sampling_degraded`. Its `subject_kind` is one of `RUN`, `PROCESS`, `SAMPLE`,
or `PROCESS_SAMPLE`; PROCESS subjects require `process_local_id`, sample
subjects require the zero-based ordinal of successfully recovered canonical
`samples.ndjson` records, and PROCESS_SAMPLE requires both. Semantic reasons
use only RUN and apply only to their exact metric for that bundle's run.
Operational reasons use an exact SAMPLE or PROCESS_SAMPLE binding; a
process-sample omission therefore always carries both the persisted process
identity and sample ordinal. PROCESS authority requires a unique persisted
process record with non-sentinel PID, start time, and boot identity; PID or a
sample-local ID alone is insufficient.

The normative required profile is scoped by V2 represented raw domains and
runtime mode, not all conceivable enum members. Job accounting is required only
when its domain is represented. Every canonical process/probe numeric leaf,
including the user/kernel CPU counters, and every listed system leaf is
conditionally absent under the V2 availability contract. For every such leaf,
a numeric value or one exact valid explanation is
required. A RUN declaration can explain the same exact leaf across samples, but
cannot explain another metric or a value that is present (including `0`). If a
RUN declaration and an exact operational declaration both match one omission,
the two explanations are ambiguous and invalid. Stale, duplicate, conflicting,
wrong-domain, wrong-subject, and no-op explanations are invalid.

`process_set_working_set_sum_bytes` and `process_set_private_bytes_sum` are
checked derived integrity witnesses. A complete contributor set requires an
exact checked-arithmetic witness (including zero for an empty set). If a working
set or private contributor is validly unavailable, its corresponding witness
must be absent. No saturating arithmetic or independent witness availability
event is allowed.

`measurement_validity` (`VALID`, `DEGRADED`, `INVALID`) is independent from
`measurement_completeness` (`COMPLETE`, `DECLARED_PARTIAL`). Semantic absence
is VALID + DECLARED_PARTIAL; authority/sampling absence is DEGRADED +
DECLARED_PARTIAL. Raw-only complete evidence can be VALID + COMPLETE without
claiming a finalized bundle. A completed V2 manifest must agree with the
reconstructed validity and completeness; it cannot rehabilitate raw evidence.

Windows numeric values retain their qualified Win32 meanings. A non-equivalent
platform proxy must be declared `semantic_mismatch`, not serialized as the
Windows-derived canonical metric. Linux and macOS collectors may emit only
their qualified direct-root observations and exact unavailability declarations;
they do not make Windows-equivalence or descendant-closure claims.

Compatibility is directional: the new reader accepts historical fully numeric
v0.1 evidence (including absent process stream where no new process-scoped
claim exists); new Windows numeric values retain their existing field names and
numeric JSON shape. Old readers are not promised to understand omitted V2
fields, `metric_unavailable`, or `measurement_completeness`.

## Raw evidence

The three NDJSON files are UTF-8 JSON objects separated by newlines. A completed
line is flushed after it is written. Consumers may discard an incomplete final
physical-EOF fragment, but any malformed interior line is an evidence error.
There is no common sequence envelope, hash inventory, signature, or authenticity
claim in this draft.

### Process identities

A process record contains a collector-local `process_local_id`, PID, process
start time, boot-session identity, optional parent local ID, discovery source,
and the platform-specific acquisition result (the serialized field name remains
`handle_acquisition_result`). A PID alone is not an identity.

On Windows, launch mode registers the root and attempts snapshot-based descendant discovery.
Windows attach mode retains only the requested root observation handle. Child discovery
and retention can be incomplete because processes can race to exit, access can
be denied, and the configured handle bound can be reached. An omitted identity
therefore does not prove that a process did not exist.

Linux and macOS register only the direct run root or specified attach root and
do not attempt descendant discovery. Their attach modes return after a bounded
observation rather than waiting for target exit. See the root README's platform
details for sampling cadence and current qualification boundaries.

### Samples and events

The optional-counter availability contract above does not govern every timing
field. In v0.2.0, Linux/macOS write `monotonic_ns`, `scheduled_monotonic_ns`, and
`sampling_delay_ns` as fixed zero values, with `gap_from_previous_sample_ns`
absent. Reconstruction consequently produces zero `elapsed_ns` and
`max_sample_gap_exact_ns`; these are not measured duration/gap evidence. This is
a current implementation limitation, not an extension of the availability rules.

Where available under the platform contract, samples preserve raw cumulative CPU
and I/O counters, working set, private bytes, thread/handle counts, timing, and
process-set sums. Windows launch samples also include Job accounting; unsupported
or semantically non-equivalent fields follow the V2 availability rules above.
`process_set_working_set_sum_bytes` is a sum of observed
working sets; shared physical pages can be counted more than once, so it is not
unique physical memory.

Events depend on runtime and mode. Windows events include launch Job assignment,
default attach observation, child discovery, collector degradation, observed exit,
and handle release; these are not required lifecycle stages on every OS. A terminal-counter
event records a capture attempt after exit; it is not a promise that every
process had terminal counters available. When an exit code is present it is a
`u32`; thread/handle counts are also `u32`; other numeric leaves are `u64`.
Malformed or out-of-range present terminal/identity values fail reconstruction
rather than becoming silently absent.

## Derived summary

`summary.json` is generated by a separate reader from persisted raw evidence,
not from sampler memory. Replace the quoted path below with a completed bundle
path printed by `run` or `attach`:

```bash
bash scripts/cargo-local.sh run --release --bin perf-probe -- summarize --bundle "path/to/bundle"
```

For identical complete input streams, its fixed serialization is byte-identical.
It reports sampled peaks, timing/gap metrics, cumulative CPU and I/O totals,
observed-process and Job counts, handle-retention degradation, terminal fields
where recorded, and measurement validity. It is derived convenience output, not
an independent measurement or a certification result. Regeneration fails when
there are no complete sample records.

## Validity, run state, and failure

`measurement_validity` is independent of target exit status. It can be `VALID`,
`DEGRADED`, or `INVALID`; bounded-handle retention overflow produces explicit
`DEGRADED` evidence. `manifest.json` records a separate `run_state` (`COMPLETE`
or `TARGET_FAILED`) for completed collection. A failed workload can still leave
usable raw evidence; conversely, a successful workload does not establish that
measurements were representative or qualified.

## Privacy and consumer responsibilities

A bundle can include timestamps, PIDs, host OS/build/hardware details, and an
executable path when available. Linux and macOS `run` persist
`launched_command_argv` in `platform.json`, even though that document also reports
`full_command_line_saved: false`; do not interpret that flag as an argument-redaction guarantee. Avoid
secrets in launch arguments and review all generated evidence before sharing it.

Consumers must not infer workload correctness, workload representativeness,
complete process coverage, unique-memory semantics, an OS lifetime peak,
calibration, or authority over the target from this data alone.
