#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TEST_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/bluey-jobs-worker-health-test.XXXXXX")"

cleanup() {
    rm -rf "$TEST_ROOT"
}
trap cleanup EXIT

mkdir -p "$TEST_ROOT/release/jobs" "$TEST_ROOT/bin" "$TEST_ROOT/direct" "$TEST_ROOT/global"
ln -s "$TEST_ROOT/release" "$TEST_ROOT/direct/current"
ln -s "$TEST_ROOT/release" "$TEST_ROOT/global/current"

cat > "$TEST_ROOT/bin/systemctl" <<'SH'
#!/usr/bin/env bash
if [ "${BLUEY_TEST_SYSTEMD_ACTIVE:-1}" != "1" ]; then
    exit 3
fi
exit 0
SH

cat > "$TEST_ROOT/bin/psql" <<'SH'
#!/usr/bin/env bash
printf '%s' "${BLUEY_TEST_OVERDUE_SOURCES:-}"
SH

chmod +x "$TEST_ROOT/bin/systemctl" "$TEST_ROOT/bin/psql"
printf 'BLUEY_DATABASE_URL=postgres://example.invalid/bluey\n' > "$TEST_ROOT/postgres.env"

run_check() {
    BLUEY_JOBS_SYSTEMCTL_BIN="$TEST_ROOT/bin/systemctl" \
    BLUEY_JOBS_PSQL_BIN="$TEST_ROOT/bin/psql" \
    BLUEY_JOBS_DISCOVERY_LINK_ROOT="$TEST_ROOT/direct" \
    BLUEY_JOBS_GLOBAL_DISCOVERY_LINK_ROOT="$TEST_ROOT/global" \
    BLUEY_JOBS_API_ENV_FILE="$TEST_ROOT/missing.env" \
    BLUEY_JOBS_POSTGRES_ENV_FILE="$TEST_ROOT/postgres.env" \
        "$ROOT/ops/check-bluey-jobs-discovery.sh"
}

run_check >/dev/null

if BLUEY_TEST_OVERDUE_SOURCES="direct:lever:acme" run_check >/dev/null 2>&1; then
    echo "expected overdue discovery source to fail" >&2
    exit 1
fi

if BLUEY_TEST_SYSTEMD_ACTIVE=0 run_check >/dev/null 2>&1; then
    echo "expected inactive worker to fail" >&2
    exit 1
fi

mkdir -p "$TEST_ROOT/other-release/jobs"
ln -sfn "$TEST_ROOT/other-release" "$TEST_ROOT/global/current"
if run_check >/dev/null 2>&1; then
    echo "expected mismatched worker releases to fail" >&2
    exit 1
fi
ln -sfn "$TEST_ROOT/release" "$TEST_ROOT/global/current"

BLUEY_JOBS_DISCOVERY_SKIP_SOURCE_FRESHNESS=1 run_check >/dev/null

echo "test-check-bluey-jobs-discovery: PASS"
