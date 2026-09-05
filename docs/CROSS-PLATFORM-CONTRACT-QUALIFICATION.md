# Cross-platform common-contract qualification — Linux runtime

**Candidate base:** `141b9c107bd2b8a9a13b85e645762c2add746d3e`

**Linux evidence inspected read-only:** `0adf9ed64094e645e8a812fbdd1dd36fe8f21e7c`
**Scope:** common evidence contract, deterministic reconstruction, completed-bundle boundary, and the requirements a future Linux `attach`/`run` implementation must meet. This is a static qualification matrix plus deterministic common-contract fixtures; it is not Linux runtime qualification.

## Classification and availability rules

| Class | Meaning | Contract treatment |
|---|---|---|
| A | Cross-platform equivalent and normally observable. | Numeric when observed; operational failure remains possible unless the sample itself cannot be formed. |
| B | Equivalent measurement, operationally fallible. | Numeric or exactly one operational `metric_unavailable` declaration at its exact sample subject. |
| C | Stable platform semantic mismatch. | Never substitute a related value; use a RUN-scoped `semantic_mismatch` declaration. |
| D | Unsupported. | RUN-scoped `unsupported` only. |
| E | Not applicable. | RUN-scoped `not_applicable` only. |
| F | Platform-specific observation point. | Keep outside the common canonical leaf until a separate qualified metric is designed. |
| G | Derived witness or summary. | Never an unavailable-event target; omit it when an exact derivation is impossible. |

A numeric `0` is an observation, never absence. For every optional raw leaf, the only valid states are: numeric (including zero), or missing plus one exact applicable declaration. RUN semantic and per-sample operational declarations must not both explain one omission. Operational declarations use `PROCESS_SAMPLE { process_local_id, sample_ordinal }` for process leaves and `SAMPLE { sample_ordinal }` for probe/system leaves. There are no wildcard declarations.

## Complete raw-evidence matrix

### Process identity and sample leaves

| Canonical leaf | Rust shape | Intent / Windows authority | Linux authority / meaning | macOS note | Class | Availability and derived consumers |
|---|---|---|---|---|---|---|
| `ProcessRecord.process_local_id` | required `u64` | Collector-local identity key. | Must identify one persisted composite process identity. | No implementation exists. | A | Not an availability target; duplicate IDs fail closed. |
| `ProcessRecord.pid` | required `u32` | PID from launch/opened handle/discovery. | `/proc/<pid>` directory name. | No implementation exists. | B | Required for process-scoped authority; out-of-range and zero fail closed. PID alone is never identity. |
| `ProcessRecord.process_start_time` | required `u64` | `GetProcessTimes` creation FILETIME. | `/proc/<pid>/stat` starttime ticks. | No implementation exists. | B | Required for authority; identity acquisition/revalidation failure must not create a replacement identity. |
| `ProcessRecord.boot_identity` | required string | OS boot-time identity from `NtQuerySystemInformation`. | `/proc/sys/kernel/random/boot_id`. | No implementation exists. | B | Required for authority. Linux parser bounds and validates it; a missing/malformed value prevents identity persistence. |
| `parent_local_id`, `discovery_source`, `handle_acquisition_result` | optional / required metadata | ToolHelp and retained-handle outcome. | Future Linux parent discovery must name its authority and retention outcome. | No implementation exists. | B/F | Metadata, not canonical numeric leaves; absent parent does not prove no parent. |
| `process.user_cpu_time_ns` | optional `u64` | `GetProcessTimes` user FILETIME. | `/proc/<pid>/stat` `utime`, checked ticks-to-ns. | No implementation exists. | B | Exact `PROCESS_SAMPLE` operational declaration on a post-identity stat read/conversion failure. No delta/run total derives from a final live sample. |
| `process.kernel_cpu_time_ns` | optional `u64` | `GetProcessTimes` kernel FILETIME. | `/proc/<pid>/stat` `stime`, checked ticks-to-ns. | No implementation exists. | B | Same exact operational policy as user CPU; source loss may co-occur with thread count, but each leaf has its own declaration. |
| `process.thread_count` | optional `u32` | ToolHelp `cntThreads`. | `/proc/<pid>/stat` `num_threads`. | No implementation exists. | B | Exact `PROCESS_SAMPLE` declaration. Present values are range-checked to `u32`; zero stays numeric. |
| `process.working_set_bytes` | optional `u64` | `GetProcessMemoryInfo.WorkingSetSize`. | `/proc/<pid>/statm` RSS is not Windows working set. | No implementation exists. | C/F | Linux must declare RUN `semantic_mismatch`; Linux RSS is a future platform-native point. Its omission suppresses `process_set_working_set_sum_bytes` and sampled/last-live working-set witnesses. |
| `process.private_bytes` | optional `u64` | `PROCESS_MEMORY_COUNTERS_EX.PrivateUsage`. | `/proc` private-page data is not qualified as Windows PrivateUsage. | No implementation exists. | C/F | RUN `semantic_mismatch`; Linux private-page/cgroup accounting is platform-native pending definition. Its omission suppresses private witnesses. |
| `process.read_bytes`, `write_bytes`, `other_bytes` | optional `u64` | `GetProcessIoCounters` transfer counts. | `/proc/<pid>/io` fields are not a Windows I/O-counter equivalence, especially `other`. | No implementation exists. | C/F | RUN `semantic_mismatch`; Linux `/proc/io` observations are platform-native pending semantic contract. |
| `process.read_operations`, `write_operations`, `other_operations` | optional `u64` | `GetProcessIoCounters` operation counts. | `/proc/<pid>/io` `syscr`/`syscw` and no qualified `other` are not equivalent. | No implementation exists. | C/F | RUN `semantic_mismatch`; never derive a known-subset process I/O total. |
| `process.handle_count` | optional `u32` | `GetProcessHandleCount`. | Open FD count is not Windows handle count. | No implementation exists. | C/F | RUN `semantic_mismatch`; Linux FD count is a future platform-native metric, range-checked to `u32` only if a common leaf is ever qualified. |

### Probe / collector sample leaves

The probe is a separate subject: target success never proves probe observability, and vice versa.

| Canonical leaf | Rust shape | Windows authority | Linux authority / classification | Availability / consumers |
|---|---|---|---|---|
| `probe.user_cpu_time_ns`, `probe.kernel_cpu_time_ns` | optional `u64` | `GetProcessTimes(GetCurrentProcess())`. | Same `/proc/self/stat` user/kernel semantics as process CPU: **B**. | `SAMPLE` operational declaration for each missing leaf. No target declaration may explain it. |
| `probe.thread_count` | optional `u32` | ToolHelp lookup of collector PID. | `/proc/self/stat` has a potentially equivalent count: **B**, but the current Linux candidate does not yet qualify it. | Numeric or exact `SAMPLE` operational declaration; zero numeric. |
| `probe.working_set_bytes`, `probe.private_bytes` | optional `u64` | Windows process memory APIs. | Linux RSS/private concepts are not Windows equivalents: **C/F**. | Current bounded Linux candidate uses RUN `semantic_mismatch`; future native metrics remain separate. |
| `probe.read_bytes`, `probe.write_bytes` | optional `u64` | Windows process I/O counters. | Linux `/proc/self/io` not qualified as equivalent: **C/F**. | RUN `semantic_mismatch`; never use a target declaration. |
| `probe.handle_count` | optional `u32` | Windows process handle count. | Linux FD count mismatch: **C/F**. | RUN `semantic_mismatch`. |

### System sample leaves

| Canonical leaf | Rust shape | Windows authority | Linux / container or namespace note | Class and availability |
|---|---|---|---|---|
| `system.system_user_cpu_time_ns` | optional `u64` | `GetSystemTimes`. | `/proc/stat` user accounting has unqualified guest/nice and aggregate-scope differences. | C until exact cross-OS semantics are pinned; current Linux candidate RUN-declares mismatch. |
| `system.system_kernel_cpu_time_ns` | optional `u64` | `GetSystemTimes` kernel time. | Linux `system` accounting and Windows kernel/idle treatment are not pinned equivalent. | C; RUN `semantic_mismatch`. |
| `system.system_idle_cpu_time_ns` | optional `u64` | `GetSystemTimes` idle time. | `/proc/stat` idle/iowait and virtualized CPU accounting require an explicit scope. | C; RUN `semantic_mismatch`. |
| `system.available_physical_memory_bytes` | optional `u64` | `GlobalMemoryStatusEx.ullAvailPhys`. | `MemAvailable`, free pages, and cgroup-effective availability differ; container/VM view must be named. | C/F; RUN mismatch until a scoped canonical intent is defined. |
| `system.commit_current_bytes`, `system.commit_limit_bytes` | optional `u64` | `GetPerformanceInfo.CommitTotal/CommitLimit × PageSize`. | Linux overcommit, `Committed_AS`, `CommitLimit`, namespaces and cgroup limits are not Windows commit equivalence. | C/F; RUN mismatch. |
| `system.disk_free_bytes` | optional `u64` | `GetDiskFreeSpaceExW(NULL)` current drive. | `statvfs` is meaningful only with an explicit mount/path and namespace. | C pending filesystem scope; current Linux candidate RUN-declares mismatch. |

All system leaves already support exact `SAMPLE` operational declarations. A single failing source may yield several declarations, but the declarations stay leaf-specific.

### Sample scaffold, Job accounting, terminal data, and metadata

| Surface | Current semantics | Linux qualification decision |
|---|---|---|
| `SampleRecord.schema_draft_version`, `record_type`, wall time, monotonic/scheduled/delay/gap timing, root-live boolean, process array | Required sample scaffolding. Windows uses local UTC/`Instant`; root liveness uses `GetExitCodeProcess`. | Not canonical optional metrics. If these facts cannot be formed, do not fabricate a sample. A post-identity Linux target disappearance may instead persist a real sample with an empty process array and `root_process_confirmed_live:false`; the derived empty-set witnesses are numeric zero, not unknown. |
| `process_set_working_set_sum_bytes`, `process_set_private_bytes_sum` | Checked sums of only represented process rows. | **G**. Emit the exact checked witness only when every row has that contributor; omit it if any contributor is unavailable. It is never a metric-unavailable target and never an exact whole-tree claim. |
| `job.*` (CPU, I/O, process counts, termination-by-limit) | `Option<JobAccounting>`; every internal field is required when a Windows Job domain is represented. Windows `QueryInformationJobObject` supplies it only for launch. | **E** for attach, **C/F** for Linux run. Do not invent a Job analogue. A future cgroup/process-group/accounting model is a platform-native observation point; leaving `job` absent does not require unavailability events and suppresses Job-derived totals. |
| `process_exit_observed.exit_code` | Optional `u32`; Windows `GetExitCodeProcess`. | **B** for launched child / owned wait authority; attach commonly lacks an exit status. Omission is truthful and not zero. Out-of-range data fails reconstruction. |
| terminal user/kernel CPU and read/write counters | Optional raw terminal capture attempt. Windows performs a post-exit `process_sample`. | **B**. Absence is permitted; no final observation is fabricated. Present values are strictly typed; malformed values fail reconstruction. |
| `target.json` exit/path, host/config/capabilities metadata | Platform-owned contextual metadata. | Linux direct run metadata declares `represented_process_set: direct_root_only`, `descendant_discovery: not_attempted`, `descendant_scope: unknown_not_observed`, `root_exits_before_descendants_scope: unknown_not_observed`, and no process-group/session/cgroup/Job authority. For a directly owned root terminated by signal, Linux writes `linux_terminal.json`, bound to the persisted composite root identity, with `kind`, signal number/name, and observed core-dump status; it never encodes a signal as `exit_code`. Missing optional target exit is allowed; unavailable metadata must not be replaced by a false Windows-shaped value. |
| completed `manifest.json` | Binds artifact sizes and reconstructed validity/completeness after streams and summary are finalized. Only `COMPLETE`/`TARGET_FAILED` are current completed states. | Common writer is platform-neutral. Interruption deliberately leaves raw NDJSON without completed manifest. A bundle with no complete `SampleRecord` remains raw-only, not a completed partial bundle. |

## Lifecycle matrix

| Transition | Authoritative facts / races | Valid evidence behavior |
|---|---|---|
| PID supplied → identity acquisition | Linux needs boot ID + PID + `/proc/<pid>/stat` starttime. PID-only is rejected. Read/access/malformed/oversized input and tick conversion failure occur before a safe identity. | Fail closed before claiming target identity. Do not sample a replacement PID. |
| Identity acquired → source reads | `/proc/<pid>/stat` backs user CPU, kernel CPU, and thread count; it can disappear, become inaccessible, truncate, or fail parsing after identity. Probe `/proc/self/stat` is independent. | Persist the exact process identity. If a process row is persisted but the source fails, omit each affected raw leaf and emit one exact `PROCESS_SAMPLE` operational declaration per leaf. Probe failures use `SAMPLE`. |
| Source reads → identity revalidation | Target can exit or PID can be reused between reads. Linux candidate compares full composite identity. | Same identity permits sample persistence. Different/unavailable identity must not attach metrics to a replacement. A real target-disappearance sample may contain no target row and root-live false; no fabricated last-live counters. |
| Repeated sampling | Counters are cumulative; any gap can contain unobserved evolution. Process discovery is separately fallible and bounded. | Do not infer zero deltas. Compare only same persisted identity and preserve gaps. Exact process-set witnesses cover represented rows only, never unobserved descendants. |
| Target exit / disappearance | Windows retained handles can observe an exit code; Linux attach does not own `waitpid` exit status. A directly owned Linux run root has `wait` status but no descendant closure authority; root may die while descendants live. | A normal direct-root status may use canonical `exit_code`; a signal outcome is platform-owned `linux_terminal.json`, never a synthetic exit code. Linux records only live samples observed before root exit and never manufactures a final live row or a run total. A complete bundle requires at least one complete sample record; an interruption/no-sample path stays raw-only. |
| PID reuse / starttime change | PID can be reused after exit or a read race. | Different composite identity is terminal for that target; never silently update the process record or continue under the same local ID. |
| Child/process-set discovery | Windows ToolHelp tree discovery and retained handles are best effort; Job totals may exceed observed IDs. Linux `/proc` parent links and cgroup/process-group membership are not equivalent closure authorities. | No common exact process-tree sum or closure claim exists. Keep platform-native discovery/accounting separate; suppress derived fields needing unknown contributors. |
| Run / launch | Windows starts suspended, assigns a non-destructive Job, verifies membership, then resumes. Linux has no Job Object equivalence. | Linux direct run records only its owned root and finishes on authoritative root exit; it does not discover or close descendants, even when parent links, process groups, sessions, or cgroups are observable. Its bounded loop samples at a fixed 500 ms interval while the owned root remains revalidated live. It omits `job` and does not label any Linux grouping/accounting surface as Windows Job accounting. Target launch failure before identity remains no completed bundle under the current contract. |
| Termination request / timeout | The current `run` CLI has no terminate-request or timeout option on Linux. | Linux records only a directly owned child's observed `wait` outcome. An external signal request is not collector request evidence; when the owned child is observed signal-terminated, `linux_terminal.json` records that observation without a synthetic `exit_code`. |
| Collector/probe failure | Writer send/flush, clock, system and self-observation can fail independently of target. | If a sample remains structurally formable, declare missing optional raw leaves exactly. If writer/scaffold cannot form a sample, stop rather than fabricate it; raw-only output is not a completed bundle. |
| Finalization / summary / manifest | Summary rereads NDJSON; manifest is written only after writer finish, summary, and platform metadata. | Summary rejects malformed records, ambiguous/stale availability events, incorrect derived witnesses, and manifest measurement-state disagreement. `TARGET_FAILED` expresses observed launch target failure, not generic collector interruption. |

## Derived and counter rules

- Process-set memory witnesses use checked addition and become absent if any contributor is unavailable. An observed empty process array has an exact zero sum for that sample; it is not an assertion of complete process coverage.
- Peaks use only exact witnesses; one missing contributor suppresses that metric rather than summing a known subset.
- Last-live memory is updated only by `root_process_confirmed_live:true`; a later non-live sample cannot overwrite it.
- Run CPU/I/O totals and CPU utilization are derived only from final Windows Job accounting. They are omitted without a qualified Job domain; they are never derived from a final process sample.
- Cumulative CPU/I/O counters must not turn a missing observation into zero delta. Counter decreases, arithmetic overflow, or non-monotonic time fail closed in existing Job-derived paths.
- Terminal counters are optional capture attempts, not substitutes for missing cumulative samples.

## Contract gaps found and disposition

1. **Required process/probe CPU leaves could not represent post-identity operational failure.** Linux `/proc/<pid>/stat` can lose the required CPU pair and thread count together. **Repaired:** CPU leaves are optional canonical raw leaves with new closed metric names and exact availability validation.
2. **Probe CPU was structurally required despite independent self-observation failure.** **Repaired:** probe CPU pair follows the same exact `SAMPLE` availability model.
3. **Thread/handle and persisted PID width could be silently widened by the summary JSON reader.** **Repaired:** known `u32` leaves and PID authority are range-checked.
4. **Malformed terminal numeric data could be silently omitted by reconstruction.** **Repaired:** exit code and terminal counters now parse strictly when present.
5. **No common equivalent exists for Linux FD/RSS/private-page, `/proc/io`, commit/overcommit, Job, or unscoped system observations.** Not repaired by numerical substitution. They remain RUN semantic mismatches or future **F** platform-native points.
6. **No completed manifest is representable without any complete sample record.** This is retained intentionally: raw-only interruption/pre-identity failure is not mispresented as a completed bundle. A real final sample with an empty target process array is representable and tested.
7. **No existing macOS implementation/branch/PR exists.** The remote contains only `main`, the two historical contract branches, and `port/linux-collector`; macOS remains a future design cross-check, not a validated producer.

## Deterministic fixture coverage

`tests/linux_contract_qualification.rs` exercises: fully observed zero values; RUN semantic partial; shared `/proc/stat` process CPU/thread authority loss; mixed semantic and operational absence; full system-source loss; probe-only CPU loss; exact wrong binding; RUN/operational ambiguity; derived witness suppression; terminal non-live sample without fabricated run total; and `u32`/terminal malformed or overflow rejection. Existing `tests/optional_unavailable_metrics.rs` retains the generic availability cardinality, no-op, duplicate, wrong-domain, historic compatibility, and Windows-shaped numeric-zero checks.

## Version decision

The serialized schema remains `perf-evidence-v2-draft`, and package version remains `0.2.0`. The change is a compatible completion of the V2 draft: existing fully numeric producers keep the same JSON fields and shapes; new producers may omit CPU leaves only with exact V2 declarations. Old readers are not promised to understand V2 omissions, as already documented. The draft identity is retained because this is a contract repair before a frozen interchange schema, not a stable-version compatibility promise.
