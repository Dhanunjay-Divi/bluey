#!/usr/bin/env bash

# Canonical local Bluey Rust test launcher. Release/package builds intentionally
# do not use this disposable target because their exact artifacts are promoted.

set -u
set -o pipefail

readonly MARKER_NAME=".bluey-test-run-v2"
readonly MARKER_MAGIC="bluey-test-run-v2"
readonly STALE_GRACE_SECONDS=60
readonly MAX_CANONICAL_DECIMAL_DIGITS=15
readonly WORKTREE_LIST_MAX_BYTES=1048576
readonly WORKTREE_LIST_CAPTURE_BYTES=1048578
readonly WORKTREE_LIST_TIMEOUT_SECONDS=10
readonly COMMAND_READY_POLL_ATTEMPTS=500
readonly STOP_POLL_ATTEMPTS=50
readonly CLEANUP_FAILURE_STATUS=74
readonly STALE_RECOVERY_FAILURE_STATUS=75

is_canonical_decimal() {
  local value="$1"
  if [[ "$value" == "0" ]]; then
    return 0
  fi
  [[ "$value" =~ ^[1-9][0-9]*$ ]] || return 1
  [[ "${#value}" -le "$MAX_CANONICAL_DECIMAL_DIGITS" ]]
}

is_canonical_decimal_gt_one() {
  local value="$1"
  is_canonical_decimal "$value" || return 1
  [[ "$value" -gt 1 ]]
}

script_path="${BASH_SOURCE[0]}"
script_dir="$(cd -P "$(dirname "$script_path")" && pwd)" || exit 70
repo_root="$(cd -P "$script_dir/.." && pwd)" || exit 70
owner_uid="$(id -u)" || exit 70
launcher_group_id="$(ps -o pgid= -p $$ 2>/dev/null | tr -d '[:space:]')"
is_canonical_decimal "$owner_uid" || exit 70
is_canonical_decimal_gt_one "$launcher_group_id" || exit 70

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
    [[ ! -e "$probe/.git" && ! -L "$probe/.git" ]] || return 1
    [[ "$probe" == "/" ]] && break
    probe="$(dirname "$probe")"
  done
}

validate_no_known_worktree_overlap() {
  local candidate="$1" line="" worktree="" physical="" listing=""
  local producer_status=0 LC_ALL=C sentinel=$'\034'
  listing="$({
    /usr/bin/perl -e '
      my ($timeout, @command) = @ARGV;
      $SIG{ALRM} = sub { exit 124 };
      alarm $timeout;
      exec @command;
      exit 125;
    ' "$WORKTREE_LIST_TIMEOUT_SECONDS" \
      git -C "$repo_root" worktree list --porcelain 2>/dev/null
    producer_status=$?
    printf '%s' "$sentinel"
    exit "$producer_status"
  } | /usr/bin/head -c "$WORKTREE_LIST_CAPTURE_BYTES")" || return 1
  [[ "$listing" == *"$sentinel" ]] || return 1
  listing="${listing%"$sentinel"}"
  [[ "${#listing}" -le "$WORKTREE_LIST_MAX_BYTES" ]] || return 1
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
  done <<< "$listing"
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
  validate_no_git_ancestor "$desired" ||
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
  read_path_identity "$run_parent" || fail "could not bind private run parent identity"
  run_parent_device_inode="$observed_path_device_inode"
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

observed_path_device_inode=""
observed_path_modified_epoch=""

read_path_identity() {
  local target="$1" raw="" device="" inode="" modified="" extra=""
  raw="$(stat -f '%d:%i:%m' "$target" 2>/dev/null)" ||
    raw="$(stat -c '%d:%i:%Y' "$target" 2>/dev/null)" || return 1
  IFS=: read -r device inode modified extra <<< "$raw"
  [[ -z "$extra" && "$device" =~ ^[0-9]+$ && "$inode" =~ ^[0-9]+$ ]] || return 1
  [[ "${#device}" -le 20 && "${#inode}" -le 20 ]] || return 1
  is_canonical_decimal "$modified" || return 1
  observed_path_device_inode="$device:$inode"
  observed_path_modified_epoch="$modified"
}

read_marker() {
  local marker="$1" l1="" l2="" l3="" l4="" l5="" l6="" l7="" l8="" l9="" extra=""
  local run_epoch="" run_pid="" run_nonce=""
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
  is_canonical_decimal "$marker_owner_uid" || return 1
  is_canonical_decimal_gt_one "$marker_owner_pid" || return 1
  is_canonical_decimal "$marker_process_group_id" || return 1
  is_canonical_decimal_gt_one "$marker_launcher_group_id" || return 1
  is_canonical_decimal "$marker_created_epoch" || return 1
  [[ "$marker_state" == "starting" || "$marker_state" == "running" ]] || return 1
  if [[ "$marker_state" == "starting" ]]; then
    [[ "$marker_process_group_id" == "0" ]] || return 1
  else
    is_canonical_decimal_gt_one "$marker_process_group_id" || return 1
  fi
  [[ "$marker_run_name" =~ ^run\.([0-9]+)\.([0-9]+)\.([A-Za-z0-9]+)$ ]] || return 1
  run_epoch="${BASH_REMATCH[1]}"
  run_pid="${BASH_REMATCH[2]}"
  run_nonce="${BASH_REMATCH[3]}"
  is_canonical_decimal "$run_epoch" || return 1
  is_canonical_decimal_gt_one "$run_pid" || return 1
  [[ "${#run_nonce}" -le 64 ]] || return 1
  [[ "$run_epoch" == "$marker_created_epoch" && "$run_pid" == "$marker_owner_pid" ]] ||
    return 1
}

pid_is_active() {
  local pid="$1" output="" status=0 observed_pid="" observed_state="" extra=""
  is_canonical_decimal_gt_one "$pid" || return 1
  output="$(ps -p "$pid" -o pid=,stat= 2>/dev/null)"
  status=$?
  if [[ "$status" -eq 0 ]]; then
    read -r observed_pid observed_state extra <<< "$output"
    if [[ -n "$extra" ]] || ! is_canonical_decimal_gt_one "$observed_pid" ||
      [[ "$observed_pid" != "$pid" || -z "$observed_state" ]];
    then
      return 0
    fi
    return 0
  fi
  if [[ "$status" -eq 1 && -z "$output" ]]; then
    kill -0 "$pid" 2>/dev/null && return 0
    return 1
  fi
  return 0
}

pid_can_execute() {
  local pid="$1" output="" status=0 observed_pid="" observed_state="" extra=""
  is_canonical_decimal_gt_one "$pid" || return 1
  output="$(ps -p "$pid" -o pid=,stat= 2>/dev/null)"
  status=$?
  if [[ "$status" -eq 0 ]]; then
    read -r observed_pid observed_state extra <<< "$output"
    if [[ -n "$extra" ]] || ! is_canonical_decimal_gt_one "$observed_pid" ||
      [[ "$observed_pid" != "$pid" || -z "$observed_state" ]];
    then
      return 0
    fi
    [[ "$observed_state" == Z* ]] && return 1
    return 0
  fi
  if [[ "$status" -eq 1 && -z "$output" ]]; then
    kill -0 "$pid" 2>/dev/null && return 0
    return 1
  fi
  return 0
}

group_is_active() {
  local pgid="$1" output="" status=0 observed_pgid="" observed_state=""
  is_canonical_decimal_gt_one "$pgid" || return 1
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
    is_canonical_decimal "$observed_pgid" && [[ -n "$observed_state" ]] || return 0
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

marker_fields_match() {
  [[ "$marker_magic" == "$1" && "$marker_run_root" == "$2" &&
    "$marker_run_name" == "$3" && "$marker_owner_uid" == "$4" &&
    "$marker_owner_pid" == "$5" && "$marker_process_group_id" == "$6" &&
    "$marker_launcher_group_id" == "$7" && "$marker_created_epoch" == "$8" &&
    "$marker_state" == "$9" ]]
}

validate_stale_candidate_age() {
  local now="$1" root_modified="$2" marker_modified="$3"
  local claimed_age=0 root_age=0 marker_age=0
  is_canonical_decimal "$now" || return 2
  is_canonical_decimal "$root_modified" || return 2
  is_canonical_decimal "$marker_modified" || return 2
  if [[ "$marker_created_epoch" -gt "$now" || "$root_modified" -gt "$now" ||
    "$marker_modified" -gt "$now" ]];
  then
    return 2
  fi
  claimed_age=$((now - marker_created_epoch))
  root_age=$((now - root_modified))
  marker_age=$((now - marker_modified))
  if [[ "$claimed_age" -lt "$STALE_GRACE_SECONDS" ]]; then
    return 1
  fi
  # An old name/marker copied into a newly created root is not stale-run
  # authority. Preserve it and block rather than deleting or starting a second
  # potentially large run alongside unknown state.
  if [[ "$root_age" -lt "$STALE_GRACE_SECONDS" ||
    "$marker_age" -lt "$STALE_GRACE_SECONDS" ]];
  then
    return 2
  fi
  return 0
}

delete_quarantine_path=""
delete_quarantine_device_inode=""

run_owned_test_hook() {
  local hook="$1"
  shift
  [[ -f "$hook" && ! -L "$hook" && -O "$hook" && -x "$hook" ]] || return 1
  "$hook" "$@"
}

create_delete_quarantine() {
  local attempts=0 candidate="" physical="" epoch=""
  delete_quarantine_path=""
  delete_quarantine_device_inode=""
  read_path_identity "$run_parent" || return 1
  [[ "$observed_path_device_inode" == "$run_parent_device_inode" ]] || return 1
  epoch="$(date +%s)" || return 1
  is_canonical_decimal "$epoch" || return 1
  while [[ "$attempts" -lt 20 ]]; do
    candidate="$run_parent/run.$epoch.$$.DELETE${RANDOM}${RANDOM}${attempts}"
    if /bin/mkdir -m 0700 -- "$candidate" 2>/dev/null; then
      [[ -d "$candidate" && ! -L "$candidate" && -O "$candidate" ]] || return 1
      [[ "$(mode_of "$candidate")" == "700" ]] || return 1
      physical="$(cd -P "$candidate" && pwd)" || return 1
      [[ "$physical" == "$candidate" && "$(dirname "$physical")" == "$run_parent" ]] ||
        return 1
      read_path_identity "$candidate" || return 1
      delete_quarantine_path="$candidate"
      delete_quarantine_device_inode="$observed_path_device_inode"
      read_path_identity "$run_parent" || return 1
      [[ "$observed_path_device_inode" == "$run_parent_device_inode" ]] || return 1
      return 0
    fi
    attempts=$((attempts + 1))
  done
  return 1
}

remove_empty_delete_quarantine() {
  local physical="" quarantine_name=""
  [[ -n "$delete_quarantine_path" && -n "$delete_quarantine_device_inode" ]] ||
    return 1
  [[ -d "$delete_quarantine_path" && ! -L "$delete_quarantine_path" &&
    -O "$delete_quarantine_path" ]] || return 1
  physical="$(cd -P "$delete_quarantine_path" && pwd)" || return 1
  [[ "$physical" == "$delete_quarantine_path" &&
    "$(dirname "$physical")" == "$run_parent" ]] || return 1
  read_path_identity "$delete_quarantine_path" || return 1
  [[ "$observed_path_device_inode" == "$delete_quarantine_device_inode" ]] || return 1
  quarantine_name="$(basename "$delete_quarantine_path")" || return 1
  [[ "$delete_quarantine_path" == "$run_parent/$quarantine_name" ]] || return 1
  (
    cd -P "$run_parent" || exit 1
    read_path_identity . || exit 1
    [[ "$observed_path_device_inode" == "$run_parent_device_inode" ]] || exit 1
    [[ -d "$quarantine_name" && ! -L "$quarantine_name" && -O "$quarantine_name" ]] ||
      exit 1
    read_path_identity "$quarantine_name" || exit 1
    [[ "$observed_path_device_inode" == "$delete_quarantine_device_inode" ]] || exit 1
    /bin/rmdir -- "$quarantine_name" || exit 1
    [[ ! -e "$quarantine_name" && ! -L "$quarantine_name" ]]
  ) || return 1
  [[ ! -e "$delete_quarantine_path" && ! -L "$delete_quarantine_path" ]]
}

verify_quarantined_payload() {
  local moved_root="$1" expected_root_device_inode="$2" marker_required="$3"
  local expected_marker_device_inode="$4" expected_marker_modified_epoch="$5"
  local expected_magic="$6" expected_run_root="$7" expected_run_name="$8"
  local expected_owner_uid="$9" expected_owner_pid="${10}"
  local expected_process_group_id="${11}" expected_launcher_group_id="${12}"
  local expected_created_epoch="${13}" expected_state="${14}"
  local physical="" moved_marker="$moved_root/$MARKER_NAME"
  local first_marker_device_inode="" first_marker_modified_epoch=""

  [[ "$marker_required" == "0" || "$marker_required" == "1" ]] || return 1
  [[ -d "$moved_root" && ! -L "$moved_root" && -O "$moved_root" ]] || return 1
  physical="$(cd -P "$moved_root" && pwd)" || return 1
  [[ "$physical" == "$moved_root" ]] || return 1
  read_path_identity "$moved_root" || return 1
  [[ "$observed_path_device_inode" == "$expected_root_device_inode" ]] || return 1

  if [[ "$marker_required" == "1" ]]; then
    [[ -n "$expected_marker_device_inode" && -n "$expected_marker_modified_epoch" ]] ||
      return 1
    read_path_identity "$moved_marker" || return 1
    first_marker_device_inode="$observed_path_device_inode"
    first_marker_modified_epoch="$observed_path_modified_epoch"
    if [[ "$first_marker_device_inode" != "$expected_marker_device_inode" ||
      "$first_marker_modified_epoch" != "$expected_marker_modified_epoch" ]] ||
      ! read_marker "$moved_marker" ||
      ! marker_fields_match \
        "$expected_magic" "$expected_run_root" "$expected_run_name" \
        "$expected_owner_uid" "$expected_owner_pid" \
        "$expected_process_group_id" "$expected_launcher_group_id" \
        "$expected_created_epoch" "$expected_state";
    then
      return 1
    fi
    read_path_identity "$moved_marker" || return 1
    [[ "$observed_path_device_inode" == "$expected_marker_device_inode" &&
      "$observed_path_modified_epoch" == "$expected_marker_modified_epoch" ]] ||
      return 1
  fi
  read_path_identity "$moved_root" || return 1
  [[ "$observed_path_device_inode" == "$expected_root_device_inode" ]]
}

delete_quarantined_payload() {
  local moved_root="$1" expected_root_device_inode="$2" marker_required="$3"
  local expected_marker_device_inode="$4" expected_marker_modified_epoch="$5"
  local expected_magic="$6" expected_run_root="$7" expected_run_name="$8"
  local expected_owner_uid="$9" expected_owner_pid="${10}"
  local expected_process_group_id="${11}" expected_launcher_group_id="${12}"
  local expected_created_epoch="${13}" expected_state="${14}"
  local platform="" quarantine_name=""

  verify_quarantined_payload "$@" || return 1
  platform="$(uname -s)" || return 1
  (
    cd -P "$moved_root" || exit 1
    read_path_identity . || exit 1
    [[ "$observed_path_device_inode" == "$expected_root_device_inode" ]] || exit 1
    if [[ "$marker_required" == "1" ]]; then
      read_path_identity "./$MARKER_NAME" || exit 1
      [[ "$observed_path_device_inode" == "$expected_marker_device_inode" &&
        "$observed_path_modified_epoch" == "$expected_marker_modified_epoch" ]] ||
        exit 1
      read_marker "./$MARKER_NAME" || exit 1
      marker_fields_match \
        "$expected_magic" "$expected_run_root" "$expected_run_name" \
        "$expected_owner_uid" "$expected_owner_pid" \
        "$expected_process_group_id" "$expected_launcher_group_id" \
        "$expected_created_epoch" "$expected_state" || exit 1
    fi
    case "$platform" in
      Darwin) /usr/bin/find -x . -depth -mindepth 1 -delete ;;
      Linux) /usr/bin/find . -xdev -depth -mindepth 1 -delete ;;
      *) exit 1 ;;
    esac
  ) || return 1

  quarantine_name="$(basename "$delete_quarantine_path")" || return 1
  [[ "$delete_quarantine_path" == "$run_parent/$quarantine_name" ]] || return 1
  (
    cd -P "$delete_quarantine_path" || exit 1
    read_path_identity . || exit 1
    [[ "$observed_path_device_inode" == "$delete_quarantine_device_inode" ]] || exit 1
    [[ -d root && ! -L root && -O root ]] || exit 1
    read_path_identity root || exit 1
    [[ "$observed_path_device_inode" == "$expected_root_device_inode" ]] || exit 1
    /bin/rmdir -- root || exit 1
    [[ ! -e root && ! -L root ]]
  ) || return 1
  remove_empty_delete_quarantine
}

quarantine_and_delete_root() {
  local source="$1" expected_root_device_inode="$2" marker_required="$3"
  local expected_marker_device_inode="$4" expected_marker_modified_epoch="$5"
  local expected_magic="$6" expected_run_root="$7" expected_run_name="$8"
  local expected_owner_uid="$9" expected_owner_pid="${10}"
  local expected_process_group_id="${11}" expected_launcher_group_id="${12}"
  local expected_created_epoch="${13}" expected_state="${14}"
  local physical="" moved_root="" hook="" report_file=""

  [[ "$marker_required" == "0" || "$marker_required" == "1" ]] || return 1
  [[ -n "$expected_root_device_inode" ]] || return 1
  [[ -d "$source" && ! -L "$source" && -O "$source" ]] || return 1
  physical="$(cd -P "$source" && pwd)" || return 1
  [[ "$physical" == "$source" && "$(dirname "$physical")" == "$run_parent" ]] ||
    return 1
  read_path_identity "$source" || return 1
  [[ "$observed_path_device_inode" == "$expected_root_device_inode" ]] || return 1

  create_delete_quarantine || return 1
  moved_root="$delete_quarantine_path/root"
  moved_marker="$moved_root/$MARKER_NAME"
  report_file="${BLUEY_TEST_DELETE_QUARANTINE_REPORT_FILE:-}"
  if [[ -n "$report_file" ]]; then
    printf '%s\n' "$delete_quarantine_path" > "$report_file" || {
      remove_empty_delete_quarantine || true
      return 1
    }
  fi
  hook="${BLUEY_TEST_INJECT_BEFORE_DELETE_RENAME_HOOK:-}"
  if [[ -n "$hook" ]]; then
    run_owned_test_hook "$hook" "$source" "$MARKER_NAME" || {
      remove_empty_delete_quarantine || true
      return 1
    }
  fi

  if ! /usr/bin/perl -e 'exit(rename($ARGV[0], $ARGV[1]) ? 0 : 1)' \
    "$source" "$moved_root";
  then
    remove_empty_delete_quarantine || true
    return 1
  fi
  if [[ -e "$source" || -L "$source" ]] ||
    ! verify_quarantined_payload \
      "$moved_root" "$expected_root_device_inode" "$marker_required" \
      "$expected_marker_device_inode" "$expected_marker_modified_epoch" \
      "$expected_magic" "$expected_run_root" "$expected_run_name" \
      "$expected_owner_uid" "$expected_owner_pid" "$expected_process_group_id" \
      "$expected_launcher_group_id" "$expected_created_epoch" "$expected_state";
  then
    printf 'Bluey test launcher: quarantined run-root identity changed; retaining %s\n' \
      "$delete_quarantine_path" >&2
    return 1
  fi

  hook="${BLUEY_TEST_INJECT_AFTER_QUARANTINE_VERIFY_HOOK:-}"
  if [[ -n "$hook" ]]; then
    run_owned_test_hook "$hook" "$moved_root" "$MARKER_NAME" || return 1
  fi
  if ! verify_quarantined_payload \
    "$moved_root" "$expected_root_device_inode" "$marker_required" \
    "$expected_marker_device_inode" "$expected_marker_modified_epoch" \
    "$expected_magic" "$expected_run_root" "$expected_run_name" \
    "$expected_owner_uid" "$expected_owner_pid" "$expected_process_group_id" \
    "$expected_launcher_group_id" "$expected_created_epoch" "$expected_state";
  then
    printf 'Bluey test launcher: final quarantined authority changed; retaining %s\n' \
      "$delete_quarantine_path" >&2
    return 1
  fi

  case "${BLUEY_TEST_INJECT_QUARANTINE_DELETE_FAILURE:-}" in
    "") ;;
    1)
      printf 'Bluey test launcher: injected quarantine deletion failure; retaining %s\n' \
        "$delete_quarantine_path" >&2
      return 1
      ;;
    *) return 1 ;;
  esac
  if ! delete_quarantined_payload \
    "$moved_root" "$expected_root_device_inode" "$marker_required" \
    "$expected_marker_device_inode" "$expected_marker_modified_epoch" \
    "$expected_magic" "$expected_run_root" "$expected_run_name" \
    "$expected_owner_uid" "$expected_owner_pid" "$expected_process_group_id" \
    "$expected_launcher_group_id" "$expected_created_epoch" "$expected_state";
  then
    printf 'Bluey test launcher: quarantine deletion failed; retaining %s\n' \
      "$delete_quarantine_path" >&2
    return 1
  fi
  if [[ -e "$source" || -L "$source" ]]; then
    printf 'Bluey test launcher: original run path was recreated; retaining %s\n' \
      "$source" >&2
    return 1
  fi
  return 0
}

reap_stale_runs() {
  local candidate="" marker="" now="" second_now="" age_status=0
  local accepted_magic="" accepted_run_root="" accepted_run_name=""
  local accepted_owner_uid="" accepted_owner_pid="" accepted_process_group_id=""
  local accepted_launcher_group_id="" accepted_created_epoch="" accepted_state=""
  local accepted_root_device_inode="" accepted_root_modified_epoch=""
  local accepted_marker_device_inode="" accepted_marker_modified_epoch=""
  local second_root_device_inode="" second_root_modified_epoch=""
  local second_marker_device_inode="" second_marker_modified_epoch=""
  now="$(date +%s)" || {
    printf 'Bluey test launcher: stale-run recovery could not read current time; no test command was started.\n' >&2
    return 1
  }
  if ! is_canonical_decimal "$now"; then
    printf 'Bluey test launcher: stale-run recovery received invalid current time; no test command was started.\n' >&2
    return 1
  fi
  for candidate in "$run_parent"/run.*; do
    [[ -e "$candidate" || -L "$candidate" ]] || continue
    [[ -d "$candidate" && ! -L "$candidate" && -O "$candidate" ]] || continue
    marker="$candidate/$MARKER_NAME"
    if ! read_marker "$marker"; then
      printf '%s\n' \
        "Bluey test launcher: stale-run recovery blocked by missing or malformed owned metadata: $candidate" \
        "Bluey test launcher: inspect that exact run root and its processes; remove only that root after proving it is inactive. No test command was started." >&2
      return 1
    fi
    if ! marker_matches_root "$candidate"; then
      printf '%s\n' \
        "Bluey test launcher: stale-run recovery blocked by mismatched owned metadata: $candidate" \
        "Bluey test launcher: inspect that exact run root and its processes; remove only that root after proving it is inactive. No test command was started." >&2
      return 1
    fi
    if ! read_path_identity "$candidate"; then
      printf '%s\n' \
        "Bluey test launcher: stale-run recovery could not stat exact owned root: $candidate" \
        "Bluey test launcher: inspect that exact run root and its metadata; no test command was started." >&2
      return 1
    fi
    accepted_root_device_inode="$observed_path_device_inode"
    accepted_root_modified_epoch="$observed_path_modified_epoch"
    if ! read_path_identity "$marker"; then
      printf '%s\n' \
        "Bluey test launcher: stale-run recovery could not stat exact owned marker: $candidate" \
        "Bluey test launcher: inspect that exact run root and its metadata; no test command was started." >&2
      return 1
    fi
    accepted_marker_device_inode="$observed_path_device_inode"
    accepted_marker_modified_epoch="$observed_path_modified_epoch"
    accepted_magic="$marker_magic"
    accepted_run_root="$marker_run_root"
    accepted_run_name="$marker_run_name"
    accepted_owner_uid="$marker_owner_uid"
    accepted_owner_pid="$marker_owner_pid"
    accepted_process_group_id="$marker_process_group_id"
    accepted_launcher_group_id="$marker_launcher_group_id"
    accepted_created_epoch="$marker_created_epoch"
    accepted_state="$marker_state"

    validate_stale_candidate_age \
      "$now" "$accepted_root_modified_epoch" "$accepted_marker_modified_epoch"
    age_status=$?
    if [[ "$age_status" -eq 1 ]]; then
      continue
    fi
    if [[ "$age_status" -ne 0 ]]; then
      printf '%s\n' \
        "Bluey test launcher: stale-run recovery blocked by future or conflicting filesystem age: $candidate" \
        "Bluey test launcher: inspect that exact run root and its clock/process evidence. No test command was started." >&2
      return 1
    fi
    pid_is_active "$marker_owner_pid" && continue
    if [[ "$marker_state" == "running" ]] && group_is_active "$marker_process_group_id"; then
      continue
    fi
    if ! read_marker "$marker" || ! marker_matches_root "$candidate" ||
      ! marker_fields_match \
        "$accepted_magic" "$accepted_run_root" "$accepted_run_name" \
        "$accepted_owner_uid" "$accepted_owner_pid" "$accepted_process_group_id" \
        "$accepted_launcher_group_id" "$accepted_created_epoch" "$accepted_state";
    then
      printf '%s\n' \
        "Bluey test launcher: stale-run metadata changed during recovery: $candidate" \
        "Bluey test launcher: inspect that exact run root and its processes. No test command was started." >&2
      return 1
    fi
    if ! read_path_identity "$candidate"; then
      printf 'Bluey test launcher: stale-run root identity became unavailable: %s\n' \
        "$candidate" >&2
      return 1
    fi
    second_root_device_inode="$observed_path_device_inode"
    second_root_modified_epoch="$observed_path_modified_epoch"
    if ! read_path_identity "$marker"; then
      printf 'Bluey test launcher: stale-run marker identity became unavailable: %s\n' \
        "$candidate" >&2
      return 1
    fi
    second_marker_device_inode="$observed_path_device_inode"
    second_marker_modified_epoch="$observed_path_modified_epoch"
    if [[ "$second_root_device_inode" != "$accepted_root_device_inode" ||
      "$second_root_modified_epoch" != "$accepted_root_modified_epoch" ||
      "$second_marker_device_inode" != "$accepted_marker_device_inode" ||
      "$second_marker_modified_epoch" != "$accepted_marker_modified_epoch" ]];
    then
      printf '%s\n' \
        "Bluey test launcher: stale-run path identity changed during recovery: $candidate" \
        "Bluey test launcher: inspect that exact run root and its processes. No test command was started." >&2
      return 1
    fi
    second_now="$(date +%s)" || {
      printf 'Bluey test launcher: stale-run recovery could not refresh current time; no test command was started.\n' >&2
      return 1
    }
    if ! is_canonical_decimal "$second_now"; then
      printf 'Bluey test launcher: stale-run recovery received invalid refreshed time; no test command was started.\n' >&2
      return 1
    fi
    validate_stale_candidate_age \
      "$second_now" "$second_root_modified_epoch" "$second_marker_modified_epoch"
    age_status=$?
    if [[ "$age_status" -ne 0 ]]; then
      printf '%s\n' \
        "Bluey test launcher: stale-run age eligibility changed during recovery: $candidate" \
        "Bluey test launcher: inspect that exact run root and its clock/process evidence. No test command was started." >&2
      return 1
    fi
    pid_is_active "$marker_owner_pid" && continue
    if [[ "$marker_state" == "running" ]] && group_is_active "$marker_process_group_id"; then
      continue
    fi
    if ! quarantine_and_delete_root \
      "$candidate" "$accepted_root_device_inode" 1 \
      "$accepted_marker_device_inode" "$accepted_marker_modified_epoch" \
      "$accepted_magic" "$accepted_run_root" "$accepted_run_name" \
      "$accepted_owner_uid" "$accepted_owner_pid" "$accepted_process_group_id" \
      "$accepted_launcher_group_id" "$accepted_created_epoch" "$accepted_state";
    then
      printf '%s\n' \
        "Bluey test launcher: eligible stale-run deletion failed for exact root: $candidate" \
        "Bluey test launcher: inspect that root and any reported quarantine; no test command was started." >&2
      return 1
    fi
  done
  return 0
}

run_root=""
run_name=""
run_root_created=0
run_parent=""
run_parent_device_inode=""
created_epoch=0
command_pid=0
process_group_id=0
cleanup_attempted=0
marker_path=""
marker_established=0
current_root_device_inode=""
current_marker_device_inode=""
current_marker_modified_epoch=""
current_marker_magic=""
current_marker_run_root=""
current_marker_run_name=""
current_marker_owner_uid=""
current_marker_owner_pid=""
current_marker_process_group_id=""
current_marker_launcher_group_id=""
current_marker_created_epoch=""
current_marker_state=""
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
  local state="$1" pgid="$2" next="$marker_path.next.$$" hook=""
  local next_device_inode="" next_modified_epoch=""
  read_path_identity "$run_root" || return 1
  [[ -n "$current_root_device_inode" &&
    "$observed_path_device_inode" == "$current_root_device_inode" ]] || return 1
  (
    umask 077
    set -o noclobber
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
  read_path_identity "$next" || return 1
  next_device_inode="$observed_path_device_inode"
  next_modified_epoch="$observed_path_modified_epoch"
  read_marker "$next" || return 1
  marker_matches_root "$run_root" || return 1
  [[ "$marker_process_group_id" == "$pgid" && "$marker_state" == "$state" ]] || return 1
  read_path_identity "$run_root" || return 1
  [[ "$observed_path_device_inode" == "$current_root_device_inode" ]] || return 1
  /usr/bin/perl -e 'exit(rename($ARGV[0], $ARGV[1]) ? 0 : 1)' \
    "$next" "$marker_path" || return 1
  hook="${BLUEY_TEST_INJECT_AFTER_MARKER_RENAME_HOOK:-}"
  if [[ -n "$hook" ]]; then
    run_owned_test_hook "$hook" "$marker_path" || return 1
  fi
  read_path_identity "$run_root" || return 1
  [[ "$observed_path_device_inode" == "$current_root_device_inode" ]] || return 1
  read_path_identity "$marker_path" || return 1
  [[ "$observed_path_device_inode" == "$next_device_inode" &&
    "$observed_path_modified_epoch" == "$next_modified_epoch" ]] || return 1
  current_marker_device_inode="$observed_path_device_inode"
  current_marker_modified_epoch="$observed_path_modified_epoch"
  read_marker "$marker_path" || return 1
  marker_matches_root "$run_root" || return 1
  [[ "$marker_process_group_id" == "$pgid" && "$marker_state" == "$state" ]] || return 1
  current_marker_magic="$marker_magic"
  current_marker_run_root="$marker_run_root"
  current_marker_run_name="$marker_run_name"
  current_marker_owner_uid="$marker_owner_uid"
  current_marker_owner_pid="$marker_owner_pid"
  current_marker_process_group_id="$marker_process_group_id"
  current_marker_launcher_group_id="$marker_launcher_group_id"
  current_marker_created_epoch="$marker_created_epoch"
  current_marker_state="$marker_state"
}

stop_process_group() {
  local signal_name="${1:-TERM}" attempts=0
  if is_canonical_decimal_gt_one "$process_group_id" &&
    [[ "$process_group_id" != "$launcher_group_id" ]] &&
    group_is_active "$process_group_id";
  then
    /bin/kill -s "$signal_name" -- "-$process_group_id" 2>/dev/null || true
    while group_is_active "$process_group_id" &&
      [[ "$attempts" -lt "$STOP_POLL_ATTEMPTS" ]];
    do
      sleep 0.05
      attempts=$((attempts + 1))
    done
    if group_is_active "$process_group_id"; then
      /bin/kill -s KILL -- "-$process_group_id" 2>/dev/null || true
      attempts=0
      while group_is_active "$process_group_id" &&
        [[ "$attempts" -lt "$STOP_POLL_ATTEMPTS" ]];
      do
        sleep 0.05
        attempts=$((attempts + 1))
      done
    fi
  elif is_canonical_decimal_gt_one "$command_pid" && pid_can_execute "$command_pid"; then
    /bin/kill -s "$signal_name" -- "$command_pid" 2>/dev/null || true
    attempts=0
    while pid_can_execute "$command_pid" &&
      [[ "$attempts" -lt "$STOP_POLL_ATTEMPTS" ]];
    do
      sleep 0.05
      attempts=$((attempts + 1))
    done
    if pid_can_execute "$command_pid"; then
      /bin/kill -s KILL -- "$command_pid" 2>/dev/null || true
      attempts=0
      while pid_can_execute "$command_pid" &&
        [[ "$attempts" -lt "$STOP_POLL_ATTEMPTS" ]];
      do
        sleep 0.05
        attempts=$((attempts + 1))
      done
    fi
  fi
  if is_canonical_decimal_gt_one "$command_pid"; then
    if pid_can_execute "$command_pid"; then
      /bin/kill -s KILL -- "$command_pid" 2>/dev/null || true
      attempts=0
      while pid_can_execute "$command_pid" &&
        [[ "$attempts" -lt "$STOP_POLL_ATTEMPTS" ]];
      do
        sleep 0.05
        attempts=$((attempts + 1))
      done
    fi
    pid_can_execute "$command_pid" && return 1
    wait "$command_pid" 2>/dev/null || true
  fi
  if is_canonical_decimal_gt_one "$process_group_id" &&
    [[ "$process_group_id" != "$launcher_group_id" ]] &&
    group_is_active "$process_group_id";
  then
    return 1
  fi
  return 0
}

cleanup_current_root() {
  local physical="" first_marker_device_inode="" first_marker_modified_epoch=""
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
  read_path_identity "$run_root" || return 1
  if [[ -z "$current_root_device_inode" ||
    "$observed_path_device_inode" != "$current_root_device_inode" ]];
  then
    printf 'Bluey test launcher: current run-root identity changed; retaining %s\n' \
      "$physical" >&2
    return 1
  fi
  if [[ "$marker_established" -eq 1 ]]; then
    if ! read_path_identity "$marker_path";
    then
      printf 'Bluey test launcher: marker was established but is missing or invalid; retaining %s\n' \
        "$physical" >&2
      return 1
    fi
    first_marker_device_inode="$observed_path_device_inode"
    first_marker_modified_epoch="$observed_path_modified_epoch"
    if [[ -z "$current_marker_device_inode" ||
      "$first_marker_device_inode" != "$current_marker_device_inode" ||
      "$first_marker_modified_epoch" != "$current_marker_modified_epoch" ]] ||
      ! read_marker "$marker_path" || ! marker_matches_root "$run_root" ||
      ! marker_fields_match \
        "$current_marker_magic" "$current_marker_run_root" "$current_marker_run_name" \
        "$current_marker_owner_uid" "$current_marker_owner_pid" \
        "$current_marker_process_group_id" "$current_marker_launcher_group_id" \
        "$current_marker_created_epoch" "$current_marker_state";
    then
      printf 'Bluey test launcher: marker was established but is missing or invalid; retaining %s\n' \
        "$physical" >&2
      return 1
    fi
    read_path_identity "$marker_path" || return 1
    if [[ "$observed_path_device_inode" != "$current_marker_device_inode" ||
      "$observed_path_modified_epoch" != "$current_marker_modified_epoch" ]];
    then
      printf 'Bluey test launcher: marker was established but is missing or invalid; retaining %s\n' \
        "$physical" >&2
      return 1
    fi
  fi
  read_path_identity "$run_root" || return 1
  if [[ "$observed_path_device_inode" != "$current_root_device_inode" ]]; then
    printf 'Bluey test launcher: current run-root changed during cleanup; retaining %s\n' \
      "$physical" >&2
    return 1
  fi
  if ! quarantine_and_delete_root \
    "$physical" "$current_root_device_inode" "$marker_established" \
    "$current_marker_device_inode" "$current_marker_modified_epoch" \
    "$current_marker_magic" "$current_marker_run_root" "$current_marker_run_name" \
    "$current_marker_owner_uid" "$current_marker_owner_pid" \
    "$current_marker_process_group_id" "$current_marker_launcher_group_id" \
    "$current_marker_created_epoch" "$current_marker_state";
  then
    printf 'Bluey test launcher: cleanup did not remove exact root %s\n' \
      "$physical" >&2
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
  local attempts=0 gate="" ready="" command_status=0 ready_pid="" ready_extra=""
  local observed_group_id="" pre_gate_report=""

  prepare_run_parent "$requested_parent"
  reap_stale_runs ||
    fail "stale-run recovery is blocked; refusing to start a new test command" \
      "$STALE_RECOVERY_FAILURE_STATUS"
  created_epoch="$(date +%s)" || fail "could not read current time"
  run_root_created=0
  marker_established=0
  current_root_device_inode=""
  current_marker_device_inode=""
  current_marker_modified_epoch=""
  current_marker_magic=""
  current_marker_run_root=""
  current_marker_run_name=""
  current_marker_owner_uid=""
  current_marker_owner_pid=""
  current_marker_process_group_id=""
  current_marker_launcher_group_id=""
  current_marker_created_epoch=""
  current_marker_state=""
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
      read_path_identity "$run_root" ||
        fail "could not bind the new test-run root identity"
      current_root_device_inode="$observed_path_device_inode"
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
  ready="$run_root/.command-ready"
  install_deferred_signal_traps
  /usr/bin/perl -MPOSIX=setsid -MFcntl=O_WRONLY,O_CREAT,O_EXCL -e '
    my ($owner, $ready, $gate, $repo_root, @command) = @ARGV;
    $SIG{HUP} = "DEFAULT";
    $SIG{INT} = "DEFAULT";
    $SIG{TERM} = "DEFAULT";
    exit 125 if setsid() == -1;
    sysopen(my $handle, $ready, O_WRONLY | O_CREAT | O_EXCL, 0600) or exit 125;
    print {$handle} "$$\n" or exit 125;
    close($handle) or exit 125;
    while (!-e $gate) {
      exit 125 unless kill 0, $owner;
      select undef, undef, undef, 0.01;
    }
    exit 125 unless chdir $repo_root;
    exec @command;
    exit 125;
  ' "$$" "$ready" "$gate" "$repo_root" "$@" &
  command_pid=$!
  attempts=0
  while [[ ! -s "$ready" && "$attempts" -lt "$COMMAND_READY_POLL_ATTEMPTS" ]] &&
    pid_is_active "$command_pid";
  do
    sleep 0.01
    attempts=$((attempts + 1))
  done
  if [[ ! -f "$ready" || -L "$ready" || ! -O "$ready" || ! -s "$ready" ]]; then
    dispatch_deferred_signal
    fail "command wrapper did not establish its private process group"
  fi
  {
    IFS= read -r ready_pid || ready_pid=""
    if IFS= read -r ready_extra; then ready_pid=""; fi
  } < "$ready"
  [[ "$ready_pid" == "$command_pid" ]] || {
    dispatch_deferred_signal
    fail "command wrapper ready identity did not match its child PID"
  }
  attempts=0
  while [[ "$attempts" -lt "$COMMAND_READY_POLL_ATTEMPTS" ]]; do
    observed_group_id="$(ps -o pgid= -p "$command_pid" 2>/dev/null | tr -d '[:space:]')"
    [[ "$observed_group_id" == "$command_pid" ]] && break
    pid_is_active "$command_pid" || break
    sleep 0.01
    attempts=$((attempts + 1))
  done
  [[ "$observed_group_id" == "$command_pid" ]] || {
    dispatch_deferred_signal
    fail "command wrapper process-group identity was not established"
  }
  process_group_id="$observed_group_id"
  [[ "$process_group_id" != "$launcher_group_id" ]] || {
    dispatch_deferred_signal
    fail "unsafe process-group collision"
  }
  pre_gate_report="${BLUEY_TEST_PRE_GATE_REPORT_FILE:-}"
  if [[ -n "$pre_gate_report" ]]; then
    printf '%s\n%s\n%s\n' "$run_root" "$command_pid" "$process_group_id" > \
      "$pre_gate_report" || {
        dispatch_deferred_signal
        fail "could not write pre-gate self-test report"
      }
  fi
  if [[ -n "${BLUEY_TEST_INJECT_BEFORE_COMMAND_GATE:-}" ]]; then
    kill -s "$BLUEY_TEST_INJECT_BEFORE_COMMAND_GATE" $$ || {
      dispatch_deferred_signal
      fail "could not inject pre-gate test signal"
    }
  fi
  if [[ -n "${BLUEY_TEST_INJECT_BEFORE_RUNNING_MARKER_HOOK:-}" ]]; then
    run_owned_test_hook \
      "$BLUEY_TEST_INJECT_BEFORE_RUNNING_MARKER_HOOK" "$run_root" "$MARKER_NAME" || {
      dispatch_deferred_signal
      fail "could not run pre-marker identity test hook"
    }
  fi
  write_marker running "$process_group_id" || {
    dispatch_deferred_signal
    fail "could not bind command process group"
  }
  dispatch_deferred_signal
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
  local signal_spec="" signal_name="" signal_status=0 signal_controller_pid=0
  local signal_controller_status=0 signal_launcher_pid="" signal_root=""
  local ready_file="" launcher_pid_file="" signal_report="" root_report=""
  local fixture_now="" future_epoch="" marker_case="" fixture_name=""
  local fixture_owner_pid="" fixture_group_id="" fixture_created_epoch="" fixture_state=""
  local fixture_started="" fixture_error="" swap_marker="" swap_done=""
  local pre_gate_signal="" pre_gate_status=0 pre_gate_report="" pre_gate_root=""
  local pre_gate_pid="" pre_gate_group="" pre_gate_started="" pre_gate_error=""
  local transition_hook="" marker_transition_hook="" delete_swap_hook=""
  local final_delete_hook=""
  local quarantine_report="" original_report=""
  test_root="$(mktemp -d /tmp/bluey-test-launcher-self-test.XXXXXXXX)" || exit 1
  parent="$test_root/bluey-test-runs-selftest"

  cleanup_self_test() {
    if [[ "$signal_controller_pid" -gt 1 ]]; then
      /bin/kill -s TERM -- "$signal_controller_pid" 2>/dev/null || true
      wait "$signal_controller_pid" 2>/dev/null || true
    fi
    if [[ "$active_group" -gt 1 ]]; then
      /bin/kill -s TERM -- "-$active_group" 2>/dev/null || true
      wait "$active_pid" 2>/dev/null || true
    fi
    if [[ "$zombie_parent_pid" -gt 1 ]]; then
      /bin/kill -s TERM -- "$zombie_parent_pid" 2>/dev/null || true
      wait "$zombie_parent_pid" 2>/dev/null || true
    fi
    /bin/rm -rf -- "$test_root"
  }

  write_self_test_marker() {
    local target_root="$1" target_name="$2" target_owner_pid="$3"
    local target_group_id="$4" target_created_epoch="$5" target_state="$6"
    printf '%s\n' \
      "magic=$MARKER_MAGIC" "run_root=$target_root" "run_name=$target_name" \
      "owner_uid=$owner_uid" "owner_pid=$target_owner_pid" \
      "process_group_id=$target_group_id" "launcher_group_id=$launcher_group_id" \
      "created_epoch=$target_created_epoch" "state=$target_state" > \
      "$target_root/$MARKER_NAME"
  }

  age_self_test_run() {
    local target_root="$1"
    /usr/bin/perl -e '
      my $updated = utime 1, 1, @ARGV;
      exit($updated == scalar(@ARGV) ? 0 : 1);
    ' "$target_root" "$target_root/$MARKER_NAME"
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

  for signal_spec in HUP:129 INT:130 TERM:143; do
    pre_gate_signal="${signal_spec%%:*}"
    pre_gate_status="${signal_spec##*:}"
    pre_gate_report="$test_root/pre-gate-$pre_gate_signal.report"
    pre_gate_started="$test_root/pre-gate-$pre_gate_signal.started"
    pre_gate_error="$test_root/pre-gate-$pre_gate_signal.err"
    set +e
    BLUEY_TEST_INJECT_BEFORE_COMMAND_GATE="$pre_gate_signal" \
      BLUEY_TEST_PRE_GATE_REPORT_FILE="$pre_gate_report" \
      COMMAND_STARTED_FILE="$pre_gate_started" BLUEY_TEST_RUN_PARENT="$parent" \
      "$launcher" -- bash -c 'printf "started\n" > "$COMMAND_STARTED_FILE"' \
      2> "$pre_gate_error"
    status=$?
    set -e
    [[ "$status" -eq "$pre_gate_status" ]] ||
      self_test_fail "pre-gate $pre_gate_signal status was not preserved"
    [[ -s "$pre_gate_report" && ! -e "$pre_gate_started" ]] ||
      self_test_fail "pre-gate $pre_gate_signal started the user command or lacked handshake proof"
    pre_gate_root="$(sed -n '1p' "$pre_gate_report")"
    pre_gate_pid="$(sed -n '2p' "$pre_gate_report")"
    pre_gate_group="$(sed -n '3p' "$pre_gate_report")"
    [[ -n "$pre_gate_root" && "$pre_gate_pid" == "$pre_gate_group" ]] ||
      self_test_fail "pre-gate $pre_gate_signal did not bind the private process group"
    is_canonical_decimal_gt_one "$pre_gate_pid" ||
      self_test_fail "pre-gate $pre_gate_signal reported an invalid wrapper PID"
    pid_is_active "$pre_gate_pid" &&
      self_test_fail "pre-gate $pre_gate_signal wrapper survived"
    [[ ! -e "$pre_gate_root" && ! -L "$pre_gate_root" ]] ||
      self_test_fail "pre-gate $pre_gate_signal exact run root survived"
  done

  for signal_spec in HUP:129 INT:130 TERM:143; do
    signal_name="${signal_spec%%:*}"
    signal_status="${signal_spec##*:}"
    ready_file="$test_root/signal-$signal_name.ready"
    launcher_pid_file="$test_root/signal-$signal_name.launcher-pid"
    signal_report="$test_root/signal-$signal_name.received"
    root_report="$test_root/signal-$signal_name.root"
    (
      attempts=0
      while [[ ! -s "$ready_file" && "$attempts" -lt 200 ]]; do
        sleep 0.02
        attempts=$((attempts + 1))
      done
      if [[ ! -s "$ready_file" ]]; then
        if [[ -s "$launcher_pid_file" ]]; then
          signal_launcher_pid="$(sed -n '1p' "$launcher_pid_file")"
          if [[ "$signal_launcher_pid" =~ ^[0-9]+$ && "$signal_launcher_pid" -gt 1 ]]; then
            /bin/kill -s TERM -- "$signal_launcher_pid" 2>/dev/null || true
          fi
        fi
        exit 92
      fi
      signal_launcher_pid="$(sed -n '1p' "$launcher_pid_file")"
      [[ "$signal_launcher_pid" =~ ^[0-9]+$ && "$signal_launcher_pid" -gt 1 ]] || exit 93
      /bin/kill -s "$signal_name" -- "$signal_launcher_pid"
    ) &
    signal_controller_pid=$!
    set +e
    READY_FILE="$ready_file" LAUNCHER_PID_FILE="$launcher_pid_file" \
      SIGNAL_REPORT="$signal_report" ROOT_REPORT="$root_report" \
      BLUEY_TEST_RUN_PARENT="$parent" "$launcher" -- bash -c '
        record_signal() {
          printf "%s\n" "$1" > "$SIGNAL_REPORT"
          exit "$2"
        }
        trap "record_signal HUP 129" HUP
        trap "record_signal INT 130" INT
        trap "record_signal TERM 143" TERM
        printf "%s\n" "$PPID" > "$LAUNCHER_PID_FILE"
        printf "%s\n" "$BLUEY_TEST_RUN_ROOT" > "$ROOT_REPORT"
        printf "ready\n" > "$READY_FILE"
        while :; do sleep 1; done
      ' 2> "$test_root/signal-$signal_name.err"
    status=$?
    wait "$signal_controller_pid"
    signal_controller_status=$?
    signal_controller_pid=0
    set -e
    [[ "$signal_controller_status" -eq 0 ]] ||
      self_test_fail "$signal_name controller could not signal the launcher"
    [[ "$status" -eq "$signal_status" ]] ||
      self_test_fail "$signal_name launcher status was not preserved"
    [[ "$(sed -n '1p' "$signal_report")" == "$signal_name" ]] ||
      self_test_fail "child did not receive $signal_name"
    signal_root="$(sed -n '1p' "$root_report")"
    [[ -n "$signal_root" && ! -e "$signal_root" && ! -L "$signal_root" ]] ||
      self_test_fail "$signal_name run root survived"
  done

  fixture_now="$(date +%s)" || self_test_fail "could not read fixture time"
  is_canonical_decimal "$fixture_now" || self_test_fail "fixture time was not canonical"
  future_epoch=$((fixture_now + 3600))
  fixture_name="run.$future_epoch.99999990.FUTURE"
  root="$parent/$fixture_name"
  mkdir -m 0700 "$root"
  write_self_test_marker "$root" "$fixture_name" 99999990 0 "$future_epoch" starting
  fixture_started="$test_root/future-marker.started"
  fixture_error="$test_root/future-marker.err"
  set +e
  COMMAND_STARTED_FILE="$fixture_started" BLUEY_TEST_RUN_PARENT="$parent" \
    "$launcher" -- bash -c 'printf "started\n" > "$COMMAND_STARTED_FILE"' \
    2> "$fixture_error"
  status=$?
  set -e
  [[ "$status" -eq "$STALE_RECOVERY_FAILURE_STATUS" && -d "$root" &&
    ! -e "$fixture_started" ]] ||
    self_test_fail "future-dated marker did not block before child start"
  grep -q 'future or conflicting filesystem age' "$fixture_error" ||
    self_test_fail "future-dated marker lacked bounded operator guidance"
  /bin/rm -rf -- "$root"

  for marker_case in \
    overflow leading-zero epoch-drift pid-drift starting-group running-zero;
  do
    fixture_owner_pid=99999990
    fixture_group_id=0
    fixture_created_epoch=1
    fixture_state=starting
    case "$marker_case" in
      overflow)
        fixture_created_epoch=18446744073709551616
        fixture_name="run.$fixture_created_epoch.$fixture_owner_pid.OVERFLOW"
        ;;
      leading-zero)
        fixture_owner_pid=09999990
        fixture_created_epoch=01
        fixture_name="run.$fixture_created_epoch.$fixture_owner_pid.LEADING"
        ;;
      epoch-drift)
        fixture_name="run.2.$fixture_owner_pid.EPOCHDRIFT"
        ;;
      pid-drift)
        fixture_name="run.1.99999991.PIDDRIFT"
        ;;
      starting-group)
        fixture_group_id=99999989
        fixture_name="run.1.$fixture_owner_pid.STARTGROUP"
        ;;
      running-zero)
        fixture_state=running
        fixture_name="run.1.$fixture_owner_pid.RUNZERO"
        ;;
      *) self_test_fail "unknown malformed marker fixture" ;;
    esac
    root="$parent/$fixture_name"
    mkdir -m 0700 "$root"
    write_self_test_marker \
      "$root" "$fixture_name" "$fixture_owner_pid" "$fixture_group_id" \
      "$fixture_created_epoch" "$fixture_state"
    fixture_started="$test_root/marker-$marker_case.started"
    fixture_error="$test_root/marker-$marker_case.err"
    set +e
    COMMAND_STARTED_FILE="$fixture_started" BLUEY_TEST_RUN_PARENT="$parent" \
      "$launcher" -- bash -c 'printf "started\n" > "$COMMAND_STARTED_FILE"' \
      2> "$fixture_error"
    status=$?
    set -e
    [[ "$status" -eq "$STALE_RECOVERY_FAILURE_STATUS" && -d "$root" &&
      ! -e "$fixture_started" ]] ||
      self_test_fail "$marker_case marker authority did not block before child start"
    grep -q 'missing or malformed owned metadata' "$fixture_error" ||
      self_test_fail "$marker_case marker authority lacked bounded operator guidance"
    /bin/rm -rf -- "$root"
  done

  fixture_name="run.1.99999990.YOUNGSTALE"
  root="$parent/$fixture_name"
  mkdir -m 0700 "$root"
  write_self_test_marker "$root" "$fixture_name" 99999990 0 1 starting
  fixture_started="$test_root/young-stale.started"
  fixture_error="$test_root/young-stale.err"
  set +e
  COMMAND_STARTED_FILE="$fixture_started" BLUEY_TEST_RUN_PARENT="$parent" \
    "$launcher" -- bash -c 'printf "started\n" > "$COMMAND_STARTED_FILE"' \
    2> "$fixture_error"
  status=$?
  set -e
  [[ "$status" -eq "$STALE_RECOVERY_FAILURE_STATUS" && -d "$root" &&
    ! -e "$fixture_started" ]] ||
    self_test_fail "young root with stale marker did not block before child start"
  grep -q 'future or conflicting filesystem age' "$fixture_error" ||
    self_test_fail "young root with stale marker lacked bounded operator guidance"
  /bin/rm -rf -- "$root"

  fixture_name="run.1.99999989.SWAP"
  root="$parent/$fixture_name"
  mkdir -m 0700 "$root"
  write_self_test_marker "$root" "$fixture_name" 99999989 0 1 starting
  swap_marker="$root/$MARKER_NAME.swap"
  /bin/cp "$root/$MARKER_NAME" "$swap_marker" ||
    self_test_fail "could not copy same-field marker-swap fixture"
  age_self_test_run "$root" || self_test_fail "could not age marker-swap fixture"
  /usr/bin/perl -e 'exit(utime(1, 1, @ARGV) == scalar(@ARGV) ? 0 : 1)' \
    "$swap_marker" || self_test_fail "could not age replacement marker fixture"
  swap_done="$test_root/marker-swap.done"
  printf '%s\n' '#!/bin/sh' \
    'if [ "$1" = "-p" ] && [ "$2" = "$SWAP_OWNER_PID" ] && [ ! -e "$SWAP_DONE" ]; then' \
    '  /bin/mv -f -- "$SWAP_MARKER_PATH" "$SWAP_TARGET_PATH" || exit 90' \
    '  /usr/bin/perl -e '\''exit(utime(1, 1, @ARGV) == scalar(@ARGV) ? 0 : 1)'\'' "$SWAP_TARGET_PATH" "$SWAP_ROOT" || exit 91' \
    '  : > "$SWAP_DONE"' \
    '  exit 1' \
    'fi' \
    'exec /bin/ps "$@"' > "$fake_bin/ps"
  chmod 0700 "$fake_bin/ps"
  fixture_started="$test_root/marker-swap.started"
  fixture_error="$test_root/marker-swap.err"
  set +e
  SWAP_OWNER_PID=99999989 SWAP_DONE="$swap_done" SWAP_MARKER_PATH="$swap_marker" \
    SWAP_TARGET_PATH="$root/$MARKER_NAME" SWAP_ROOT="$root" \
    COMMAND_STARTED_FILE="$fixture_started" \
    PATH="$fake_bin:$PATH" BLUEY_TEST_RUN_PARENT="$parent" \
    "$launcher" -- bash -c 'printf "started\n" > "$COMMAND_STARTED_FILE"' \
    2> "$fixture_error"
  status=$?
  set -e
  [[ "$status" -eq "$STALE_RECOVERY_FAILURE_STATUS" && -e "$swap_done" &&
    -d "$root" && ! -e "$fixture_started" ]] ||
    self_test_fail "same-field marker inode replacement did not block before child start"
  grep -q 'path identity changed during recovery' "$fixture_error" ||
    self_test_fail "same-field marker inode replacement missed path-identity authority"
  /bin/rm -rf -- "$root"
  /bin/rm -f -- "$fake_bin/ps"

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
  age_self_test_run "$root" || self_test_fail "could not age active-group fixture"
  BLUEY_TEST_RUN_PARENT="$parent" "$launcher" -- true || self_test_fail "group reaper probe failed"
  [[ -d "$root" ]] || self_test_fail "reaper touched an active process group"
  /bin/kill -s TERM -- "-$active_group" 2>/dev/null || true
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
  age_self_test_run "$root" || self_test_fail "could not age zombie-group fixture"
  BLUEY_TEST_RUN_PARENT="$parent" "$launcher" -- true ||
    self_test_fail "zombie-only process-group cleanup probe failed"
  [[ ! -e "$root" ]] || self_test_fail "zombie-only process group blocked cleanup"
  /bin/kill -s TERM -- "$zombie_parent_pid" 2>/dev/null || true
  wait "$zombie_parent_pid" 2>/dev/null || true
  zombie_parent_pid=0
  zombie_pid=0

  root="$parent/run.1.99999998.STALEOK"
  mkdir -m 0700 "$root"
  printf '%s\n' \
    "magic=$MARKER_MAGIC" "run_root=$root" 'run_name=run.1.99999998.STALEOK' \
    "owner_uid=$owner_uid" 'owner_pid=99999998' 'process_group_id=0' \
    "launcher_group_id=$launcher_group_id" 'created_epoch=1' 'state=starting' > "$root/$MARKER_NAME"
  age_self_test_run "$root" || self_test_fail "could not age stale-root fixture"
  BLUEY_TEST_RUN_PARENT="$parent" "$launcher" -- true || self_test_fail "stale recovery failed"
  [[ ! -e "$root" ]] || self_test_fail "inactive marker-valid stale root survived"

  root="$parent/run.1.99999992.STALEFAIL"
  mkdir -m 0700 "$root"
  printf '%s\n' \
    "magic=$MARKER_MAGIC" "run_root=$root" 'run_name=run.1.99999992.STALEFAIL' \
    "owner_uid=$owner_uid" 'owner_pid=99999992' 'process_group_id=0' \
    "launcher_group_id=$launcher_group_id" 'created_epoch=1' 'state=starting' > "$root/$MARKER_NAME"
  age_self_test_run "$root" || self_test_fail "could not age deletion-failure fixture"
  quarantine_report="$test_root/stale-delete-quarantine.report"
  set +e
  COMMAND_STARTED_FILE="$test_root/stale-delete-started" \
    BLUEY_TEST_INJECT_QUARANTINE_DELETE_FAILURE=1 \
    BLUEY_TEST_DELETE_QUARANTINE_REPORT_FILE="$quarantine_report" \
    BLUEY_TEST_RUN_PARENT="$parent" "$launcher" -- bash -c \
      'printf "started\n" > "$COMMAND_STARTED_FILE"' \
      2> "$test_root/stale-delete.err"
  status=$?
  set -e
  [[ "$status" -eq "$STALE_RECOVERY_FAILURE_STATUS" ]] ||
    self_test_fail "stale deletion failure did not block the next command"
  root="$(sed -n '1p' "$quarantine_report")"
  [[ -d "$root/root" && ! -e "$test_root/stale-delete-started" ]] ||
    self_test_fail "stale deletion failure started child command or lost evidence"
  grep -q 'eligible stale-run deletion failed for exact root' \
    "$test_root/stale-delete.err" ||
    self_test_fail "stale deletion failure lacked bounded operator guidance"
  /bin/rm -rf -- "$root"

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
  age_self_test_run "$root" || self_test_fail "could not age ambiguous-ps fixture"
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

  local current_identity_case="" current_identity_error="" old_root_report=""
  local old_root=""
  for current_identity_case in marker-inode marker-fields root-swap; do
    report="$test_root/current-$current_identity_case.report"
    old_root_report="$test_root/current-$current_identity_case.old-root"
    current_identity_error="$test_root/current-$current_identity_case.err"
    set +e
    REPORT_FILE="$report" OLD_ROOT_REPORT="$old_root_report" \
      IDENTITY_CASE="$current_identity_case" BLUEY_TEST_RUN_PARENT="$parent" \
      "$launcher" -- bash -c '
        set -eu
        marker="$BLUEY_TEST_RUN_ROOT/.bluey-test-run-v2"
        printf "%s\n" "$BLUEY_TEST_RUN_ROOT" > "$REPORT_FILE"
        case "$IDENTITY_CASE" in
          marker-inode)
            /bin/cp -p "$marker" "$marker.replacement"
            /bin/mv -f "$marker.replacement" "$marker"
            ;;
          marker-fields)
            cp -p -- "$marker" "$marker.timestamp"
            rewritten="$(sed \
              -e "s/^process_group_id=[0-9][0-9]*$/process_group_id=0/" \
              -e "s/^state=running$/state=starting/" "$marker")"
            printf "%s\n" "$rewritten" > "$marker"
            touch -r "$marker.timestamp" "$marker"
            rm -f -- "$marker.timestamp"
            ;;
          root-swap)
            old_root="$BLUEY_TEST_RUN_ROOT.old"
            /bin/mv "$BLUEY_TEST_RUN_ROOT" "$old_root"
            /bin/mkdir -m 0700 "$BLUEY_TEST_RUN_ROOT"
            /bin/cp -p "$old_root/.bluey-test-run-v2" \
              "$BLUEY_TEST_RUN_ROOT/.bluey-test-run-v2"
            printf "keep\n" > "$old_root/foreign-sentinel"
            printf "%s\n" "$old_root" > "$OLD_ROOT_REPORT"
            ;;
          *) exit 88 ;;
        esac
      ' 2> "$current_identity_error"
    status=$?
    set -e
    [[ "$status" -eq "$CLEANUP_FAILURE_STATUS" ]] ||
      self_test_fail "current $current_identity_case cleanup did not fail closed"
    root="$(sed -n '1p' "$report")"
    [[ -d "$root" && ! -L "$root" ]] ||
      self_test_fail "current $current_identity_case substituted root was not retained"
    case "$current_identity_case" in
      marker-inode)
        grep -q 'marker was established but is missing or invalid' \
          "$current_identity_error" ||
          self_test_fail "current marker inode replacement lacked retention guidance"
        ;;
      marker-fields)
        grep -q '^process_group_id=0$' "$root/$MARKER_NAME" &&
          grep -q '^state=starting$' "$root/$MARKER_NAME" ||
          self_test_fail "current marker field rewrite fixture did not preserve its mutation"
        grep -q 'marker was established but is missing or invalid' \
          "$current_identity_error" ||
          self_test_fail "current marker field rewrite lacked retention guidance"
        ;;
      root-swap)
        old_root="$(sed -n '1p' "$old_root_report")"
        [[ -d "$old_root" && -f "$old_root/foreign-sentinel" ]] ||
          self_test_fail "current root substitution removed the original command data"
        grep -q 'current run-root identity changed' "$current_identity_error" ||
          self_test_fail "current root substitution lacked retention guidance"
        /bin/rm -rf -- "$old_root"
        ;;
    esac
    /bin/rm -rf -- "$root"
  done

  transition_hook="$fake_bin/transition-root-before-running-marker"
  printf '%s\n' \
    '#!/bin/sh' \
    'set -eu' \
    'root="$1"' \
    'marker_name="$2"' \
    'old_root="$root.transition-original"' \
    '/bin/mv "$root" "$old_root"' \
    '/bin/mkdir -m 0700 "$root"' \
    '/bin/cp -p "$old_root/$marker_name" "$root/$marker_name"' \
    'printf "original\n" > "$old_root/transition-original-sentinel"' \
    'printf "substitute\n" > "$root/transition-substitute-sentinel"' \
    'printf "%s\n" "$old_root" > "$OLD_ROOT_REPORT"' \
    'printf "%s\n" "$root" > "$NEW_ROOT_REPORT"' > "$transition_hook"
  chmod 0700 "$transition_hook"
  report="$test_root/transition-new-root.report"
  original_report="$test_root/transition-old-root.report"
  set +e
  OLD_ROOT_REPORT="$original_report" NEW_ROOT_REPORT="$report" \
    COMMAND_STARTED_FILE="$test_root/transition-command-started" \
    BLUEY_TEST_INJECT_BEFORE_RUNNING_MARKER_HOOK="$transition_hook" \
    BLUEY_TEST_RUN_PARENT="$parent" "$launcher" -- bash -c \
      'printf "started\n" > "$COMMAND_STARTED_FILE"' \
      2> "$test_root/transition-root.err"
  status=$?
  set -e
  [[ "$status" -eq 70 && ! -e "$test_root/transition-command-started" ]] ||
    self_test_fail "pre-running-marker root substitution gained command authority"
  root="$(sed -n '1p' "$report")"
  old_root="$(sed -n '1p' "$original_report")"
  [[ -f "$root/transition-substitute-sentinel" &&
    -f "$old_root/transition-original-sentinel" ]] ||
    self_test_fail "pre-running-marker root substitution deleted retained evidence"
  grep -q 'current run-root identity changed' "$test_root/transition-root.err" ||
    self_test_fail "pre-running-marker root substitution lacked retention guidance"
  /bin/rm -rf -- "$root" "$old_root"

  marker_transition_hook="$fake_bin/swap-marker-after-transition-rename"
  printf '%s\n' \
    '#!/bin/sh' \
    'set -eu' \
    'marker="$1"' \
    'grep -q "^state=running$" "$marker" || exit 0' \
    '/bin/cp -p "$marker" "$marker.replacement"' \
    '/bin/mv -f "$marker.replacement" "$marker"' \
    'printf "replacement\n" > "$marker.transition-sentinel"' > \
    "$marker_transition_hook"
  chmod 0700 "$marker_transition_hook"
  report="$test_root/marker-transition-root.report"
  set +e
  BLUEY_TEST_PRE_GATE_REPORT_FILE="$report" \
    COMMAND_STARTED_FILE="$test_root/marker-transition-command-started" \
    BLUEY_TEST_INJECT_AFTER_MARKER_RENAME_HOOK="$marker_transition_hook" \
    BLUEY_TEST_RUN_PARENT="$parent" "$launcher" -- bash -c \
      'printf "started\n" > "$COMMAND_STARTED_FILE"' \
      2> "$test_root/marker-transition.err"
  status=$?
  set -e
  [[ "$status" -eq 70 && ! -e "$test_root/marker-transition-command-started" ]] ||
    self_test_fail "post-rename marker substitution gained command authority"
  root="$(sed -n '1p' "$report")"
  [[ -f "$root/$MARKER_NAME.transition-sentinel" ]] ||
    self_test_fail "post-rename marker substitution deleted retained evidence"
  grep -q 'could not bind command process group' "$test_root/marker-transition.err" ||
    self_test_fail "post-rename marker substitution lacked retention guidance"
  /bin/rm -rf -- "$root"

  delete_swap_hook="$fake_bin/swap-root-before-delete-rename"
  printf '%s\n' \
    '#!/bin/sh' \
    'set -eu' \
    'root="$1"' \
    'marker_name="$2"' \
    'old_root="$root.delete-original"' \
    '/bin/mv "$root" "$old_root"' \
    '/bin/mkdir -m 0700 "$root"' \
    '/bin/cp -p "$old_root/$marker_name" "$root/$marker_name"' \
    'printf "original\n" > "$old_root/delete-original-sentinel"' \
    'printf "substitute\n" > "$root/delete-substitute-sentinel"' \
    'printf "%s\n" "$old_root" > "$OLD_ROOT_REPORT"' > "$delete_swap_hook"
  chmod 0700 "$delete_swap_hook"
  quarantine_report="$test_root/delete-swap-quarantine.report"
  original_report="$test_root/delete-swap-original.report"
  set +e
  OLD_ROOT_REPORT="$original_report" \
    BLUEY_TEST_INJECT_BEFORE_DELETE_RENAME_HOOK="$delete_swap_hook" \
    BLUEY_TEST_DELETE_QUARANTINE_REPORT_FILE="$quarantine_report" \
    BLUEY_TEST_RUN_PARENT="$parent" "$launcher" -- true \
      2> "$test_root/delete-swap.err"
  status=$?
  set -e
  [[ "$status" -eq "$CLEANUP_FAILURE_STATUS" ]] ||
    self_test_fail "pre-delete root substitution did not fail closed"
  root="$(sed -n '1p' "$quarantine_report")"
  old_root="$(sed -n '1p' "$original_report")"
  [[ -f "$root/root/delete-substitute-sentinel" &&
    -f "$old_root/delete-original-sentinel" ]] ||
    self_test_fail "pre-delete root substitution removed original or substitute data"
  grep -q 'quarantined run-root identity changed' "$test_root/delete-swap.err" ||
    self_test_fail "pre-delete root substitution lacked quarantine guidance"
  /bin/rm -rf -- "$root" "$old_root"

  final_delete_hook="$fake_bin/swap-quarantined-root-before-final-delete"
  printf '%s\n' \
    '#!/bin/sh' \
    'set -eu' \
    'root="$1"' \
    'marker_name="$2"' \
    'old_root="$root.final-original"' \
    '/bin/mv "$root" "$old_root"' \
    '/bin/mkdir -m 0700 "$root"' \
    '/bin/cp -p "$old_root/$marker_name" "$root/$marker_name"' \
    'printf "original\n" > "$old_root/final-original-sentinel"' \
    'printf "substitute\n" > "$root/final-substitute-sentinel"' \
    'printf "%s\n" "$old_root" > "$FINAL_ORIGINAL_REPORT"' > "$final_delete_hook"
  chmod 0700 "$final_delete_hook"

  quarantine_report="$test_root/final-delete-current-quarantine.report"
  original_report="$test_root/final-delete-current-original.report"
  set +e
  FINAL_ORIGINAL_REPORT="$original_report" \
    BLUEY_TEST_INJECT_AFTER_QUARANTINE_VERIFY_HOOK="$final_delete_hook" \
    BLUEY_TEST_DELETE_QUARANTINE_REPORT_FILE="$quarantine_report" \
    BLUEY_TEST_RUN_PARENT="$parent" "$launcher" -- true \
      2> "$test_root/final-delete-current.err"
  status=$?
  set -e
  [[ "$status" -eq "$CLEANUP_FAILURE_STATUS" ]] ||
    self_test_fail "final-delete current substitution did not fail closed"
  root="$(sed -n '1p' "$quarantine_report")"
  old_root="$(sed -n '1p' "$original_report")"
  [[ -f "$root/root/final-substitute-sentinel" &&
    -f "$old_root/final-original-sentinel" ]] ||
    self_test_fail "final-delete current substitution removed retained evidence"
  grep -q 'final quarantined authority changed' "$test_root/final-delete-current.err" ||
    self_test_fail "final-delete current substitution lacked quarantine guidance"
  set +e
  COMMAND_STARTED_FILE="$test_root/final-delete-retained.started" \
    BLUEY_TEST_RUN_PARENT="$parent" "$launcher" -- bash -c \
      'printf "started\n" > "$COMMAND_STARTED_FILE"' \
      2> "$test_root/final-delete-retained.err"
  status=$?
  set -e
  [[ "$status" -eq "$STALE_RECOVERY_FAILURE_STATUS" &&
    ! -e "$test_root/final-delete-retained.started" ]] ||
    self_test_fail "retained quarantine did not block the next command"
  /bin/rm -rf -- "$root"

  root="$parent/run.1.99999991.FINALSTALE"
  mkdir -m 0700 "$root"
  write_self_test_marker "$root" "run.1.99999991.FINALSTALE" \
    99999991 0 1 starting
  age_self_test_run "$root" || self_test_fail "could not age final-delete stale fixture"
  quarantine_report="$test_root/final-delete-stale-quarantine.report"
  original_report="$test_root/final-delete-stale-original.report"
  set +e
  FINAL_ORIGINAL_REPORT="$original_report" \
    COMMAND_STARTED_FILE="$test_root/final-delete-stale.started" \
    BLUEY_TEST_INJECT_AFTER_QUARANTINE_VERIFY_HOOK="$final_delete_hook" \
    BLUEY_TEST_DELETE_QUARANTINE_REPORT_FILE="$quarantine_report" \
    BLUEY_TEST_RUN_PARENT="$parent" "$launcher" -- bash -c \
      'printf "started\n" > "$COMMAND_STARTED_FILE"' \
      2> "$test_root/final-delete-stale.err"
  status=$?
  set -e
  [[ "$status" -eq "$STALE_RECOVERY_FAILURE_STATUS" &&
    ! -e "$test_root/final-delete-stale.started" ]] ||
    self_test_fail "final-delete stale substitution did not block the next command"
  root="$(sed -n '1p' "$quarantine_report")"
  old_root="$(sed -n '1p' "$original_report")"
  [[ -f "$root/root/final-substitute-sentinel" &&
    -f "$old_root/final-original-sentinel" ]] ||
    self_test_fail "final-delete stale substitution removed retained evidence"
  grep -q 'final quarantined authority changed' "$test_root/final-delete-stale.err" ||
    self_test_fail "final-delete stale substitution lacked quarantine guidance"
  /bin/rm -rf -- "$root"

  local retained_case="" retained_error="" retained_started=""
  for retained_case in missing malformed; do
    root="$parent/run.1.9999999${#retained_case}.RETAINED"
    mkdir -m 0700 "$root"
    if [[ "$retained_case" == "malformed" ]]; then
      printf 'corrupt\n' > "$root/$MARKER_NAME"
    fi
    retained_error="$test_root/retained-$retained_case.err"
    retained_started="$test_root/retained-$retained_case.started"
    set +e
    COMMAND_STARTED_FILE="$retained_started" BLUEY_TEST_RUN_PARENT="$parent" \
      "$launcher" -- bash -c 'printf "started\n" > "$COMMAND_STARTED_FILE"' \
      2> "$retained_error"
    status=$?
    set -e
    [[ "$status" -eq "$STALE_RECOVERY_FAILURE_STATUS" ]] ||
      self_test_fail "$retained_case retained marker did not block a new command"
    [[ -d "$root" && ! -e "$retained_started" ]] ||
      self_test_fail "$retained_case retained marker started a command or lost evidence"
    grep -q 'stale-run recovery blocked by missing or malformed owned metadata' \
      "$retained_error" ||
      self_test_fail "$retained_case retained marker lacked bounded operator guidance"
    /bin/rm -rf -- "$root"
  done

  local symlink_target="$test_root/symlink-target" symlink_run="" marker_link_root=""
  mkdir -m 0700 "$symlink_target"
  printf 'keep\n' > "$symlink_target/foreign"
  symlink_run="$parent/run.1.99999995.SYMLINK"
  ln -s "$symlink_target" "$symlink_run"
  BLUEY_TEST_RUN_PARENT="$parent" "$launcher" -- true ||
    self_test_fail "candidate symlink safety failed"
  [[ -L "$symlink_run" && -f "$symlink_target/foreign" ]] ||
    self_test_fail "reaper followed a candidate symlink"

  marker_link_root="$parent/run.1.99999994.MARKLINK"
  mkdir -m 0700 "$marker_link_root"
  ln -s "$symlink_target/foreign" "$marker_link_root/$MARKER_NAME"
  set +e
  COMMAND_STARTED_FILE="$test_root/marker-link.started" BLUEY_TEST_RUN_PARENT="$parent" \
    "$launcher" -- bash -c 'printf "started\n" > "$COMMAND_STARTED_FILE"' \
    2> "$test_root/marker-link.err"
  status=$?
  set -e
  [[ "$status" -eq "$STALE_RECOVERY_FAILURE_STATUS" &&
    ! -e "$test_root/marker-link.started" ]] ||
    self_test_fail "linked retained marker did not block a new command"
  [[ -L "$marker_link_root/$MARKER_NAME" && -f "$symlink_target/foreign" ]] ||
    self_test_fail "reaper followed a retained marker symlink"
  /bin/rm -rf -- "$symlink_run" "$marker_link_root" "$symlink_target"

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

  local exact_git_kind="" exact_git_started=""
  for exact_git_kind in directory file symlink; do
    root="$test_root/bluey-test-runs-exact-git-$exact_git_kind"
    mkdir -m 0700 "$root"
    if [[ "$exact_git_kind" == "directory" ]]; then
      mkdir -m 0700 "$root/.git"
    elif [[ "$exact_git_kind" == "file" ]]; then
      printf 'gitdir: /private/nonexistent/bluey-self-test\n' > "$root/.git"
    else
      ln -s /private/nonexistent/bluey-self-test "$root/.git"
    fi
    exact_git_started="$test_root/exact-git-$exact_git_kind.started"
    set +e
    COMMAND_STARTED_FILE="$exact_git_started" BLUEY_TEST_RUN_PARENT="$root" \
      "$launcher" -- bash -c 'printf "started\n" > "$COMMAND_STARTED_FILE"' \
      > /dev/null 2>&1
    status=$?
    set -e
    [[ "$status" -ne 0 && ! -e "$exact_git_started" &&
      ! -e "$root/run.invalid" ]] ||
      self_test_fail "existing exact Git $exact_git_kind parent was accepted"
    if compgen -G "$root/run.*" >/dev/null; then
      self_test_fail "existing exact Git $exact_git_kind parent was mutated"
    fi
    /bin/rm -rf -- "$root"
  done

  printf '%s\n' '#!/bin/sh' 'exit 91' > "$fake_bin/git"
  chmod 0700 "$fake_bin/git"
  exact_git_started="$test_root/worktree-list-failure.started"
  set +e
  COMMAND_STARTED_FILE="$exact_git_started" PATH="$fake_bin:$PATH" \
    BLUEY_TEST_RUN_PARENT="$parent" "$launcher" -- bash -c \
      'printf "started\n" > "$COMMAND_STARTED_FILE"' > /dev/null 2>&1
  status=$?
  set -e
  [[ "$status" -ne 0 && ! -e "$exact_git_started" ]] ||
    self_test_fail "worktree enumeration failure did not block the command"
  if compgen -G "$parent/run.*" >/dev/null; then
    self_test_fail "worktree enumeration failure created a test root"
  fi

  printf '%s\n' \
    '#!/bin/sh' \
    '/usr/bin/yes x | /usr/bin/head -c 1048577' \
    'exit 0' > "$fake_bin/git"
  chmod 0700 "$fake_bin/git"
  exact_git_started="$test_root/worktree-list-oversize.started"
  set +e
  COMMAND_STARTED_FILE="$exact_git_started" PATH="$fake_bin:$PATH" \
    BLUEY_TEST_RUN_PARENT="$parent" "$launcher" -- bash -c \
      'printf "started\n" > "$COMMAND_STARTED_FILE"' > /dev/null 2>&1
  status=$?
  set -e
  [[ "$status" -ne 0 && ! -e "$exact_git_started" ]] ||
    self_test_fail "oversized worktree enumeration did not block the command"
  if compgen -G "$parent/run.*" >/dev/null; then
    self_test_fail "oversized worktree enumeration created a test root"
  fi
  /bin/rm -f -- "$fake_bin/git"

  quarantine_report="$test_root/cleanup-pass-quarantine.report"
  set +e
  BLUEY_TEST_INJECT_QUARANTINE_DELETE_FAILURE=1 \
    BLUEY_TEST_DELETE_QUARANTINE_REPORT_FILE="$quarantine_report" \
    BLUEY_TEST_RUN_PARENT="$parent" \
    "$launcher" -- true 2> "$test_root/cleanup-pass.err"
  status=$?
  set -e
  [[ "$status" -eq "$CLEANUP_FAILURE_STATUS" ]] ||
    self_test_fail "cleanup failure did not fail a successful command"
  grep -q 'command passed but cleanup failed' "$test_root/cleanup-pass.err" ||
    self_test_fail "successful cleanup failure was not explicit"
  root="$(sed -n '1p' "$quarantine_report")"
  [[ -d "$root/root" ]] || self_test_fail "cleanup failure did not retain evidence"
  /bin/rm -rf -- "$root"

  quarantine_report="$test_root/cleanup-fail-quarantine.report"
  set +e
  BLUEY_TEST_INJECT_QUARANTINE_DELETE_FAILURE=1 \
    BLUEY_TEST_DELETE_QUARANTINE_REPORT_FILE="$quarantine_report" \
    BLUEY_TEST_RUN_PARENT="$parent" \
    "$launcher" -- bash -c 'exit 39' 2> "$test_root/cleanup-fail.err"
  status=$?
  set -e
  [[ "$status" -eq 39 ]] || self_test_fail "cleanup failure hid child status"
  grep -q 'preserving command status 39' "$test_root/cleanup-fail.err" ||
    self_test_fail "nonzero cleanup failure was not explicit"
  root="$(sed -n '1p' "$quarantine_report")"
  [[ -d "$root/root" ]] || self_test_fail "nonzero cleanup failure lost evidence"
  /bin/rm -rf -- "$root"

  quarantine_report="$test_root/cleanup-signal-quarantine.report"
  set +e
  BLUEY_TEST_INJECT_QUARANTINE_DELETE_FAILURE=1 \
    BLUEY_TEST_DELETE_QUARANTINE_REPORT_FILE="$quarantine_report" \
    BLUEY_TEST_RUN_PARENT="$parent" \
    BLUEY_TEST_INJECT_AFTER_RUN_ROOT_MKDIR=TERM \
    "$launcher" -- true 2> "$test_root/cleanup-signal.err"
  status=$?
  set -e
  [[ "$status" -eq 143 ]] || self_test_fail "cleanup failure hid signal status"
  grep -q 'preserving command status 143' "$test_root/cleanup-signal.err" ||
    self_test_fail "signal cleanup failure was not explicit"
  root="$(sed -n '1p' "$quarantine_report")"
  [[ -d "$root/root" ]] || self_test_fail "signal cleanup failure lost evidence"
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
  printf 'Bluey test launcher self-tests passed (canonical modes, repository CWD, poisoned env, process groups, running and pre-gate signal receipt, zombie-safe reap, canonical stale and current root/marker identity, retained-run admission, exact Git/worktree rejection, cleanup failure, post-mkdir signals, and residue).\n'
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
