# PerformanceEvidenceProbe v0.2.0

## Qualified platform scope

- **Windows x86_64:** full-accounting `run` and observation-only `attach`, including Windows Job accounting for launched workloads.
- **Linux x86_64:** bounded direct-root `run` and observation-only single-root `attach`.
- **macOS arm64:** bounded direct-root `run` and observation-only single-root `attach`.

Linux and macOS are deliberately narrower than Windows. They do not claim Job accounting, containment, complete descendant or process-tree ownership, complete process-set totals, FD-to-handle equivalence, RSS or `phys_footprint` equivalence to Windows private bytes, Windows-shaped I/O, or Linux/macOS commit values as Windows commit values. Direct-root signal outcomes are platform terminal metadata, never synthetic exit codes.

## Evidence contract

- Package version: `0.2.0`.
- Evidence schema: `perf-evidence-v2-draft` (implemented draft; not a frozen interchange contract).
- A numeric zero is an observed value, never an unavailable value.
- Missing optional canonical metrics use exact typed `metric_unavailable` evidence; `semantic_mismatch`, `authority_unavailable`, and `sampling_degraded` remain distinct.
- Derived witnesses are omitted when their exact inputs are unavailable; no witness is derived from an incomplete contributor set.

## Distribution

The intended user-facing release artifact is `perf-probe` (`perf-probe.exe` on Windows). `perf-workload` is a controlled synthetic test workload and is not a distributable product artifact.

See the root [README](../README.md), [known limitations](KNOWN-LIMITATIONS.md), [evidence schema draft](EVIDENCE-SCHEMA-DRAFT.md), and [third-party notices](../THIRD-PARTY-NOTICES.md) for operational limits and redistribution material.
