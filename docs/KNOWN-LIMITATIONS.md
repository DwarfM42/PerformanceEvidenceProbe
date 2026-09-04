# Known limitations

This is a runnable Windows performance-evidence collector, not a performance
qualification or certification system. These are current limitations of the
implemented public release, not promises about future scope.

## Platform and scope

- **Supported collection:** Windows 10/11 x64 through `perf-probe run` and
  default `perf-probe attach`.
- **Unsupported collection:** Linux and macOS; the CLI rejects `run` and
  `attach` outside Windows. Other Windows architectures are unqualified.
- **Out of scope:** advanced sensors, profiler-style function attribution,
  debugging, code injection, automatic diagnosis or tuning, calibration, and
  performance certification.
- The evidence schema is an implementation draft (`perf-evidence-v1-draft`),
  not a frozen or portable interchange contract.

## Observation boundary

- Launch mode assigns the launched target to a non-destructive Job with zero
  limit flags. It does not enable kill-on-close or apply Job performance limits.
  This is observation/accounting containment, not a guarantee that a workload
  is safe, correct, isolated, or representative.
- Default attach does not create a Job or assign the specified target to one.
  `--attach-job` intentionally fails closed as unimplemented.
- The collector uses process handles and Windows APIs; it does not inspect
  source, heap objects, allocation stacks, file contents, network payloads,
  environment variables, or application-level correctness.

## Coverage and measurement limits

- Sampling is nominally every 500 ms. Peaks that occur between samples can be
  missed; sampled peaks are not OS lifetime peaks.
- Descendant discovery is snapshot-based and retention is bounded. Process
  races, access failures, PID reuse protection needs, and the configured handle
  limit can leave discovery or terminal measurements incomplete. The collector
  records degradation rather than fabricating values.
- Launch-mode Job accounting is OS aggregate accounting. It can exceed the set
  of observed identities; the derived difference is not filled with invented
  process records. Default attach has no Probe Job accounting.
- Process-set working-set sums are not unique physical-memory measurements and
  may double count shared pages.
- A terminal event is an observation attempt after exit. Do not assume every
  counter or every observed process has a final terminal value.
- A normal completed run writes a summary and metadata. Forced interruption can
  leave only parseable raw NDJSON; it does not produce a completed summary or
  manifest. There is no graceful Control-C finalization protocol in this release.

## Evidence and trust limits

- Completed NDJSON records are flushed and an incomplete final EOF fragment may
  be discarded, but there is no hash inventory, signature, sequence envelope,
  authenticity proof, or offline verification command.
- `summary.json` is deterministic for identical complete raw inputs, but it is
  derived output, not an independent evidence authority.
- The project has Windows runtime and synthetic workload tests, but no
  calibration suite, OS-peak comparison, independent semantic pinning, or
  qualification campaign. Passing tests do not certify a workload or result.
- Generated bundles can contain host and target metadata. Review them before
  disclosure; raw evidence is not automatically sanitized for redistribution.

See the [evidence schema](EVIDENCE-SCHEMA-DRAFT.md) for exact draft artifact
semantics and the root [README](../README.md) for the supported workflow.
