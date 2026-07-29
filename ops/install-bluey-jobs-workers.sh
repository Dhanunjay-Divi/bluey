#!/usr/bin/env bash
# Install and activate an exact Bluey Jobs discovery worker artifact.
#
# Usage:
#   sudo ops/install-bluey-jobs-workers.sh /path/to/jobs-workers-<commit>.tar.gz

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ARCHIVE="${1:-}"
CHECKSUM_FILE="${2:-${ARCHIVE}.sha256}"
INSTALL_ROOT="${BLUEY_JOBS_WORKER_INSTALL_ROOT:-/opt/bluey-jobs-workers}"
DIRECT_ROOT="${BLUEY_JOBS_DISCOVERY_LINK_ROOT:-/opt/bluey-jobs-discovery}"
GLOBAL_ROOT="${BLUEY_JOBS_GLOBAL_DISCOVERY_LINK_ROOT:-/opt/bluey-jobs-global-discovery}"
SYSTEMD_DIR="${BLUEY_JOBS_SYSTEMD_DIR:-/etc/systemd/system}"
SBIN_DIR="${BLUEY_JOBS_SBIN_DIR:-/usr/local/sbin}"
SYSTEMCTL_BIN="${BLUEY_JOBS_SYSTEMCTL_BIN:-systemctl}"
KEEP_RELEASES="${BLUEY_JOBS_RELEASE_KEEP:-3}"
SKIP_SYSTEMD="${BLUEY_JOBS_SKIP_SYSTEMD:-0}"

fail() {
    echo "install-bluey-jobs-workers: $*" >&2
    exit 1
}

sha256_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    else
        fail "sha256sum or shasum is required"
    fi
}

if [ "$SKIP_SYSTEMD" != "1" ] &&
    [ "${BLUEY_JOBS_ALLOW_NON_ROOT_TEST:-0}" != "1" ] &&
    [ "$(id -u)" != "0" ]; then
    fail "run as root"
fi
[ -f "$ARCHIVE" ] || fail "worker archive is required"
[ -f "$CHECKSUM_FILE" ] || fail "checksum sidecar is required"
[[ "$KEEP_RELEASES" =~ ^[1-9][0-9]*$ ]] || fail "release retention must be positive"

expected_sha="$(awk 'NR == 1 {print $1}' "$CHECKSUM_FILE")"
[[ "$expected_sha" =~ ^[0-9A-Fa-f]{64}$ ]] || fail "checksum sidecar is invalid"
actual_sha="$(sha256_file "$ARCHIVE")"
expected_sha="$(printf '%s' "$expected_sha" | tr '[:upper:]' '[:lower:]')"
[ "$actual_sha" = "$expected_sha" ] || fail "worker archive checksum mismatch"

ARCHIVE_ENTRIES="$(tar -tzf "$ARCHIVE")"
while IFS= read -r entry; do
    case "$entry" in
        /*|../*|*/../*|*/..)
            fail "archive contains an unsafe path"
            ;;
    esac
done <<< "$ARCHIVE_ENTRIES"

TOP_LEVELS="$(printf '%s\n' "$ARCHIVE_ENTRIES" | cut -d/ -f1 | sort -u)"
[ "$(printf '%s\n' "$TOP_LEVELS" | wc -l | tr -d ' ')" = "1" ] ||
    fail "archive must have one release root"
RELEASE_ID="$TOP_LEVELS"
[[ "$RELEASE_ID" =~ ^[A-Za-z0-9._-]+$ ]] || fail "release ID contains unsafe characters"

RELEASES_ROOT="$INSTALL_ROOT/releases"
RELEASE_DIR="$RELEASES_ROOT/$RELEASE_ID"
ARCHIVE_SHA="$actual_sha"
mkdir -p "$RELEASES_ROOT" "$DIRECT_ROOT" "$GLOBAL_ROOT"

if [ -d "$RELEASE_DIR" ]; then
    [ -f "$RELEASE_DIR/.archive.sha256" ] ||
        fail "existing release is missing its archive checksum"
    [ "$(cat "$RELEASE_DIR/.archive.sha256")" = "$ARCHIVE_SHA" ] ||
        fail "existing release ID has different bytes"
else
    EXTRACT_ROOT="$(mktemp -d "$RELEASES_ROOT/.install.XXXXXX")"
    trap 'rm -rf "${EXTRACT_ROOT:-}"' EXIT
    tar -C "$EXTRACT_ROOT" -xzf "$ARCHIVE"
    [ -f "$EXTRACT_ROOT/$RELEASE_ID/manifest.json" ] ||
        fail "release manifest is missing"
    [ -f "$EXTRACT_ROOT/$RELEASE_ID/jobs/workflows/dist/discovery-worker.js" ] ||
        fail "direct discovery worker is missing"
    [ -f "$EXTRACT_ROOT/$RELEASE_ID/jobs/workflows/dist/global-discovery-worker.js" ] ||
        fail "global discovery worker is missing"
    printf '%s\n' "$ARCHIVE_SHA" > "$EXTRACT_ROOT/$RELEASE_ID/.archive.sha256"
    chmod -R go-w "$EXTRACT_ROOT/$RELEASE_ID"
    mv "$EXTRACT_ROOT/$RELEASE_ID" "$RELEASE_DIR"
    rmdir "$EXTRACT_ROOT"
    trap - EXIT
fi

old_direct="$(readlink "$DIRECT_ROOT/current" 2>/dev/null || true)"
old_global="$(readlink "$GLOBAL_ROOT/current" 2>/dev/null || true)"

replace_link() {
    local target="$1"
    local link="$2"
    local pending="${link}.new.$$"
    rm -f "$pending"
    ln -s "$target" "$pending"
    if mv --help >/dev/null 2>&1; then
        mv -Tf "$pending" "$link"
    else
        mv -fh "$pending" "$link"
    fi
}

rollback() {
    if [ -n "$old_direct" ]; then
        replace_link "$old_direct" "$DIRECT_ROOT/current"
    else
        rm -f "$DIRECT_ROOT/current"
    fi
    if [ -n "$old_global" ]; then
        replace_link "$old_global" "$GLOBAL_ROOT/current"
    else
        rm -f "$GLOBAL_ROOT/current"
    fi
    if [ "$SKIP_SYSTEMD" != "1" ]; then
        "$SYSTEMCTL_BIN" restart bluey-jobs-discovery.service || true
        "$SYSTEMCTL_BIN" restart bluey-jobs-global-discovery.service || true
    fi
}

replace_link "$RELEASE_DIR" "$DIRECT_ROOT/current"
replace_link "$RELEASE_DIR" "$GLOBAL_ROOT/current"

if [ "$SKIP_SYSTEMD" != "1" ]; then
    install -m 0750 "$ROOT/ops/check-bluey-jobs-discovery.sh" \
        "$SBIN_DIR/check-bluey-jobs-discovery.sh"
    install -m 0644 "$ROOT/ops/bluey-jobs-discovery.service.example" \
        "$SYSTEMD_DIR/bluey-jobs-discovery.service"
    install -m 0644 "$ROOT/ops/bluey-jobs-global-discovery.service.example" \
        "$SYSTEMD_DIR/bluey-jobs-global-discovery.service"
    install -m 0644 "$ROOT/ops/bluey-jobs-discovery-health.service.example" \
        "$SYSTEMD_DIR/bluey-jobs-discovery-health.service"
    install -m 0644 "$ROOT/ops/bluey-jobs-discovery-health.timer.example" \
        "$SYSTEMD_DIR/bluey-jobs-discovery-health.timer"

    "$SYSTEMCTL_BIN" daemon-reload
    "$SYSTEMCTL_BIN" enable bluey-jobs-discovery.service
    "$SYSTEMCTL_BIN" enable bluey-jobs-global-discovery.service
    "$SYSTEMCTL_BIN" enable --now bluey-jobs-discovery-health.timer
    if ! "$SYSTEMCTL_BIN" restart bluey-jobs-discovery.service ||
        ! "$SYSTEMCTL_BIN" restart bluey-jobs-global-discovery.service ||
        ! "$SYSTEMCTL_BIN" is-active --quiet bluey-jobs-discovery.service ||
        ! "$SYSTEMCTL_BIN" is-active --quiet bluey-jobs-global-discovery.service; then
        rollback
        fail "worker activation failed; previous release links restored"
    fi
    if ! BLUEY_JOBS_DISCOVERY_SKIP_SOURCE_FRESHNESS=1 \
        "$SBIN_DIR/check-bluey-jobs-discovery.sh"; then
        rollback
        fail "worker release check failed; previous release links restored"
    fi
fi

mapfile_supported=0
if command -v mapfile >/dev/null 2>&1; then
    mapfile_supported=1
fi
if [ "$mapfile_supported" = "1" ]; then
    mapfile -t release_dirs < <(find "$RELEASES_ROOT" -mindepth 1 -maxdepth 1 -type d \
        ! -name '.*' -print | sort)
else
    release_dirs=()
    while IFS= read -r release_dir; do
        release_dirs+=("$release_dir")
    done < <(find "$RELEASES_ROOT" -mindepth 1 -maxdepth 1 -type d \
        ! -name '.*' -print | sort)
fi

while [ "${#release_dirs[@]}" -gt "$KEEP_RELEASES" ]; do
    candidate="${release_dirs[0]}"
    if [ "$candidate" != "$(readlink "$DIRECT_ROOT/current")" ] &&
        [ "$candidate" != "$(readlink "$GLOBAL_ROOT/current")" ]; then
        rm -rf "$candidate"
    fi
    release_dirs=("${release_dirs[@]:1}")
done

echo "Installed Bluey Jobs workers: $RELEASE_ID"
echo "Direct: $(readlink "$DIRECT_ROOT/current")"
echo "Global: $(readlink "$GLOBAL_ROOT/current")"
