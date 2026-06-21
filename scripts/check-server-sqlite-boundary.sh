#!/usr/bin/env bash
set -euo pipefail

# Report bluey-server runtime code that still depends directly on rusqlite.
#
# This is intentionally informational by default because SQLite remains the
# default/local server backend until production cutover. Use
# BLUEY_SQLITE_BOUNDARY_STRICT=1 only when a branch is meant to remove expected
# direct SQLite-only runtime files outside server/src/db.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STRICT="${BLUEY_SQLITE_BOUNDARY_STRICT:-0}"

cd "$ROOT"

if ! command -v rg >/dev/null 2>&1; then
  echo "fatal: rg is required" >&2
  exit 2
fi

patterns='rusqlite|r2d2_sqlite|SqliteConnectionManager|rusqlite::params|params!|query_row|execute_batch|prepare\('

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT

rg -n "$patterns" server/src server/tests \
  -g '*.rs' \
  -g '!server/src/db/**' \
  -g '!server/tests/**' \
  | grep -Ev 'server/src/main.rs:|server/src/config.rs:' >"$tmp" || true

echo "Bluey SQLite boundary check"
echo "allowed: server/src/db/**, server/src/main.rs startup guard, server/src/config.rs backend selector, tests"

count="$(wc -l <"$tmp" | tr -d ' ')"
if [ "$count" = "0" ]; then
  echo "ok: no direct SQLite usage outside the DB layer"
  exit 0
fi

echo "found $count runtime SQLite-bound line(s) outside server/src/db:"
cat "$tmp"

if [ "$STRICT" = "1" ]; then
  echo "fatal: BLUEY_SQLITE_BOUNDARY_STRICT=1 rejects direct SQLite usage outside server/src/db" >&2
  exit 1
fi

echo "warn: current runtime is SQLite-backed; keep this list shrinking during the Postgres adapter work"
