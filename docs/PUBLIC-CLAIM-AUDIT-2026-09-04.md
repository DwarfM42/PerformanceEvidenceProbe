# Public-claim audit — 2026-09-04

This release-preparation record reviews the public claims in the root README,
current schema and limitations documents, CLI help, Cargo metadata, licensing
documents, and related-tool references. It is an engineering evidence record,
not a certification or legal opinion.

## Claim classification

| Category | Current public claim boundary |
|---|---|
| Directly implemented and verified | On the verified Windows 10/11 x64 workflow, `run` and default `attach` collect a bounded bundle with raw NDJSON streams, context metadata, lifecycle/degradation records, and a deterministic reconstructed `summary.json` for identical complete raw inputs. |
| Implemented but not fully qualified | The `run` and `attach` source paths use Windows APIs and deliberately return `requires Windows` outside Windows. That behavior is implemented, but no non-Windows build or workflow has been qualified. |
| Design intent | The Probe owns observation and deterministic evidence production. It intentionally leaves visualization and interpretation to consumers. |
| Limitations | Observations are sampled, child/process coverage can be incomplete, the schema is a draft, summaries are derived rather than independent measurements, and bundles require privacy review before sharing. |
| Non-goals | Performance correctness, workload representativeness, diagnosis, profiling, tracing, tuning, cross-platform qualification, certification, and OS lifetime-peak measurement are not established by this project. |

## Canonical test command and 33-versus-31 reconciliation

The canonical command is:

```bash
bash scripts/cargo-local.sh test --all-targets --locked -- --nocapture
```

Its corresponding `--list` inventory was run at both
`ce400b72501f4bf30a1db95b6fe2dc1d793d7a60` and
`1fccd1538830e0404c153a79541f51b0be8dad19`. Both enumerate **33 tests**:

| Integration test binary | Tests |
|---|---:|
| `cli_summary` | 2 |
| `core_contracts` | 3 |
| `milestone1_contract_windows` | 7 |
| `milestone1_workloads_windows` | 14 |
| `ndjson_recovery` | 2 |
| `single_writer` | 1 |
| `summary_reconstruction` | 2 |
| `windows_runtime_smoke` | 2 |
| **Total** | **33** |

`src/lib.rs`, `src/main.rs`, and `src/bin/perf-workload.rs` are also selected
by `--all-targets`; each has zero unit tests. The `ce400b7..1fccd15` delta has
identical blobs for every `tests/` file and no `src/`, `Cargo.lock`, feature, or
target-definition change. Its only `Cargo.toml` change is the license expression.
Deleted examples are evidence artifacts, not Cargo test targets.

The reported **31** was an incomplete aggregation, not a changed invocation or
lost coverage: it omitted the two-test `windows_runtime_smoke` target. The other
seven integration binaries total 31. No relevant test coverage was removed.

## Evidence authority and downstream views

The authority boundary is:

```text
observation → canonical machine-readable evidence → deterministic Probe-derived summary → optional downstream view
```

Canonical evidence consists of the raw process, sample, and event streams plus
host, target, configuration, and capability metadata. `summary.json` is a
deterministic Probe-derived output reconstructed from persisted raw evidence. A
consumer may use scripts, `jq`, spreadsheets, data-analysis software,
visualization systems, or AI assistants to create a table, chart, explanation,
diagnosis, or conclusion. Those consumer-produced views do not become canonical
Probe evidence, and a deterministic summary does not make a workload
representative or correct.

## Related tools

The README comparison names `prmon`, `psrecord`, Metrace, Windows Performance
Recorder / ETW, `hyperfine`, ReBench, and Phoronix Test Suite only as neighboring
tools. It makes no novelty, priority, or ownership claim about monitoring,
sampling, counters, evidence recording, visualization, or benchmarking. The
Probe's stated distinction is limited to its own inspectable evidence-bundle
boundary: an explicitly bound observed workload, lifecycle/degradation records,
raw samples, context metadata, and a deterministic derived summary.