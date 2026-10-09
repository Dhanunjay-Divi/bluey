#!/usr/bin/env bash
# Build the pinned Bluey server locally for the isolated Pinky Linux preprod host.
set -euo pipefail
set -m
umask 077

readonly script=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")
readonly repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
readonly scheduler=/Users/uno/Projects/shared-runners/runner_schedule.py
readonly known_hosts=/Users/uno/.config/bluey-pinky-integration/known_hosts
readonly archive_host=162.243.248.189
readonly archive_remote=/root/assist-linux-openssl-sysroot.tar.gz
readonly archive_sha256=2d960d2b686783043678c6fa90f3669aa1dbf23c22c673b2e63545f93aa8505d
readonly rust_target=x86_64-unknown-linux-gnu
readonly glibc_version=2.39
readonly zigbuild_target="$rust_target.$glibc_version"
readonly zig_cc_target="x86_64-linux-gnu.$glibc_version"
readonly private_build_root="$HOME/.local/state/bluey-pinky-integration/linux-builds"

usage() {
  cat <<'EOF'
Usage:
  scripts/build-pinky-integration-linux.sh --source-sha <40-hex-sha>
  scripts/build-pinky-integration-linux.sh --dry-run [--source-sha <40-hex-sha>]
  scripts/build-pinky-integration-linux.sh --self-test

Options:
  --output-dir <absolute-path>  Final private artifact directory. It must not exist.
                                Defaults below ~/.local/state/bluey-pinky-integration.

The real build requires an exact clean commit, acquires the shared mac-heavy lock,
copies only the pinned OpenSSL sysroot archive from the preprod VM, and compiles on
this Mac. No compilation or repository access occurs on the VM.
EOF
}

cleanup_root() {
  local root_path=$1
  case "$root_path" in
    */bluey-pinky-linux-build.*)
      if [[ -d "$root_path" && ! -L "$root_path" ]]; then
        find "$root_path" -depth -delete
      fi
      ;;
    *)
      printf 'Unexpected temporary root; cleanup held: %s\n' "$root_path" >&2
      return 1
      ;;
  esac
  [[ ! -e "$root_path" ]]
}

resolve_openssl_layout() {
  local extracted_root=$1
  openssl_include_dir="$extracted_root/include"
  openssl_multiarch_include_dir="$extracted_root/include/x86_64-linux-gnu"
  openssl_lib_dir="$extracted_root/lib/x86_64-linux-gnu"
  [[ -f "$openssl_include_dir/openssl/ssl.h" ]] || {
    printf 'Pinned archive is missing include/openssl/ssl.h\n' >&2
    return 1
  }
  [[ -f "$openssl_multiarch_include_dir/openssl/opensslconf.h" ]] || {
    printf 'Pinned archive is missing the multiarch opensslconf.h\n' >&2
    return 1
  }
  [[ -f "$openssl_multiarch_include_dir/openssl/configuration.h" ]] || {
    printf 'Pinned archive is missing the multiarch configuration.h\n' >&2
    return 1
  }
  [[ -e "$openssl_lib_dir/libssl.so" && -e "$openssl_lib_dir/libcrypto.so" ]] || {
    printf 'Pinned archive is missing the unversioned OpenSSL libraries\n' >&2
    return 1
  }
}

run_probe() {
  local probe=${1:-}
  local probe_root child=""
  probe_root=$(mktemp -d "${TMPDIR:-/tmp}/bluey-pinky-linux-build.XXXXXX")
  chmod 700 "$probe_root"
  cleanup_probe() {
    local status=$?
    trap - EXIT HUP INT TERM
    if [[ -n "$child" ]] && kill -0 -- "-$child" 2>/dev/null; then
      kill -TERM -- "-$child" 2>/dev/null || true
      for _ in {1..10}; do
        if ! kill -0 -- "-$child" 2>/dev/null; then break; fi
        sleep 1
      done
      if kill -0 -- "-$child" 2>/dev/null; then
        kill -KILL -- "-$child" 2>/dev/null || true
      fi
      wait "$child" 2>/dev/null || true
    fi
    cleanup_root "$probe_root" || exit 1
    exit "$status"
  }
  trap cleanup_probe EXIT
  trap 'exit 129' HUP
  trap 'exit 130' INT
  trap 'exit 143' TERM
  printf '%s\n' "$probe_root"
  case "$probe" in
    success) exit 0 ;;
    failure) exit 7 ;;
    layout)
      mkdir -p "$probe_root/sysroot/include/openssl" \
        "$probe_root/sysroot/include/x86_64-linux-gnu/openssl" \
        "$probe_root/sysroot/lib/x86_64-linux-gnu"
      : >"$probe_root/sysroot/include/openssl/ssl.h"
      : >"$probe_root/sysroot/include/x86_64-linux-gnu/openssl/opensslconf.h"
      : >"$probe_root/sysroot/include/x86_64-linux-gnu/openssl/configuration.h"
      : >"$probe_root/sysroot/lib/x86_64-linux-gnu/libssl.so"
      : >"$probe_root/sysroot/lib/x86_64-linux-gnu/libcrypto.so"
      resolve_openssl_layout "$probe_root/sysroot"
      [[ "$openssl_include_dir" == "$probe_root/sysroot/include" ]]
      [[ "$openssl_multiarch_include_dir" == \
        "$probe_root/sysroot/include/x86_64-linux-gnu" ]]
      [[ "$openssl_lib_dir" == "$probe_root/sysroot/lib/x86_64-linux-gnu" ]]
      exit 0
      ;;
    signal)
      env -i PATH="$PATH" bash -c 'sleep 30' &
      child=$!
      kill -TERM "$$"
      wait "$child"
      exit 1
      ;;
    *) exit 2 ;;
  esac
}

run_self_test() {
  local probe status result
  for probe in success failure layout signal; do
    status=0
    result=$(bash "$script" --probe "$probe") || status=$?
    case "$probe:$status" in
      success:0|failure:7|layout:0|signal:143) ;;
      *) printf 'Cleanup self-test status failed for %s: %s\n' "$probe" "$status" >&2; exit 1 ;;
    esac
    case "$result" in
      */bluey-pinky-linux-build.*) ;;
      *) printf 'Cleanup self-test returned an unexpected root\n' >&2; exit 1 ;;
    esac
    if [[ -e "$result" ]]; then
      printf 'Cleanup self-test residue remains: %s\n' "$result" >&2
      exit 1
    fi
  done
  printf 'Linux build self-test passed (flat sysroot, success, failure, TERM cleanup)\n'
}

if [[ "${1:-}" == "--probe" ]]; then
  run_probe "${2:-}"
fi
if [[ "${1:-}" == "--self-test" ]]; then
  [[ $# -eq 1 ]] || { usage >&2; exit 2; }
  run_self_test
  exit 0
fi

mode=build
source_sha=""
output_dir=""
original_args=("$@")
while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)
      mode=dry-run
      shift
      ;;
    --source-sha)
      [[ $# -ge 2 ]] || { usage >&2; exit 2; }
      source_sha=$2
      shift 2
      ;;
    --output-dir)
      [[ $# -ge 2 ]] || { usage >&2; exit 2; }
      output_dir=$2
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      printf 'Unknown argument: %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

require_command() {
  command -v "$1" >/dev/null 2>&1 || {
    printf 'Required command is unavailable: %s\n' "$1" >&2
    exit 1
  }
}
for command_name in cargo file git python3 rustc rustup scp shasum ssh-keygen tar zig; do
  require_command "$command_name"
done
[[ -f "$scheduler" && ! -L "$scheduler" ]] || {
  printf 'Shared runner scheduler is unavailable or is a symlink\n' >&2
  exit 1
}
[[ -f "$known_hosts" && ! -L "$known_hosts" ]] || {
  printf 'Strict SSH known-hosts file is unavailable or is a symlink\n' >&2
  exit 1
}
ssh-keygen -F "$archive_host" -f "$known_hosts" >/dev/null || {
  printf 'Pinned archive host is absent from the strict known-hosts file\n' >&2
  exit 1
}
cargo zigbuild --help >/dev/null
cargo zigbuild --target "$zigbuild_target" --help >/dev/null
rustup target list --installed | grep -qx "$rust_target" || {
  printf 'Rust target is not installed: %s\n' "$rust_target" >&2
  exit 1
}
zig_target=$(zig cc -target "$zig_cc_target" --version 2>&1)
case "$zig_target" in
  *"Target: x86_64-"*"linux"*"gnu2.39.0"*) ;;
  *)
    printf 'Installed Zig does not accept the required glibc %s target\n' \
      "$glibc_version" >&2
    exit 1
    ;;
esac

readonly head_sha=$(git -C "$repo" rev-parse HEAD)
if [[ -z "$source_sha" ]]; then
  if [[ "$mode" == "build" ]]; then
    printf 'A pinned --source-sha is required for a retained build\n' >&2
    exit 2
  fi
  source_sha=$head_sha
fi
[[ "$source_sha" =~ ^[0-9a-f]{40}$ ]] || {
  printf 'Source SHA must be exactly 40 lowercase hexadecimal characters\n' >&2
  exit 2
}
[[ "$source_sha" == "$head_sha" ]] || {
  printf 'Pinned source does not equal current HEAD (%s)\n' "$head_sha" >&2
  exit 1
}
git -C "$repo" cat-file -e "$source_sha^{commit}"

if [[ -z "$output_dir" ]]; then
  output_dir="$private_build_root/$source_sha"
fi
[[ "$output_dir" == /* ]] || {
  printf 'Final output directory must be absolute\n' >&2
  exit 2
}
case "$output_dir" in
  "$private_build_root"/*) ;;
  *)
    printf 'Final output must remain below %s\n' "$private_build_root" >&2
    exit 2
    ;;
esac

dirty=$(git -C "$repo" status --porcelain --untracked-files=all)
if [[ "$mode" == "dry-run" ]]; then
  printf 'Dry run only; no SSH, build, queue acquisition, or file writes performed.\n'
  printf 'Source SHA: %s\n' "$source_sha"
  printf 'Rust target: %s\n' "$rust_target"
  printf 'Zigbuild target: %s\n' "$zigbuild_target"
  printf 'Final output: %s\n' "$output_dir"
  printf 'Archive: root@%s:%s\n' "$archive_host" "$archive_remote"
  printf 'Archive SHA-256: %s\n' "$archive_sha256"
  printf 'Zig: %s\n' "$(zig version)"
  printf 'Rust: %s\n' "$(rustc --version)"
  if [[ -n "$dirty" ]]; then
    printf 'Clean-worktree gate: WOULD FAIL (commit or remove all tracked/untracked changes)\n'
  elif [[ -e "$output_dir" || -L "$output_dir" ]]; then
    printf 'Fresh-output gate: WOULD FAIL (output already exists)\n'
  else
    printf 'Retained-build admission gates: ready\n'
  fi
  exit 0
fi

if [[ "${BLUEY_PINKY_MAC_HEAVY_LOCKED:-0}" != "1" ]]; then
  exec python3 "$scheduler" with-machine-lock --machine mac-heavy -- \
    /usr/bin/env BLUEY_PINKY_MAC_HEAVY_LOCKED=1 "$script" "${original_args[@]}"
fi

[[ -z "$dirty" ]] || {
  printf 'Retained builds require an exact clean worktree\n' >&2
  exit 1
}
[[ ! -e "$output_dir" && ! -L "$output_dir" ]] || {
  printf 'Refusing to replace existing output: %s\n' "$output_dir" >&2
  exit 1
}

root=$(mktemp -d "${TMPDIR:-/tmp}/bluey-pinky-linux-build.XXXXXX")
child=""
cleanup() {
  local status=$?
  trap - EXIT HUP INT TERM
  if [[ -n "$child" ]] && kill -0 -- "-$child" 2>/dev/null; then
    kill -TERM -- "-$child" 2>/dev/null || true
    for _ in {1..10}; do
      if ! kill -0 -- "-$child" 2>/dev/null; then break; fi
      sleep 1
    done
    if kill -0 -- "-$child" 2>/dev/null; then
      kill -KILL -- "-$child" 2>/dev/null || true
    fi
    wait "$child" 2>/dev/null || true
  fi
  cleanup_root "$root" || exit 1
  exit "$status"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
mkdir -p "$root"/{cargo,zig-global-cache,zig-local-cache,tmp,data,config,runtime,logs,db,source,sysroot,stage}
chmod 700 "$root" "$root/runtime" "$root/stage"

# Build an immutable archive of the exact commit rather than reading a shared
# worktree that another local agent could edit while cargo is running.
git -C "$repo" archive --format=tar --output="$root/source.tar" "$source_sha"
tar -xf "$root/source.tar" -C "$root/source"

ssh_options=(
  -o BatchMode=yes
  -o ConnectTimeout=15
  -o StrictHostKeyChecking=yes
  -o "UserKnownHostsFile=$known_hosts"
)
ssh_environment=(env -i PATH="$PATH" HOME="$HOME")
if [[ -n "${SSH_AUTH_SOCK:-}" ]]; then
  ssh_environment+=(SSH_AUTH_SOCK="$SSH_AUTH_SOCK")
fi
printf 'Fetching pinned OpenSSL sysroot archive over strict SSH...\n'
"${ssh_environment[@]}" scp "${ssh_options[@]}" \
  "root@$archive_host:$archive_remote" "$root/openssl-sysroot.tar.gz"

actual_archive_sha=$(shasum -a 256 "$root/openssl-sysroot.tar.gz" | awk '{print $1}')
[[ "$actual_archive_sha" == "$archive_sha256" ]] || {
  printf 'OpenSSL sysroot archive checksum mismatch\n' >&2
  exit 1
}

# Python's data filter rejects traversal and unsafe links. Special device/FIFO
# entries are rejected explicitly before anything is extracted.
python3 - "$root/openssl-sysroot.tar.gz" "$root/sysroot" <<'PY'
import pathlib
import sys
import tarfile

archive = pathlib.Path(sys.argv[1])
destination = pathlib.Path(sys.argv[2])
with tarfile.open(archive, "r:gz") as source:
    members = source.getmembers()
    for member in members:
        if member.ischr() or member.isblk() or member.isfifo():
            raise SystemExit(f"unsafe special archive member: {member.name}")
        if any(ord(character) < 32 for character in member.name):
            raise SystemExit("archive member contains control characters")
    source.extractall(destination, members=members, filter="data")
PY

sysroot="$root/sysroot"
resolve_openssl_layout "$sysroot"
openssl_cflags="-I$openssl_include_dir -I$openssl_multiarch_include_dir"

readonly source_epoch=$(git -C "$repo" show -s --format=%ct "$source_sha")
printf 'Building bluey-server for %s (glibc %s) in an owned temporary root...\n' \
  "$rust_target" "$glibc_version"
env -i PATH="$PATH" HOME="$HOME" \
  CARGO_TARGET_DIR="$root/cargo" TMPDIR="$root/tmp" \
  ZIG_GLOBAL_CACHE_DIR="$root/zig-global-cache" \
  ZIG_LOCAL_CACHE_DIR="$root/zig-local-cache" \
  XDG_DATA_HOME="$root/data" XDG_CONFIG_HOME="$root/config" \
  XDG_RUNTIME_DIR="$root/runtime" BLUEY_DATA_DIR="$root/data" \
  BLUEY_DB_PATH="$root/db/build.db" BLUEY_LOG_DIR="$root/logs" \
  OPENSSL_DIR="$sysroot" OPENSSL_INCLUDE_DIR="$openssl_include_dir" \
  OPENSSL_LIB_DIR="$openssl_lib_dir" OPENSSL_NO_VENDOR=1 \
  CFLAGS_x86_64_unknown_linux_gnu="$openssl_cflags" \
  PKG_CONFIG_ALLOW_CROSS=1 PKG_CONFIG_SYSROOT_DIR="$sysroot" \
  PKG_CONFIG_LIBDIR="$openssl_lib_dir/pkgconfig:$sysroot/share/pkgconfig" \
  SOURCE_DATE_EPOCH="$source_epoch" \
  cargo zigbuild --offline --locked --release \
    --manifest-path "$root/source/server/Cargo.toml" --target "$zigbuild_target" \
    --bin bluey-server -j 2 &
child=$!
wait "$child"
child=""

built_binary="$root/cargo/$rust_target/release/bluey-server"
[[ -s "$built_binary" ]] || { printf 'Expected Linux binary is missing\n' >&2; exit 1; }
binary_description=$(file -b "$built_binary")
case "$binary_description" in
  *ELF*64-bit*x86-64*) ;;
  *) printf 'Unexpected binary format: %s\n' "$binary_description" >&2; exit 1 ;;
esac

install -m 700 "$built_binary" "$root/stage/bluey-server"
binary_sha=$(shasum -a 256 "$root/stage/bluey-server" | awk '{print $1}')
printf '%s  bluey-server\n' "$binary_sha" >"$root/stage/bluey-server.sha256"
chmod 600 "$root/stage/bluey-server.sha256"
lock_sha=$(shasum -a 256 "$root/source/server/Cargo.lock" | awk '{print $1}')
zig_version=$(zig version)
rust_version=$(rustc --version)
cargo_zigbuild_path=$(command -v cargo-zigbuild)
cargo_zigbuild_sha=$(shasum -a 256 "$cargo_zigbuild_path" | awk '{print $1}')
python3 - "$root/stage/manifest.json" "$source_sha" "$source_epoch" \
  "$rust_target" "$zigbuild_target" "$glibc_version" \
  "$binary_sha" "$binary_description" "$archive_sha256" \
  "$lock_sha" "$zig_version" "$rust_version" "$cargo_zigbuild_sha" <<'PY'
import json
import pathlib
import sys

(
    output,
    source_sha,
    source_epoch,
    rust_target,
    zigbuild_target,
    glibc_version,
    binary_sha,
    binary_description,
    archive_sha,
    lock_sha,
    zig_version,
    rust_version,
    cargo_zigbuild_sha,
) = sys.argv[1:]
manifest = {
    "schema_version": 1,
    "source_sha": source_sha,
    "source_date_epoch": int(source_epoch),
    "target": rust_target,
    "rust_target": rust_target,
    "zigbuild_target": zigbuild_target,
    "minimum_glibc_version": glibc_version,
    "artifact": "bluey-server",
    "artifact_sha256": binary_sha,
    "artifact_format": binary_description,
    "openssl_sysroot_sha256": archive_sha,
    "server_cargo_lock_sha256": lock_sha,
    "zig_version": zig_version,
    "rustc_version": rust_version,
    "cargo_zigbuild_binary_sha256": cargo_zigbuild_sha,
}
pathlib.Path(output).write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
PY
chmod 600 "$root/stage/manifest.json"

mkdir -p "$private_build_root"
chmod 700 "$HOME/.local/state/bluey-pinky-integration" "$private_build_root" 2>/dev/null || true
mkdir -p "$(dirname "$output_dir")"
chmod 700 "$(dirname "$output_dir")"
mv "$root/stage" "$output_dir"
printf 'Retained Linux artifact: %s/bluey-server\n' "$output_dir"
printf 'Artifact SHA-256: %s\n' "$binary_sha"
