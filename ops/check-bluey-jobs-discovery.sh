#!/usr/bin/env bash
# Fail when either Jobs discovery worker is unavailable or active sources have
# not completed a successful sync within the freshness window.

set -euo pipefail

SYSTEMCTL_BIN="${BLUEY_JOBS_SYSTEMCTL_BIN:-systemctl}"
PSQL_BIN="${BLUEY_JOBS_PSQL_BIN:-psql}"
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

set -a
[ ! -f "$API_ENV" ] || . "$API_ENV"
[ ! -f "$POSTGRES_ENV" ] || . "$POSTGRES_ENV"
set +a

[ -n "${BLUEY_DATABASE_URL:-}" ] || fail "BLUEY_DATABASE_URL is unavailable"
command -v "$PSQL_BIN" >/dev/null 2>&1 || fail "psql is required"

NOW_MS="$(( $(date +%s) * 1000 ))"
CUTOFF_MS="$(( NOW_MS - STALE_AFTER_MS ))"
OVERDUE="$(
    "$PSQL_BIN" "$BLUEY_DATABASE_URL" -AtX \
        -v ON_ERROR_STOP=1 \
        -v cutoff_ms="$CUTOFF_MS" <<'SQL'
SELECT concat(kind, ':', provider, ':', source_key)
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
)"

if [ -n "$OVERDUE" ]; then
    echo "$OVERDUE" | sed 's/^/bluey-jobs-discovery-health: overdue source: /' >&2
    exit 1
fi

echo "bluey-jobs-discovery-health: workers active and sources current"
