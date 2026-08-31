#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
script_path="$repo_root/scripts/run-bluey-tests.sh"
run_root=""
child_pid=""
child_pgid=""

usage() {
  cat <<'EOF'
Usage:
  bash scripts/run-bluey-tests.sh all
  bash scripts/run-bluey-tests.sh workspace
  bash scripts/run-bluey-tests.sh server
  bash scripts/run-bluey-tests.sh -- <focused cargo test command...>
  bash scripts/run-bluey-tests.sh --self-test

Every test invocation gets a fresh temporary Cargo target, application data,
configuration, runtime, log, general temporary-file, and SQLite directory.
The launcher removes that workspace after success, failure, SIGINT, or SIGTERM.

Set BLUEY_RUST_TOOLCHAIN (for example, 1.98) to select a rustup toolchain.
Set BLUEY_TEST_TEMP_PARENT to choose a short parent for the temporary workspace;
long paths are rejected so macOS IPC socket tests remain valid.
EOF
}

fail() {
  printf 'Bluey isolated tests: %s\n' "$*" >&2
  exit 1
}

clear_inherited_authority_env() {
  local name
  while IFS= read -r name; do
    case "$name" in
      BLUEY_ACCESS_TOKEN | BLUEY_API_TOKEN | CUE_API_TOKEN | BLUEY_API_BASE | \
        BLUEY_API_BASE_URL | BLUEY_API_HOST | BLUEY_API_URL | CUE_API_URL | \
        BLUEY_CLOUD_* | CUE_CLOUD_* | BLUEY_JOBS_* | \
        OPENAI_* | ANTHROPIC_* | GEMINI_* | GOOGLE_* | DEEPSEEK_* | ZAI_* | ZHIPU_* | \
        DEEPGRAM_* | GROQ_* | CEREBRAS_* | OLLAMA_* | CUE_PROVIDER_* | \
        BLUEY_STT_* | BLUEY_LLM_* | BLUEY_DEV_BYOK* | BLUEY_DIRECT_* | \
        BLUEY_ALLOW_DIRECT_* | BLUEY_WEB_SEARCH_* | BRAVE_SEARCH_* | TAVILY_* | \
        STRIPE_* | SQUARE_* | AWS_* | BLUEY_R2_* | BLUEY_LOG_R2_* | BLUEY_OBJECT_* | \
        SMTP_* | RESEND_* | BLUEY_SMTP_* | BLUEY_RESEND_* | \
        REDIS_URL | BLUEY_REDIS_URL | BLUEY_TEST_REDIS_URL | \
        BLUEY_JWT_SECRET | BLUEY_TURNSTILE_* | TURNSTILE_* | \
        BLUEY_TEST_ANTHROPIC_URL | BLUEY_TEST_DEEPGRAM_URL | BLUEY_TEST_DEEPGRAM_WS_URL | \
        BLUEY_TEST_DEEPSEEK_URL | BLUEY_TEST_GEMINI_URL | BLUEY_TEST_OPENAI_URL | \
        BLUEY_TEST_POSTGRES_URL | BLUEY_TEST_SECRET | BLUEY_TEST_SQUARE_URL | \
        BLUEY_TEST_STRIPE_URL | BLUEY_TEST_ZAI_URL | \
        BLUEY_DAEMON_BIN | CUE_DAEMON_BIN | BLUEY_OVERLAY_BIN | CUE_OVERLAY_BIN | \
        BLUEY_AUDIO_HELPER_BIN | CUE_AUDIO_HELPER_BIN | BLUEY_SYSTEM_AUDIO_BINARY | \
        BLUEY_CAPTURE_BIN | CUE_CAPTURE_BIN | BLUEY_CONTEXT_PICKER_APP | \
        BLUEY_LOCAL_WHISPER_BINARY | \
        BLUEY_DOC_CONVERTER_BIN | BLUEY_MARKITDOWN_BIN | BLUEY_FFMPEG_PATH | FFMPEG_PATH | \
        BLUEY_INSTALL_ROOT)
        unset "$name"
        ;;
    esac
  done < <(compgen -e)
}

cleanup_workspace() {
  if [[ -z "$run_root" || ! -e "$run_root" ]]; then
    return
  fi

  case "$(basename "$run_root")" in
    bluey-tests.*) ;;
    *)
      printf 'Bluey isolated tests: refusing to remove unexpected path: %s\n' \
        "$run_root" >&2
      return 1
      ;;
  esac

  if [[ ! -f "$run_root/.bluey-test-workspace" ]]; then
    printf 'Bluey isolated tests: refusing to remove unmarked path: %s\n' \
      "$run_root" >&2
    return 1
  fi

  if ! rm -rf -- "$run_root"; then
    printf 'Bluey isolated tests: cleanup failed for %s\n' "$run_root" >&2
    return 1
  fi
  if [[ -e "$run_root" ]]; then
    printf 'Bluey isolated tests: cleanup left residue at %s\n' "$run_root" >&2
    return 1
  fi
  printf 'Bluey isolated tests: cleaned %s\n' "$run_root" >&2
}

stop_child() {
  local signal_name="${1:-TERM}"
  local target=""

  if [[ -n "$child_pgid" ]] && kill -0 -- "-$child_pgid" 2>/dev/null; then
    target="-$child_pgid"
  elif [[ -n "$child_pid" ]] && kill -0 "$child_pid" 2>/dev/null; then
    target="$child_pid"
  else
    child_pid=""
    child_pgid=""
    return
  fi

  # The test command runs as a monitored background job. With job control on,
  # its PID is also its process-group ID, so Cargo and active rustc/test children
  # receive the same interruption before their temporary target is removed.
  kill -"$signal_name" -- "$target" 2>/dev/null || true
  for _ in {1..20}; do
    if ! kill -0 -- "$target" 2>/dev/null; then
      break
    fi
    sleep 0.1
  done
  if kill -0 -- "$target" 2>/dev/null; then
    printf 'Bluey isolated tests: forcing stopped test process group cleanup.\n' >&2
    kill -KILL -- "$target" 2>/dev/null || true
  fi
  wait "$child_pid" 2>/dev/null || true
  child_pid=""
  child_pgid=""
}

on_exit() {
  local status=$?
  local cleanup_status=0
  trap - EXIT HUP INT TERM
  stop_child TERM
  cleanup_workspace || cleanup_status=$?
  if [[ "$status" -eq 0 && "$cleanup_status" -ne 0 ]]; then
    status="$cleanup_status"
  fi
  exit "$status"
}

on_signal() {
  local signal_name="$1"
  local status="$2"
  trap - "$signal_name"
  stop_child "$signal_name"
  exit "$status"
}

assert_self_test_cleanup() {
  local output="$1"
  local label="$2"
  local workspace

  workspace="$(
    printf '%s\n' "$output" \
      | sed -n 's/^Bluey isolated tests: workspace //p' \
      | head -n 1
  )"
  [[ -n "$workspace" ]] || fail "$label did not report its temporary workspace"
  [[ ! -e "$workspace" ]] || fail "$label left temporary workspace $workspace"
}

self_test() (
  local self_root success_output failure_output failure_status
  local signal_launcher_pid=""
  local signal_orphan_pid signal_output signal_status signal_workspace
  self_root="$(mktemp -d /tmp/btl.XXXXXX)"

  self_test_cleanup() {
    if [[ -n "$signal_launcher_pid" ]] \
      && kill -0 "$signal_launcher_pid" 2>/dev/null; then
      kill -TERM "$signal_launcher_pid" 2>/dev/null || true
      wait "$signal_launcher_pid" 2>/dev/null || true
    fi
    rm -rf -- "$self_root"
  }
  trap self_test_cleanup EXIT
  trap 'exit 129' HUP
  trap 'exit 130' INT
  trap 'exit 143' TERM

  mkdir -p \
    "$self_root/s" \
    "$self_root/f" \
    "$self_root/g"

  success_output="$(
    BLUEY_TEST_TEMP_PARENT="$self_root/s" \
      BLUEY_DATABASE_URL='postgres://must-not-survive.invalid/bluey' \
      BLUEY_TEST_POSTGRES_URL='postgres://must-not-survive.invalid/bluey_test' \
      BLUEY_USE_OS_KEYCHAIN=1 \
      BLUEY_USE_SECURE_STORE=1 \
      BLUEY_LEGACY_KEYRING_FALLBACK=1 \
      BLUEY_UPDATE_FORCE=1 \
      BLUEY_UPDATE_ALLOW_UNSIGNED=true \
      BLUEY_UPDATE_ASSUME_YES=1 \
      BLUEY_UPDATE_MANIFEST_URL='https://must-not-survive.invalid/latest.json' \
      BLUEY_UPDATE_INSTALL_URL='https://must-not-survive.invalid/install.sh' \
      BLUEY_CLOUD_TOKEN='must-not-survive-cloud-token' \
      BLUEY_ACCESS_TOKEN='must-not-survive-managed-token' \
      BLUEY_API_BASE='https://must-not-survive.invalid' \
      BLUEY_API_HOST='0.0.0.0' \
      OPENAI_API_KEY='must-not-survive-provider-key' \
      STRIPE_SECRET_KEY='must-not-survive-billing-key' \
      AWS_SECRET_ACCESS_KEY='must-not-survive-storage-key' \
      BLUEY_SMTP_PASSWORD='must-not-survive-mail-password' \
      BLUEY_JOBS_WORKER_TOKEN='must-not-survive-jobs-token' \
      BLUEY_DAEMON_BIN='/must/not/survive/daemon' \
      BLUEY_CONTEXT_PICKER_APP='/must/not/survive/picker.app' \
      FFMPEG_PATH='/must/not/survive/ffmpeg' \
      bash "$script_path" -- bash -c '
        test -d "$CARGO_TARGET_DIR"
        test -d "$BLUEY_DATA_DIR"
        test -d "$BLUEY_CONFIG_DIR"
        test -d "$BLUEY_RUNTIME_DIR"
        test -d "$BLUEY_LOG_DIR"
        test -d "$TMPDIR"
        test "$BLUEY_TEST_WORKSPACE_ROOT" = "${CARGO_TARGET_DIR%/target}"
        test "$BLUEY_SERVER_DB_BACKEND" = sqlite
        test -z "${BLUEY_DATABASE_URL+x}"
        test -z "${BLUEY_TEST_POSTGRES_URL+x}"
        test "$BLUEY_USE_OS_KEYCHAIN" = 0
        test "$BLUEY_USE_SECURE_STORE" = 0
        test "$BLUEY_LEGACY_KEYRING_FALLBACK" = 0
        test "$BLUEY_SKIP_UPDATE" = 1
        test "$BLUEY_SKIP_PERMISSION_PREFLIGHT" = 1
        test -z "${BLUEY_UPDATE_FORCE+x}"
        test -z "${BLUEY_UPDATE_ALLOW_UNSIGNED+x}"
        test -z "${BLUEY_UPDATE_ASSUME_YES+x}"
        test -z "${BLUEY_UPDATE_MANIFEST_URL+x}"
        test -z "${BLUEY_UPDATE_INSTALL_URL+x}"
        test -z "${BLUEY_CLOUD_TOKEN+x}"
        test -z "${BLUEY_ACCESS_TOKEN+x}"
        test -z "${BLUEY_API_BASE+x}"
        test -z "${BLUEY_API_HOST+x}"
        test -z "${OPENAI_API_KEY+x}"
        test -z "${STRIPE_SECRET_KEY+x}"
        test -z "${AWS_SECRET_ACCESS_KEY+x}"
        test -z "${BLUEY_SMTP_PASSWORD+x}"
        test -z "${BLUEY_JOBS_WORKER_TOKEN+x}"
        test -z "${BLUEY_DAEMON_BIN+x}"
        test -z "${BLUEY_CONTEXT_PICKER_APP+x}"
        test -z "${FFMPEG_PATH+x}"
        : > "$BLUEY_DB_PATH"
        : > "$CARGO_TARGET_DIR/self-test-success"
      ' 2>&1
  )"
  assert_self_test_cleanup "$success_output" "success case"

  set +e
  failure_output="$(
    BLUEY_TEST_TEMP_PARENT="$self_root/f" \
      bash "$script_path" -- bash -c '
        : > "$BLUEY_DB_PATH"
        : > "$CARGO_TARGET_DIR/self-test-failure"
        exit 37
      ' 2>&1
  )"
  failure_status=$?
  set -e
  [[ "$failure_status" -eq 37 ]] \
    || fail "failure case returned $failure_status instead of 37"
  assert_self_test_cleanup "$failure_output" "failure case"

  BLUEY_TEST_TEMP_PARENT="$self_root/g" \
    BLUEY_TEST_SIGNAL_READY="$self_root/signal-ready" \
    BLUEY_TEST_ORPHAN_PID_FILE="$self_root/orphan-pid" \
    bash "$script_path" -- bash -c '
      trap "" HUP INT TERM
      (trap "" HUP INT TERM; while :; do sleep 1; done) &
      printf "%s\n" "$!" > "$BLUEY_TEST_ORPHAN_PID_FILE"
      : > "$BLUEY_TEST_SIGNAL_READY"
      while :; do sleep 1; done
    ' >"$self_root/signal-output" 2>&1 &
  signal_launcher_pid=$!

  for _ in {1..100}; do
    [[ -e "$self_root/signal-ready" ]] && break
    sleep 0.02
  done
  [[ -e "$self_root/signal-ready" ]] \
    || fail "SIGTERM case did not start before the timeout"
  signal_orphan_pid="$(<"$self_root/orphan-pid")"
  [[ "$signal_orphan_pid" =~ ^[0-9]+$ ]] \
    || fail "SIGTERM case did not report its orphan probe PID"

  kill -TERM "$signal_launcher_pid"
  set +e
  wait "$signal_launcher_pid"
  signal_status=$?
  set -e
  signal_launcher_pid=""
  [[ "$signal_status" -eq 143 ]] \
    || fail "SIGTERM case returned $signal_status instead of 143"

  signal_output="$(<"$self_root/signal-output")"
  signal_workspace="$(
    printf '%s\n' "$signal_output" \
      | sed -n 's/^Bluey isolated tests: workspace //p' \
      | head -n 1
  )"
  [[ -n "$signal_workspace" ]] \
    || fail "SIGTERM case did not report its temporary workspace"
  [[ ! -e "$signal_workspace" ]] \
    || fail "SIGTERM case left temporary workspace $signal_workspace"
  for _ in {1..100}; do
    ! kill -0 "$signal_orphan_pid" 2>/dev/null && break
    sleep 0.02
  done
  ! kill -0 "$signal_orphan_pid" 2>/dev/null \
    || fail "SIGTERM case left orphan process $signal_orphan_pid"

  printf '%s\n' \
    'Bluey isolated test launcher self-test passed: success, failure, SIGTERM, and orphan cleanup.'
)

mode="${1:-all}"
if [[ "$mode" == "--self-test" ]]; then
  [[ "$#" -eq 1 ]] || fail "--self-test does not accept additional arguments"
  self_test
  exit 0
fi

case "$mode" in
  all | workspace | server)
    shift || true
    [[ "$#" -eq 0 ]] || fail "$mode does not accept additional arguments; use -- for a focused command"
    ;;
  --)
    shift
    [[ "$#" -gt 0 ]] || fail "-- requires a command"
    ;;
  -h | --help)
    usage
    exit 0
    ;;
  *)
    usage >&2
    fail "unknown test selection: $mode"
    ;;
esac

# Keep the default root short. macOS Unix-domain sockets have a small path
# budget, and inheriting its long per-user TMPDIR makes otherwise valid IPC
# tests fail before they can bind.
temp_parent="${BLUEY_TEST_TEMP_PARENT:-/tmp}"
mkdir -p "$temp_parent"
[[ -d "$temp_parent" && -w "$temp_parent" ]] \
  || fail "temporary parent is not a writable directory: $temp_parent"
temp_parent="$(cd "$temp_parent" && pwd -P)"

run_root="$(mktemp -d "$temp_parent/bluey-tests.XXXXXX")"
if ! : > "$run_root/.bluey-test-workspace"; then
  rmdir "$run_root" 2>/dev/null || true
  fail "could not mark temporary workspace under $temp_parent"
fi

# Install cleanup before creating any potentially large child directories so a
# disk-full or permission failure during setup cannot leave the workspace.
trap on_exit EXIT
trap 'on_signal HUP 129' HUP
trap 'on_signal INT 130' INT
trap 'on_signal TERM 143' TERM

# Tests create owner-only Unix sockets below TMPDIR. Reject an explicitly
# configured parent that cannot leave enough room for their bounded suffixes.
if [[ "${#run_root}" -gt 48 ]]; then
  fail "temporary workspace path is too long for local IPC sockets: $run_root"
fi

mkdir -p \
  "$run_root/target" \
  "$run_root/d" \
  "$run_root/c" \
  "$run_root/r" \
  "$run_root/l" \
  "$run_root/db" \
  "$run_root/t" \
  "$run_root/x"

export CARGO_TARGET_DIR="$run_root/target"
export BLUEY_TEST_WORKSPACE_ROOT="$run_root"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
export CARGO_PROFILE_DEV_DEBUG="${CARGO_PROFILE_DEV_DEBUG:-0}"
export CARGO_PROFILE_TEST_DEBUG="${CARGO_PROFILE_TEST_DEBUG:-0}"

export BLUEY_DATA_DIR="$run_root/d"
export CUE_DATA_DIR="$run_root/d"
export BLUEY_CONFIG_DIR="$run_root/c"
export CUE_CONFIG_DIR="$run_root/c"
export BLUEY_RUNTIME_DIR="$run_root/r"
export CUE_RUNTIME_DIR="$run_root/r"
export BLUEY_LOG_DIR="$run_root/l"
export CUE_LOG_DIR="$run_root/l"
export BLUEY_DB_PATH="$run_root/db/bluey-server.sqlite"
export BLUEY_SERVER_DB_BACKEND=sqlite
clear_inherited_authority_env
unset BLUEY_DATABASE_URL
unset BLUEY_TEST_POSTGRES_URL

# Tests must never touch or migrate the owner's real credential store. Bluey's
# normal account-file path is already redirected above; these values also keep
# legacy or explicitly enabled Keychain paths off during a test run.
export BLUEY_USE_OS_KEYCHAIN=0
export BLUEY_USE_SECURE_STORE=0
export BLUEY_LEGACY_KEYRING_FALLBACK=0
export BLUEY_SKIP_SIGNIN_OPEN=1
export BLUEY_SKIP_UPDATE=1
export BLUEY_SKIP_PERMISSION_PREFLIGHT=1
unset BLUEY_UPDATE_FORCE
unset BLUEY_UPDATE_ALLOW_UNSIGNED
unset BLUEY_UPDATE_ASSUME_YES
unset BLUEY_UPDATE_MANIFEST_URL
unset BLUEY_UPDATE_INSTALL_URL
unset BLUEY_UPDATE_STRICT

export TMPDIR="$run_root/t"
export TMP="$run_root/t"
export TEMP="$run_root/t"
export XDG_CACHE_HOME="$run_root/x"

printf 'Bluey isolated tests: workspace %s\n' "$run_root"
printf 'Bluey isolated tests: Cargo target and local databases are temporary.\n'

cargo_command=(cargo)
if [[ -n "${BLUEY_RUST_TOOLCHAIN:-}" ]]; then
  cargo_command+=("+$BLUEY_RUST_TOOLCHAIN")
fi

run_selected_tests() {
  cd "$repo_root"
  case "$mode" in
    all)
      "${cargo_command[@]}" test --workspace --all-targets
      "${cargo_command[@]}" test --manifest-path server/Cargo.toml --all-targets
      ;;
    workspace)
      "${cargo_command[@]}" test --workspace --all-targets
      ;;
    server)
      "${cargo_command[@]}" test --manifest-path server/Cargo.toml --all-targets
      ;;
    --)
      exec "$@"
      ;;
  esac
}

# A distinct process group lets signal handlers stop Cargo and its active
# compiler/test children before removing the workspace they are writing into.
set -m
run_selected_tests "$@" &
child_pid=$!
child_pgid=$child_pid
set +m

set +e
wait "$child_pid"
status=$?
set -e
if [[ -n "$child_pgid" ]] && kill -0 -- "-$child_pgid" 2>/dev/null; then
  stop_child TERM
else
  child_pid=""
  child_pgid=""
fi
exit "$status"
