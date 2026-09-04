# Third-party notices

PerformanceEvidenceProbe is licensed under `MIT OR Apache-2.0`. That license
does not relicense the vendored Rust dependencies under [`.vendor/`](.vendor/).
Their manifests and license files are the authoritative notices for that source.

A normal Windows build uses the dependency closure recorded in `Cargo.lock` and
vendored by `.cargo/config.toml`. It contains permissively licensed crates
(MIT, Apache-2.0, MIT/Apache alternatives, Unlicense OR MIT, and the
Unicode-3.0 notice paired with `unicode-ident`); it contains no dependency that
requires GPL, AGPL, LGPL, MPL, EPL, CDDL, SSPL, BUSL/BSL, Elastic License,
Commons Clause, non-commercial, custom, unknown, or missing terms in that
Windows build closure.

## Redistribution

- Source distributions that retain `.vendor/` must retain the applicable files
  beneath each vendored crate, including copyright, license, and attribution
  files.
- Binary distributors should preserve the applicable dependency license and
  attribution notices with their distribution. In particular,
  `unicode-ident 1.0.24` carries a Unicode License v3 notice in
  `.vendor/unicode-ident/LICENSE-UNICODE`.
- `.vendor/crossbeam-channel` contains an upstream `matching.rs` example based
  on Stefan Nilsson's `matching.go` under CC-BY-3.0; its retained upstream
  attribution is in `.vendor/crossbeam-channel/LICENSE-THIRD-PARTY`. That
  example is not part of PerformanceEvidenceProbe source and is not compiled
  by the normal Windows build.

See the [license and provenance audit](docs/LICENSE-AUDIT-2026-09-04.md) for
the audited dependency scope, provenance result, and inactive lockfile entries.
