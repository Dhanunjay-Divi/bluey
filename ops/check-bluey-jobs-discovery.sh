#!/usr/bin/env bash
# Fail when either Jobs discovery worker is unavailable or active sources have
# not completed a successful sync within the freshness window.

set -euo pipefail

SYSTEMCTL_BIN="${BLUEY_JOBS_SYSTEMCTL_BIN:-systemctl}"
PSQL_BIN="${BLUEY_JOBS_PSQL_BIN:-psql}"
PYTHON_BIN="${BLUEY_JOBS_PYTHON_BIN:-python3}"
DIRECT_SERVICE="${BLUEY_JOBS_DISCOVERY_SERVICE:-bluey-jobs-discovery.service}"
GLOBAL_SERVICE="${BLUEY_JOBS_GLOBAL_DISCOVERY_SERVICE:-bluey-jobs-global-discovery.service}"
DIRECT_LINK="${BLUEY_JOBS_DISCOVERY_LINK_ROOT:-/opt/bluey-jobs-discovery}/current"
GLOBAL_LINK="${BLUEY_JOBS_GLOBAL_DISCOVERY_LINK_ROOT:-/opt/bluey-jobs-global-discovery}/current"
API_ENV="${BLUEY_JOBS_API_ENV_FILE:-/etc/bluey-api/bluey-api.env}"
POSTGRES_ENV="${BLUEY_JOBS_POSTGRES_ENV_FILE:-/etc/bluey-api/bluey-postgres.env}"
STALE_AFTER_MS="${BLUEY_JOBS_DISCOVERY_STALE_AFTER_MS:-43200000}"

fail() {
    echo "bluey-jobs-discovery-health: $*" >&2
    exit 1
}

[[ "$STALE_AFTER_MS" =~ ^[0-9]+$ ]] || fail "stale threshold must be an integer"

for link in "$DIRECT_LINK" "$GLOBAL_LINK"; do
    [ -L "$link" ] || fail "missing release link: $link"
    target="$(readlink "$link")"
    [ -d "$target/jobs" ] || fail "release target is unavailable: $target"
done

DIRECT_TARGET="$(readlink "$DIRECT_LINK")"
GLOBAL_TARGET="$(readlink "$GLOBAL_LINK")"
[ "$DIRECT_TARGET" = "$GLOBAL_TARGET" ] ||
    fail "direct and global workers reference different releases"

"$SYSTEMCTL_BIN" is-active --quiet "$DIRECT_SERVICE" ||
    fail "$DIRECT_SERVICE is not active"
"$SYSTEMCTL_BIN" is-active --quiet "$GLOBAL_SERVICE" ||
    fail "$GLOBAL_SERVICE is not active"

if [ "${BLUEY_JOBS_DISCOVERY_SKIP_SOURCE_FRESHNESS:-0}" = "1" ]; then
    echo "bluey-jobs-discovery-health: workers active"
    exit 0
fi

set +a
[ ! -f "$API_ENV" ] || . "$API_ENV"
[ ! -f "$POSTGRES_ENV" ] || . "$POSTGRES_ENV"

[ -n "${BLUEY_DATABASE_URL:-}" ] || fail "BLUEY_DATABASE_URL is unavailable"
[ -n "${BLUEY_JOBS_DIAGNOSTIC_KEY:-}" ] ||
    fail "BLUEY_JOBS_DIAGNOSTIC_KEY is unavailable"
PSQL_PATH="$(command -v "$PSQL_BIN")" || fail "psql is required"
PYTHON_PATH="$(command -v "$PYTHON_BIN")" || fail "python3 is required"

run_psql_sanitized() (
    exec -c "$PYTHON_PATH" -c '
import os
import sys

database_url = os.fdopen(3, encoding="utf-8").read().strip()
if not database_url:
    raise SystemExit("database connection is unavailable")
psql_path, child_path, child_home, *arguments = sys.argv[1:]
environment = {
    "HOME": child_home,
    "LC_ALL": "C",
    "PATH": child_path,
    "PGAPPNAME": "bluey-jobs-discovery-health",
    "PGDATABASE": database_url,
}
os.execve(psql_path, [psql_path, *arguments], environment)
' "$PSQL_PATH" "${PATH:-/usr/bin:/bin}" "${HOME:-/}" "$@" \
        3<<<"$BLUEY_DATABASE_URL"
)

render_diagnostic_refs_sanitized() (
    exec -c "$PYTHON_PATH" -c '
import hashlib
import hmac
import json
import os
import sys

for forbidden in (
    "BLUEY_DATABASE_URL",
    "BLUEY_JOBS_DATA_KEY",
    "BLUEY_JOBS_DIAGNOSTIC_KEY",
    "BLUEY_OPENAI_API_KEY",
):
    if forbidden in os.environ:
        raise SystemExit("diagnostic child environment is not sanitized")

raw_key = os.fdopen(3, encoding="utf-8").read().strip()
if len(raw_key) != 64 or any(character not in "0123456789abcdefABCDEF" for character in raw_key):
    raise SystemExit("BLUEY_JOBS_DIAGNOSTIC_KEY must be a 32-byte hex key")
key = bytes.fromhex(raw_key)
domain = b"bluey-jobs-discovery-source:v1\0"
allowed = {
    "direct": {
        "ashby",
        "curated_feed",
        "greenhouse",
        "lever",
        "smartrecruiters",
        "workday",
    },
    "global": {"jobhive"},
}
for raw_line in sys.stdin:
    encoded = raw_line.strip()
    if not encoded:
        continue
    try:
        payload = bytes.fromhex(encoded)
        values = json.loads(payload)
    except (UnicodeDecodeError, ValueError, json.JSONDecodeError):
        raise SystemExit("invalid discovery diagnostic row") from None
    if not isinstance(values, list) or len(values) != 3 or not all(
        isinstance(value, str) for value in values
    ):
        raise SystemExit("invalid discovery diagnostic row")
    kind, provider, _source_key = values
    if provider not in allowed.get(kind, set()):
        raise SystemExit("invalid discovery diagnostic dimension")
    digest = hmac.new(key, domain + payload, hashlib.sha256).hexdigest()
    print(f"{kind}:{provider}:ref-{digest}")
' 3<<<"$BLUEY_JOBS_DIAGNOSTIC_KEY"
)

NOW_MS="$(( $(date +%s) * 1000 ))"
CUTOFF_MS="$(( NOW_MS - STALE_AFTER_MS ))"
OVERDUE="$(
    run_psql_sanitized -AtX \
        -v ON_ERROR_STOP=1 \
        -v cutoff_ms="$CUTOFF_MS" <<'SQL' |
SELECT encode(
    convert_to(jsonb_build_array(kind, provider, source_key)::text, 'UTF8'),
    'hex'
)
FROM (
    SELECT
        'direct' AS kind,
        provider,
        source_key,
        last_success_at_ms
    FROM jobs_discovery_sources
    WHERE status = 'active'
      AND (last_success_at_ms IS NULL OR last_success_at_ms < :'cutoff_ms'::bigint)
    UNION ALL
    SELECT
        'global' AS kind,
        provider,
        source_key,
        last_success_at_ms
    FROM jobs_global_discovery_sources
    WHERE status = 'active'
      AND (last_success_at_ms IS NULL OR last_success_at_ms < :'cutoff_ms'::bigint)
) overdue
ORDER BY kind, provider, source_key;
SQL
    render_diagnostic_refs_sanitized
)"

if [ -n "$OVERDUE" ]; then
    echo "$OVERDUE" | sed 's/^/bluey-jobs-discovery-health: overdue source: /' >&2
    exit 1
fi

echo "bluey-jobs-discovery-health: workers active and sources current"
