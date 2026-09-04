# Performance Evidence Probe
# Milestone 1 Implementation Contract v0.1 (historical technical contract)

> This retained contract records the Milestone 1 design/acceptance boundary.
> For current behavior, the implementation source and tests, root
> [README](../README.md), [evidence schema](EVIDENCE-SCHEMA-DRAFT.md), and
> [known limitations](KNOWN-LIMITATIONS.md) take precedence.

## 0. Authority

**Historical parent specification:** Performance Evidence Probe Specification v0.2.1
**Scope:** Milestone 1 only  
**Primary platform:** Windows 10/11 x64  
**Implementation:** Rust-first  
**Status:** Implementation Contract

本contractの目的は、Performance Evidence Probeの最初の実用的なWindows縦スライスを完成させることである。

親仕様v0.2.1は設計referenceとする。

日常的なMilestone 1実装判断では本contractを優先する。

親仕様と矛盾する解釈が必要になった場合は勝手に拡張せず、親仕様へ戻る。

---

# 1. Milestone Goal

Windows上で、

```text
target launch / attach
        ↓
process observation
        ↓
500ms raw sampling
        ↓
Job accounting where applicable
        ↓
bounded NDJSON Evidence
        ↓
independent deterministic summary
```

までを一通り完成させる。

Milestone 1では、

**「広く測るProbeの核」**

を作る。

advanced sensor、Linux/macOS、automatic diagnosisは作らない。

---

# 2. Mandatory Modes

## M1. Launch

```text
perf-probe run -- <command>
```

を実装する。

Windows Launch modeでは原則:

```text
CreateProcess suspended
↓
Probe Job create
↓
Job configuration
↓
completion port preparation
↓
root Job assignment
↓
membership verification
↓
root handle retention
↓
resume
```

を行う。

---

## M2. Attach

```text
perf-probe attach --pid <pid>
```

を実装する。

defaultではtargetをProbe Jobへassignしてはならない。

Milestone 1では`--attach-job`を未実装でもよい。

---

# 3. Mandatory Job Safety

Probe Jobで、

```text
JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
```

を設定してはならない。

Job limitを性能制御目的で使用してはならない。

Probe crashでtargetがterminateされる設計は禁止。

---

# 4. Mandatory Process Identity

各processについて最低:

```text
process_local_id
pid
process_start_time
boot_identity
parent identity where available
```

を保持する。

PIDだけをidentityとして使用してはならない。

---

# 5. Mandatory Process Handle Lifecycle

## Alive

process生存中は、取得済みhandleを原則保持する。

---

## Exit

exit検出時:

1. terminal CPU counter取得を試みる
2. terminal I/O counter取得を試みる
3. exit code取得を試みる
4. exit Evidenceをwriterへ送る
5. process registry stateを更新する
6. handleをcloseする

---

## Bound

handle retentionはboundedでなければならない。

設定:

```text
max_retained_process_handles
```

を持つ。

大量short-lived childによりhandle countがrun durationまたはprocess総数へ比例して増え続けてはならない。

---

## Degradation

上限その他の理由で必要handleを保持できない場合:

```text
handle_retention_degraded = true
```

をEvidenceへ残す。

不足terminal counterを推測で補完してはならない。

---

# 6. Mandatory Fidelity Evidence

最低以下を保存する。

```text
containment_fidelity
count_fidelity
identity_event_fidelity
```

Launch + Job modeでは最低:

```text
job_total_processes_os
job_active_processes_os
job_total_terminated_by_limit_os
observed_distinct_job_processes
```

を取得する。

可能なら:

```text
job_processes_without_observed_identity
=
job_total_processes_os
-
observed_distinct_job_processes
```

をderived summaryへ出す。

---

# 7. Forbidden Process Claims

Milestone 1では以下を禁止する。

```text
TotalTerminatedProcesses
=
all exited processes
```

という解釈。

```text
TotalProcesses - ActiveProcesses
=
all exited processes
```

という解釈。

sampling / completion eventだけを根拠に、

```text
all child processes were individually observed
```

と主張すること。

---

# 8. Mandatory Core Sampling

nominal interval:

```text
500 ms
```

最低以下をper-process raw sampleとして取得する。

```text
working_set_bytes
private_bytes

user_cpu_time
kernel_cpu_time

read_bytes
write_bytes
other_bytes where available

read_operations
write_operations
other_operations where available

thread_count
handle_count
```

rateやutilizationをraw authorityとして保存してはならない。

---

# 9. Mandatory Set-level Job Accounting

Launch + Job modeでは最低:

```text
job_total_user_time
job_total_kernel_time

job_read_operation_count
job_write_operation_count
job_other_operation_count

job_read_transfer_bytes
job_write_transfer_bytes
job_other_transfer_bytes
```

を取得する。

per-process cumulative sampleの単純和をJob accountingの代替authorityにしてはならない。

両方残す。

---

# 10. Mandatory System Sampling

最低:

```text
system cumulative CPU counters
available physical memory
commit current
commit limit
disk free bytes
```

を取得する。

Milestone 1ではadvanced disk queue / temperature / GPUは不要。

---

# 11. Sampling Clock

各recordへ最低:

```text
wall_time_utc
monotonic_ns
```

を持たせる。

elapsed / gap / rate計算はmonotonic timeを使用する。

---

# 12. Scheduling

samplingは、

```text
start + n * 500ms
```

のabsolute deadline schedulingを使用する。

相対sleepの累積driftへ依存してはならない。

Windows system timer resolutionを変更するための`timeBeginPeriod`は使用禁止。

---

# 13. Sampling Quality

最低以下をrawまたはsummaryで計算可能にする。

```text
scheduled_time
actual_time
sampling_delay
gap_from_previous_sample

sample_count
max_sample_gap_exact
```

Milestone 1ではquantile histogram実装は必須でなくてもよい。

ただし`max_sample_gap_exact`は必須。

---

# 14. Memory Aggregation

working setのprocess-set合計は、

```text
process_set_working_set_sum_bytes
```

と呼ぶ。

これはshared pageを多重計上し得る。

unique physical memory量と呼んではならない。

private bytesは、

```text
process_set_private_bytes_sum
```

として合計可能。

---

# 15. Raw-first Rule

raw cumulative counterが存在する場合、必ずrawを保存する。

禁止例:

```text
CPU utilizationだけ保存
read MB/sだけ保存
IOPSだけ保存
```

正:

```text
CPU cumulative time
I/O cumulative bytes
I/O cumulative operations
```

を保存し、rateはsummary側で導出する。

---

# 16. Mandatory Evidence Bundle

Milestone 1完了runは最低以下を持つ。

```text
<run-id>/
  manifest.json
  host.json
  target.json
  config.json
  capabilities.json

  processes.ndjson
  samples.ndjson
  events.ndjson

  summary.json
```

必要なら:

```text
platform/windows.json
```

を追加可能。

---

# 17. manifest.json

bundle entry pointとする。

最低:

```text
run_id
schema_draft_version
Probe version
Probe build identity

run state

artifact list
artifact sizes

measurement_validity
```

を持つ。

Milestone 1時点では正式schema freezeを主張しない。

---

# 18. target.json

最低:

```text
mode
root process identity
normalized executable path where available
target exit code where available
launch / attach metadata
```

を持つ。

full command line保存はdefault OFF。

---

# 19. config.json

最低:

```text
sampling interval
timer backend
sampler priority
handle retention policy
handle retention limit
Job policy
output policy
flush policy
```

を持つ。

---

# 20. host.json

最低:

```text
OS
OS version
OS build
architecture

CPU model
physical core count
logical processor count

installed RAM

Probe version
collector version
```

を持つ。

---

# 21. capabilities.json

run中ほぼ固定のmetric availabilityを保存する。

例:

```text
windows.private_usage_bytes = AVAILABLE
windows.job_accounting = AVAILABLE
gpu.temperature = UNSUPPORTED
```

missing metricを0で表現してはならない。

---

# 22. processes.ndjson

process identity registryとする。

最低:

```text
process_local_id
pid
start time
boot identity
parent local id where known
discovery source
handle acquisition result
```

を保存する。

---

# 23. samples.ndjson

sampleは`process_local_id`参照を使用する。

identity三つ組を毎sample繰り返さない。

raw cumulative valuesを優先する。

---

# 24. events.ndjson

最低:

```text
process observed
process exit observed
terminal counter query result
handle released
Job assignment result
containment result
collector degradation
sampling anomaly
```

等を記録可能にする。

---

# 25. Single Writer

NDJSON fileへ書き込むwriterはsingle-writer modelを使用する。

複数threadが同一fileへ直接appendして行を交錯させてはならない。

---

# 26. Crash Semantics

Probe crash時、最終NDJSON行は途中で切れ得る。

readerは、

```text
EOF直前の不完全な最終行
```

のみ破棄可能。

途中破損はEvidence errorとする。

---

# 27. Bounded Memory

全sampleをRAMへ保持してはならない。

runtime stateはboundedであること。

少なくとも:

```text
sample history
process exit history
process handle retention
writer queues
```

がrun durationへ比例して無制限増加しないこと。

---

# 28. Probe Self-monitoring

Probe自身について最低:

```text
probe_working_set_bytes
probe_private_bytes
probe_cpu_time
probe_read_bytes
probe_write_bytes
probe_thread_count
probe_handle_count
```

を取得する。

target aggregateへ含めない。

---

# 29. Independent Summary Generator

`summary.json`は保存済みraw Evidenceを読み直す独立code pathで生成する。

sampling loop内部のstateをsummary authorityにしない。

---

# 30. summary.json Determinism

summaryへ非決定的fieldを入れてはならない。

禁止例:

```text
summary generated at
current hostname
summary process pid
random UUID
```

同一raw evidence + 同一schema/versionからbyte-equivalent summaryを生成可能にする。

---

# 31. Mandatory Summary Fields

最低:

## Timing

```text
elapsed
sample_count
max_sample_gap_exact
```

## Memory

```text
peak_working_set_sampled
peak_private_sampled

last_live_working_set_sample_bytes
last_live_working_set_sample_time

last_live_private_sample_bytes
last_live_private_sample_time
```

## CPU

```text
total_cpu_time
average_cpu_utilization
peak_cpu_utilization
```

CPU utilizationはraw cumulative counterからderiveする。

## I/O

```text
total_read_bytes
total_write_bytes
total_read_operations
total_write_operations
```

## Process

```text
maximum_observed_process_count
observed_distinct_process_count

job_total_processes_os where available
job_processes_without_observed_identity where available

maximum_probe_handle_count
handle_retention_degraded
```

## Terminal

可能なら:

```text
exit_code

terminal_user_cpu_time
terminal_kernel_cpu_time

terminal_read_bytes
terminal_write_bytes

terminal_counter_fidelity
```

---

# 32. Measurement Validity

最低:

```text
VALID
DEGRADED
INVALID
```

を持つ。

例:

`DEGRADED`

```text
handle retention degraded
some optional terminal counters unavailable
```

`INVALID`

```text
critical raw Evidence corruption
required core metric unavailable
```

---

# 33. Run States

最低:

```text
COMPLETE
TARGET_FAILED
PROBE_FAILED
ABORTED
INCOMPLETE
EVIDENCE_INVALID
```

を持つ。

target failureとmeasurement failureを分ける。

---

# 34. Mandatory Synthetic Workloads

Milestone 1では最低以下を作る。

## W1. Memory Ramp

private / working-set増加確認。

## W2. Memory Spike

短時間memory spike。

Milestone 1ではOS-reported peak比較未実装でもよい。

## W3. Child Tree

parent → child → grandchild。

## W4. Short-lived Child

500ms未満を含むchildを多数生成。

## W5. CPU Single-thread

1 core負荷。

## W6. CPU Multi-thread

all-core負荷。

## W7. Child CPU then Exit

終了済みchild CPUがJob accountingから消えないことを確認。

## W8. Child I/O then Exit

終了済みchild I/OがJob accountingから消えないことを確認。

## W9. Sequential Read

raw cumulative I/O検証。

## W10. Sequential Write

raw cumulative I/O検証。

---

# 35. Mandatory Acceptance Gates

## A1 Process Identity

root identityを安定して取得できる。

## A2 Child Observation

観測可能childを個別identityとして登録できる。

## A3 Job Accounting

terminated child分のJob CPU / I/O accountingが失われない。

## A4 Aggregation Semantics

working-set sumをunique memoryとして扱わない。

## A5 Sampling

500ms nominal samplingを実行し、exact max gapを記録できる。

## A6 Crash Readability

target crash後もEvidenceがparse可能。

## A7 Probe Crash Readability

flush済みEvidenceがparse可能。

## A8 Truncated Line

EOFの不完全最終行だけ安全に捨てられる。

## A9 Bounded RAM

long runでProbe RAMがrun durationへ比例して増加しない。

## A10 Bounded Handles

大量short-lived childでProbe handle countが無制限増加しない。

## A11 Exit Handle Release

exit finalization後にprocess handleをreleaseする。

## A12 Missing Metric

取得不能counterを0としない。

## A13 Self-exclusion

Probe自身をtarget aggregateへ含めない。

## A14 Attach Safety

default attachでJob assignmentを行わない。

## A15 Job Safety

KILL_ON_JOB_CLOSEを設定しない。

## A16 Deterministic Summary

同一rawからbyte-equivalent summaryを再生成できる。

## A17 Process Count Honesty

Job total process countとobserved identity countの差を偽identityで補完しない。

---

# 36. Independent Calibration Gate

Milestone 1 completion後、正式schema freeze前に、

独立に保守された別系列とprivate bytes等を比較する。

この比較はMilestone 1 completionそのものの必須gateではない。

ただし、

**strict qualification利用およびschema freezeには必要。**

---

# 37. API Semantics Pinning Gate

Milestone 1完成後、formal schema freeze前に最低以下を実測する。

```text
PrivateUsage semantics
WorkingSet semantics

Job CPU after child exit
Job I/O after child exit

exited process handle:
  GetProcessTimes
  GetProcessIoCounters
  GetExitCodeProcess
```

結果を`api_semantics.json`候補schemaで保存する。

---

# 38. Schema Policy

Milestone 1中のEvidence schemaはdraftとする。

例:

```text
perf-evidence-v1-draft
```

を使用可能。

Milestone 1 completionだけでは正式freezeしない。

---

# 39. Schema Freeze Gate

以下すべてを満たすまで`perf-evidence-v1`をfreezeしてはならない。

- Milestone 1 acceptance PASS
- initial API semantics pinning完了
-重大なcounter semantics ambiguityなし
- independent summary reconstruction PASS
- Evidence bundle実run検証PASS

---

# 40. Explicitly Out of Scope

Milestone 1では以下を作らなくてよい。

```text
GPU collector
temperature collector
SMART
advanced storage latency
ETW
PDH primary collector
Linux collector
macOS collector
automatic performance diagnosis
graph UI
web UI
automatic optimization
product-specific phase adapter
strict-memory qualification certification
```

scope creepさせない。

---

# 41. STOP Conditions

以下の場合のみ安全停止してよい。

1. Windows API semanticsが仕様とmaterialに矛盾する
2. Job containmentがtargetの正常動作をmaterialに変える
3. required core metricがWindows上で取得不能
4. Evidence schemaでは表現不能なmaterial ambiguityを発見
5. boundednessとrequired observationを同時に成立させられない
6. parent specificationとのmaterial contradictionを発見

それ以外の、

- optional metric unavailable
- sensor未実装
- cosmetic issue
- summary追加候補
- future portability issue

ではMilestone 1を停止しない。

---

# 43. Completion Definition

Milestone 1は以下すべてで完了。

```text
Windows launch works
Windows attach works

Job safety policy implemented
bounded handle lifecycle implemented

core 500ms sampling works

per-process raw memory / CPU / I/O works

Job set-level CPU / I/O accounting works

process identity registry works

bounded NDJSON writer works

crash/truncated-line handling works

Probe self-monitoring works

independent deterministic summary works

mandatory synthetic tests pass

mandatory acceptance gates pass

Evidence bundle is produced
```

---

# 44. Deliverables

最低:

```text
Rust source
tests
synthetic workloads
CLI

sample Evidence bundle
Milestone 1 test report
known limitations
draft schema description
```

を残す。

---

# 45. Final Milestone Rule

Milestone 1の目的は、Performance Evidence Probe全体を完成させることではない。

目的は、

> **安全でboundedなWindows Performance Evidence coreを一本通すこと。**

新しい測定項目を増やすより、

```text
correct identity
correct semantics
correct lifetime
correct raw storage
correct boundedness
correct summary reconstruction
```

を優先する。

Milestone 1完了後に初めて、

- API semantics pinning
- calibration
- peak cross-check
- strict qualification
- advanced counters

へ進む。
