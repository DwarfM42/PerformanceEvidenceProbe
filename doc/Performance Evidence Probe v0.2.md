# Performance Evidence Probe Specification v0.2.1

## 0. Status

**Status:** Draft / Implementation-ready / Schema not frozen
**Working name:** Performance Evidence Probe (`perf-probe`)
**Primary initial target:** Windows 10/11 x64
**Architecture target:** Windows / Linux / macOS
**Implementation preference:** Rust-first

本仕様は、CloseRAG本体とは独立した性能観測・Performance Evidence生成ツールを定義する。

本Probeの目的は、特定のベンチマーク結果だけを取得することではない。

**一度の測定で、後から性能・メモリ・I/O・CPU・process lineage・process accounting・phase・sampling quality・実行環境について再調査したくなる可能性の高い情報を、被測定処理への干渉を抑えながら広く保存する。**

さらに、本Probe自身の測定結果について、

* どのOS API / counterから得た値か
* そのcounterの意味論は何か
* その意味論はdocumentationで確認されたか
* 実機でsemantic pinningされたか
* 独立系列と較正されたか
* 現在の環境でcalibrationが適用可能か
* qualification用途へ使用可能か

を追跡可能にする。

---

# 1. Motivation

従来の性能調査では、

1. workloadを実行する
2. 結果を見る
3. 新しい疑問が生じる
4. 必要なcounterが取得されていないと分かる
5. workloadを再実行する
6. 別の疑問が生じる
7. 再び測定する

という往復が発生しやすい。

特に、

* 大規模ingestion
* Generation build
* embedding
* indexing
* sealing
* publication
* 1M-scale retrieval
* 大容量file I/O

等は再実行コストが高い。

Performance Evidence Probeは、この往復を減らす。

基本思想は、

> **Cheap observability should be collected now; interpretation can happen later.**

とする。

測定時には、取得コストが合理的なraw evidenceを可能な限り残し、分析・可視化・比較・原因推定は後工程で行う。

---

# 2. Goals

## G1. Broad Capture

OSから低侵襲に取得可能な性能情報を広く採取する。

単一の問題仮説だけに最適化しない。

---

## G2. Low Observer Effect

Probe自身がtargetへ与える影響を最小化する。

Probe自身の、

* CPU
* memory
* I/O
* thread count
* handle count
* scheduling policy

も記録し、観測コストを後から評価可能にする。

---

## G3. Crash-resilient Evidence

測定途中で、

* target crash
* Probe crash
* OS shutdown
* disk full
* user abort

が発生しても、それまで取得したraw evidenceを可能な限り失わない。

---

## G4. Re-analysis Without Rerun

保存済みraw evidenceから後日、

* peak memory
* memory growth
* CPU saturation
* I/O bottleneck候補
* process proliferation
* phase別resource usage
* sampling gap
* system memory pressure
* disk pressure
* CPU frequency variation
* thermal variation
* sampled peak取り逃し
* Probe observer effect

等を再計算可能にする。

---

## G5. Existing CloseRAG Measurement Compatibility

既存CloseRAG canonical memory measurementで必要となる主要概念を表現可能にする。

少なくとも、

* selected process set
* per-process RSS / working set
* per-process private bytes
* process-set working-set sum
* process-set private-byte sum
* CPU
* read/write bytes
* read/write operations
* process creation / exit observation
* phase
* peak
* terminal observation
* sampling gap

を失わない。

通常のbroad profileはcanonical qualificationそのものではない。

qualification用途ではstrict profileを使用する。

---

## G6. Calibratability and Measurement Trust

Probeが出力する数値を無条件に正しいものとして扱わない。

Probe自身について、

* API semantics
* collector correctness
* independent measurementとの一致
* OS build compatibility
* calibration applicability

をEvidenceとして保持する。

strict qualification用途への利用は、必要なcalibration条件が満たされた場合のみ許可する。

---

# 3. Non-goals

v0.2.1では以下を目的としない。

* profilerによる関数単位CPU attribution
* target processへのcode injection
* debugger attach
* ptrace
* heap object enumeration
* allocation stack tracing
* source instrumentation必須化
* target memory dump
* packet capture
* file contents取得
* environment variable dump
* 自動performance tuning
* 自動原因断定

必要なら将来optional adapterとして追加する。

---

# 4. Fundamental Architecture

```text
Target / Target Process Tree
            │
            │ OS-visible observation
            ▼
     Platform Collector
            │
      ┌─────┴────────┐
      │              │
      ▼              ▼
Per-process Raw   Set-level Authority
Observation       e.g. Windows Job Accounting
      │              │
      └─────┬────────┘
            ▼
       Raw Evidence
            │
            ▼
       NDJSON Bundle
            │
            ▼
 Independent Summary Pass
            │
            ▼
       summary.json
            │
            ▼
     Calibration Layer
            │
            ▼
 Qualification / Analysis
```

Probeは原則としてtargetへ書き込みを行わない。

Launch modeにおけるJob Object containmentは明示的な例外であり、observer effectとしてEvidenceへ記録する。

Attach modeでは原則read-only observationを維持する。

---

# 5. Target Modes

## 5.1 Launch Mode

Probe自身がcommandをlaunchする。

```text
perf-probe run -- command args...
```

Windowsでは原則、

1. target rootをsuspendedで作成
2. Probe Job Objectを作成
3. Job accounting / completion portを準備
4. rootをJobへassign
5. containmentを確認
6. root process handleを保持
7. targetをresume

の順を使用する。

rootがJobへ収容される前にchildを生成するraceを縮小する。

---

## 5.2 Attach Mode

既存PIDをrootとして観測する。

```text
perf-probe attach --pid 12345
```

defaultでは既存processをProbe Jobへassignしない。

attach以前に終了したchildや、sampling間隔より短いprocessについて完全なlineageを再構築できない場合がある。

---

## 5.3 System-only Mode

特定processを指定せずsystem resourceのみ観測する。

```text
perf-probe system
```

storage / memory pressure / thermal / frequency等の環境調査に使用可能。

---

# 6. Process Identity

PIDだけをprocess identityとして使用してはならない。

PID reuseを考慮し、

```text
ProcessIdentity {
    pid
    process_start_time
    boot_identity
}
```

の組を論理identityとする。

可能なら、

```text
parent_process_identity
executable_identity
```

も保持する。

---

# 7. Windows Process Handle Lifecycle

Launch modeで正常に取得したprocess handleは、生存中processについて原則保持する。

目的:

* PID reuse防止
* process identity安定化
* exit code取得
* terminal CPU / I/O counter取得可能性の向上

ただし、終了済みprocess handleをrun終了まで無制限に保持してはならない。

---

## 7.1 Normal Lifecycle

標準lifecycleは以下とする。

```text
PROCESS DISCOVERED / CREATED
        ↓
handle acquired
        ↓
process alive
        ↓
handle retained
        ↓
exit detected
        ↓
terminal counters queried
        ↓
exit event durably queued / written
        ↓
handle closed
        ↓
PID reuse permitted
```

process identity authorityはhandleではなく§6の`ProcessIdentity`である。

---

## 7.2 Exit Finalization

exit検出時、可能な限り以下を取得する。

```text
exit_code
final_user_cpu_time
final_kernel_cpu_time
final_read_bytes
final_write_bytes
final_read_operations
final_write_operations
final_other_io
```

memory counterはprocess終了後のexact terminal値として要求しない。

terminal counter取得結果自体もEvidenceへ記録する。

---

## 7.3 Handle Retention Bound

Probeが保持するprocess handle数には明示的上限を設ける。

設定例:

```text
max_retained_process_handles
```

通常は生存中processのhandleを優先する。

上限到達時に必要なhandleを保持できない場合、

```text
handle_retention_degraded = true
```

をEvidenceへ記録する。

さらに、

```text
handle_retention_degradation_reason
handle_retention_limit
maximum_retained_handle_count
```

を保存する。

---

## 7.4 Forced Early Release

保持上限超過などにより終了確認前にhandleをreleaseする必要がある場合、

古い非critical handleからreleaseしてよい。

そのidentityについて、

```text
terminal_counter_fidelity = DEGRADED
```

等を記録する。

不足したterminal counterを推測で補完してはならない。

---

## 7.5 Observer Effect

handle保持により、

* process object lifetime延長
* PID再利用遅延
* Probe handle resource消費

が発生し得る。

そのため、

```text
process_handle_retention_policy
process_handle_retention_limit
```

をrun Evidenceへ保存する。

---

# 8. Process Set and Fidelity

target rootとそのdescendant、または観測対象として選択されたprocess群を、

**selected process set**

とする。

以下は別保証として扱う。

* containment
* process count
* process identity
* create / exit timing

---

## 8.1 Containment Fidelity

```text
containment_fidelity:
    JOB_CONTAINED
    JOB_CONTAINED_WITH_PARENT_JOB
    JOB_BREAKAWAY_ALLOWED
    SNAPSHOT_DERIVED
    UNKNOWN
```

値は、

* assignment result
* Job limit flags
* root membership verification
* nested Job状態
* observed containment

から導出する。

---

## 8.2 Count Fidelity

```text
count_fidelity:
    JOB_EXACT_ASSOCIATION_COUNT
    SAMPLING_DERIVED
    UNKNOWN
```

Windows Job Objectの`TotalProcesses`は、

**Job lifetime中にJobへ関連付けられたprocess総数**

としてraw counterを保存する。

---

## 8.3 Identity/Event Fidelity

```text
identity_event_fidelity:
    COMPLETION_PORT_BEST_EFFORT
    SNAPSHOT_DIFF_QUANTIZED
    MIXED
    UNKNOWN
```

Jobへ入ったprocess総数が分かることと、

すべてのprocess identity・create時刻・exit時刻を観測できたことを混同してはならない。

---

# 9. Windows Job Process Counts

最低以下をrawで保持する。

```text
job_total_processes_os
job_active_processes_os
job_total_terminated_by_limit_os
```

意味はWindows API semanticsそのものに従う。

`TotalTerminatedProcesses`を、

「終了したprocess総数」

として扱ってはならない。

---

## 9.1 Observed Identity Count

Probe側で個別identityまで観測できたJob process数を、

```text
observed_distinct_job_processes
```

として保持する。

---

## 9.2 Unobserved Identity Count

```text
job_processes_without_observed_identity
=
job_total_processes_os
-
observed_distinct_job_processes
```

を導出可能にする。

これは、

**Jobへ関連付けられたことはOS counter上確認できるが、Probeが個別identityとして観測できなかったprocess数**

を意味する。

process tree全体で見逃した総数を意味しない。

---

# 10. ActiveProcesses Semantics

`job_active_processes_os`はWindows Job Object APIが返すraw counterとして保存する。

Probeは、

```text
TotalProcesses - ActiveProcesses
```

を「終了済みprocess数」と解釈してはならない。

process handleその他のreference保持により、終了後も`ActiveProcesses` semanticsへ影響し得るためである。

さらに、

**`job_active_processes_os`はprocess-handle retention policyが異なるrun間で直接比較可能なperformance metricとして扱ってはならない。**

raw Job state Evidenceとしてのみ使用する。

---

# 11. Windows Job Safety Policy

## 11.1 KILL_ON_JOB_CLOSEは禁止

Probe Jobでは、

```text
JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
```

を設定してはならない。

Probe異常終了によってtargetをterminateすることを防ぐ。

---

## 11.2 Attach Job Assignment

Attach modeではJob assignmentをdefault OFFとする。

```text
--attach-job
```

等の明示opt-inでのみ許可する。

Evidenceへ、

```text
attach_job_assignment_requested
attach_job_assignment_result
```

を保存する。

---

## 11.3 Breakaway

以下をraw evidenceとして保存する。

```text
job_limit_flags
root_is_in_probe_job
nested_job_detected
assignment_result
```

breakaway可能性を設定だけから推測しない。

---

# 12. Sampling Model

すべてのcounterを同一頻度で取得する必要はない。

tiered samplingを使用する。

---

## Tier A: Core Process Counters

Default:

**500 ms**

raw対象:

* working set / RSS
* private bytes where available
* virtual memory
* cumulative user CPU time
* cumulative kernel CPU time
* process cumulative I/O counters
* thread count
* handle / fd count
* process state
* process membership observation

**CPU utilization等のrateはcollectorで一次Evidenceとして生成しない。**

---

## Tier B: System Counters

Default:

**1 second**

raw対象:

* system cumulative CPU times
* per-core cumulative CPU times where available
* available RAM
* committed memory
* swap / pagefile
* cache indicators
* cumulative disk counters
* memory pressure source counters
* load counters where natively supplied

---

## Tier C: Slow Hardware Sensors

Default:

**2 seconds**

対象:

* CPU frequency
* CPU temperature
* GPU telemetry
* storage temperature
* power indicators

取得可能なもののみ。

---

## Tier D: Static Environment

run開始時に一度取得する。

* CPU model
* CPU topology
* physical cores
* logical processors
* NUMA topology
* installed RAM
* OS
* kernel/build
* storage model
* filesystem
* volume
* Probe version
* executable version

---

# 13. Time Model

各recordには最低2種類の時刻を持たせる。

```text
wall_time_utc
monotonic_ns
```

duration、sample gap、rateの計算にはmonotonic clockをauthorityとして使用する。

---

# 14. Sampling Scheduling

samplingは相対sleepの繰り返しではなく、

```text
start_time + n * interval
```

によるabsolute deadline schedulingを基本とする。

---

# 15. Windows Timer Policy

Windowsでcore samplerを実装する場合、

```text
timeBeginPeriod
```

等によるsystem-wide timer resolution変更を使用してはならない。

対応OSではhigh-resolution waitable timerを優先使用可能とする。

使用したmechanismをEvidenceへ記録する。

```text
timer_backend
timer_resolution_mode
```

---

# 16. Sampler Thread Priority

sampler thread priorityは設定可能とする。

例:

```text
NORMAL
ABOVE_NORMAL
```

選択値をEvidenceへ記録する。

priority変更自体もobserver effectとして扱う。

---

# 17. Sampling Quality

各sampleについて、

```text
scheduled_time
actual_time
sampling_delay
gap_from_previous_sample
```

を計算可能にする。

summaryへ最低以下を出す。

```text
sample_count
max_sample_gap_exact
p50_sample_gap_histogram_estimate
p95_sample_gap_histogram_estimate
p99_sample_gap_histogram_estimate
missed_or_delayed_samples
```

---

# 18. Exact Gates vs Estimated Distribution

分位数はbounded deterministic histogramから推定可能とする。

strict gateは推定分位数を使用しない。

例えば、

```text
max_sample_gap_exact <= 1.0s
```

のようにexact値を使用する。

---

# 19. Memory Metrics

## 19.1 Per-process Raw Metrics

可能なものを保存する。

```text
working_set_bytes
rss_bytes
private_bytes
virtual_bytes

os_reported_peak_working_set_bytes
os_reported_peak_private_commit_bytes

shared_bytes
uss_bytes
pss_bytes
swap_bytes

page_faults
major_page_faults
minor_page_faults
```

availabilityはplatform依存。

---

# 20. Memory Semantics

異なるOSの似たcounterを同一意味として偽装してはならない。

Windows例:

```text
windows.working_set_bytes
windows.private_usage_bytes
windows.peak_working_set_bytes
windows.peak_pagefile_usage_bytes
```

Linux例:

```text
linux.rss_bytes
linux.pss_bytes
linux.uss_bytes
```

macOS例:

```text
macos.resident_size_bytes
macos.phys_footprint_bytes
```

portable viewを作る場合、

```text
source_metric
semantic_quality
```

を必ず記録する。

---

# 21. Aggregation Semantics

## 21.1 Working Set / RSS

per-process working setの単純合計はshared pageを多重計上する。

よって、

```text
process_set_working_set_sum_bytes
```

と明示する。

```text
aggregation_semantics:
SUM_OF_PROCESS_WORKING_SETS_SHARED_PAGES_MAY_BE_DOUBLE_COUNTED
```

unique physical memory量とは呼ばない。

---

## 21.2 Private Bytes

platform semantics上妥当な場合、

```text
process_set_private_bytes_sum
```

として合計する。

---

## 21.3 Linux PSS

取得可能な場合、

```text
process_set_pss_sum_bytes
```

を別metricとして保持する。

---

# 22. Process-set CPU and I/O Authority

Windows Launch modeでJob-contained measurementを行う場合、

process-set cumulative CPU / I/OについてJob Object accountingをset-level authority候補とする。

保存例:

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

per-process raw cumulative valuesも別系統で残す。

---

# 23. Exited Process Accounting

生存processだけを単純合計して、

process終了時に累積CPU/I/O aggregateが減少する実装を禁止する。

---

## 23.1 Windows Job Launch Mode

Job accountingをset-level cumulative authorityとして使用可能とする。

---

## 23.2 Attach / Non-job Mode

可能な限り、

```text
exited_process_final_accumulator
```

を使用する。

```text
sum(live current)
+
sum(exited final)
```

としてprocess-set cumulative aggregateを導出する。

取得不能final counterを推測してはならない。

---

# 24. Peak Memory

sampled maximumとOS-maintained lifetime peakを分離する。

```text
peak_working_set_sampled_bytes
peak_working_set_os_reported_bytes

peak_private_sampled_bytes
peak_private_os_reported_bytes
```

---

# 25. Peak Sampling Cross-check

OS-reported peakとsampled peakの両方が存在する場合、

```text
peak_working_set_sampling_miss_bytes
peak_working_set_sampling_miss_ratio
```

を導出可能にする。

通常期待:

```text
OS-reported peak >= sampled peak
```

---

## 25.1 Inconsistent Peak Evidence

もし、

```text
OS-reported peak < sampled peak
```

となった場合、

値を0へclampしてはならない。

以下のようなcross-check anomalyを生成する。

```text
PEAK_CROSSCHECK_INCONSISTENT
```

保存対象:

```text
sampled_peak
os_reported_peak
difference
first_detected_time
affected_metric
```

measurement semantics、collector bug、counter timing、platform behaviour等の調査対象とする。

---

# 26. Private-vs-Working-set Gap

```text
private_minus_working_set_bytes(t)
=
private_bytes(t)
-
working_set_bytes(t)
```

を定義し、

```text
max_private_minus_working_set_bytes
max_private_minus_working_set_time
```

を導出する。

異なる時刻の独立peak同士を引いてはならない。

---

# 27. System Memory

可能ならrawとして以下を取得する。

```text
physical_memory_total
physical_memory_available

swap_total
swap_used

commit_current
commit_limit

system_cache_bytes
standby_or_cache_indicators

paging_cumulative_counters
```

derived percentage等はsummary側で計算する。

---

# 28. Page Cache

system-wide cache量をtarget固有cache residencyと解釈してはならない。

```text
target_page_cache_residency
```

を根拠なく導出してはならない。

---

# 29. CPU Metrics

process raw:

```text
user_cpu_time
kernel_cpu_time
total_cpu_time
processor_number where available
priority
affinity
context_switch_count where cumulative counter exists
thread_count
```

derived:

```text
cpu_utilization
average_cpu_utilization
peak_cpu_utilization
```

**derived metricはraw collector authorityではない。**

system raw:

```text
system_user_cpu_time
system_kernel_cpu_time
system_idle_cpu_time
per_core_cumulative_cpu_times
```

derived:

```text
total_cpu_utilization
per_core_cpu_utilization
```

---

# 30. CPU Frequency and Throttling

取得可能なら、

```text
current_frequency
base_frequency
maximum_frequency
per_core_frequency
```

等を保存する。

hardware sensor系はcumulativeでない場合があるため、瞬時観測値として明示する。

---

# 31. Temperature

取得可能なら、

```text
cpu_temperature
gpu_temperature
storage_temperature
```

を取得する。

取得不能でcore measurementを失敗させない。

---

# 32. Process I/O

各processについて可能ならraw cumulativeとして、

```text
read_bytes
write_bytes
other_bytes

read_operations
write_operations
other_operations
```

を保存する。

rateは後処理で導出する。

---

# 33. Storage Device Metrics

collectorは可能な限りraw cumulative counterを保存する。

例:

```text
disk_bytes_read_total
disk_bytes_written_total

disk_reads_total
disk_writes_total

disk_read_time_total
disk_write_time_total
disk_busy_time_total
```

以下はderivedであり、一次raw collection値とはしない。

```text
read_bytes_per_second
write_bytes_per_second
read_iops
write_iops
average_latency
busy_ratio
```

OSがrateそのものしか提供しない場合は、その事実をsemantic metadataへ記録する。

---

# 34. Path-to-Storage Mapping

指定されたroot/pathについて、

* logical volume
* filesystem
* physical device

を記録可能にする。

対象例:

```text
source root
generation root
temp root
spool root
database path
evidence output root
```

---

# 35. Path Privacy

defaultではuser profile部分を、

```text
%USERPROFILE%
$HOME
```

等へ正規化する。

raw absolute path保存はopt-in。

---

# 36. Disk Capacity

開始時・終了時に、

```text
free_bytes
total_bytes
```

を保存する。

`used_bytes`は必要に応じderived可能。

---

# 37. GPU Metrics

安全かつ低侵襲に取得可能ならoptionalで保存する。

---

# 38. Network Metrics

可能ならraw cumulativeとして、

```text
bytes_sent_total
bytes_received_total
packets_sent_total
packets_received_total
```

を保存する。

throughputはderived。

---

# 39. Scheduler / Process Behaviour

可能なら、

```text
process_create
process_exit
thread_count
context_switch_count
priority
affinity
handle_count
fd_count
```

を保存する。

---

# 40. Phase / Progress Integration

ProbeはCloseRAG固有であってはならない。

optional external adapterで、

```text
phase
subphase
operation_id
processed_items
processed_bytes
total_items
total_bytes
```

等を取得可能にする。

---

# 41. Read-only Phase Adapters

既存、

* progress file
* structured log
* event file

をread-onlyで読むadapterを優先する。

Probe使用のためだけにCloseRAG本体へ変更を要求しない。

---

# 42. Generic Marker Channel

Probe側所有channelを使用可能。

例:

* named pipe
* local socket
* marker directory
* marker file

markerなしでもProbeは動作する。

---

# 43. Baseline Windows

optionalで、

```text
pre-run baseline
active run
post-run observation
```

を取得する。

strict profileでは60秒baseline等を設定可能。

---

# 44. Terminal Observation

target終了時、可能な限りterminal stateを取得する。

```text
target_exit_time
exit_code

last_successful_live_sample_time

terminal_cpu_counter_present
terminal_io_counter_present
terminal_memory_sample_present
```

memoryについて、

```text
last_live_working_set_sample_bytes
last_live_private_sample_bytes
```

等として保存する。

終了後memoryをexact terminal値と偽装しない。

---

# 45. Evidence Storage

run directory:

```text
perf-evidence/
  <run-id>/
    manifest.json

    host.json
    target.json
    config.json
    capabilities.json

    samples.ndjson
    processes.ndjson
    events.ndjson
    progress.ndjson

    summary.json

    api_semantics.json
    calibration.json

    platform/
      windows.json
      linux.json
      macos.json

    hashes.json
```

未使用artifactは省略可能。

---

# 46. NDJSON First

raw streamはNDJSONを基本とする。

全sampleをRAMへ蓄積して最後にdumpする方式を禁止する。

---

# 47. Process Local Index

`processes.ndjson`で、

```text
process_local_id
```

を確定し、sample側はlocal id参照を使用する。

---

# 48. Crash-safe NDJSON Semantics

writerはsingle-writerを基本とする。

複数threadによる同一fileへの直接交錯appendを禁止する。

readerは、

**EOF直前の不完全な最終行のみ**

破棄可能とする。

途中破損を黙って無視してはならない。

---

# 49. Boundedness

Probe memoryはrun durationにも終了済みprocess総数にも比例して無制限増加してはならない。

raw sampleは逐次diskへ流す。

runtime aggregateは、

```text
running min/max
exact counters
fixed histogram
bounded deterministic structures
```

を使用する。

process handle retentionについても§7のbounded lifecycleに従う。

---

# 50. Deterministic Histogram

quantile approximationにはschema固定のdeterministic histogramを使用する。

merge orderやrandomnessに依存する近似器をstrict authorityとして使用しない。

---

# 51. Independent Summary Pass

`summary.json`は、

**保存済みraw evidenceを読み直す独立code path**

から生成する。

---

# 52. Deterministic Summary Artifact

`summary.json`は決定論的artifactとする。

以下のような非決定的fieldを含めてはならない。

```text
summary_generation_wall_clock_time
summary_generation_hostname
random identifier
current process id
```

その種のprovenanceが必要なら`manifest.json`へ置く。

同一raw evidenceと同一summary schema/versionから生成した`summary.json`は、canonical serialization条件下でbyte-equivalentであることを目標とする。

---

# 53. Flush Policy

例:

```text
buffered append
periodic flush
final durable flush
```

flush cadenceをEvidenceへ保存する。

---

# 54. Capability Model

run単位で固定availabilityは、

```text
capabilities.json
```

へまとめる。

---

# 55. Dynamic Metric Failure

sampleごとにavailabilityが変化する場合のみ、

```text
TEMPORARILY_UNAVAILABLE
COLLECTION_ERROR
```

等をrecordする。

missingを0で埋めない。

---

# 56. Metric Availability States

```text
AVAILABLE
UNSUPPORTED
PERMISSION_DENIED
TEMPORARILY_UNAVAILABLE
UNATTRIBUTABLE
COLLECTION_ERROR
```

を最低限扱う。

---

# 57. Profiles

## broad

可能なmetricを広く取得する。

---

## strict-memory

required evidence例:

```text
selected process observation
private bytes
working set
per-process values
CPU
I/O
process fidelity evidence
phase where configured
<=500 ms nominal interval
exact max sample-gap gate
terminal observation
applicable calibration
```

欠落時fail-closed。

---

## minimal

低observer-effect用途。

---

# 58. Observer Self-monitoring

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

を保存する。

---

# 59. No Hidden Collection

defaultで以下を取得しない。

* environment variables
* file contents
* network payload
* clipboard
* credentials
* process memory contents
* secret tokens
* arbitrary user documents

full command line保存はopt-in。

---

# 60. Static Environment Evidence

最低:

```text
OS name
OS version
OS build
architecture

CPU vendor
CPU model
physical cores
logical processors
NUMA nodes

installed RAM

storage device model
storage bus/type
filesystem

Probe version
Probe build identity
collector version
Probe configuration
```

を保存する。

---

# 61. Target Executable Evidence

可能なら、

```text
normalized_path
file_size
mtime
sha256 optional
version
```

を保存する。

---

# 62. Reproducibility Context

可能なら、

```text
power plan / power mode
battery / AC
virtualization status
container status
CPU affinity
process priority
system uptime
```

を保存する。

---

# 63. Derived Summary

`summary.json`はraw evidenceから決定論的に生成する。

---

## 63.1 Memory

```text
baseline_working_set

peak_working_set_sampled
peak_working_set_os_reported

peak_private_sampled
peak_private_os_reported

max_private_minus_working_set_bytes
max_private_minus_working_set_time
```

---

## 63.2 Terminal Memory Observation

```text
last_live_working_set_sample_bytes
last_live_working_set_sample_time

last_live_private_sample_bytes
last_live_private_sample_time
```

これらはterminal exact memoryではなく、

**最後に成功したlive memory observation**

である。

---

## 63.3 Terminal CPU / I/O

取得可能なら、

```text
terminal_user_cpu_time
terminal_kernel_cpu_time

terminal_read_bytes
terminal_write_bytes
terminal_read_operations
terminal_write_operations

terminal_counter_fidelity
```

をsummaryへ含める。

---

## 63.4 CPU Derived Metrics

```text
total_cpu_time
average_cpu_utilization
peak_cpu_utilization
```

raw cumulative time seriesから導出する。

---

## 63.5 I/O Derived Metrics

```text
total_read_bytes
total_write_bytes
total_read_operations
total_write_operations

peak_read_rate
peak_write_rate
```

raw cumulative countersから導出する。

---

## 63.6 Timing

```text
elapsed
sample_count
max_sample_gap_exact

p50_gap_histogram_estimate
p95_gap_histogram_estimate
p99_gap_histogram_estimate
```

---

## 63.7 Process

```text
maximum_observed_process_count
observed_distinct_process_count

job_total_processes_os where available
job_processes_without_observed_identity where derivable

maximum_thread_count
maximum_probe_handle_count
handle_retention_degraded
```

---

## 63.8 System

取得可能な範囲で、

```text
minimum_available_memory
maximum_memory_pressure
maximum_disk_queue
minimum_free_disk
```

等を導出する。

---

# 64. Phase-derived Summary

phase markerが存在する場合、

phaseごとに、

```text
elapsed
peak_working_set
peak_private
CPU time
read bytes
write bytes
observed process count
```

を導出する。

---

# 65. Growth Analysis Primitives

raw evidenceから、

```text
memory slope
I/O rate
CPU saturation duration
working-set stabilization
burst behaviour
plateau
```

等を後から分析可能にする。

---

# 66. No Automatic Causal Claims

Probeはraw counterだけから、

```text
memory leak
disk bottleneck
thermal throttling caused slowdown
page cache caused slowdown
```

等を断定しない。

---

# 67. Raw First Principle

可能な限りOS cumulative counterをraw evidenceへ保存する。

rate・utilization・percentage等は原則として後処理で導出する。

---

# 68. No Premature Aggregation

summaryだけを保存してraw sampleを捨ててはならない。

raw evidenceが一次authority。

---

# 69. Windows Collector

Windowsでは最低以下をnativeに実装する。

候補API:

```text
GetProcessMemoryInfo
PROCESS_MEMORY_COUNTERS_EX

GetProcessIoCounters
GetProcessTimes
GetProcessHandleCount

CreateProcess / suspended launch
Job Object APIs

GlobalMemoryStatusEx
GetPerformanceInfo

GetSystemTimes
GetLogicalProcessorInformationEx

GetDiskFreeSpaceEx

Waitable Timer APIs
```

---

# 70. Linux Adapter

platform abstractionを最初から分離する。

候補:

```text
/proc/<pid>/stat
/proc/<pid>/status
/proc/<pid>/io
/proc/<pid>/smaps_rollup

/proc/meminfo
/proc/stat
/proc/diskstats

sysfs
```

---

# 71. macOS Adapter

候補:

```text
Mach APIs
host_statistics
task_info
sysctl
IOKit
```

---

# 72. Platform Trait

概念例:

```text
trait PlatformCollector {
    collect_static_host_info();
    enumerate_processes();
    collect_process_metrics();
    collect_process_set_metrics();
    collect_system_metrics();
    collect_storage_metrics();
    collect_sensor_metrics();
}
```

---

# 73. Optional Advanced Collectors

将来追加候補:

```text
ETW
Linux perf
eBPF
hardware performance counters
Intel RAPL
AMD telemetry
NVIDIA NVML
SMART
IOKit sensors
```

---

# 74. API Semantics Registry

各metricについて、

```text
metric_id
platform
collector_api
documented_semantics
aggregation_semantics
availability_constraints
semantic_basis
```

を管理する。

---

# 75. Semantics Evidence Levels

最低以下を区別する。

```text
DOCUMENTED_SEMANTICS
EMPIRICALLY_PINNED
CROSS_CALIBRATED
```

---

# 76. API Semantics Pinning Harness

counterの意味を実測で固定するminimal workloadを用意する。

対象例:

### Private Commit

reserve / commit / touch / decommit / release。

### Working Set

allocate / touch / trim相当操作。

### Job CPU

child CPU workload → child exit → Job cumulative CPU確認。

### Job I/O

child I/O workload → child exit → Job cumulative I/O確認。

### OS Peak

短時間memory spike → sampled peak / OS peak比較。

### Peak Job Memory

memory limit未設定時のpeak Job memory挙動確認。

### Exited Process Handle

process終了後の、

```text
GetProcessTimes
GetProcessIoCounters
GetExitCodeProcess
```

挙動確認。

---

# 77. api_semantics.json

semantic pinning結果として最低:

```text
Probe version
collector version
OS version
OS build
architecture

test name
metric
expected semantic
observed result
PASS / FAIL / UNKNOWN
```

を保存する。

---

# 78. Calibration

Probe measurementを独立系列と比較する。

候補:

* existing `canonical_monitor.py`
* PDH
* OS built-in instrumentation
* known synthetic ground truth

同一API code path同士だけの比較をcross-calibrationと呼ばない。

---

# 79. Calibration Scope

## 79.1 API Semantic Calibration

主に、

```text
Probe collector version
OS family
OS build
architecture
```

に紐付く。

---

## 79.2 Hardware / Sensor Calibration

主に、

```text
CPU model
GPU model
storage model
driver
sensor backend
firmware
```

等に紐付く。

---

# 80. Calibration Status

```text
calibration_status:
    CURRENT
    STALE
    INAPPLICABLE
    UNCALIBRATED
```

を使用する。

measurement validityとは別軸。

---

# 81. Calibration Reference

`manifest.json`から、

```text
calibration_reference
```

を参照可能にする。

---

# 82. Qualification Eligibility

例:

```text
ELIGIBLE
INELIGIBLE_REQUIRED_METRIC_MISSING
INELIGIBLE_CALIBRATION_STALE
INELIGIBLE_SAMPLING_GAP
INELIGIBLE_PROCESS_SET_AMBIGUOUS
```

---

# 83. Measurement Validity

```text
VALID
DEGRADED
INVALID
```

を最低限扱う。

calibration statusとは別軸。

---

# 84. Run Identity

各runへopaque `run_id`を割り当てる。

performance結果から生成しない。

---

# 85. Evidence Integrity

run完了時、各Evidence fileについて、

```text
SHA-256
size
```

を保存する。

---

# 86. Final Run States

```text
COMPLETE
TARGET_FAILED
PROBE_FAILED
ABORTED
INCOMPLETE
EVIDENCE_INVALID
```

を最低限扱う。

---

# 87. CLI Example

```text
perf-probe run \
  --output D:/AgentData/perf-evidence \
  --profile broad \
  --interval 500ms \
  -- <target command>
```

```text
perf-probe attach \
  --pid 12345 \
  --output D:/AgentData/perf-evidence
```

```text
perf-probe run \
  --profile strict-memory \
  --baseline 60s \
  -- <target command>
```

---

# 88. Streaming Requirement

sampling durationまたはprocess数増加に対してmemory boundedであること。

---

# 89. Separation from CloseRAG

ProbeをCloseRAG production runtime dependencyにしない。

基本関係:

```text
CloseRAG
   │
   │ observed by
   ▼
perf-probe
```

---

# 90. Parallel Development Rule

Probe実装のために、

* CloseRAG production code
* Generation format
* retrieval engine
* ingestion engine
* Product DB
* current qualification evidence

を変更してはならない。

---

# 91. Existing Monitor Calibration

既存`canonical_monitor.py`を即削除しない。

比較結果をProbe calibration evidenceとして扱う。

---

# 92. Synthetic Test Workloads

最低:

* Memory Ramp
* Memory Spike
* Child Tree
* Short-lived Child
* I/O Sequential Read
* I/O Sequential Write
* CPU Single-thread
* CPU Multi-thread
* Child CPU then Exit
* Child I/O then Exit

を用意する。

Short-lived Childでは、

* Job association count
* observed identity count
* identity miss count
* handle retention boundedness

も検証する。

---

# 93. Acceptance Tests

## A1

target rootと観測可能child identityを正しく識別できる。

## A2

per-process raw値とderived aggregate semanticsが仕様どおり。

## A3

Windows private bytesを独立系列とcross-calibrateできる。

## A4

500ms samplingでexact max gapを取得できる。

## A5

target crash後も既存NDJSONがparse可能。

## A6

Probe crash後もflush済みEvidenceがparse可能。

## A7

long runおよび大量short-lived processでもProbe resource usageが無制限増加しない。

## A8

missing metricを0としない。

## A9

Probe resource usageがtarget aggregateへ混入しない。

## A10

同一raw evidence + 同一summary schema/versionからbyte-equivalent summaryを再生成できる。

## A11

Job CPU accountingがterminated child分を失わない。

## A12

Job I/O accountingがterminated child分を失わない。

## A13

Job process count差分を偽identityで補完しない。

## A14

OS peakとsampled peakを分離する。

## A15

Attach defaultでtargetをProbe Jobへassignしない。

## A16

`KILL_ON_JOB_CLOSE`を設定しない。

## A17

required Windows counterについてsemantics pinning Evidenceを生成できる。

## A18

exit finalization後に終了済みprocess handleをcloseする。

## A19

大量short-lived processでhandle countが設定上限を超えて無制限増加しない。

## A20

`OS peak < sampled peak`をclampせずcross-check inconsistencyとして保存する。

---

# 94. Performance Overhead Qualification

Probe ON/OFFを複数回、交互またはrandomized orderで実行する。

固定順、

```text
OFF全部
↓
ON全部
```

のみの比較は禁止する。

---

# 95. Observer Effect Report

平均だけでなく分布を報告する。

最低:

```text
elapsed distribution
CPU distribution
I/O distribution
memory distribution
sample gap distribution
```

---

# 96. Data Volume

compact NDJSONを基本とする。

将来、

```text
chunked NDJSON
zstd stream
```

等を追加可能。

---

# 97. Sensor Failure Isolation

optional sensor failureがcore samplerを止めてはならない。

---

# 98. Core Loop Priority

優先順位:

1. monotonic timestamp
2. process identity
3. working set
4. private memory
5. CPU raw counters
6. I/O raw counters
7. process-set accounting
8. sampling quality

---

# 99. Threading Model

例:

```text
Core Sampler
System Sampler
Slow Sensor Sampler
Process Event Collector
Writer
```

slow collectorがcore samplingをblockしてはならない。

---

# 100. Schema Lifecycle

現在のschemaは**draft**である。

暫定識別子として、

```text
perf-evidence-v1-draft
```

等を使用してよい。

正式な`perf-evidence-v1`をfreezeしてはならない。

---

# 101. Schema Freeze Gate

正式schema freezeは最低以下を満たした後とする。

1. Milestone 1実装完了
2. Milestone 1 acceptance tests PASS
3. API semantics pinning harness初回実行完了
4. required Windows counter semanticsの重大な未解決事項なし
5. Evidence bundle schemaの実run検証完了
6. summary独立再生成PASS

これらを満たした後に、

```text
perf-evidence-v1
```

をfreeze可能とする。

---

# 102. Milestone 1 Core Evidence

Milestone 1の実装内容は別文書、

**Performance Evidence Probe Milestone 1 Implementation Contract**

をauthorityとして使用する。

本仕様は設計referenceであり、Milestone 1の日常的な実装contractを兼ねない。

---

# 103. Milestone 2 Measurement Trust

追加予定:

* OS-reported peak
* sampled / OS peak comparison
* per-core CPU
* advanced storage counters
* page cache indicators
* page faults
* frequency
* power context
* phase adapters
* baseline support
* strict-memory profile
* API semantics registry
* pinning harness
* canonical monitor calibration
* calibration applicability
* qualification eligibility

ただしschema freeze gateに必要なpinning harnessの最小部分はMilestone 1直後に先行実行可能とする。

---

# 104. Milestone 3 Optional Sensors and Platforms

追加予定:

* temperature
* GPU
* SMART
* advanced disk latency
* hardware counters
* Linux adapter
* macOS adapter

---

# 105. Authority Separation

例:

```text
Per-process memory
    → per-process OS counter authority

Process-set CPU/I/O
    → Job accounting authority

Lifetime peak
    → OS peak counter authority

Time series
    → sampled raw evidence authority

Observed identity
    → process registry authority

Job-associated process count
    → Job TotalProcesses authority
```

複数authorityを雑に一つへ統合しない。

---

# 106. Cross-check Principle

複数authority系列が存在する場合、一方を捨てず両方残す。

差異自体をmeasurement quality evidenceとして扱う。

---

# 107. No False Completeness

以下を禁止する。

* sampled eventだけから全process観測済みと主張
* working-set sumをunique physical memoryと呼ぶ
* Job `TotalTerminatedProcesses`を全exit数と呼ぶ
* `TotalProcesses - ActiveProcesses`を終了済み総数と呼ぶ
* missing metricを0で補う
* sampled peakをtrue lifetime peakと呼ぶ
* inconsistent peakを0 clampする
* stale calibrationをcurrent扱いする
* documentation semanticsとempirical behaviourを混同する
* rateのみ保存してraw cumulative counterを捨てる

---

# 108. Measurement Claim Structure

将来、主要claimを、

```text
claim
metric
source API
raw evidence
semantic basis
calibration status
fidelity
applicability
```

へ結びつけられる構造を維持する。

---

# 109. Design Principle

本Probeは、

「測定後に別counterが必要になり、高コストworkloadを再実行する」

事態を可能な限り減らす。

> **測定時には、安く取得できる事実を広く保存する。
> 何を見るかは後から決める。**

---

# 110. Trust Principle

大量に数字を採ることと、数字の意味が正しいことは別問題である。

したがって、

* counter semantics
* aggregation semantics
* process fidelity
* calibration
* sampling quality
* observer effect

もEvidenceとして保存する。

---

# 111. Final Boundary

Performance Evidence Probeは、

**Performance Evidence Acquisition Infrastructure**

である。

```text
Probe
    ↓
Raw Observations

Semantics / Calibration
    ↓
Trusted Evidence

Analysis
    ↓
Interpretation

Qualification
    ↓
PASS / FAIL

Engineering
    ↓
Optimization
```

を分離する。

---

# 112. v0.2.1 Core Thesis

> **Raw first.
> Semantics explicit.
> Fidelity explicit.
> Calibration explicit.
> Interpretation later.**

性能測定値だけでなく、

**その測定値をどこまで信用してよいか**

まで保存することを、本Probeの完成条件とする。
