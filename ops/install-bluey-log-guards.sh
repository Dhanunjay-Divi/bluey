#!/usr/bin/env bash
# Install Bluey production log rotation, off-host log archive, and disk guard
# cron jobs on an API host.
#
# Run on the droplet from a checked-out release repo. Preparation is the safe
# default; use --activate only after storage/bootstrap canaries pass:
#   sudo ops/install-bluey-log-guards.sh --prepare

set +x
set -euo pipefail

BLUEY_ENV_FILE="${BLUEY_ENV_FILE:-/etc/bluey-api/bluey-api.env}"
BLUEY_STORAGE_ENV_FILE="${BLUEY_STORAGE_ENV_FILE:-/etc/bluey-api/bluey-storage.env}"

bootstrap_stat() {
    local format_gnu="$1" format_bsd="$2" path="$3"
    if stat "$format_gnu" -- "$path" >/dev/null 2>&1; then
        stat "$format_gnu" -- "$path"
    else
        stat "$format_bsd" -- "$path"
    fi
}

bootstrap_trusted_file() {
    local path="$1" owner mode mode_value
    [ -f "$path" ] && [ ! -L "$path" ] || return 1
    owner="$(bootstrap_stat -c%u -f%u "$path")" || return 1
    if [ "$EUID" = "0" ]; then
        [ "$owner" = "0" ] || return 1
    else
        { [ "$owner" = "0" ] || [ "$owner" = "$EUID" ]; } || return 1
    fi
    mode="$(bootstrap_stat -c%a -f%Lp "$path")" || return 1
    case "$mode" in ""|*[!0-9]*) return 1 ;; esac
    mode_value=$((8#$mode))
    [ $((mode_value & 8#022)) -eq 0 ]
}

bootstrap_trusted_parent_chain() {
    local path="$1" current owner mode value
    current="$(cd -P -- "$(dirname "$path")" 2>/dev/null && pwd -P)" || return 1
    while :; do
        [ -d "$current" ] && [ ! -L "$current" ] || return 1
        owner="$(bootstrap_stat -c%u -f%u "$current")" || return 1
        { [ "$owner" = 0 ] || [ "$owner" = "$EUID" ]; } || return 1
        mode="$(bootstrap_stat -c%a -f%Lp "$current")" || return 1
        case "$mode" in ''|*[!0-9]*) return 1 ;; esac
        value=$((8#$mode))
        if [ $((value & 8#022)) -ne 0 ] &&
            ! { [ "$owner" = 0 ] && [ $((value & 8#1000)) -ne 0 ]; }; then return 1; fi
        [ "$current" = / ] && break
        current="$(dirname "$current")"
    done
}

bootstrap_identity() { bootstrap_stat -c%i -f%i "$1"; }

bootstrap_follow_identity() {
    if stat -L -c%i -- "$1" >/dev/null 2>&1; then
        stat -L -c%i -- "$1"
    else
        stat -L -f%i -- "$1"
    fi
}

load_env_file() {
    local env_file="$1" before after
    if [ -e "$env_file" ] || [ -L "$env_file" ]; then
        [ -r "$env_file" ] && bootstrap_trusted_parent_chain "$env_file" && bootstrap_trusted_file "$env_file" || {
            echo "install-bluey-log-guards.sh: environment file is not trusted and write-protected" >&2
            exit 1
        }
        before="$(bootstrap_identity "$env_file")" || exit 1
        exec 9<"$env_file"
        after="$(bootstrap_follow_identity /dev/fd/9)" || exit 1
        [ "$before" = "$after" ] || exit 1
        # shellcheck disable=SC1090
        . /dev/fd/9
        exec 9<&-
    fi
}

load_env_file "$BLUEY_ENV_FILE"
load_env_file "$BLUEY_STORAGE_ENV_FILE"
if [ -n "${BLUEY_EXTRA_ENV_FILES:-}" ]; then
    # shellcheck disable=SC2086
    for env_file in $BLUEY_EXTRA_ENV_FILES; do
        load_env_file "$env_file"
    done
fi
unset -f bootstrap_stat bootstrap_trusted_file bootstrap_trusted_parent_chain bootstrap_identity \
    bootstrap_follow_identity
while IFS= read -r inherited_name; do
    case "$inherited_name" in
        *KEY*|*TOKEN*|*SECRET*|*PASSWORD*|*DATABASE_URL*|*DSN*|*WEBHOOK*|*CREDENTIAL*|*COOKIE*|*AUTH*)
            export -n "$inherited_name" 2>/dev/null || true
            ;;
    esac
done < <(compgen -e)
unset inherited_name

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SERVICE_NAME="${BLUEY_API_SERVICE_NAME:-bluey-api}"
API_ROOT="${BLUEY_API_ROOT-/opt/bluey-api}"
LOG_DIR="${BLUEY_API_LOG_DIR-/var/log/bluey-api}"
ARCHIVE_ROOT="${BLUEY_LOG_ARCHIVE_LOCAL_DIR-/var/backups/bluey-api/logs}"
BACKUP_ROOT="${BLUEY_BACKUP_DIR-/var/backups/bluey-api}"
RESTORE_DEADMAN_ROOT="$BACKUP_ROOT/deadman"
RESTORE_LOCK_ROOT="$BACKUP_ROOT/.restore-drill-locks"
STATUS_ROOT="${BLUEY_OPS_STATE_ROOT:-${BLUEY_DISK_GUARD_STATUS_ROOT:-/var/lib/bluey-ops}}"
TMP_PRUNE_ROOT="${BLUEY_DISK_GUARD_TMP_ROOT-$STATUS_ROOT/tmp}"
LOG_WORK_ROOT="${BLUEY_LOG_ARCHIVE_WORK_DIR-/var/lib/bluey-ops/log-archive}"
OPS_LOG_DIR="${BLUEY_OPS_LOG_DIR-/var/log/bluey-ops}"
BACKUP_STATUS_FILE="${BLUEY_BACKUP_STATUS_FILE-$BACKUP_ROOT/.backup.status}"
GUARD_STATUS_FILE="${BLUEY_DISK_GUARD_STATUS_FILE-$STATUS_ROOT/disk-guard.status}"
GUARD_LOCK_FILE="${BLUEY_DISK_GUARD_LOCK_FILE-$STATUS_ROOT/disk-guard.lock}"
ARCHIVE_STATUS_FILE="${BLUEY_LOG_ARCHIVE_STATUS_FILE-$STATUS_ROOT/log-archive.status}"
USER_NAME="${BLUEY_API_USER:-bluey}"
GROUP_NAME="${BLUEY_API_GROUP:-bluey}"
JOURNAL_MAX_USE="${BLUEY_JOURNAL_SYSTEM_MAX_USE:-1G}"
JOURNAL_KEEP_FREE="${BLUEY_JOURNAL_SYSTEM_KEEP_FREE:-16G}"
MODE="${1:---prepare}"

die() {
    echo "install-bluey-log-guards.sh: $*" >&2
    exit 1
}

validate_safe_directory() {
    local name="$1" path="$2" minimum_components="$3"
    local remainder component component_count=0 canonical
    [ -n "$path" ] || die "$name must not be empty"
    case "$path" in
        /*) ;;
        *) die "$name must be an absolute path" ;;
    esac
    remainder="${path#/}"
    while [ -n "$remainder" ]; do
        component="${remainder%%/*}"
        if [ "$remainder" = "$component" ]; then
            remainder=""
        else
            remainder="${remainder#*/}"
        fi
        case "$component" in
            "") ;;
            .|..) die "$name contains an unsafe path segment" ;;
            *) component_count=$((component_count + 1)) ;;
        esac
    done
    [ "$component_count" -ge "$minimum_components" ] ||
        die "$name is an unsafe broad filesystem root"
    [ ! -L "$path" ] || die "$name must not be a symlink"
    if [ -d "$path" ]; then
        canonical="$(cd -P -- "$path" 2>/dev/null && pwd -P)" ||
            die "$name cannot be resolved"
        [ -n "${canonical//\//}" ] || die "$name resolves to the filesystem root"
    fi
}

path_owner_uid() {
    if stat -c%u "$1" >/dev/null 2>&1; then stat -c%u "$1"; else stat -f%u "$1"; fi
}

path_mode() {
    if stat -c%a "$1" >/dev/null 2>&1; then stat -c%a "$1"; else stat -f%Lp "$1"; fi
}

path_dev_inode() {
    if stat -c '%d:%i' "$1" >/dev/null 2>&1; then stat -c '%d:%i' "$1"; else stat -f '%d:%i' "$1"; fi
}

path_follow_owner_uid() {
    if stat -L -c%u -- "$1" >/dev/null 2>&1; then
        stat -L -c%u -- "$1"
    else
        stat -L -f%u -- "$1"
    fi
}

path_follow_mode() {
    if stat -L -c%a -- "$1" >/dev/null 2>&1; then
        stat -L -c%a -- "$1"
    else
        stat -L -f%Lp -- "$1"
    fi
}

path_follow_dev_inode() {
    if stat -L -c '%d:%i' -- "$1" >/dev/null 2>&1; then
        stat -L -c '%d:%i' -- "$1"
    else
        stat -L -f '%d:%i' -- "$1"
    fi
}

validate_trusted_existing_chain() {
    local path="$1" remainder component current="" owner mode mode_value
    remainder="${path#/}"
    while [ -n "$remainder" ]; do
        component="${remainder%%/*}"
        if [ "$remainder" = "$component" ]; then remainder=""; else remainder="${remainder#*/}"; fi
        [ -n "$component" ] || continue
        current="$current/$component"
        [ -e "$current" ] || continue
        [ -d "$current" ] && [ ! -L "$current" ] ||
            die "untrusted symlink/non-directory in root-owned path chain: $current"
        owner="$(path_owner_uid "$current")"
        [ "$owner" = "0" ] || die "untrusted owner in root-owned path chain: $current"
        mode="$(path_mode "$current")"
        printf '%s\n' "$mode" | grep -Eq '^[0-7]+$' || die "invalid mode for $current"
        mode_value=$((8#$mode))
        [ $((mode_value & 8#022)) -eq 0 ] ||
            die "group/world-writable root-owned path chain: $current"
    done
}

validate_existing_chain_without_symlinks() {
    local path="$1" remainder component current=""
    remainder="${path#/}"
    while [ -n "$remainder" ]; do
        component="${remainder%%/*}"
        if [ "$remainder" = "$component" ]; then remainder=""; else remainder="${remainder#*/}"; fi
        [ -n "$component" ] || continue
        current="$current/$component"
        [ -e "$current" ] || continue
        [ -d "$current" ] && [ ! -L "$current" ] ||
            die "symlink/non-directory in service path chain: $current"
    done
}

secure_root_directory() {
    local path="$1" mode="$2"
    validate_trusted_existing_chain "$(dirname "$path")"
    [ ! -L "$path" ] || die "refusing symlinked root-owned path: $path"
    [ ! -e "$path" ] || [ -d "$path" ] ||
        die "refusing non-directory root-owned path: $path"
    mkdir -p "$path"
    chown root:root "$path"
    chmod "$mode" "$path"
    validate_trusted_existing_chain "$path"
}

secure_root_control_file_if_present() {
    local path="$1"
    if [ ! -e "$path" ] && [ ! -L "$path" ]; then
        return
    fi
    [ -f "$path" ] && [ ! -L "$path" ] ||
        die "refusing symlinked/non-regular root control file: $path"
    chown root:root "$path"
    chmod 0600 "$path"
}

validate_safe_directory BLUEY_API_ROOT "$API_ROOT" 2
validate_safe_directory BLUEY_API_LOG_SUBDIR "$API_ROOT/logs" 3
validate_safe_directory BLUEY_API_LOG_DIR "$LOG_DIR" 3
validate_safe_directory BLUEY_LOG_ARCHIVE_LOCAL_DIR "$ARCHIVE_ROOT" 2
validate_safe_directory BLUEY_BACKUP_DIR "$BACKUP_ROOT" 2
validate_safe_directory BLUEY_RESTORE_DRILL_DEADMAN_ROOT "$RESTORE_DEADMAN_ROOT" 3
validate_safe_directory BLUEY_RESTORE_DRILL_LOCK_ROOT "$RESTORE_LOCK_ROOT" 3
validate_safe_directory BLUEY_OPS_STATE_ROOT "$STATUS_ROOT" 2
validate_safe_directory BLUEY_DISK_GUARD_TMP_ROOT "$TMP_PRUNE_ROOT" 2
validate_safe_directory BLUEY_LOG_ARCHIVE_WORK_DIR "$LOG_WORK_ROOT" 2
validate_safe_directory BLUEY_OPS_LOG_DIR "$OPS_LOG_DIR" 3
for status_file in "$BACKUP_STATUS_FILE" "$GUARD_STATUS_FILE" \
    "$GUARD_LOCK_FILE" "$ARCHIVE_STATUS_FILE"; do
    validate_safe_directory BLUEY_OPS_CONTROL_ROOT "$(dirname "$status_file")" 2
    validate_safe_directory BLUEY_OPS_CONTROL_FILE "$status_file" 3
done
case "$BACKUP_STATUS_FILE" in "$BACKUP_ROOT"/*) ;; *) die "backup status escapes backup root" ;; esac
[ "$(dirname "$BACKUP_STATUS_FILE")" = "$BACKUP_ROOT" ] ||
    die "backup status must be directly inside backup root"
for status_file in "$GUARD_STATUS_FILE" "$GUARD_LOCK_FILE" "$ARCHIVE_STATUS_FILE"; do
    case "$status_file" in "$STATUS_ROOT"/*) ;; *) die "ops status/lock escapes state root" ;; esac
    [ "$(dirname "$status_file")" = "$STATUS_ROOT" ] ||
        die "ops status/lock must be directly inside state root"
done
printf '%s\n' "$JOURNAL_MAX_USE" | grep -Eq '^[1-9][0-9]*[KMG]$' ||
    die "invalid BLUEY_JOURNAL_SYSTEM_MAX_USE"
printf '%s\n' "$JOURNAL_KEEP_FREE" | grep -Eq '^[1-9][0-9]*[KMG]$' ||
    die "invalid BLUEY_JOURNAL_SYSTEM_KEEP_FREE"

INSTALL_LOCK_FILE="${BLUEY_LOG_GUARD_INSTALL_LOCK_FILE:-/run/lock/bluey-log-guards-install.lock}"
validate_safe_directory BLUEY_LOG_GUARD_INSTALL_LOCK_FILE "$INSTALL_LOCK_FILE" 3
validate_trusted_existing_chain "$(dirname "$INSTALL_LOCK_FILE")"
[ ! -L "$INSTALL_LOCK_FILE" ] || die "installer lock must not be a symlink"
if [ -e "$INSTALL_LOCK_FILE" ]; then
    [ -f "$INSTALL_LOCK_FILE" ] || die "installer lock must be a regular file"
    [ "$(path_owner_uid "$INSTALL_LOCK_FILE")" = 0 ] || die "installer lock must be root-owned"
    lock_mode="$(path_mode "$INSTALL_LOCK_FILE")"
    printf '%s\n' "$lock_mode" | grep -Eq '^[0-7]+$' || die "installer lock mode is invalid"
    [ $((8#$lock_mode & 8#022)) -eq 0 ] || die "installer lock must not be group/world writable"
fi

case "$MODE" in
    --check-config)
        echo "Bluey log guard configuration is safe."
        exit 0
        ;;
    --prepare) ACTIVATE_CRON=0 ;;
    install|--install|--activate) ACTIVATE_CRON=1 ;;
    --test-lock-open)
        [ "${BLUEY_INSTALLER_ENABLE_TEST_HOOK:-0}" = 1 ] && [ "$EUID" != 0 ] ||
            die "lock-open test hook is unavailable in production/root execution"
        ACTIVATE_CRON=test
        ;;
    *) die "usage: $0 [--prepare|--activate|--check-config]" ;;
esac

if [ "$ACTIVATE_CRON" != test ] && [ "$(id -u)" != "0" ]; then
    echo "install-bluey-log-guards.sh must run as root" >&2
    exit 1
fi
command -v flock >/dev/null 2>&1 || die "flock is required for installer process fencing"
mkdir -p "$(dirname "$INSTALL_LOCK_FILE")"
exec 8>>"$INSTALL_LOCK_FILE"
if [ "$ACTIVATE_CRON" = test ] && [ -n "${BLUEY_INSTALLER_LOCK_POST_OPEN_TEST_HOOK_COMMAND:-}" ]; then
    /bin/bash -c "$BLUEY_INSTALLER_LOCK_POST_OPEN_TEST_HOOK_COMMAND" ||
        die "lock-open test hook failed"
fi
validate_trusted_existing_chain "$(dirname "$INSTALL_LOCK_FILE")"
[ ! -L "$INSTALL_LOCK_FILE" ] && [ -f "$INSTALL_LOCK_FILE" ] ||
    die "installer lock changed type during open"
[ "$(path_owner_uid "$INSTALL_LOCK_FILE")" = 0 ] || die "installer lock changed owner during open"
lock_mode="$(path_mode "$INSTALL_LOCK_FILE")"
printf '%s\n' "$lock_mode" | grep -Eq '^[0-7]+$' || die "installer lock mode changed invalidly"
[ $((8#$lock_mode & 8#022)) -eq 0 ] || die "installer lock became group/world writable"
path_lock_identity="$(path_dev_inode "$INSTALL_LOCK_FILE")" || die "cannot stat installer lock path"
fd_lock_identity="$(path_follow_dev_inode /dev/fd/8)" || die "cannot stat opened installer lock"
[ "$path_lock_identity" = "$fd_lock_identity" ] || die "installer lock identity changed across open"
[ -f /dev/fd/8 ] || die "opened installer lock is not regular"
[ "$(path_follow_owner_uid /dev/fd/8)" = 0 ] || die "opened installer lock is not root-owned"
fd_lock_mode="$(path_follow_mode /dev/fd/8)"
printf '%s\n' "$fd_lock_mode" | grep -Eq '^[0-7]+$' || die "opened installer lock mode is invalid"
[ $((8#$fd_lock_mode & 8#022)) -eq 0 ] || die "opened installer lock is group/world writable"
chmod 0600 "$INSTALL_LOCK_FILE"
flock -n 8 || die "another Bluey storage installation is active"
if [ "$ACTIVATE_CRON" = test ]; then
    echo "Bluey installer lock-open attestation is safe."
    exit 0
fi
if [ "$ACTIVATE_CRON" = "0" ]; then
    [ ! -e /etc/cron.d/bluey-log-guards ] ||
        die "disable the existing bluey-log-guards cron file before --prepare"
    [ ! -e /etc/cron.d/bluey-api-backup ] ||
        die "disable the existing bluey-api-backup cron file before --prepare"
    [ ! -e "/etc/logrotate.d/${SERVICE_NAME}" ] ||
        die "disable the existing Bluey logrotate policy before --prepare"
    [ ! -e /etc/logrotate.d/bluey-ops ] ||
        die "disable the existing Bluey ops-logrotate policy before --prepare"
    if command -v systemctl >/dev/null 2>&1 &&
        systemctl is-active --quiet "${SERVICE_NAME}.service"; then
        die "stop ${SERVICE_NAME}.service before the ownership/credential migration"
    fi
fi

install_atomically() {
    local source="$1" destination="$2" mode="$3" temp
    temp="$(mktemp "$(dirname "$destination")/.$(basename "$destination").tmp.XXXXXX")"
    install -m "$mode" "$source" "$temp"
    mv "$temp" "$destination"
}

# Migrate the privileged storage boundary before installing or opening any
# root-run artifact. Parents must already be root-owned and write-protected;
# leaf directories may be safely changed non-recursively from the legacy
# bluey-owned deployment only while the API and old cron are stopped.
for directory_target in \
    "$BACKUP_ROOT" "$BACKUP_ROOT/hourly" "$BACKUP_ROOT/daily" \
    "$BACKUP_ROOT/.staging" "$RESTORE_DEADMAN_ROOT" "$RESTORE_LOCK_ROOT" \
    "$ARCHIVE_ROOT" "$ARCHIVE_ROOT/.staging" \
    "$STATUS_ROOT" "$TMP_PRUNE_ROOT" "$LOG_WORK_ROOT" "$OPS_LOG_DIR" \
    "$LOG_DIR" "$API_ROOT/logs"; do
    validate_existing_chain_without_symlinks "$directory_target"
done
for control_file in "$BACKUP_ROOT/.backup.lock" "$BACKUP_STATUS_FILE" \
    "$GUARD_LOCK_FILE" "$GUARD_STATUS_FILE" "$ARCHIVE_STATUS_FILE" \
    "$LOG_WORK_ROOT/.archive.lock"; do
    if [ -e "$control_file" ] || [ -L "$control_file" ]; then
        [ -f "$control_file" ] && [ ! -L "$control_file" ] ||
            die "refusing symlinked/non-regular root control file: $control_file"
    fi
done
secure_root_directory "$BACKUP_ROOT" 0700
secure_root_directory "$BACKUP_ROOT/hourly" 0700
secure_root_directory "$BACKUP_ROOT/daily" 0700
secure_root_directory "$BACKUP_ROOT/.staging" 0700
secure_root_directory "$RESTORE_DEADMAN_ROOT" 0700
secure_root_directory "$RESTORE_LOCK_ROOT" 0700
secure_root_directory "$ARCHIVE_ROOT" 0700
secure_root_directory "$ARCHIVE_ROOT/.staging" 0700
secure_root_directory "$STATUS_ROOT" 0700
secure_root_directory "$TMP_PRUNE_ROOT" 0700
secure_root_directory "$LOG_WORK_ROOT" 0700
secure_root_directory "$OPS_LOG_DIR" 0750
# The guard holds a shared copy of the same lock for its complete metadata
# scan. Create the canonical inode during the stopped-service migration so the
# first guard and first backup cannot race to establish different state.
touch "$BACKUP_ROOT/.backup.lock"
secure_root_control_file_if_present "$BACKUP_ROOT/.backup.lock"
secure_root_control_file_if_present "$BACKUP_STATUS_FILE"
secure_root_control_file_if_present "$GUARD_LOCK_FILE"
secure_root_control_file_if_present "$GUARD_STATUS_FILE"
secure_root_control_file_if_present "$ARCHIVE_STATUS_FILE"
secure_root_control_file_if_present "$LOG_WORK_ROOT/.archive.lock"

validate_existing_chain_without_symlinks "$LOG_DIR"
validate_existing_chain_without_symlinks "$API_ROOT/logs"
mkdir -p "$LOG_DIR" "$API_ROOT/logs"
validate_existing_chain_without_symlinks "$LOG_DIR"
validate_existing_chain_without_symlinks "$API_ROOT/logs"

install_atomically "$ROOT/ops/archive-bluey-logs.sh" \
    /usr/local/sbin/archive-bluey-logs.sh 0750
install_atomically "$ROOT/ops/bluey-disk-guard.sh" \
    /usr/local/sbin/bluey-disk-guard.sh 0750

# Secure the root cron-log directory before opening any path inside it. Never
# follow a file planted by the unprivileged service from an older install.
cron_logs=(
    "$OPS_LOG_DIR/backup-cron.log"
    "$OPS_LOG_DIR/log-archive-cron.log"
    "$OPS_LOG_DIR/disk-guard-cron.log"
)
for cron_log in "${cron_logs[@]}"; do
    [ ! -L "$cron_log" ] || die "refusing symlinked root cron log: $cron_log"
    [ ! -e "$cron_log" ] || [ -f "$cron_log" ] ||
        die "refusing non-regular root cron log: $cron_log"
    touch "$cron_log"
    chmod 0640 "$cron_log"
done
chown root:root "$OPS_LOG_DIR" "${cron_logs[@]}"
if id "$USER_NAME" >/dev/null 2>&1; then
    chown "$USER_NAME:$GROUP_NAME" "$LOG_DIR" "$API_ROOT/logs"
fi
chmod 0750 "$LOG_DIR" "$API_ROOT/logs"

if [ -d /etc/systemd ]; then
    mkdir -p /etc/systemd/journald.conf.d
    journal_tmp="$(mktemp /etc/systemd/journald.conf.d/.bluey-storage.conf.tmp.XXXXXX)"
    cat > "$journal_tmp" <<JOURNAL
[Journal]
SystemMaxUse=$JOURNAL_MAX_USE
SystemKeepFree=$JOURNAL_KEEP_FREE
JOURNAL
    chmod 0644 "$journal_tmp"
    mv "$journal_tmp" /etc/systemd/journald.conf.d/bluey-storage.conf
    if command -v systemctl >/dev/null 2>&1; then
        systemctl restart systemd-journald
    fi
else
    echo "warn: systemd config directory missing; persistent journal cap not installed" >&2
fi

/usr/local/sbin/archive-bluey-logs.sh --check-config >/dev/null
/usr/local/sbin/bluey-disk-guard.sh --check-config >/dev/null

if [ "$ACTIVATE_CRON" = "1" ]; then
    command -v logrotate >/dev/null 2>&1 ||
        die "logrotate is required before storage guards can be activated"
    [ -d /etc/logrotate.d ] ||
        die "/etc/logrotate.d is required before storage guards can be activated"
    activation_rollback_dir="$(mktemp -d /var/tmp/bluey-log-guards-rollback.XXXXXX)"
    activation_committed=0
    for activation_path in "/etc/logrotate.d/${SERVICE_NAME}" \
        /etc/logrotate.d/bluey-ops /etc/cron.d/bluey-log-guards; do
        if [ -f "$activation_path" ] && [ ! -L "$activation_path" ]; then
            cp -p "$activation_path" "$activation_rollback_dir/$(basename "$activation_path")"
        elif [ -e "$activation_path" ] || [ -L "$activation_path" ]; then
            die "activation target is not a trusted regular file: $activation_path"
        fi
    done
    rollback_activation() {
        local path base
        [ "$activation_committed" = 0 ] || return 0
        for path in "/etc/logrotate.d/${SERVICE_NAME}" \
            /etc/logrotate.d/bluey-ops /etc/cron.d/bluey-log-guards; do
            base="$(basename "$path")"
            if [ -f "$activation_rollback_dir/$base" ]; then
                cp -p "$activation_rollback_dir/$base" "$path"
            else
                rm -f -- "$path"
            fi
        done
    }
    trap 'rollback_activation; rm -rf -- "$activation_rollback_dir"' EXIT
    if command -v logrotate >/dev/null 2>&1 && [ -d /etc/logrotate.d ]; then
        rotate_tmp="$(mktemp "/etc/logrotate.d/.${SERVICE_NAME}.tmp.XXXXXX")"
        cat > "$rotate_tmp" <<ROTATE
$LOG_DIR/*.log
$API_ROOT/logs/*.log
$API_ROOT/logs/*/*.log {
    daily
    rotate 7
    maxsize 50M
    missingok
    notifempty
    compress
    delaycompress
    dateext
    copytruncate
    # Run the copy/truncate as root so a single unexpected root-owned file
    # cannot disable rotation for every service-owned file in the stanza.
    # copytruncate preserves each source inode and its existing ownership.
    su root root
}
ROTATE
        chmod 0644 "$rotate_tmp"
        logrotate -d "$rotate_tmp" >/dev/null 2>&1 || die "generated service logrotate policy is invalid"
        mv "$rotate_tmp" "/etc/logrotate.d/${SERVICE_NAME}"

        ops_rotate_tmp="$(mktemp /etc/logrotate.d/.bluey-ops.tmp.XXXXXX)"
        cat > "$ops_rotate_tmp" <<OPS_ROTATE
$OPS_LOG_DIR/*.log {
    daily
    rotate 7
    maxsize 20M
    missingok
    notifempty
    compress
    delaycompress
    dateext
    copytruncate
    su root root
}
OPS_ROTATE
        chmod 0644 "$ops_rotate_tmp"
        logrotate -d "$ops_rotate_tmp" >/dev/null 2>&1 || die "generated ops logrotate policy is invalid"
        mv "$ops_rotate_tmp" /etc/logrotate.d/bluey-ops
    fi
    cron_tmp="$(mktemp /etc/cron.d/.bluey-log-guards.tmp.XXXXXX)"
    cat > "$cron_tmp" <<CRON
SHELL=/bin/bash
PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

# Archive production operational logs off-host every hour.
17 * * * * root /usr/local/sbin/archive-bluey-logs.sh >> $OPS_LOG_DIR/log-archive-cron.log 2>&1

# Detect capacity and archive/backup dead-man regressions within 15 minutes.
*/15 * * * * root /usr/local/sbin/bluey-disk-guard.sh check >> $OPS_LOG_DIR/disk-guard-cron.log 2>&1

# Reclaim only bounded temp/archive caches hourly, then evaluate post-prune capacity.
27 * * * * root /usr/local/sbin/bluey-disk-guard.sh --prune >> $OPS_LOG_DIR/disk-guard-cron.log 2>&1
CRON
    chmod 0644 "$cron_tmp"
    mv "$cron_tmp" /etc/cron.d/bluey-log-guards
    activation_committed=1
    rm -rf -- "$activation_rollback_dir"
    trap - EXIT
    echo "Bluey log guards installed and schedules activated."
else
    echo "Bluey log guards prepared; schedules remain disabled until --activate."
fi

echo "Next checks:"
echo "  /usr/local/sbin/archive-bluey-logs.sh"
echo "  /usr/local/sbin/bluey-disk-guard.sh"
