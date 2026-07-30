#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TEST_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/bluey-jobs-worker-archive-test.XXXXXX")"
RELEASE_ID="jobs-workers-test"

cleanup() {
    rm -rf "$TEST_ROOT"
}
trap cleanup EXIT

make_required_files() {
    local root="$1/$RELEASE_ID"
    mkdir -p "$root/jobs/workflows/dist"
    printf '{}\n' > "$root/manifest.json"
    printf 'export {};\n' > "$root/jobs/workflows/dist/discovery-worker.js"
    printf 'export {};\n' > "$root/jobs/workflows/dist/global-discovery-worker.js"
}

make_required_files "$TEST_ROOT/good"
COPYFILE_DISABLE=1 tar -C "$TEST_ROOT/good" -czf "$TEST_ROOT/good.tar.gz" "$RELEASE_ID"
"$ROOT/ops/verify-bluey-jobs-workers-archive.py" \
    "$TEST_ROOT/good.tar.gz" "$RELEASE_ID"

make_required_files "$TEST_ROOT/appledouble"
printf 'metadata\n' > "$TEST_ROOT/appledouble/._$RELEASE_ID"
tar -C "$TEST_ROOT/appledouble" -czf "$TEST_ROOT/appledouble.tar.gz" \
    "$RELEASE_ID" "._$RELEASE_ID"
if "$ROOT/ops/verify-bluey-jobs-workers-archive.py" \
    "$TEST_ROOT/appledouble.tar.gz" "$RELEASE_ID" >/dev/null 2>&1; then
    echo "expected AppleDouble archive rejection" >&2
    exit 1
fi

make_required_files "$TEST_ROOT/multiple"
mkdir -p "$TEST_ROOT/multiple/other-root"
tar -C "$TEST_ROOT/multiple" -czf "$TEST_ROOT/multiple.tar.gz" \
    "$RELEASE_ID" other-root
if "$ROOT/ops/verify-bluey-jobs-workers-archive.py" \
    "$TEST_ROOT/multiple.tar.gz" "$RELEASE_ID" >/dev/null 2>&1; then
    echo "expected multiple-root archive rejection" >&2
    exit 1
fi

echo "test-verify-bluey-jobs-workers-archive: PASS"
