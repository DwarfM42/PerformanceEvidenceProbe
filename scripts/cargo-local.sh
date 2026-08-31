#!/usr/bin/env bash
# Run Cargo with all project build state retained under this checkout.
set -euo pipefail

repo_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
repo_dir_windows="$(cygpath -aw "$repo_dir")"

export CARGO_HOME="$repo_dir_windows\\cargo-home"
export CARGO_TARGET_DIR="$repo_dir_windows\\target"
export TEMP="$repo_dir_windows\\tmp"
export TMP="$TEMP"
export TMPDIR="$TEMP"

mkdir -p "$CARGO_HOME" "$CARGO_TARGET_DIR" "$TEMP"
cd "$repo_dir"

exec cargo "$@"
