#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/bluey-windows-audio.XXXXXX")"
trap 'rm -rf "$tmp_dir"' EXIT

host_cc="${CC:-cc}"
mingw_cc="${MINGW_CC:-x86_64-w64-mingw32-gcc}"
warnings=(-std=c11 -O2 -Wall -Wextra -Wpedantic -Werror -Wformat=2 -Wstrict-prototypes)

"$host_cc" "${warnings[@]}" \
  "$script_dir/audio_args.c" \
  "$script_dir/audio_args_test.c" \
  -o "$tmp_dir/audio-args-test"
"$tmp_dir/audio-args-test"

"$host_cc" "${warnings[@]}" \
  "$script_dir/resampler.c" \
  "$script_dir/resampler_test.c" \
  -lm \
  -o "$tmp_dir/resampler-test"
"$tmp_dir/resampler-test"

if ! command -v "$mingw_cc" >/dev/null 2>&1; then
  echo "required MinGW cross compiler not found: $mingw_cc" >&2
  exit 1
fi

"$mingw_cc" "${warnings[@]}" \
  -D_WIN32_WINNT=0x0A00 \
  "$script_dir/main.c" \
  "$script_dir/audio_args.c" \
  "$script_dir/resampler.c" \
  -lole32 \
  -luuid \
  -lm \
  -Wl,--subsystem,console:10.0 \
  -o "$tmp_dir/bluey-audio.exe"

test -s "$tmp_dir/bluey-audio.exe"
echo "MinGW Windows 10 cross-build passed: $tmp_dir/bluey-audio.exe"
