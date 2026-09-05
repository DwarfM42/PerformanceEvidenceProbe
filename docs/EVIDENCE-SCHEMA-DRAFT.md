# Evidence schema — draft (`perf-evidence-v2-draft`)

> **Status:** implemented draft, not a frozen interchange contract. This
> document describes the current public output surface; the root
> [README](../README.md) and [known limitations](KNOWN-LIMITATIONS.md) state
> its supported use and boundaries.

## Bundle lifecycle and layout

A normally completed `run` or default `attach` creates a unique subdirectory
beneath `--output` and writes:

| File | Kind | Current purpose |
|---|---|---|
| `processes.ndjson` | Raw evidence | Process identity records and handle-acquisition outcomes. |
| `samples.ndjson` | Raw evidence | Timestamped cumulative process counters, process-set sums, optional Job accounting, system samples, and collector samples. |
| `events.ndjson` | Raw evidence | Lifecycle, retention/degradation, and terminal-counter events. |
| `summary.json` | Derived | Deterministic summary reconstructed from saved raw evidence. |
| `host.json` | Context metadata | Windows version/build, architecture, CPU/RAM details, and collector version. |
| `target.json` | Context metadata | Mode, root identity, executable path where available, exit code, and Job/command-line handling. |
| `config.json` | Context metadata | Sampling, handle-retention, Job, writer, and flush settings. |
| `capabilities.json` | Context metadata | Current collection capability status. |
| `manifest.json` | Context metadata | Run ID, schema/version identity, run state, artifact sizes, and measurement validity. |

The final JSON metadata is written only after the raw streams are finalized and
the summary has been generated. Platform runtimes write their own host, target,
configuration, and capability documents; the common writer then builds the
completed manifest from those documents plus the raw streams and summary. If the
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
`process.private_bytes`, `process.read_bytes`, `process.write_bytes`,
`process.other_bytes`, `process.read_operations`, `process.write_operations`,
`process.other_operations`, `process.thread_count`, `process.handle_count`,
`probe.working_set_bytes`, `probe.private_bytes`, `probe.read_bytes`,
`probe.write_bytes`, `probe.thread_count`, `probe.handle_count`, and
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
when its domain is represented. The conditionally absent canonical raw leaves
are process/probe working set, private bytes, read bytes, write bytes, optional
I/O-operation leaves, thread count, handle count, and the listed system leaves.
Process/probe CPU counters remain required numeric raw observations. For every
conditionally absent leaf, a numeric value or one exact valid explanation is
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
Windows-derived canonical metric. V2 does not claim macOS or Linux collection
support.

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
and handle-acquisition result. A PID alone is not an identity.

Launch mode registers the root and attempts snapshot-based descendant discovery.
Attach mode retains only the requested root observation handle. Child discovery
and retention can be incomplete because processes can race to exit, access can
be denied, and the configured handle bound can be reached. An omitted identity
therefore does not prove that a process did not exist.

### Samples and events

Samples preserve raw cumulative CPU and I/O counters, working set, private bytes,
thread/handle counts, timing, and process-set sums. Launch samples also include
Windows Job accounting. `process_set_working_set_sum_bytes` is a sum of observed
working sets; shared physical pages can be counted more than once, so it is not
unique physical memory.

Events include launch Job assignment, default attach observation, child discovery,
collector degradation, observed exit, and handle release. A terminal-counter
event records a capture attempt after exit; it is not a promise that every
process had terminal counters available.

## Derived summary

`summary.json` is generated by a separate reader from persisted raw evidence,
not from sampler memory. Run:

```bash
bash scripts/cargo-local.sh run --release --bin perf-probe -- summarize --bundle <bundle>
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

A bundle can include timestamps, PIDs, host OS/build/hardware details, and a
normalized executable path when available. It deliberately does not persist the
full launch command line. Review generated evidence before sharing it.

Consumers must not infer workload correctness, workload representativeness,
complete process coverage, unique-memory semantics, an OS lifetime peak,
calibration, or authority over the target from this data alone.
