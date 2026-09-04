# Documentation map

The root [README](../README.md) is the starting point for users. It states the
current qualified platform, exact CLI workflow, safety boundary, and public
claim limits.

| Document | Classification | Use it for |
|---|---|---|
| [Evidence schema draft](EVIDENCE-SCHEMA-DRAFT.md) | Current public technical reference | Bundle layout, raw/derived distinction, recovery behavior, and privacy considerations. |
| [Known limitations](KNOWN-LIMITATIONS.md) | Current public technical reference | Qualified scope and explicit limits that consumers must preserve. |
| [Public-claim audit](PUBLIC-CLAIM-AUDIT-2026-09-04.md) | Current release-preparation record | Claim classifications, canonical test-count reconciliation, and the no-dashboard authority boundary. |
| [Milestone 1 architecture](MILESTONE-1-ARCHITECTURE.md) | Useful implementation note | Collector ownership, sampling, writer, and summary design. |
| [Milestone 1 implementation contract v0.1](Performance%20Evidence%20Probe%20Milestone%201%20Implementation%20Contract%20v0.1.md) | Historical but valuable technical contract | Detailed Windows Milestone 1 requirements and rationale. Current source, tests, and user-facing documents take precedence where it differs. |
| [Repository closeout](REPOSITORY-CLOSEOUT.md) | Historical record | The August 2026 source-control baseline and its then-current verification. |
| [License and provenance audit](LICENSE-AUDIT-2026-09-04.md) | Current release record | License decision, source provenance, dependency notices, and future-reuse boundary. |

The former checked-in smoke bundle was removed because its emitted schema and
boot-identity format were stale. Generate a fresh bundle with the root README
instead of treating old output as representative evidence.

The former v0.2.1 design specification was removed from the current public
document set because it mixed unimplemented cross-platform plans and
product-specific development context with the implemented collector. It remains
available in Git history; it is not a current support or product claim.
