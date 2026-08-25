# Known limitations — Milestone 1 work-in-progress

This repository has a runnable Windows launch and default-attach vertical slice, but it **is not eligible for a Milestone 1 acceptance claim**.

## Authority and provenance blockers

1. The authoritative *Performance Evidence Probe Milestone 1 Implementation Contract* is absent. The parent specification says that contract governs day-to-day M1 implementation, so requirement-by-requirement certification and A1–A20 closure cannot be established.
2. The directory is not a Git repository. Source revision/clean-tree provenance cannot be recorded from this checkout.

## Runtime coverage gaps

- The process registry observes only the specified root process. It does not enumerate or retain child-process identities, so Job aggregate accounting cannot yet be reconciled against a complete observed process set.
- Launch uses suspended `CreateProcessW`, a non-destructive Job (zero limit flags), assignment, and membership verification. Completion-port notifications are not implemented; root exit is polled.
- Default attach opens a read/query/synchronization observation handle and never creates or assigns a Job. It waits for target exit; cancellation/control-C finalization is not implemented.
- `--attach-job` deliberately fails closed as unsupported.
- Job accounting is sampled with `QueryInformationJobObject`; `total_terminated_by_limit_os` is not collected and is recorded as zero only because no limits are configured.
- Process memory, CPU, I/O, and handle count are collected for the root. Thread count, host/system counters, storage counters, and probe self-observation are incomplete or represented as zero.
- `boot_identity` is a collector-time estimate rather than an authoritative OS boot identity.

## Evidence and boundedness gaps

- The writer channel is bounded and single-owner, but the current raw schema lacks a sequence envelope, strict closed deserialization, explicit availability values, periodic durability policy, manifest, capability inventory, final-state record, and hashes inventory.
- The summary reader remains a draft implementation and is not yet a bounded streaming verifier for arbitrarily long evidence runs.
- A zero metric can currently mean a true zero or an unavailable counter. Do not draw metric conclusions where collection provenance is absent.
- Writer backpressure error/degradation events, output path privacy normalization, and full crash-finalization semantics remain to be implemented.

## Validation gaps

- The project has smoke tests for real Windows `run` and default `attach`, but no full A1–A20 synthetic workload suite, calibration suite, OS-peak comparison, independent semantic pinning, or offline `verify` command.
- The sample bundle is a short `cmd.exe /c exit 0` smoke workload, not a performance characterization or calibration result.

These limitations are intentional fail-open-in-documentation disclosures, not claims that missing observations equal zero or that an unavailable capability was validated.
