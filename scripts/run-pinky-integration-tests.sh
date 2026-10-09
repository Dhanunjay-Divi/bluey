#!/usr/bin/env bash
# Local-only Rust gate with owned temporary build/data roots and cleanup.
set -euo pipefail
set -m

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
if [[ "${1:-}" == "--self-test" ]]; then
  for probe in success failure signal; do
    status=0
    result=$(bash "${BASH_SOURCE[0]}" --probe "$probe") || status=$?
    case "$probe:$status" in
      success:0|failure:7|signal:143) ;;
      *) printf 'Cleanup self-test status failed\n' >&2; exit 1 ;;
    esac
    case "$result" in
      */bluey-pinky-tests.*) ;;
      *) printf 'Cleanup self-test root failed\n' >&2; exit 1 ;;
    esac
    if [[ -e "$result" ]]; then printf 'Cleanup self-test residue\n' >&2; exit 1; fi
  done
  printf 'Cleanup self-test passed (success, failure, TERM with child)\n'
  exit 0
fi
mode=${1:-focused}
case "$mode" in focused|all|--probe) ;; *) printf 'Unknown test mode\n' >&2; exit 2 ;; esac
root=$(mktemp -d "${TMPDIR:-/tmp}/bluey-pinky-tests.XXXXXX")
child=""
cleanup() {
  local status=$?
  trap - EXIT HUP INT TERM
  if [[ -n "$child" ]] && kill -0 -- "-$child" 2>/dev/null; then
    kill -TERM -- "-$child" 2>/dev/null || true
    for _ in {1..10}; do
      if ! kill -0 -- "-$child" 2>/dev/null; then break; fi
      sleep 1
    done
    if kill -0 -- "-$child" 2>/dev/null; then
      kill -KILL -- "-$child" 2>/dev/null || true
    fi
    wait "$child" 2>/dev/null || true
  fi
  case "$root" in
    */bluey-pinky-tests.*)
      if [[ -d "$root" && ! -L "$root" ]]; then find "$root" -depth -delete; fi
      ;;
    *) printf 'Unexpected temporary root; cleanup held\n' >&2; exit 1 ;;
  esac
  if [[ -e "$root" ]]; then printf 'Temporary residue remains\n' >&2; exit 1; fi
  exit "$status"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
mkdir -p "$root"/{cargo,tmp,data,config,runtime,logs,db}
chmod 700 "$root" "$root/runtime"

if [[ "$mode" == "--probe" ]]; then
  printf '%s\n' "$root"
  case "${2:-}" in
    success) exit 0 ;;
    failure) exit 7 ;;
    signal)
      env -i PATH="$PATH" bash -c 'sleep 30' &
      child=$!
      kill -TERM "$$"
      wait "$child"
      exit 1 ;;
    *) exit 2 ;;
  esac
fi

# Keep only toolchain identity and fresh task data. No inherited DB/provider keys.
run_cargo() {
  env -i PATH="$PATH" HOME="$HOME" \
  CARGO_TARGET_DIR="$root/cargo" TMPDIR="$root/tmp" \
  XDG_DATA_HOME="$root/data" XDG_CONFIG_HOME="$root/config" \
  XDG_RUNTIME_DIR="$root/runtime" BLUEY_DATA_DIR="$root/data" \
  BLUEY_DB_PATH="$root/db/test.db" BLUEY_LOG_DIR="$root/logs" \
  RUST_TEST_THREADS=2 \
  cargo "$@" --offline --locked --manifest-path "$repo/server/Cargo.toml" -j 2 &
  child=$!
  wait "$child"
  child=""
}
printf 'Owned validation root: %s\n' "$root"
if [[ "$mode" == "all" ]]; then
  run_cargo test --all-targets
else
  run_cargo test --lib pinky_integration
fi
# The same temporary target covers the actual main/Jobs binaries and all tests.
env -i PATH="$PATH" HOME="$HOME" \
  CARGO_TARGET_DIR="$root/cargo" TMPDIR="$root/tmp" \
  XDG_DATA_HOME="$root/data" XDG_CONFIG_HOME="$root/config" \
  XDG_RUNTIME_DIR="$root/runtime" BLUEY_DATA_DIR="$root/data" \
  BLUEY_DB_PATH="$root/db/test.db" BLUEY_LOG_DIR="$root/logs" \
  cargo clippy --offline --locked --manifest-path "$repo/server/Cargo.toml" \
    --all-targets -j 2 -- -D warnings &
child=$!
wait "$child"
child=""
