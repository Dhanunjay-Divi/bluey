#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TEST_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/bluey-jobs-worker-install-test.XXXXXX")"

cleanup() {
    rm -rf "$TEST_ROOT"
}
trap cleanup EXIT

sha256_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        shasum -a 256 "$1" | awk '{print $1}'
    fi
}

make_archive() {
    local release_id="$1"
    local source_root="$TEST_ROOT/source-$release_id"
    local archive="$TEST_ROOT/$release_id.tar.gz"
    mkdir -p "$source_root/$release_id/jobs/workflows/dist"
    printf '{"release_id":"%s"}\n' "$release_id" > "$source_root/$release_id/manifest.json"
    printf 'export {};\n' > "$source_root/$release_id/jobs/workflows/dist/discovery-worker.js"
    printf 'export {};\n' > "$source_root/$release_id/jobs/workflows/dist/global-discovery-worker.js"
    tar -C "$source_root" -czf "$archive" "$release_id"
    (
        cd "$TEST_ROOT"
        archive_name="$(basename "$archive")"
        printf '%s  %s\n' "$(sha256_file "$archive_name")" "$archive_name" \
            > "$archive_name.sha256"
    )
    printf '%s\n' "$archive"
}

install_archive() {
    local archive="$1"
    BLUEY_JOBS_SKIP_SYSTEMD=1 \
    BLUEY_JOBS_WORKER_INSTALL_ROOT="$TEST_ROOT/install" \
    BLUEY_JOBS_DISCOVERY_LINK_ROOT="$TEST_ROOT/direct" \
    BLUEY_JOBS_GLOBAL_DISCOVERY_LINK_ROOT="$TEST_ROOT/global" \
    BLUEY_JOBS_RELEASE_KEEP=2 \
        "$ROOT/ops/install-bluey-jobs-workers.sh" "$archive"
}

first_archive="$(make_archive release-one)"
install_archive "$first_archive"

first_release="$TEST_ROOT/install/releases/release-one"
[ "$(readlink "$TEST_ROOT/direct/current")" = "$first_release" ]
[ "$(readlink "$TEST_ROOT/global/current")" = "$first_release" ]
[ -f "$first_release/.archive.sha256" ]

# Reinstalling identical bytes is idempotent.
install_archive "$first_archive"
[ "$(find "$TEST_ROOT/install/releases" -mindepth 1 -maxdepth 1 -type d | wc -l | tr -d ' ')" = "1" ]

second_archive="$(make_archive release-two)"
install_archive "$second_archive"
second_release="$TEST_ROOT/install/releases/release-two"
[ "$(readlink "$TEST_ROOT/direct/current")" = "$second_release" ]
[ "$(readlink "$TEST_ROOT/global/current")" = "$second_release" ]

third_archive="$(make_archive release-three)"
install_archive "$third_archive"
third_release="$TEST_ROOT/install/releases/release-three"
[ "$(readlink "$TEST_ROOT/direct/current")" = "$third_release" ]
[ "$(readlink "$TEST_ROOT/global/current")" = "$third_release" ]
[ -d "$second_release" ]
[ -d "$third_release" ]
[ ! -e "$first_release" ]

SYSTEMD_TEST="$TEST_ROOT/systemd-test"
mkdir -p "$SYSTEMD_TEST/bin" "$SYSTEMD_TEST/systemd" "$SYSTEMD_TEST/sbin"
cat > "$SYSTEMD_TEST/bin/systemctl" <<'SH'
#!/usr/bin/env bash
if [ "${BLUEY_TEST_FAIL_GLOBAL_RESTART:-0}" = "1" ] &&
    [ "${1:-}" = "restart" ] &&
    [ "${2:-}" = "bluey-jobs-global-discovery.service" ]; then
    exit 1
fi
exit 0
SH
chmod +x "$SYSTEMD_TEST/bin/systemctl"

install_with_systemd() {
    local archive="$1"
    BLUEY_JOBS_ALLOW_NON_ROOT_TEST=1 \
    BLUEY_JOBS_WORKER_INSTALL_ROOT="$SYSTEMD_TEST/install" \
    BLUEY_JOBS_DISCOVERY_LINK_ROOT="$SYSTEMD_TEST/direct" \
    BLUEY_JOBS_GLOBAL_DISCOVERY_LINK_ROOT="$SYSTEMD_TEST/global" \
    BLUEY_JOBS_SYSTEMD_DIR="$SYSTEMD_TEST/systemd" \
    BLUEY_JOBS_SBIN_DIR="$SYSTEMD_TEST/sbin" \
    BLUEY_JOBS_SYSTEMCTL_BIN="$SYSTEMD_TEST/bin/systemctl" \
        "$ROOT/ops/install-bluey-jobs-workers.sh" "$archive"
}

BLUEY_TEST_FAIL_GLOBAL_RESTART=0 install_with_systemd "$first_archive" >/dev/null
systemd_first="$SYSTEMD_TEST/install/releases/release-one"
[ "$(readlink "$SYSTEMD_TEST/direct/current")" = "$systemd_first" ]
[ "$(readlink "$SYSTEMD_TEST/global/current")" = "$systemd_first" ]

if BLUEY_TEST_FAIL_GLOBAL_RESTART=1 install_with_systemd "$second_archive" >/dev/null 2>&1; then
    echo "expected failed activation to roll back" >&2
    exit 1
fi
[ "$(readlink "$SYSTEMD_TEST/direct/current")" = "$systemd_first" ]
[ "$(readlink "$SYSTEMD_TEST/global/current")" = "$systemd_first" ]

echo "test-install-bluey-jobs-workers: PASS"
