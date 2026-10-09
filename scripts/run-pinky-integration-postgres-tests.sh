#!/usr/bin/env bash
# Local-only PostgreSQL parity gate for the Pinky integration store.
set -euo pipefail
set -m
umask 077

readonly script=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")
readonly repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
readonly scheduler=/Users/uno/Projects/shared-runners/runner_schedule.py
readonly pg_bin=/opt/homebrew/opt/postgresql@17/bin
readonly pg_port=55439
readonly pg_user=bluey_pinky_test
readonly pg_database=bluey_pinky_test
readonly test_name=pinky_integration::store::tests::\
postgres_lifecycle_claim_and_cancel_admit_use_real_store_transactions

cleanup_root() {
  local root_path=$1
  case "$root_path" in
    */bluey-pinky-postgres-tests.*)
      if [[ -d "$root_path" && ! -L "$root_path" ]]; then
        find "$root_path" -depth -delete
      fi
      ;;
    *)
      printf 'Unexpected temporary root; cleanup held: %s\n' "$root_path" >&2
      return 1
      ;;
  esac
  [[ ! -e "$root_path" ]]
}

run_probe() {
  local probe=${1:-}
  local probe_root child=""
  probe_root=$(mktemp -d "/tmp/bluey-pinky-postgres-tests.XXXXXX")
  chmod 700 "$probe_root"
  cleanup_probe() {
    local status=$?
    trap - EXIT HUP INT TERM
    if [[ -n "$child" ]] && kill -0 -- "-$child" 2>/dev/null; then
      kill -TERM -- "-$child" 2>/dev/null || true
      wait "$child" 2>/dev/null || true
    fi
    cleanup_root "$probe_root" || exit 1
    exit "$status"
  }
  trap cleanup_probe EXIT
  trap 'exit 129' HUP
  trap 'exit 130' INT
  trap 'exit 143' TERM
  printf '%s\n' "$probe_root"
  case "$probe" in
    success) exit 0 ;;
    failure) exit 7 ;;
    signal)
      env -i PATH="$PATH" bash -c 'sleep 30' &
      child=$!
      kill -TERM "$$"
      wait "$child"
      exit 1
      ;;
    *) exit 2 ;;
  esac
}

run_self_test() {
  local probe status result
  for probe in success failure signal; do
    status=0
    result=$(bash "$script" --probe "$probe") || status=$?
    case "$probe:$status" in
      success:0|failure:7|signal:143) ;;
      *) printf 'Cleanup self-test status failed for %s: %s\n' "$probe" "$status" >&2; exit 1 ;;
    esac
    case "$result" in
      */bluey-pinky-postgres-tests.*) ;;
      *) printf 'Cleanup self-test returned an unexpected root\n' >&2; exit 1 ;;
    esac
    if [[ -e "$result" ]]; then
      printf 'Cleanup self-test residue remains: %s\n' "$result" >&2
      exit 1
    fi
  done
  printf 'PostgreSQL harness cleanup self-test passed (success, failure, TERM)\n'
}

if [[ "${1:-}" == "--probe" ]]; then
  run_probe "${2:-}"
fi
if [[ "${1:-}" == "--self-test" ]]; then
  [[ $# -eq 1 ]] || { printf 'Usage: %s [--self-test]\n' "$0" >&2; exit 2; }
  run_self_test
  exit 0
fi
[[ $# -eq 0 ]] || { printf 'Usage: %s [--self-test]\n' "$0" >&2; exit 2; }

[[ -f "$scheduler" && ! -L "$scheduler" ]] || {
  printf 'Shared runner scheduler is unavailable or is a symlink\n' >&2
  exit 1
}
if [[ "${BLUEY_PINKY_MAC_HEAVY_LOCKED:-0}" != "1" ]]; then
  exec python3 "$scheduler" with-machine-lock --machine mac-heavy -- \
    /usr/bin/env BLUEY_PINKY_MAC_HEAVY_LOCKED=1 "$script"
fi

for tool in initdb pg_ctl postgres psql createdb; do
  [[ -x "$pg_bin/$tool" ]] || { printf 'PostgreSQL 17 tool missing: %s\n' "$tool" >&2; exit 1; }
done
[[ "$($pg_bin/postgres --version)" == "postgres (PostgreSQL) 17.10 (Homebrew)" ]] || {
  printf 'The PostgreSQL harness requires the reviewed Homebrew 17.10 toolchain\n' >&2
  exit 1
}
[[ -f /opt/homebrew/share/postgresql@17/extension/vector.control ]] || {
  printf 'PostgreSQL 17 pgvector control file is missing\n' >&2
  exit 1
}
[[ -e /opt/homebrew/lib/postgresql@17/vector.dylib ]] || {
  printf 'PostgreSQL 17 pgvector library is missing\n' >&2
  exit 1
}
for migration in 001_server_runtime_compat.sql 002_usage_reservations.sql; do
  [[ -f "$repo/infra/postgres/server-runtime/$migration" ]] || {
    printf 'Required PostgreSQL migration is missing: %s\n' "$migration" >&2
    exit 1
  }
done

root=$(mktemp -d "/tmp/bluey-pinky-postgres-tests.XXXXXX")
child=""
postgres_started=0
cleanup() {
  local status=$?
  local cleanup_failed=0
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
  if [[ "$postgres_started" == "1" || -f "$root/pgdata/postmaster.pid" ]]; then
    if ! "$pg_bin/pg_ctl" -D "$root/pgdata" stop -m fast -w -t 15 >/dev/null 2>&1; then
      if ! "$pg_bin/pg_ctl" -D "$root/pgdata" stop -m immediate -w -t 15 \
        >/dev/null 2>&1; then
        cleanup_failed=1
      fi
    fi
  fi
  if [[ "$status" -ne 0 && -f "$root/logs/postgres.log" ]]; then
    tail -n 80 "$root/logs/postgres.log" >&2 || true
  fi
  if [[ "$cleanup_failed" == "1" ]]; then
    printf 'PostgreSQL did not stop; owned root retained for safe inspection: %s\n' \
      "$root" >&2
    exit 1
  fi
  cleanup_root "$root" || exit 1
  exit "$status"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
mkdir -p "$root"/{pgdata,socket,cargo,tmp,data,config,runtime,logs,db,home}
chmod 700 "$root" "$root/pgdata" "$root/socket" "$root/runtime" "$root/home"

pg_environment=(env -i PATH="$pg_bin:/usr/bin:/bin" HOME="$root/home" LC_ALL=C)
"${pg_environment[@]}" "$pg_bin/initdb" -D "$root/pgdata" --no-locale \
  --encoding=UTF8 --auth-local=trust --auth-host=reject --username="$pg_user" \
  --no-instructions >/dev/null
"${pg_environment[@]}" "$pg_bin/pg_ctl" -D "$root/pgdata" \
  -l "$root/logs/postgres.log" -w -t 30 start \
  -o "-c listen_addresses='' -c unix_socket_directories='$root/socket' -c unix_socket_permissions=0700 -c port=$pg_port"
postgres_started=1

connection_environment=(
  env -i PATH="$pg_bin:/usr/bin:/bin" HOME="$root/home" LC_ALL=C
  PGHOST="$root/socket" PGPORT="$pg_port" PGUSER="$pg_user"
)
"${connection_environment[@]}" "$pg_bin/createdb" "$pg_database"
"${connection_environment[@]}" "$pg_bin/psql" -X -v ON_ERROR_STOP=1 -1 \
  -d "$pg_database" \
  -f "$repo/infra/postgres/server-runtime/001_server_runtime_compat.sql" \
  -f "$repo/infra/postgres/server-runtime/002_usage_reservations.sql" >/dev/null

database_url="host=$root/socket port=$pg_port user=$pg_user dbname=$pg_database sslmode=disable connect_timeout=5"
printf 'Owned PostgreSQL validation root: %s\n' "$root"
run_cargo() {
  env -i PATH="$PATH" HOME="$HOME" \
    CARGO_TARGET_DIR="$root/cargo" TMPDIR="$root/tmp" \
    XDG_DATA_HOME="$root/data" XDG_CONFIG_HOME="$root/config" \
    XDG_RUNTIME_DIR="$root/runtime" BLUEY_DATA_DIR="$root/data" \
    BLUEY_DB_PATH="$root/db/test.db" BLUEY_LOG_DIR="$root/logs" \
    "$@" &
  child=$!
  wait "$child"
  child=""
}

printf 'Running exact PostgreSQL Pinky store transaction test...\n'
run_cargo BLUEY_PINKY_TEST_POSTGRES_EPHEMERAL=1 \
  BLUEY_TEST_POSTGRES_URL="$database_url" RUST_TEST_THREADS=1 \
  cargo test --offline --locked --manifest-path "$repo/server/Cargo.toml" \
  --lib "$test_name" -j 2 -- --exact --test-threads=1

# The broader source gates deliberately omit the ephemeral marker and URL. The
# PostgreSQL test therefore skips instead of running twice, and no inherited
# database or provider configuration reaches Cargo or the test binaries.
printf 'Running focused Pinky integration tests...\n'
run_cargo RUST_TEST_THREADS=2 \
  cargo test --offline --locked --manifest-path "$repo/server/Cargo.toml" \
  --lib 'pinky_integration::' -j 2

printf 'Running all-target tests...\n'
run_cargo RUST_TEST_THREADS=2 \
  cargo test --offline --locked --manifest-path "$repo/server/Cargo.toml" \
  --all-targets -j 2

printf 'Running strict all-target Clippy...\n'
run_cargo cargo clippy --offline --locked --manifest-path "$repo/server/Cargo.toml" \
  --all-targets -j 2 -- -D warnings
