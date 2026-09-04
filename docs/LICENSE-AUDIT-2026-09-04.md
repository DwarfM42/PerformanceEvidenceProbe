# License and provenance audit — 2026-09-04

## Decision

**Chosen outbound license: `MIT OR Apache-2.0`.**

This audit found no material provenance or dependency barrier to offering the
first-party PerformanceEvidenceProbe code and documentation under that dual
license. The decision is reproducible from the audited baseline:

- commit: `ce400b72501f4bf30a1db95b6fe2dc1d793d7a60`
- tree: `c9e6c0823256f749ecfa903ba476d93748d73d98`
- audit time: `2026-09-04 18:07:46 +0900`

This is a repository evidence record, not a legal opinion or a guarantee of
copyright title beyond the evidence inspected.

## State before this change

The baseline had one root `LICENSE` containing MIT text, `Cargo.toml` declared
`license = "MIT"`, and the README linked only that file. There were no
first-party source-file SPDX headers, no root `NOTICE`, `COPYING`, or
`COPYRIGHT` files, and no separate contribution policy. Copyright was stated
only in the MIT file as `Copyright (c) 2026 PerformanceEvidenceProbe
contributors`.

This change replaces that prior single-license presentation with `LICENSE-MIT`,
`LICENSE-APACHE`, Cargo metadata `MIT OR Apache-2.0`, this audit, third-party
notices, and an inbound-equals-outbound contribution policy.

## First-party provenance

The audited first-party set contained 31 tracked paths: Rust source and tests,
Cargo metadata/lockfile, shell tooling, and Markdown documentation. It contains
no images, logos, icons, fonts, sample data, fixtures, schemas imported from
external specifications, archives, executable binaries, or generated source
that embeds third-party material. Historical checked-in evidence samples were
removed before this audit; no current evidence bundle is tracked.

All reachable commits list the same Git author identity (`mikado
<sng.mem@gmail.com>`); history has no merge commits, co-author/sign-off
trailers, or external-contributor commits. Searches of first-party source and
history found no copied/adapted-source declarations, Stack Overflow references,
external-tool names, or preserved third-party copyright/license headers.

The implementation uses Rust crates and Windows APIs through `windows-sys`; it
does not include source copied from the process-monitoring, tracing, or
benchmarking projects reviewed below. Functional similarity, use of an API, or
use of a general programming technique is not treated here as copied source.

This supports a first-party provenance result of **no material third-party
source identified**. It cannot prove that a contributor had authority to submit
code not revealed by the repository; the contribution policy addresses that
forward-looking boundary.

## Dependencies and vendored source

`.cargo/config.toml` resolves crates through tracked `.vendor/`; Cargo package
contents exclude `.vendor/`. Checks of 3,100 vendored file digests against each
crate's `.cargo-checksum.json`, and of crate-package checksums against
`Cargo.lock`, reported zero mismatches.

For `x86_64-pc-windows-msvc`, the normal build closure has 46 third-party
packages (47 including this package). Its SPDX expressions are:

| Terms | Third-party packages | Result |
|---|---:|---|
| `MIT OR Apache-2.0` | 39 | Permissive; static linking does not impose a reciprocal project license. |
| `MIT` | 3 | Permissive; retain required copyright/license notices. |
| `Apache-2.0 OR MIT` | 1 | Permissive alternative. |
| `MIT/Apache-2.0` | 1 | Permissive alternative. |
| `Unlicense OR MIT` | 1 | Permissive alternative. |
| `(MIT OR Apache-2.0) AND Unicode-3.0` | 1 | Permissive, with a Unicode notice to retain. |

The direct runtime dependencies are `anyhow`, `clap`, `crossbeam-channel`,
`serde`, `serde_json`, `sha2`, `time`, and Windows-only `windows-sys`. The
normal closure also includes proc-macro crates used while compiling. There are
no manifest build dependencies and no optional dependency features. The
dev/test-only addition is `tempfile` and its four-package closure
(`fastrand`, `getrandom`, `once_cell`, and `tempfile`), all permissive.

No normal or dev Windows dependency has GPL, AGPL, LGPL, MPL, EPL, CDDL, SSPL,
BUSL/BSL, Elastic License, Commons Clause, non-commercial, custom, unknown, or
missing metadata. The complete 56-crate vendor set has one expression mentioning
LGPL: `r-efi 6.0.0` is `MIT OR Apache-2.0 OR LGPL-2.1-or-later`. It is absent
from the resolved Windows normal/dev closure and offers permissive alternatives;
it is not a static-linking obligation for this project configuration.

Two retained upstream notices deserve explicit handling:

- `unicode-ident 1.0.24` is in the normal closure and is additionally subject
to Unicode License v3. Preserve `.vendor/unicode-ident/LICENSE-UNICODE` or its
copyright/permission notice with redistributed source or binary notices.
- `.vendor/crossbeam-channel` retains a CC-BY-3.0 attribution for its upstream
`examples/matching.rs`, based on Stefan Nilsson's `matching.go`. The example is
not compiled by this project's normal Windows build. If vendored source is
redistributed, retain its upstream third-party notice.

Other vendor-only platform paths can carry their own upstream notices (for
example, a CC0-attributed `rustix` Linux vDSO translation). They are not in the
Windows normal/dev build closure. The repository must not represent vendored
material as newly relicensed under the project license; `THIRD-PARTY-NOTICES.md`
and each vendored crate's own files preserve that boundary.

## Non-code material and generated evidence

Current public material is text documentation only. No third-party media,
datasets, screenshots, benchmark workloads, logos, fonts, or copied schemas
were found. The documentation links to external tools but copies neither their
source nor their documentation.

The software license covers this repository's source and documentation, not
rights in content observed from a target. Running the Probe against another
program does not, merely by that act, apply the Probe license to that program.
Evidence can contain paths, metadata, or other target-related content; users
remain responsible for rights and redistribution decisions. This does not claim
that every generated artifact is copyright-free.

## Future `onigiranai` compatibility

| Scenario | License result |
|---|---|
| A. Separate project merely observed by the Probe | No Probe source-license obligation attaches to the observed program. |
| B. Invokes Probe as a separate executable | Ordinary process/tool use and factual interoperability do not copy Probe source. Preserve the Probe license only when redistributing the Probe itself. |
| C. Consumes evidence files or documented schemas | The interface/evidence boundary does not itself force an `onigiranai` source license. Rights in captured content remain separate. |
| D. Reuses Probe source directly | The copied portion must be used under MIT or Apache-2.0 and carry that selected license's required notices. The surrounding `onigiranai` project need not adopt the same license for independent code. |
| E. Extracts a shared crate/schema library | License the extracted shared source deliberately. If it remains `MIT OR Apache-2.0`, each consumer can choose either path and preserve the required notices. |
| F. `onigiranai` later uses commercial, proprietary, or source-available terms | Separate code can do so. Only copied/shared Probe-origin source retains its own license obligations. |

The direct-source case is the meaningful boundary. Under MIT, preserve the
copyright and permission notice. Under Apache-2.0, provide the Apache license,
retain applicable notices, mark modified files when distributed, and receive
its contributor patent grant subject to its patent-litigation termination. A
consumer may choose the MIT alternative instead; dual licensing does not impose
Apache terms on every downstream use.

## MIT, Apache-2.0, and dual licensing

- **MIT only:** simplest notice burden and broad commercial reuse, but no
express patent grant.
- **Apache-2.0 only:** adds an express contributor patent grant and patent
termination provision, plus its notice/change-notice requirements; it is still
compatible with proprietary derivative distribution under its conditions.
- **MIT OR Apache-2.0:** preserves the short MIT adoption path while offering
Apache's patent terms to downstreams that select it. It matches common Rust
practice, does not require `onigiranai` to use either license when it remains
separate, and avoids selecting a single patent posture for all consumers.

For this project, dual licensing is the strongest fit. It has modest operational
cost: keep both root license files and preserve applicable third-party notices.

## Inbound contributions

`CONTRIBUTING.md` uses inbound-equals-outbound licensing: an accepted
contribution is offered under `MIT OR Apache-2.0` at the recipient's option.
That preserves reuse of third-party contributions in this project and in
permissively licensed downstreams without a CLA. It does **not** transfer
copyright ownership or create a right to relicense another contributor's code
under arbitrary proprietary terms; such a relicensing needs separate permission
from the relevant rightsholder.

## Patent and trademark boundary

No patent notices, patent claims, trademarks, logo assets, or trademark-use
policy were found in the repository. Apache-2.0 grants only the patent rights
that each contributor can license for their contribution; it does not promise
unidentified third-party rights. Neither license grants a trademark license.
No additional patent or trademark notice is warranted by the inspected record.

## Comparison references

The following projects were reviewed only to keep the README comparison modest
and accurate. `prmon` documents monitoring resource consumption for a process
and its children and uses Apache-2.0. [1] [2] `psrecord` records a process's CPU
and memory activity and is BSD-2-Clause. [12] [14] `Metrace` collects CPU/memory
metrics for process trees and is MIT. [5] [6] `hyperfine` is a command-line
benchmarking tool and is dual MIT/Apache-2.0. [7] [Windows Performance Recorder]
is an ETW-based recorder. [11] ReBench describes itself as a tool to run and
document benchmark experiments and is MIT, while Phoronix Test Suite states
that it is GPLv3 automated testing/benchmarking software. [13] [15] [10]

Those licenses create no obligation here because the reviewed projects are not
linked, bundled, copied, or integrated. Copying or integrating their source or
documentation would require a fresh provenance and license review.

## Sources

[1] https://raw.githubusercontent.com/HSF/prmon/main/README.md — HSF prmon README
    > "The PRocess MONitor is a small stand alone program that can monitor the resource consumption of a process and its children."
[2] https://raw.githubusercontent.com/HSF/prmon/main/LICENSE — HSF prmon Apache-2.0 license
    > "Apache License Version 2.0, January 2004"
[5] https://raw.githubusercontent.com/sloev/metrace/master/README.md — Metrace README
    > "Runs a process and collects cpu/memory metrics for both the process and its children seperately."
[6] https://raw.githubusercontent.com/sloev/metrace/master/LICENSE — Metrace MIT license
    > "MIT License Copyright (c) 2019 Johannes Gårdsted Valbjørn"
[7] https://raw.githubusercontent.com/sharkdp/hyperfine/master/README.md — hyperfine README
    > "`hyperfine` is dual-licensed under the terms of the MIT License and the Apache License 2.0."
[10] https://raw.githubusercontent.com/phoronix-test-suite/phoronix-test-suite/master/README.md — Phoronix Test Suite README
    > "The Phoronix Test Suite is open-source under the GNU GPLv3 license"
[11] https://learn.microsoft.com/en-us/windows-hardware/test/wpt/windows-performance-recorder — Windows Performance Recorder documentation
    > "Windows Performance Recorder (WPR) is a performance recording tool that is based on Event Tracing for Windows (ETW)."
[12] https://raw.githubusercontent.com/astrofrog/psrecord/main/README.rst — psrecord README
    > "``psrecord`` is a small utility that uses the `psutil <https://github.com/giampaolo/psutil/>`__ library to record the CPU and memory activity of a process."
[13] https://raw.githubusercontent.com/smarr/ReBench/master/README.md — ReBench README
    > "ReBench is a tool to run and document benchmark experiments."
[14] https://api.github.com/repos/astrofrog/psrecord — psrecord GitHub API metadata
    > ""key": "bsd-2-clause", "name": "BSD 2-Clause \"Simplified\" License", "spdx_id": "BSD-2-Clause""
[15] https://api.github.com/repos/smarr/ReBench — ReBench GitHub API metadata
    > ""key": "mit", "name": "MIT License", "spdx_id": "MIT""
