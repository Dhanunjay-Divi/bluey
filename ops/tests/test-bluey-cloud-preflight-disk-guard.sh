#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TEST_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/bluey-preflight-disk-guard.XXXXXX")"
trap 'rm -rf "$TEST_ROOT"' EXIT

fail() {
    echo "test-bluey-cloud-preflight-disk-guard: FAIL: $*" >&2
    exit 1
}

mkdir -p "$TEST_ROOT/bin" "$TEST_ROOT/data"
cat > "$TEST_ROOT/bin/stat" <<'SH'
#!/usr/bin/env bash
path="${!#}"
case "$path" in
    /dev/fd/*)
        case " $* " in
            *" -L "*) ;;
            *) exit 64 ;;
        esac
        ;;
esac
exec /usr/bin/stat "$@"
SH
cat > "$TEST_ROOT/bin/aws" <<'SH'
#!/usr/bin/env bash
[ -n "${AWS_ACCESS_KEY_ID:-}" ] && [ -n "${AWS_SECRET_ACCESS_KEY:-}" ] || exit 90
exit 0
SH
cat > "$TEST_ROOT/bin/curl" <<'SH'
#!/usr/bin/env bash
[ -z "${AWS_SECRET_ACCESS_KEY:-}" ] || exit 90
exit 0
SH
cat > "$TEST_ROOT/disk-guard" <<'SH'
#!/usr/bin/env bash
[ -z "${AWS_SECRET_ACCESS_KEY:-}" ] || exit 90
printf '%s\n' "${1:-}" >> "${MOCK_DISK_GUARD_LOG:?}"
exit "${MOCK_DISK_GUARD_EXIT:-0}"
SH
chmod +x "$TEST_ROOT/bin/stat" "$TEST_ROOT/bin/aws" "$TEST_ROOT/bin/curl" \
    "$TEST_ROOT/disk-guard"

ENV_FILE="$TEST_ROOT/preflight.env"
cat > "$ENV_FILE" <<EOF
BLUEY_PREFLIGHT_PROFILE=single-server-alpha
BLUEY_PREFLIGHT_STRICT=0
BLUEY_PUBLIC_URL=https://bluey.example.test
BLUEY_JWT_SECRET=0123456789abcdef0123456789abcdef
BLUEY_DB_PATH=$TEST_ROOT/data/bluey.db
BLUEY_BILLING_PROVIDER=manual-test
OPENAI_API_KEYS=test-openai-key
ANTHROPIC_API_KEYS=test-anthropic-key
GEMINI_API_KEYS=test-gemini-key
DEEPGRAM_API_KEYS=test-deepgram-key
BLUEY_ROUTE_POLICY=provider_mix
OFFSITE_DESTINATION=s3://bluey-test/backups
BLUEY_BACKUP_S3_ENDPOINT_URL=https://r2.example.test
AWS_ACCESS_KEY_ID=test-backup-key
AWS_SECRET_ACCESS_KEY=test-backup-secret
BLUEY_PREFLIGHT_DISK_GUARD_SCRIPT=$TEST_ROOT/disk-guard
EOF
chmod 0600 "$ENV_FILE"

MOCK_DISK_GUARD_LOG="$TEST_ROOT/disk-guard.log"
export MOCK_DISK_GUARD_LOG
: > "$MOCK_DISK_GUARD_LOG"

PATH="$TEST_ROOT/bin:$PATH" MOCK_DISK_GUARD_EXIT=0 \
BLUEY_PREFLIGHT_REQUIRE_DISK_GUARD=1 \
    "$ROOT/scripts/bluey-cloud-preflight.sh" "$ENV_FILE" >/dev/null
grep -Fqx 'check' "$MOCK_DISK_GUARD_LOG" ||
    fail "required preflight did not execute the durable guard"

# A writable ancestor is accepted only if it is root-owned and sticky.
# This synthetic ancestor is owned by the fixture user, even when sticky.
# A root test caller skips this user-ownership case.
if [ "$EUID" -ne 0 ]; then
    untrusted_dir="$TEST_ROOT/writable-user-parent"
    mkdir -m 0700 "$untrusted_dir"
    cp "$ENV_FILE" "$untrusted_dir/preflight.env"
    chmod 0600 "$untrusted_dir/preflight.env"
    chmod 1777 "$untrusted_dir"
    : > "$MOCK_DISK_GUARD_LOG"
    if PATH="$TEST_ROOT/bin:$PATH" MOCK_DISK_GUARD_EXIT=0 \
        BLUEY_PREFLIGHT_REQUIRE_DISK_GUARD=1 \
            "$ROOT/scripts/bluey-cloud-preflight.sh" "$untrusted_dir/preflight.env" \
            >"$TEST_ROOT/untrusted-preflight.out" 2>&1; then
        fail "preflight accepted a writable user-owned sticky parent"
    fi
    grep -Fq 'env file or its parent chain is not trusted' "$TEST_ROOT/untrusted-preflight.out" ||
        fail "preflight did not report the untrusted parent"
    [ ! -s "$MOCK_DISK_GUARD_LOG" ] || fail "untrusted parent reached the disk guard"
fi

: > "$MOCK_DISK_GUARD_LOG"
if PATH="$TEST_ROOT/bin:$PATH" MOCK_DISK_GUARD_EXIT=1 \
    BLUEY_PREFLIGHT_REQUIRE_DISK_GUARD=1 \
        "$ROOT/scripts/bluey-cloud-preflight.sh" "$ENV_FILE" >/dev/null 2>&1; then
    fail "required preflight passed after the durable guard failed"
fi
grep -Fqx 'check' "$MOCK_DISK_GUARD_LOG" ||
    fail "failed durable guard was not executed"

: > "$MOCK_DISK_GUARD_LOG"
PATH="$TEST_ROOT/bin:$PATH" MOCK_DISK_GUARD_EXIT=1 \
BLUEY_PREFLIGHT_REQUIRE_DISK_GUARD=0 \
    "$ROOT/scripts/bluey-cloud-preflight.sh" "$ENV_FILE" >/dev/null
[ ! -s "$MOCK_DISK_GUARD_LOG" ] ||
    fail "explicit non-production opt-out still executed the durable guard"

DEADMAN_OUTPUT="$TEST_ROOT/deadman-preflight.out"
if PATH="$TEST_ROOT/bin:$PATH" MOCK_DISK_GUARD_EXIT=0 \
    BLUEY_PREFLIGHT_REQUIRE_DISK_GUARD=0 \
    BLUEY_PREFLIGHT_REQUIRE_RESTORE_DEADMAN_PROVIDER=1 \
        "$ROOT/scripts/bluey-cloud-preflight.sh" "$ENV_FILE" \
        >"$DEADMAN_OUTPUT" 2>&1; then
    fail "production preflight claimed an unimplemented external restore dead-man"
fi
grep -Fq \
    'external restore-drill dead-man provider is not implemented; production release remains blocked' \
    "$DEADMAN_OUTPUT" ||
    fail "restore dead-man stop-line reason was not reported"

if PATH="$TEST_ROOT/bin:$PATH" MOCK_DISK_GUARD_EXIT=0 \
    BLUEY_PREFLIGHT_REQUIRE_RESTORE_DEADMAN_PROVIDER=invalid \
        "$ROOT/scripts/bluey-cloud-preflight.sh" "$ENV_FILE" >/dev/null 2>&1; then
    fail "preflight accepted an invalid restore dead-man requirement switch"
fi
grep -Fqx 'BLUEY_PREFLIGHT_REQUIRE_RESTORE_DEADMAN_PROVIDER=1' \
    "$ROOT/ops/bluey-storage.env.example" ||
    fail "production storage profile does not fix the restore dead-man stop line"

# A child that ignores TERM must still be killed after the bounded grace and
# release every inherited filesystem lock.
HOSTILE_GUARD="$TEST_ROOT/bin/hostile-disk-guard"
HOSTILE_LOCK="$TEST_ROOT/hostile.lock"
HOSTILE_ENV="$TEST_ROOT/hostile-preflight.env"
HOSTILE_OUTPUT="$TEST_ROOT/hostile-preflight.out"
cat > "$HOSTILE_GUARD" <<'SH'
#!/usr/bin/env bash
exec perl -MFcntl=:flock -e '
  open my $fh, ">>", $ENV{HOSTILE_LOCK} or die $!;
  flock($fh, LOCK_EX) or die $!;
  $SIG{TERM} = "IGNORE";
  my $witness = "TERM-ignoring guard acquired the lock\n";
  syswrite($fh, $witness) == length($witness) or die "missing lock witness";
  sleep 60;
'
SH
chmod 0755 "$HOSTILE_GUARD"
awk -v guard="$HOSTILE_GUARD" '
  /^BLUEY_PREFLIGHT_DISK_GUARD_SCRIPT=/ { print "BLUEY_PREFLIGHT_DISK_GUARD_SCRIPT=" guard; next }
  { print }
' "$ENV_FILE" > "$HOSTILE_ENV"
chmod 0600 "$HOSTILE_ENV"
start_epoch="$(date +%s)"
if PATH="$TEST_ROOT/bin:$PATH" HOSTILE_LOCK="$HOSTILE_LOCK" \
    BLUEY_PREFLIGHT_REQUIRE_DISK_GUARD=1 \
    BLUEY_PREFLIGHT_DISK_GUARD_SCRIPT="$HOSTILE_GUARD" \
    BLUEY_PREFLIGHT_COMMAND_TIMEOUT_SECONDS=1 \
    BLUEY_PREFLIGHT_TIMEOUT_KILL_GRACE_SECONDS=1 \
    "$ROOT/scripts/bluey-cloud-preflight.sh" "$HOSTILE_ENV" >"$HOSTILE_OUTPUT" 2>&1; then
  fail "TERM-ignoring preflight child passed"
fi
elapsed=$(( $(date +%s) - start_epoch ))
[ "$elapsed" -le 5 ] || fail "TERM-ignoring child exceeded the timeout plus kill grace"
grep -Fxq 'fail: durable production disk/backup guard failed' "$HOSTILE_OUTPUT" ||
    fail "hostile guard did not reach the bounded disk check"
grep -Fxq 'TERM-ignoring guard acquired the lock' "$HOSTILE_LOCK" ||
    fail "hostile guard never acquired the fixture lock"
HOSTILE_LOCK="$HOSTILE_LOCK" perl -MFcntl=:flock -e '
  open my $fh, ">>", $ENV{HOSTILE_LOCK} or exit 2;
  exit(flock($fh, LOCK_EX | LOCK_NB) ? 0 : 1);
' || fail "killed preflight child retained its lock"

echo "test-bluey-cloud-preflight-disk-guard: PASS"
