#!/usr/bin/env bash

# Canonical local Bluey Rust test launcher. Release/package builds intentionally
# do not use this disposable target because their exact artifacts are promoted.

set -u
set -o pipefail

readonly MARKER_NAME=".bluey-test-run-v2"
readonly MARKER_MAGIC="bluey-test-run-v2"
readonly STALE_GRACE_SECONDS=60
readonly CLEANUP_FAILURE_STATUS=74

script_path="${BASH_SOURCE[0]}"
script_dir="$(cd -P "$(dirname "$script_path")" && pwd)" || exit 70
repo_root="$(cd -P "$script_dir/.." && pwd)" || exit 70
owner_uid="$(id -u)" || exit 70
launcher_group_id="$(ps -o pgid= -p $$ 2>/dev/null | tr -d '[:space:]')"
[[ "$launcher_group_id" =~ ^[0-9]+$ && "$launcher_group_id" -gt 1 ]] || exit 70

usage() {
  printf '%s\n' \
    "Usage: $script_path --self-test" \
    "       $script_path all" \
    "       $script_path [--] command [argument ...]" >&2
}

fail() {
  printf 'Bluey test launcher: %s\n' "$1" >&2
  exit "${2:-70}"
}

mode_of() {
  local target="$1" mode=""
  mode="$(stat -f '%Lp' "$target" 2>/dev/null)" ||
    mode="$(stat -c '%a' "$target" 2>/dev/null)" || return 1
  printf '%s\n' "$mode"
}

path_is_same_or_below() {
  local candidate="$1" boundary="$2"
  [[ "$candidate" == "$boundary" || "$candidate" == "$boundary"/* ]]
}

path_is_same_or_above() {
  local candidate="$1" boundary="$2"
  [[ "$boundary" == "$candidate" || "$boundary" == "$candidate"/* ]]
}

validate_no_git_ancestor() {
  local probe="$1"
  while :; do
    [[ ! -e "$probe/.git" ]] || return 1
    [[ "$probe" == "/" ]] && break
    probe="$(dirname "$probe")"
  done
}

validate_no_known_worktree_overlap() {
  local candidate="$1" line="" worktree="" physical=""
  while IFS= read -r line; do
    [[ "$line" == worktree\ * ]] || continue
    worktree="${line#worktree }"
    [[ -d "$worktree" ]] || continue
    physical="$(cd -P "$worktree" && pwd)" || return 1
    if path_is_same_or_below "$candidate" "$physical" ||
      path_is_same_or_above "$candidate" "$physical";
    then
      return 1
    fi
  done < <(git -C "$repo_root" worktree list --porcelain)
  return 0
}

prepare_run_parent() {
  local requested="$1" leaf="" existing_parent="" parent_physical=""
  local desired="" temp_physical="" existing_mode=""

  [[ "$requested" == /* ]] || fail "BLUEY_TEST_RUN_PARENT must be absolute" 64
  [[ "$requested" != */ ]] || fail "run parent must not end with a slash" 64
  case "/$requested/" in
    *'/./'* | *'/../'*) fail "run parent must not contain dot segments" 64 ;;
  esac
  case "$requested" in
    *$'\n'* | *$'\r'*) fail "run parent must not contain line breaks" 64 ;;
  esac

  leaf="$(basename "$requested")"
  case "$leaf" in
    bluey-test-runs-*) ;;
    *) fail "run parent leaf must start with bluey-test-runs-" 64 ;;
  esac
  existing_parent="$(dirname "$requested")"
  [[ -d "$existing_parent" ]] ||
    fail "run parent base must already exist: $existing_parent" 64
  parent_physical="$(cd -P "$existing_parent" && pwd)" ||
    fail "could not resolve existing run parent base"
  desired="$parent_physical/$leaf"
  temp_physical="$(cd -P /tmp && pwd)" || fail "could not resolve local /tmp"

  path_is_same_or_below "$desired" "$temp_physical" ||
    fail "run parent must stay under local /tmp" 64
  case "$desired" in
    / | /private | /private/tmp | /tmp | /Volumes | /Volumes/*)
      fail "refusing root, broad temporary, or external-volume run parent" 64
      ;;
  esac
  if path_is_same_or_below "$desired" "$repo_root" ||
    path_is_same_or_above "$desired" "$repo_root";
  then
    fail "run parent must not overlap the current repository" 64
  fi
  validate_no_git_ancestor "$parent_physical" ||
    fail "run parent must not be inside a Git repository or worktree" 64
  validate_no_known_worktree_overlap "$desired" ||
    fail "run parent must not overlap any registered Git worktree" 64

  if [[ -e "$desired" || -L "$desired" ]]; then
    [[ -d "$desired" && ! -L "$desired" && -O "$desired" ]] ||
      fail "existing run parent must be an owned non-symlink directory" 64
    existing_mode="$(mode_of "$desired")" || fail "could not read run parent mode"
    [[ "$existing_mode" == "700" ]] ||
      fail "existing run parent must already have mode 0700" 64
  else
    mkdir -m 0700 -- "$desired" || fail "could not create private run parent"
    [[ -d "$desired" && ! -L "$desired" && -O "$desired" ]] ||
      fail "new run parent failed ownership validation"
    [[ "$(mode_of "$desired")" == "700" ]] ||
      fail "new run parent failed mode validation"
  fi
  run_parent="$desired"
}

marker_magic=""
marker_run_root=""
marker_run_name=""
marker_owner_uid=""
marker_owner_pid=""
marker_process_group_id=""
marker_launcher_group_id=""
marker_created_epoch=""
marker_state=""

read_marker() {
  local marker="$1" l1="" l2="" l3="" l4="" l5="" l6="" l7="" l8="" l9="" extra=""
  [[ -f "$marker" && ! -L "$marker" && -O "$marker" ]] || return 1
  {
    IFS= read -r l1 || return 1
    IFS= read -r l2 || return 1
    IFS= read -r l3 || return 1
    IFS= read -r l4 || return 1
    IFS= read -r l5 || return 1
    IFS= read -r l6 || return 1
    IFS= read -r l7 || return 1
    IFS= read -r l8 || return 1
    IFS= read -r l9 || return 1
    if IFS= read -r extra; then return 1; fi
  } < "$marker"
  [[ "$l1" == "magic=$MARKER_MAGIC" && "$l2" == run_root=* &&
    "$l3" == run_name=* && "$l4" == owner_uid=* && "$l5" == owner_pid=* &&
    "$l6" == process_group_id=* && "$l7" == launcher_group_id=* &&
    "$l8" == created_epoch=* && "$l9" == state=* ]] || return 1
  marker_magic="${l1#magic=}"
  marker_run_root="${l2#run_root=}"
  marker_run_name="${l3#run_name=}"
  marker_owner_uid="${l4#owner_uid=}"
  marker_owner_pid="${l5#owner_pid=}"
  marker_process_group_id="${l6#process_group_id=}"
  marker_launcher_group_id="${l7#launcher_group_id=}"
  marker_created_epoch="${l8#created_epoch=}"
  marker_state="${l9#state=}"
  [[ "$marker_owner_uid" =~ ^[0-9]+$ ]] || return 1
  [[ "$marker_owner_pid" =~ ^[0-9]+$ && "$marker_owner_pid" -gt 1 ]] || return 1
  [[ "$marker_process_group_id" =~ ^[0-9]+$ ]] || return 1
  [[ "$marker_launcher_group_id" =~ ^[0-9]+$ && "$marker_launcher_group_id" -gt 1 ]] || return 1
  [[ "$marker_created_epoch" =~ ^[0-9]+$ ]] || return 1
  [[ "$marker_state" == "starting" || "$marker_state" == "running" ]] || return 1
  [[ "$marker_run_name" =~ ^run\.[0-9]+\.[0-9]+\.[A-Za-z0-9]+$ ]] || return 1
}

pid_is_active() {
  local pid="$1" output="" status=0
  [[ "$pid" =~ ^[0-9]+$ && "$pid" -gt 1 ]] || return 1
  kill -0 "$pid" 2>/dev/null && return 0
  output="$(ps -p "$pid" -o pid= 2>/dev/null)"
  status=$?
  [[ "$(printf '%s' "$output" | tr -d '[:space:]')" == "$pid" ]] && return 0
  [[ "$status" -eq 1 && -z "$output" ]] && return 1
  return 0
}

group_is_active() {
  local pgid="$1" output="" status=0 observed_pgid="" observed_state=""
  [[ "$pgid" =~ ^[0-9]+$ && "$pgid" -gt 1 ]] || return 1
  [[ "$pgid" != "$launcher_group_id" ]] || return 0
  output="$(ps -eo pgid=,stat= 2>/dev/null)"
  status=$?
  # A failed process listing is not evidence that the group is gone. Keep the
  # root when inspection is ambiguous rather than risk deleting a live run.
  # Zombie-only groups cannot execute or retain open files, so they must not
  # turn successful cleanup into an intermittent failure while their parent
  # is waiting to reap them.
  [[ "$status" -eq 0 ]] || return 0
  while read -r observed_pgid observed_state; do
    [[ "$observed_pgid" =~ ^[0-9]+$ && -n "$observed_state" ]] || return 0
    if [[ "$observed_pgid" == "$pgid" && "$observed_state" != Z* ]]; then
      return 0
    fi
  done <<< "$output"
  return 1
}

marker_matches_root() {
  local candidate="$1" physical="" name=""
  [[ -d "$candidate" && ! -L "$candidate" && -O "$candidate" ]] || return 1
  physical="$(cd -P "$candidate" && pwd)" || return 1
  name="$(basename "$physical")"
  [[ "$(dirname "$physical")" == "$run_parent" ]] || return 1
  [[ "$marker_magic" == "$MARKER_MAGIC" && "$marker_run_root" == "$physical" &&
    "$marker_run_name" == "$name" && "$marker_owner_uid" == "$owner_uid" ]] || return 1
  [[ "$marker_launcher_group_id" != "$marker_process_group_id" ]] || return 1
}

reap_stale_runs() {
  local candidate="" now="" age=0
  now="$(date +%s)" || return 0
  for candidate in "$run_parent"/run.*; do
    [[ -e "$candidate" || -L "$candidate" ]] || continue
    [[ -d "$candidate" && ! -L "$candidate" && -O "$candidate" ]] || continue
    read_marker "$candidate/$MARKER_NAME" || continue
    marker_matches_root "$candidate" || continue
    [[ "$marker_created_epoch" -le "$now" ]] || continue
    age=$((now - marker_created_epoch))
    [[ "$age" -ge "$STALE_GRACE_SECONDS" ]] || continue
    pid_is_active "$marker_owner_pid" && continue
    if [[ "$marker_process_group_id" -gt 1 ]] && group_is_active "$marker_process_group_id"; then
      continue
    fi
    read_marker "$candidate/$MARKER_NAME" || continue
    marker_matches_root "$candidate" || continue
    pid_is_active "$marker_owner_pid" && continue
    if [[ "$marker_process_group_id" -gt 1 ]] && group_is_active "$marker_process_group_id"; then
      continue
    fi
    if ! rm -rf -- "$candidate" || [[ -e "$candidate" || -L "$candidate" ]]; then
      printf 'Bluey test launcher: stale cleanup failed for %s\n' "$candidate" >&2
    fi
  done
}

run_root=""
run_name=""
run_root_created=0
run_parent=""
created_epoch=0
command_pid=0
process_group_id=0
cleanup_attempted=0
marker_path=""
marker_established=0
deferred_signal_name=""
deferred_signal_status=0

install_runtime_signal_traps() {
  trap 'on_signal HUP 129' HUP
  trap 'on_signal INT 130' INT
  trap 'on_signal TERM 143' TERM
}

defer_signal() {
  local signal_name="$1" status="$2"
  if [[ -z "$deferred_signal_name" ]]; then
    deferred_signal_name="$signal_name"
    deferred_signal_status="$status"
  fi
}

install_deferred_signal_traps() {
  trap 'defer_signal HUP 129' HUP
  trap 'defer_signal INT 130' INT
  trap 'defer_signal TERM 143' TERM
}

dispatch_deferred_signal() {
  local signal_name="$deferred_signal_name" status="$deferred_signal_status"
  install_runtime_signal_traps
  deferred_signal_name=""
  deferred_signal_status=0
  [[ -n "$signal_name" ]] && on_signal "$signal_name" "$status"
}

write_marker() {
  local state="$1" pgid="$2" next="$marker_path.next.$$"
  (
    umask 077
    printf '%s\n' \
      "magic=$MARKER_MAGIC" \
      "run_root=$run_root" \
      "run_name=$run_name" \
      "owner_uid=$owner_uid" \
      "owner_pid=$$" \
      "process_group_id=$pgid" \
      "launcher_group_id=$launcher_group_id" \
      "created_epoch=$created_epoch" \
      "state=$state" > "$next"
  ) || return 1
  mv -f -- "$next" "$marker_path"
}

stop_process_group() {
  local signal_name="${1:-TERM}" attempts=0
  if [[ "$process_group_id" -gt 1 && "$process_group_id" != "$launcher_group_id" ]] &&
    group_is_active "$process_group_id";
  then
    kill -s "$signal_name" "-$process_group_id" 2>/dev/null || true
    while group_is_active "$process_group_id" && [[ "$attempts" -lt 50 ]]; do
      sleep 0.1
      attempts=$((attempts + 1))
    done
    if group_is_active "$process_group_id"; then
      kill -KILL "-$process_group_id" 2>/dev/null || true
    fi
  elif [[ "$command_pid" -gt 1 ]] && pid_is_active "$command_pid"; then
    kill -s "$signal_name" "$command_pid" 2>/dev/null || true
  fi
  if [[ "$command_pid" -gt 1 ]]; then
    wait "$command_pid" 2>/dev/null || true
  fi
  if [[ "$process_group_id" -gt 1 && "$process_group_id" != "$launcher_group_id" ]] &&
    group_is_active "$process_group_id";
  then
    return 1
  fi
  return 0
}

cleanup_current_root() {
  local physical=""
  [[ "$cleanup_attempted" -eq 0 ]] || return 0
  cleanup_attempted=1
  [[ "$run_root_created" -eq 1 ]] || return 0
  if [[ "$process_group_id" -gt 1 ]] && group_is_active "$process_group_id"; then
    printf 'Bluey test launcher: refusing cleanup while process group %s is active\n' \
      "$process_group_id" >&2
    return 1
  fi
  [[ -d "$run_root" && ! -L "$run_root" && -O "$run_root" ]] || return 1
  physical="$(cd -P "$run_root" && pwd)" || return 1
  [[ "$physical" == "$run_root" && "$(dirname "$physical")" == "$run_parent" ]] || return 1
  [[ "$run_name" =~ ^run\.[0-9]+\.[0-9]+\.[A-Za-z0-9]+$ ]] || return 1
  if [[ "$marker_established" -eq 1 ]]; then
    if ! read_marker "$marker_path" || ! marker_matches_root "$run_root" ||
      [[ "$marker_owner_pid" != "$$" ]];
    then
      printf 'Bluey test launcher: marker was established but is missing or invalid; retaining %s\n' \
        "$physical" >&2
      return 1
    fi
  fi
  if ! rm -rf -- "$physical"; then
    printf 'Bluey test launcher: cleanup command failed for %s\n' "$physical" >&2
    return 1
  fi
  if [[ -e "$physical" || -L "$physical" ]]; then
    printf 'Bluey test launcher: cleanup did not remove exact root %s\n' "$physical" >&2
    return 1
  fi
  return 0
}

finish_exit() {
  local original_status="$1" cleanup_status=0
  trap - EXIT HUP INT TERM
  stop_process_group TERM || cleanup_status=1
  cleanup_current_root || cleanup_status=1
  if [[ "$cleanup_status" -ne 0 ]]; then
    if [[ "$original_status" -eq 0 ]]; then
      printf 'Bluey test launcher: command passed but cleanup failed; returning %s\n' \
        "$CLEANUP_FAILURE_STATUS" >&2
      exit "$CLEANUP_FAILURE_STATUS"
    fi
    printf 'Bluey test launcher: cleanup failed; preserving command status %s\n' \
      "$original_status" >&2
  fi
  exit "$original_status"
}

on_exit() {
  local status=$?
  finish_exit "$status"
}

on_signal() {
  local signal_name="$1" status="$2"
  trap - HUP INT TERM
  stop_process_group "$signal_name" || true
  exit "$status"
}

run_one() {
  local requested_parent="$1"
  shift
  local attempts=0 gate="" command_status=0

  prepare_run_parent "$requested_parent"
  reap_stale_runs
  created_epoch="$(date +%s)" || fail "could not read current time"
  run_root_created=0
  marker_established=0
  deferred_signal_name=""
  deferred_signal_status=0

  # The EXIT trap exists before the run directory is created. During mkdir,
  # ordinary signals are deferred; the owned flag is set before normal signal
  # cleanup is restored, so a signal cannot observe an unmarked root as
  # unowned.
  trap on_exit EXIT
  install_runtime_signal_traps
  while [[ "$attempts" -lt 20 ]]; do
    run_name="run.$created_epoch.$$.${RANDOM}${RANDOM}${attempts}"
    run_root="$run_parent/$run_name"
    install_deferred_signal_traps
    if mkdir -m 0700 -- "$run_root" 2>/dev/null; then
      if [[ -n "${BLUEY_TEST_INJECT_AFTER_RUN_ROOT_MKDIR:-}" ]]; then
        kill -s "$BLUEY_TEST_INJECT_AFTER_RUN_ROOT_MKDIR" $$ ||
          fail "could not inject post-mkdir test signal"
      fi
      run_root_created=1
      dispatch_deferred_signal
      break
    fi
    dispatch_deferred_signal
    attempts=$((attempts + 1))
  done
  [[ "$run_root_created" -eq 1 ]] || fail "could not create unique test run root"
  marker_path="$run_root/$MARKER_NAME"

  mkdir -m 0700 -- \
    "$run_root/cargo-target" "$run_root/tmp" "$run_root/data" \
    "$run_root/config" "$run_root/runtime" "$run_root/logs" ||
    fail "could not create isolated test directories"
  # Set this before the marker write. An interrupted/failed write is retained
  # for audit rather than being mistaken for a safely unmarked early root.
  marker_established=1
  write_marker starting 0 || fail "could not establish test-run marker"

  export BLUEY_TEST_RUN_ROOT="$run_root"
  export CARGO_TARGET_DIR="$run_root/cargo-target"
  export TMPDIR="$run_root/tmp"
  export TMP="$run_root/tmp"
  export TEMP="$run_root/tmp"
  export SQLITE_TMPDIR="$run_root/tmp"
  export BLUEY_DB_PATH="$run_root/data/bluey-test.sqlite"
  export BLUEY_SERVER_DB_BACKEND=sqlite
  unset BLUEY_DATABASE_URL BLUEY_TEST_POSTGRES_URL
  export BLUEY_DATA_DIR="$run_root/data"
  export BLUEY_CONFIG_DIR="$run_root/config"
  export BLUEY_RUNTIME_DIR="$run_root/runtime"
  export BLUEY_LOG_DIR="$run_root/logs"
  export CUE_DATA_DIR="$BLUEY_DATA_DIR"
  export CUE_CONFIG_DIR="$BLUEY_CONFIG_DIR"
  export CUE_RUNTIME_DIR="$BLUEY_RUNTIME_DIR"
  export CUE_LOG_DIR="$BLUEY_LOG_DIR"
  export CARGO_INCREMENTAL=0
  export CARGO_PROFILE_DEV_INCREMENTAL=false
  export CARGO_PROFILE_TEST_INCREMENTAL=false
  export CARGO_PROFILE_DEV_DEBUG=0
  export CARGO_PROFILE_TEST_DEBUG=0

  gate="$run_root/.command-start"
  /usr/bin/perl -MPOSIX=setsid -e '
    my ($owner, $gate, $repo_root, @command) = @ARGV;
    while (!-e $gate) {
      exit 125 unless kill 0, $owner;
      select undef, undef, undef, 0.01;
    }
    exit 125 unless chdir $repo_root;
    exit 125 if setsid() == -1;
    exec @command;
    exit 125;
  ' "$$" "$gate" "$repo_root" "$@" &
  command_pid=$!
  process_group_id=$command_pid
  [[ "$process_group_id" != "$launcher_group_id" ]] || fail "unsafe process-group collision"
  write_marker running "$process_group_id" || fail "could not bind command process group"
  printf 'start\n' > "$gate" || fail "could not release command start gate"

  wait "$command_pid"
  command_status=$?
  command_pid=0
  finish_exit "$command_status"
}

self_test_fail() {
  printf 'Bluey test launcher self-test: %s\n' "$1" >&2
  exit 1
}

run_self_test() {
  local launcher="$script_dir/$(basename "$script_path")" test_root="" parent="" report="" status=0 root=""
  local fake_bin="" cargo_log="" all_line="" active_pid=0 active_group=0 attempts=0
  local zombie_parent_pid=0 zombie_pid=0 zombie_group="" zombie_state=""
  test_root="$(mktemp -d /tmp/bluey-test-launcher-self-test.XXXXXXXX)" || exit 1
  parent="$test_root/bluey-test-runs-selftest"

  cleanup_self_test() {
    if [[ "$active_group" -gt 1 ]]; then
      kill -TERM "-$active_group" 2>/dev/null || true
      wait "$active_pid" 2>/dev/null || true
    fi
    if [[ "$zombie_parent_pid" -gt 1 ]]; then
      kill -TERM "$zombie_parent_pid" 2>/dev/null || true
      wait "$zombie_parent_pid" 2>/dev/null || true
    fi
    /bin/rm -rf -- "$test_root"
  }
  trap cleanup_self_test EXIT HUP INT TERM

  report="$test_root/env.report"
  REPORT_FILE="$report" BLUEY_TEST_RUN_PARENT="$parent" \
    BLUEY_DB_PATH=/tmp/poison.db BLUEY_SERVER_DB_BACKEND=postgres \
    BLUEY_DATABASE_URL=postgres://poison BLUEY_TEST_POSTGRES_URL=postgres://poison-test \
    BLUEY_LOG_DIR=/tmp/poison-logs CUE_LOG_DIR=/tmp/poison-cue-logs \
    "$launcher" -- bash -c '
      set -eu
      test "$TMPDIR" = "$TMP" && test "$TMPDIR" = "$TEMP"
      test "$TMPDIR" = "$SQLITE_TMPDIR"
      test "$BLUEY_SERVER_DB_BACKEND" = sqlite
      test -z "${BLUEY_DATABASE_URL+x}" && test -z "${BLUEY_TEST_POSTGRES_URL+x}"
      test "$BLUEY_LOG_DIR" = "$CUE_LOG_DIR"
      test "$CARGO_INCREMENTAL" = 0
      printf "%s\n%s\n%s\n%s\n" "$BLUEY_TEST_RUN_ROOT" "$BLUEY_DB_PATH" \
        "$BLUEY_LOG_DIR" "$CARGO_TARGET_DIR" > "$REPORT_FILE"
      printf sqlite > "$BLUEY_DB_PATH"
      printf log > "$BLUEY_LOG_DIR/test.log"
    ' || self_test_fail "poisoned-environment isolation failed"
  parent="$(cd -P "$parent" && pwd)" || self_test_fail "could not resolve self-test parent"
  root="$(sed -n '1p' "$report")"
  [[ "$(sed -n '2p' "$report")" == "$root/data/bluey-test.sqlite" ]] ||
    self_test_fail "primary SQLite path was not isolated"
  [[ "$(sed -n '3p' "$report")" == "$root/logs" ]] ||
    self_test_fail "log path was not isolated"
  [[ "$(sed -n '4p' "$report")" == "$root/cargo-target" ]] ||
    self_test_fail "Cargo path was not isolated"
  [[ ! -e "$root" ]] || self_test_fail "successful run root survived"

  set +e
  BLUEY_TEST_RUN_PARENT="$parent" "$launcher" -- bash -c 'exit 37'
  status=$?
  set -e
  [[ "$status" -eq 37 ]] || self_test_fail "direct-command status was not preserved"

  fake_bin="$test_root/fake-bin"
  mkdir -m 0700 "$fake_bin"
  printf '%s\n' '#!/bin/sh' 'printf "%s|%s\n" "$PWD" "$*" >> "$CARGO_LOG"' > "$fake_bin/cargo"
  chmod 0700 "$fake_bin/cargo"
  cargo_log="$test_root/cargo.log"
  (
    cd /tmp || exit 1
    CARGO_LOG="$cargo_log" PATH="$fake_bin:$PATH" BLUEY_TEST_RUN_PARENT="$parent" \
      "$launcher" all
  ) || self_test_fail "all mapping failed"
  all_line="$(sed -n '1p' "$cargo_log")"
  [[ "$all_line" == "$repo_root|test --all-targets" ]] ||
    self_test_fail "all mapping did not run from repository root"
  [[ "$(sed -n '2p' "$cargo_log")" == "$repo_root|test --manifest-path server/Cargo.toml" ]] ||
    self_test_fail "all mapping omitted server tests"
  [[ "$(wc -l < "$cargo_log" | tr -d ' ')" == "2" ]] ||
    self_test_fail "all mapping ran an unexpected command"

  report="$test_root/direct-command-cwd.report"
  (
    cd /tmp || exit 1
    REPORT_FILE="$report" BLUEY_TEST_RUN_PARENT="$parent" "$launcher" -- bash -c \
      'printf "%s\\n" "$PWD" > "$REPORT_FILE"'
  ) || self_test_fail "direct command from /tmp failed"
  [[ "$(sed -n '1p' "$report")" == "$repo_root" ]] ||
    self_test_fail "direct command did not run from repository root"

  set +e
  BLUEY_TEST_RUN_PARENT="$parent" BLUEY_TEST_INJECT_AFTER_RUN_ROOT_MKDIR=TERM \
    "$launcher" -- true
  status=$?
  set -e
  [[ "$status" -eq 143 ]] || self_test_fail "post-mkdir TERM status was not preserved"
  if compgen -G "$parent/run.*" >/dev/null; then
    self_test_fail "post-mkdir signal run root survived"
  fi

  report="$test_root/descendant.pid"
  REPORT_FILE="$report" BLUEY_TEST_RUN_PARENT="$parent" "$launcher" -- bash -c '
    sleep 30 &
    printf "%s\n" "$!" > "$REPORT_FILE"
    exit 0
  ' || self_test_fail "descendant cleanup command failed"
  active_pid="$(sed -n '1p' "$report")"
  attempts=0
  while kill -0 "$active_pid" 2>/dev/null && [[ "$attempts" -lt 50 ]]; do
    sleep 0.05
    attempts=$((attempts + 1))
  done
  kill -0 "$active_pid" 2>/dev/null && self_test_fail "ordinary descendant survived"
  active_pid=0

  /usr/bin/perl -MPOSIX=setsid -e 'setsid(); exec "sleep", "30"' &
  active_pid=$!
  active_group=$active_pid
  attempts=0
  while [[ "$(ps -o pgid= -p "$active_pid" 2>/dev/null | tr -d ' ')" != "$active_group" &&
    "$attempts" -lt 100 ]]; do
    sleep 0.02
    attempts=$((attempts + 1))
  done
  root="$parent/run.1.99999999.GROUPACT"
  mkdir -m 0700 "$root"
  printf '%s\n' \
    "magic=$MARKER_MAGIC" "run_root=$root" 'run_name=run.1.99999999.GROUPACT' \
    "owner_uid=$owner_uid" 'owner_pid=99999999' "process_group_id=$active_group" \
    "launcher_group_id=$launcher_group_id" 'created_epoch=1' 'state=running' > "$root/$MARKER_NAME"
  BLUEY_TEST_RUN_PARENT="$parent" "$launcher" -- true || self_test_fail "group reaper probe failed"
  [[ -d "$root" ]] || self_test_fail "reaper touched an active process group"
  kill -TERM "-$active_group" 2>/dev/null || true
  wait "$active_pid" 2>/dev/null || true
  active_pid=0
  active_group=0
  /bin/rm -rf -- "$root"

  report="$test_root/zombie.pid"
  /usr/bin/perl -MPOSIX=setpgid -e '
    my ($report) = @ARGV;
    my $child = fork();
    die "fork failed" unless defined $child;
    if ($child == 0) {
      POSIX::setpgid(0, 0) == 0 or die "setpgid failed";
      exit 0;
    }
    open my $handle, ">", $report or die "report open failed";
    print {$handle} "$child\n";
    close $handle or die "report close failed";
    sleep 30;
  ' "$report" &
  zombie_parent_pid=$!
  attempts=0
  while [[ ! -s "$report" && "$attempts" -lt 100 ]]; do
    sleep 0.02
    attempts=$((attempts + 1))
  done
  [[ -s "$report" ]] || self_test_fail "zombie probe did not report its child"
  zombie_pid="$(sed -n '1p' "$report")"
  [[ "$zombie_pid" =~ ^[0-9]+$ && "$zombie_pid" -gt 1 ]] ||
    self_test_fail "zombie probe reported an invalid child"
  attempts=0
  while [[ "$attempts" -lt 100 ]]; do
    zombie_group="$(ps -o pgid= -p "$zombie_pid" 2>/dev/null | tr -d '[:space:]')"
    zombie_state="$(ps -o stat= -p "$zombie_pid" 2>/dev/null | tr -d '[:space:]')"
    [[ "$zombie_group" == "$zombie_pid" && "$zombie_state" == Z* ]] && break
    sleep 0.02
    attempts=$((attempts + 1))
  done
  [[ "$zombie_group" == "$zombie_pid" && "$zombie_state" == Z* ]] ||
    self_test_fail "zombie-only process-group probe was not established"
  root="$parent/run.1.99999993.ZOMBIE"
  mkdir -m 0700 "$root"
  printf '%s\n' \
    "magic=$MARKER_MAGIC" "run_root=$root" 'run_name=run.1.99999993.ZOMBIE' \
    "owner_uid=$owner_uid" 'owner_pid=99999993' "process_group_id=$zombie_pid" \
    "launcher_group_id=$launcher_group_id" 'created_epoch=1' 'state=running' > "$root/$MARKER_NAME"
  BLUEY_TEST_RUN_PARENT="$parent" "$launcher" -- true ||
    self_test_fail "zombie-only process-group cleanup probe failed"
  [[ ! -e "$root" ]] || self_test_fail "zombie-only process group blocked cleanup"
  kill -TERM "$zombie_parent_pid" 2>/dev/null || true
  wait "$zombie_parent_pid" 2>/dev/null || true
  zombie_parent_pid=0
  zombie_pid=0

  root="$parent/run.1.99999998.STALEOK"
  mkdir -m 0700 "$root"
  printf '%s\n' \
    "magic=$MARKER_MAGIC" "run_root=$root" 'run_name=run.1.99999998.STALEOK' \
    "owner_uid=$owner_uid" 'owner_pid=99999998' 'process_group_id=0' \
    "launcher_group_id=$launcher_group_id" 'created_epoch=1' 'state=starting' > "$root/$MARKER_NAME"
  BLUEY_TEST_RUN_PARENT="$parent" "$launcher" -- true || self_test_fail "stale recovery failed"
  [[ ! -e "$root" ]] || self_test_fail "inactive marker-valid stale root survived"

  printf '%s\n' '#!/bin/sh' \
    'if [ "$1" = "-p" ] || { [ "$1" = "-o" ] && [ "$2" = "pgid=" ] && [ "$3" = "-p" ]; }; then' \
    '  exec /bin/ps "$@"' \
    'fi' \
    'exit 2' > "$fake_bin/ps"
  chmod 0700 "$fake_bin/ps"
  root="$parent/run.1.99999997.PSFAIL"
  mkdir -m 0700 "$root"
  printf '%s\n' \
    "magic=$MARKER_MAGIC" "run_root=$root" 'run_name=run.1.99999997.PSFAIL' \
    "owner_uid=$owner_uid" 'owner_pid=99999997' 'process_group_id=99999997' \
    "launcher_group_id=$launcher_group_id" 'created_epoch=1' 'state=running' > "$root/$MARKER_NAME"
  set +e
  PATH="$fake_bin:$PATH" BLUEY_TEST_RUN_PARENT="$parent" "$launcher" -- bash -c 'exit 39'
  status=$?
  set -e
  [[ "$status" -eq 39 ]] || self_test_fail "ps-ambiguous reaper probe hid child status"
  [[ -d "$root" ]] || self_test_fail "reaper treated ps failure as inactive"
  /bin/rm -rf -- "$root"
  root="$(find "$parent" -mindepth 1 -maxdepth 1 -type d -name 'run.*' -print -quit)"
  [[ -n "$root" ]] || self_test_fail "ps-ambiguous current root was not retained"
  /bin/rm -rf -- "$root"
  /bin/rm -f -- "$fake_bin/ps"

  local marker_case="" marker_error=""
  for marker_case in missing corrupt; do
    report="$test_root/marker-$marker_case.report"
    marker_error="$test_root/marker-$marker_case.err"
    set +e
    REPORT_FILE="$report" MARKER_CASE="$marker_case" BLUEY_TEST_RUN_PARENT="$parent" \
      "$launcher" -- bash -c '
        printf "%s\\n" "$BLUEY_TEST_RUN_ROOT" > "$REPORT_FILE"
        case "$MARKER_CASE" in
          missing) rm -f -- "$BLUEY_TEST_RUN_ROOT/.bluey-test-run-v2" ;;
          corrupt) printf "corrupt\\n" > "$BLUEY_TEST_RUN_ROOT/.bluey-test-run-v2" ;;
          *) exit 88 ;;
        esac
      ' 2> "$marker_error"
    status=$?
    set -e
    case "$marker_case" in
      missing)
        [[ "$status" -eq "$CLEANUP_FAILURE_STATUS" ]] ||
          self_test_fail "marker-missing cleanup did not fail closed"
        ;;
      corrupt)
        [[ "$status" -eq "$CLEANUP_FAILURE_STATUS" ]] ||
          self_test_fail "marker-corrupt cleanup did not fail closed"
        ;;
    esac
    root="$(sed -n '1p' "$report")"
    [[ -d "$root" && ! -L "$root" ]] ||
      self_test_fail "marker-$marker_case root was not retained"
    grep -q 'marker was established but is missing or invalid' "$marker_error" ||
      self_test_fail "marker-$marker_case retention was not explicit"
    /bin/rm -rf -- "$root"
  done

  root="$parent/run.1.99999996.FOREIGN"
  mkdir -m 0700 "$root"
  printf 'keep\n' > "$root/foreign"
  local symlink_target="$test_root/symlink-target" symlink_run="" marker_link_root=""
  mkdir -m 0700 "$symlink_target"
  printf 'keep\n' > "$symlink_target/foreign"
  symlink_run="$parent/run.1.99999995.SYMLINK"
  ln -s "$symlink_target" "$symlink_run"
  marker_link_root="$parent/run.1.99999994.MARKLINK"
  mkdir -m 0700 "$marker_link_root"
  ln -s "$root/foreign" "$marker_link_root/$MARKER_NAME"
  BLUEY_TEST_RUN_PARENT="$parent" "$launcher" -- true || self_test_fail "symlink safety failed"
  [[ -f "$root/foreign" ]] || self_test_fail "reaper touched a foreign directory"
  [[ -L "$symlink_run" && -f "$symlink_target/foreign" ]] ||
    self_test_fail "reaper followed a candidate symlink"
  [[ -L "$marker_link_root/$MARKER_NAME" ]] || self_test_fail "reaper followed a marker symlink"
  /bin/rm -rf -- "$root" "$symlink_run" "$marker_link_root" "$symlink_target"

  for root in \
    "$repo_root/bluey-test-runs-reject-$$" \
    "$(dirname "$repo_root")/bluey-test-runs-reject-$$" \
    "/Volumes/bluey-test-runs-reject-$$";
  do
    [[ ! -e "$root" && ! -L "$root" ]] || self_test_fail "rejection target already exists"
    set +e
    BLUEY_TEST_RUN_PARENT="$root" "$launcher" -- true >/dev/null 2>&1
    status=$?
    set -e
    [[ "$status" -ne 0 && ! -e "$root" && ! -L "$root" ]] ||
      self_test_fail "unsafe parent was mutated before rejection: $root"
  done
  set +e
  BLUEY_TEST_RUN_PARENT="$test_root/./bluey-test-runs-dot" \
    "$launcher" -- true >/dev/null 2>&1
  status=$?
  set -e
  [[ "$status" -ne 0 && ! -e "$test_root/bluey-test-runs-dot" ]] ||
    self_test_fail "dot-segment parent was not rejected before mutation"

  root="$test_root/bluey-test-runs-public"
  mkdir -m 0755 "$root"
  set +e
  BLUEY_TEST_RUN_PARENT="$root" "$launcher" -- true >/dev/null 2>&1
  status=$?
  set -e
  [[ "$status" -ne 0 && "$(mode_of "$root")" == "755" ]] ||
    self_test_fail "existing public parent was changed or accepted"
  /bin/rm -rf -- "$root"

  printf '%s\n' '#!/bin/sh' 'exit 91' > "$fake_bin/rm"
  chmod 0700 "$fake_bin/rm"
  set +e
  PATH="$fake_bin:$PATH" BLUEY_TEST_RUN_PARENT="$parent" \
    "$launcher" -- true 2> "$test_root/cleanup-pass.err"
  status=$?
  set -e
  [[ "$status" -eq "$CLEANUP_FAILURE_STATUS" ]] ||
    self_test_fail "cleanup failure did not fail a successful command"
  grep -q 'command passed but cleanup failed' "$test_root/cleanup-pass.err" ||
    self_test_fail "successful cleanup failure was not explicit"
  root="$(find "$parent" -mindepth 1 -maxdepth 1 -type d -name 'run.*' -print -quit)"
  [[ -n "$root" ]] || self_test_fail "cleanup failure did not retain evidence"
  /bin/rm -rf -- "$root"

  set +e
  PATH="$fake_bin:$PATH" BLUEY_TEST_RUN_PARENT="$parent" \
    "$launcher" -- bash -c 'exit 39' 2> "$test_root/cleanup-fail.err"
  status=$?
  set -e
  [[ "$status" -eq 39 ]] || self_test_fail "cleanup failure hid child status"
  grep -q 'preserving command status 39' "$test_root/cleanup-fail.err" ||
    self_test_fail "nonzero cleanup failure was not explicit"
  root="$(find "$parent" -mindepth 1 -maxdepth 1 -type d -name 'run.*' -print -quit)"
  [[ -n "$root" ]] || self_test_fail "nonzero cleanup failure lost evidence"
  /bin/rm -rf -- "$root"

  set +e
  PATH="$fake_bin:$PATH" BLUEY_TEST_RUN_PARENT="$parent" \
    BLUEY_TEST_INJECT_AFTER_RUN_ROOT_MKDIR=TERM \
    "$launcher" -- true 2> "$test_root/cleanup-signal.err"
  status=$?
  set -e
  [[ "$status" -eq 143 ]] || self_test_fail "cleanup failure hid signal status"
  grep -q 'preserving command status 143' "$test_root/cleanup-signal.err" ||
    self_test_fail "signal cleanup failure was not explicit"
  root="$(find "$parent" -mindepth 1 -maxdepth 1 -type d -name 'run.*' -print -quit)"
  [[ -n "$root" ]] || self_test_fail "signal cleanup failure lost evidence"
  /bin/rm -rf -- "$root"

  if compgen -G "$parent/run.*" >/dev/null; then
    self_test_fail "test run residue remains"
  fi
  for root in "$repo_root/bluey-test-residue.db" "$repo_root/target/bluey-test-residue"; do
    [[ ! -e "$root" ]] || self_test_fail "repository residue exists: $root"
  done

  trap - EXIT HUP INT TERM
  cleanup_self_test
  [[ ! -e "$test_root" && ! -L "$test_root" ]] ||
    self_test_fail "self-test residue remains: $test_root"
  printf 'Bluey test launcher self-tests passed (canonical modes, repository CWD, poisoned env, process groups, zombie-only groups, marker fail-closed, cleanup failure, parent rejection, post-mkdir signals, and residue).\n'
}

if [[ "${1:-}" == "--self-test" ]]; then
  run_self_test
  exit 0
fi

requested_parent="${BLUEY_TEST_RUN_PARENT:-/tmp/bluey-test-runs-$owner_uid}"
if [[ "${1:-}" == "all" ]]; then
  shift
  [[ "$#" -eq 0 ]] || fail "all does not accept additional arguments" 64
  run_one "$requested_parent" bash -c \
    'cargo test --all-targets && cargo test --manifest-path server/Cargo.toml'
fi
if [[ "${1:-}" == "--" ]]; then shift; fi
[[ "$#" -gt 0 ]] || { usage; exit 64; }
run_one "$requested_parent" "$@"
