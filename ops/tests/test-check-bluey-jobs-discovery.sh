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
for argument in "$@"; do
    [[ "$argument" != *postgres://* ]] || exit 9
done
[ "${PGDATABASE:-}" = "postgres://example.invalid/bluey" ] || exit 9
[ -z "${BLUEY_JOBS_DIAGNOSTIC_KEY:-}" ] || exit 9
[ -z "${BLUEY_JOBS_DATA_KEY:-}" ] || exit 9
[ -z "${BLUEY_OPENAI_API_KEY:-}" ] || exit 9
query="$(cat)"
! grep -q 'BLUEY_JOBS_DATA_KEY' <<< "$query" || exit 9
! grep -q 'DIAGNOSTIC_KEY' <<< "$query" || exit 9
grep -q "encode(" <<< "$query" || exit 9
grep -q "convert_to(" <<< "$query" || exit 9
grep -q "jsonb_build_array(kind, provider, source_key)" <<< "$query" || exit 9
grep -q "'hex'" <<< "$query" || exit 9
overdue_file="$(cd "$(dirname "$0")/.." && pwd)/overdue.hex"
[ ! -f "$overdue_file" ] || cat "$overdue_file"
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
    BLUEY_JOBS_DIAGNOSTIC_KEY="$(printf 'ab%.0s' {1..32})" \
    BLUEY_JOBS_DATA_KEY="data-key-must-not-reach-child" \
    BLUEY_OPENAI_API_KEY="provider-key-must-not-reach-child" \
        "$ROOT/ops/check-bluey-jobs-discovery.sh"
}

run_check >/dev/null

overdue_output="$TEST_ROOT/overdue-output"
printf '%s' '5b22646972656374222c20226c65766572222c202261636d65225d' \
    > "$TEST_ROOT/overdue.hex"
if run_check >"$overdue_output" 2>&1; then
    echo "expected overdue discovery source to fail" >&2
    exit 1
fi
rm "$TEST_ROOT/overdue.hex"
grep -Eq 'direct:lever:ref-[0-9a-f]{64}' "$overdue_output"
if grep -q 'acme' "$overdue_output"; then
    echo "raw discovery source escaped the diagnostic boundary" >&2
    exit 1
fi

invalid_dimension_hex="$(python3 -c \
    'print("[\"direct\", \"evil\\ninjected\", \"acme\"]".encode().hex())')"
invalid_dimension_output="$TEST_ROOT/invalid-dimension-output"
printf '%s' "$invalid_dimension_hex" > "$TEST_ROOT/overdue.hex"
if run_check >"$invalid_dimension_output" 2>&1; then
    echo "expected an unknown diagnostic dimension to fail" >&2
    exit 1
fi
rm "$TEST_ROOT/overdue.hex"
grep -q 'invalid discovery diagnostic dimension' "$invalid_dimension_output"
if grep -Eq 'evil|injected|acme' "$invalid_dimension_output"; then
    echo "untrusted discovery dimension escaped the diagnostic boundary" >&2
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
