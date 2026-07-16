#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

swift build
swift run --skip-build cue-audio-core-tests
swift run --skip-build audio-bridge-tests

tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/bluey-macos-audio-test.XXXXXX")"
trap 'rm -rf "$tmp_dir"' EXIT

set +e
swift run --skip-build cue-audio --bogus \
  >"$tmp_dir/stdout" \
  2>"$tmp_dir/stderr"
exit_code=$?
set -e

[[ "$exit_code" -eq 2 ]]
[[ ! -s "$tmp_dir/stdout" ]]
[[ "$(wc -l <"$tmp_dir/stderr" | tr -d ' ')" -eq 2 ]]
grep -q '"event":"error"' "$tmp_dir/stderr"
grep -q '"protocol_version":1' "$tmp_dir/stderr"
grep -q '"code":"unknown_option"' "$tmp_dir/stderr"
grep -q '"event":"stopped"' "$tmp_dir/stderr"
grep -q '"exit_code":2' "$tmp_dir/stderr"

swift build -c release --product cue-audio

echo "cue-audio macOS tests passed"
