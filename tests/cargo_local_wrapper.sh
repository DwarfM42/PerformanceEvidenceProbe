#!/usr/bin/env bash
# Exercise the repository-local Cargo wrapper without invoking Cargo itself.
set -euo pipefail

repo_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
wrapper_source="$repo_dir/scripts/cargo-local.sh"
temp_root="$(mktemp -d)"
cleanup() {
    rm -rf "$temp_root"
}
trap cleanup EXIT

wrapper_repo="$temp_root/repository with spaces"
wrapper_dir="$wrapper_repo/scripts"
fake_bin="$temp_root/fake-bin"
capture_dir="$temp_root/capture"
mkdir -p "$wrapper_dir" "$fake_bin"
cp "$wrapper_source" "$wrapper_dir/cargo-local.sh"
wrapper_repo_physical="$(cd -- "$wrapper_repo" && pwd -P)"

cat > "$fake_bin/cargo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
: "${CAPTURE:?}"
mkdir -p "$CAPTURE"
printf '%s' "$#" > "$CAPTURE/argc"
index=0
for argument in "$@"; do
    printf '%s' "$argument" > "$CAPTURE/arg-$index"
    index=$((index + 1))
done
printf '%s' "$CARGO_HOME" > "$CAPTURE/cargo-home"
printf '%s' "$CARGO_TARGET_DIR" > "$CAPTURE/cargo-target-dir"
printf '%s' "$TEMP" > "$CAPTURE/temp"
printf '%s' "$TMP" > "$CAPTURE/tmp"
printf '%s' "$TMPDIR" > "$CAPTURE/tmpdir"
EOF
chmod +x "$fake_bin/cargo"

fail() {
    printf 'cargo-local wrapper regression: %s\n' "$*" >&2
    exit 1
}

assert_file_equals() {
    local expected=$1
    local path=$2
    local actual
    actual=$(<"$path")
    [[ "$actual" == "$expected" ]] || fail "expected [$expected], got [$actual] at $path"
}

expected_repository_path() {
    if [[ -n "${MSYSTEM:-}" || "${OSTYPE:-}" == cygwin* ]]; then
        cygpath -aw "$wrapper_repo_physical"
    else
        printf '%s' "$wrapper_repo_physical"
    fi
}

run_wrapper() {
    rm -rf "$capture_dir"
    CAPTURE="$capture_dir" PATH="$fake_bin:$PATH" bash "$wrapper_dir/cargo-local.sh" "$@"
}

assert_wrapper_environment() {
    local path_prefix
    local path_separator
    path_prefix=$(expected_repository_path)
    if [[ -n "${MSYSTEM:-}" || "${OSTYPE:-}" == cygwin* ]]; then
        path_separator='\'
    else
        path_separator='/'
    fi
    assert_file_equals "${path_prefix}${path_separator}cargo-home" "$capture_dir/cargo-home"
    assert_file_equals "${path_prefix}${path_separator}target" "$capture_dir/cargo-target-dir"
    assert_file_equals "${path_prefix}${path_separator}tmp" "$capture_dir/temp"
    assert_file_equals "${path_prefix}${path_separator}tmp" "$capture_dir/tmp"
    assert_file_equals "${path_prefix}${path_separator}tmp" "$capture_dir/tmpdir"
}

run_wrapper
assert_file_equals '0' "$capture_dir/argc"
assert_wrapper_environment

run_wrapper check --locked --manifest-path "$wrapper_repo/manifest with spaces.toml" -- '--literal=$()' ''
assert_file_equals '7' "$capture_dir/argc"
assert_file_equals 'check' "$capture_dir/arg-0"
assert_file_equals '--locked' "$capture_dir/arg-1"
assert_file_equals '--manifest-path' "$capture_dir/arg-2"
assert_file_equals "$wrapper_repo/manifest with spaces.toml" "$capture_dir/arg-3"
assert_file_equals '--' "$capture_dir/arg-4"
assert_file_equals '--literal=$()' "$capture_dir/arg-5"
assert_file_equals '' "$capture_dir/arg-6"
assert_wrapper_environment

printf 'cargo-local wrapper regression tests passed\n'
