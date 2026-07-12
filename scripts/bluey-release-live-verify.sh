#!/usr/bin/env bash
# Verify the live Bluey download manifest and release artifact.
#
# Required for production release sign-off:
#   BLUEY_RELEASE_PUBKEY_FILE=/secure/bluey-release-ed25519.pub.pem \
#     scripts/bluey-release-live-verify.sh 0.1.50
#
# Local operator convenience, when only the signing key is available:
#   BLUEY_RELEASE_SIGNING_KEY_FILE=/secure/bluey-release-ed25519.pem \
#     scripts/bluey-release-live-verify.sh 0.1.50

set -euo pipefail

BASE_URL="${BLUEY_PUBLIC_BASE:-https://bluey.sh}"
EXPECTED_VERSION="${1:-${BLUEY_EXPECTED_VERSION:-}}"
PUBKEY_FILE="${BLUEY_RELEASE_PUBKEY_FILE:-}"
SIGNING_KEY_FILE="${BLUEY_RELEASE_SIGNING_KEY_FILE:-}"
PLATFORM="${BLUEY_RELEASE_VERIFY_PLATFORM:-darwin-arm64}"
TMP_DIR="$(mktemp -d)"

cleanup() {
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT

fail() {
    printf 'fail: %s\n' "$*" >&2
    exit 1
}

ok() {
    printf 'ok: %s\n' "$*"
}

need_cmd() {
    command -v "$1" >/dev/null 2>&1 || fail "$1 is required"
}

need_cmd curl
need_cmd python3
need_cmd openssl
need_cmd shasum

manifest="$TMP_DIR/latest.json"
sig_b64="$TMP_DIR/latest.json.sig.b64"
sig_raw="$TMP_DIR/latest.json.sig"
pubkey="$TMP_DIR/release-pub.pem"

curl -fsSL "$BASE_URL/latest.json" -o "$manifest"
curl -fsSL "$BASE_URL/latest.json.sig" -o "$sig_b64"

if base64 -D -i "$sig_b64" -o "$sig_raw" >/dev/null 2>&1; then
    :
elif base64 -d "$sig_b64" > "$sig_raw" 2>/dev/null; then
    :
else
    fail "could not decode latest.json.sig"
fi

if [ -n "$PUBKEY_FILE" ]; then
    [ -f "$PUBKEY_FILE" ] || fail "BLUEY_RELEASE_PUBKEY_FILE does not exist: $PUBKEY_FILE"
    cp "$PUBKEY_FILE" "$pubkey"
elif [ -n "$SIGNING_KEY_FILE" ]; then
    [ -f "$SIGNING_KEY_FILE" ] || fail "BLUEY_RELEASE_SIGNING_KEY_FILE does not exist: $SIGNING_KEY_FILE"
    openssl pkey -in "$SIGNING_KEY_FILE" -pubout -out "$pubkey" >/dev/null 2>&1
else
    fail "set BLUEY_RELEASE_PUBKEY_FILE or BLUEY_RELEASE_SIGNING_KEY_FILE to verify latest.json.sig"
fi

openssl pkeyutl -verify -rawin -pubin -inkey "$pubkey" \
    -sigfile "$sig_raw" -in "$manifest" >/dev/null
ok "latest.json signature verified"

version="$(python3 - "$manifest" <<'PY'
import json, sys
with open(sys.argv[1], "r", encoding="utf-8") as fh:
    data = json.load(fh)
print(data["version"])
PY
)"
ok "live version $version"

if [ -n "$EXPECTED_VERSION" ] && [ "$version" != "${EXPECTED_VERSION#v}" ]; then
    fail "expected version ${EXPECTED_VERSION#v}, got $version"
fi

python3 - "$manifest" "$PLATFORM" <<'PY' > "$TMP_DIR/artifact.env"
import json, sys
with open(sys.argv[1], "r", encoding="utf-8") as fh:
    data = json.load(fh)
platform = sys.argv[2]
artifact = data["platforms"].get(platform)
if not artifact:
    raise SystemExit(f"missing platform in manifest: {platform}")
print(f"ARTIFACT_URL={data['base_url'].rstrip('/')}/{artifact['url']}")
print(f"ARTIFACT_SHA={artifact['sha256']}")
print(f"ARTIFACT_SIZE={artifact.get('size_bytes', 0)}")
PY
# shellcheck disable=SC1090
. "$TMP_DIR/artifact.env"

check_content_type() {
    local path="$1"
    local expected="$2"
    local headers
    headers="$(curl -fsSI "$BASE_URL/$path")"
    printf '%s\n' "$headers" | grep -qi "content-type: $expected" \
        || fail "$path did not return content-type $expected"
    ok "$path content-type $expected"
}

check_content_type install.sh application/x-shellscript
check_content_type install.ps1 application/x-powershell

archive="$TMP_DIR/bluey-release"
curl -fsSL "$ARTIFACT_URL" -o "$archive"
actual_sha="$(shasum -a 256 "$archive" | awk '{print $1}')"
[ "$actual_sha" = "$ARTIFACT_SHA" ] || fail "artifact sha mismatch: $actual_sha != $ARTIFACT_SHA"
ok "$PLATFORM artifact sha verified"

if [ "$PLATFORM" = "darwin-arm64" ] && file "$archive" | grep -qi 'gzip compressed'; then
    mkdir -p "$TMP_DIR/unpack"
    tar -xzf "$archive" -C "$TMP_DIR/unpack"
    "$TMP_DIR/unpack/bin/bluey" --version | grep -F "bluey $version" >/dev/null \
        || fail "bluey CLI version mismatch"
    "$TMP_DIR/unpack/bin/bluey-daemon" --version | grep -F "bluey-daemon $version" >/dev/null \
        || fail "bluey-daemon version mismatch"
    [ -x "$TMP_DIR/unpack/bin/termb" ] || fail "termb daemon identity missing from artifact"
    "$TMP_DIR/unpack/bin/termb" --version | grep -F "bluey-daemon $version" >/dev/null \
        || fail "termb daemon identity version mismatch"
    [ -x "$TMP_DIR/unpack/bin/Terminal" ] || fail "Terminal daemon identity missing from artifact"
    "$TMP_DIR/unpack/bin/Terminal" --version | grep -F "bluey-daemon $version" >/dev/null \
        || fail "Terminal daemon identity version mismatch"
    [ -x "$TMP_DIR/unpack/bin/hostovb" ] || fail "hostovb helper missing from artifact"
    [ -x "$TMP_DIR/unpack/bin/host-overlay" ] || fail "host-overlay helper missing from artifact"
    [ -x "$TMP_DIR/unpack/bin/adriverb" ] || fail "adriverb helper missing from artifact"
    [ -x "$TMP_DIR/unpack/bin/audio-driver" ] || fail "audio-driver helper missing from artifact"
    if command -v strings >/dev/null 2>&1 && command -v rg >/dev/null 2>&1; then
        for binary in "$TMP_DIR/unpack/bin/bluey-daemon" "$TMP_DIR/unpack/bin/termb" "$TMP_DIR/unpack/bin/Terminal"; do
            if strings "$binary" |
                rg "BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE|BLUEY_DEV_OVERLAY|capture-visible|overlay_capture_visible" >/dev/null
            then
                fail "$(basename "$binary") contains dev capture-visible markers"
            fi
        done
    fi
    ok "unpacked $PLATFORM binaries report $version"
else
    ok "artifact verified; binary unpack/version smoke skipped for $PLATFORM"
fi
