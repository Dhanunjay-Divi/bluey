#!/usr/bin/env bash
# Deterministic fixture for the signed release staging and visibility contract.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

TMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/bluey-publish-test.XXXXXX")"
cleanup() {
    rm -rf "$TMP_ROOT"
}
trap cleanup EXIT

DIST_DIR="$TMP_ROOT/dist"
STAGE_DIR="$TMP_ROOT/stage"
FIRST_STAGE="$TMP_ROOT/stage-first"
PUBLIC_DIR="$TMP_ROOT/public"
FAKE_BIN="$TMP_ROOT/bin"
EVENT_LOG="$TMP_ROOT/events.log"
SIGNING_KEY="$TMP_ROOT/release-ed25519.pem"
VERSION="9.9.9"
mkdir -p "$DIST_DIR" "$FAKE_BIN"

python3 - \
    "$DIST_DIR/bluey-$VERSION-darwin-arm64.tar.gz" \
    "$DIST_DIR/bluey-$VERSION-windows-x86_64.zip" <<'PY'
import io
import sys
import tarfile
import zipfile

with tarfile.open(sys.argv[1], "w:gz") as archive:
    payload = b"fixture-bluey-binary\n"
    member = tarfile.TarInfo("bin/bluey")
    member.mode = 0o755
    member.mtime = 1_700_000_000
    member.size = len(payload)
    archive.addfile(member, io.BytesIO(payload))

with zipfile.ZipFile(sys.argv[2], "w") as archive:
    archive.writestr("bin/bluey.exe", b"fixture-bluey-windows-binary\n")
PY
openssl genpkey -algorithm ED25519 -out "$SIGNING_KEY" >/dev/null 2>&1

cat > "$FAKE_BIN/ssh" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
shift
if [ "$#" -eq 1 ]; then
    FAKE_REMOTE=1 bash -c "$1"
else
    FAKE_REMOTE=1 "$@"
fi
SH

cat > "$FAKE_BIN/rsync" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
previous=""
last=""
for argument in "$@"; do
    previous="$last"
    last="$argument"
done
source_path="$previous"
destination="${last#*:}"
printf 'rsync:%s -> %s\n' "$source_path" "$destination" >> "$BLUEY_TEST_EVENT_LOG"
if [[ "$source_path" == */ ]]; then
    mkdir -p "$destination"
    cp -R "${source_path%/}/." "$destination/"
else
    mkdir -p "$(dirname "$destination")"
    cp "$source_path" "$destination"
fi
SH

cat > "$FAKE_BIN/mv" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
if [ "${FAKE_REMOTE:-0}" = "1" ]; then
    printf 'mv:%s\n' "$*" >> "$BLUEY_TEST_EVENT_LOG"
fi
exec /bin/mv "$@"
SH

cat > "$FAKE_BIN/curl" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
output=""
head_only=0
url=""
while [ "$#" -gt 0 ]; do
    case "$1" in
        -o)
            output="$2"
            shift 2
            ;;
        -*I*)
            head_only=1
            shift
            ;;
        -*)
            shift
            ;;
        http://*|https://*)
            url="$1"
            shift
            ;;
        *)
            shift
            ;;
    esac
done
[ -n "$url" ] || exit 2
relative="${url#*://}"
relative="${relative#*/}"
path="$BLUEY_TEST_PUBLIC_DIR/$relative"
[ -f "$path" ] || exit 22
if [ "$head_only" = "1" ]; then
    case "$relative" in
        *.sh) content_type="application/x-shellscript" ;;
        *.ps1) content_type="application/x-powershell" ;;
        *) content_type="application/octet-stream" ;;
    esac
    printf 'HTTP/1.1 200 OK\r\ncontent-type: %s\r\n\r\n' "$content_type"
elif [ -n "$output" ]; then
    cp "$path" "$output"
else
    cat "$path"
fi
SH
chmod +x "$FAKE_BIN/ssh" "$FAKE_BIN/rsync" "$FAKE_BIN/mv" "$FAKE_BIN/curl"

env \
    PATH="$FAKE_BIN:$PATH" \
    BLUEY_DIST_DIR="$DIST_DIR" \
    BLUEY_RELEASE_STAGE="$STAGE_DIR" \
    BLUEY_RELEASE_SIGNING_KEY_FILE="$SIGNING_KEY" \
    BLUEY_RELEASE_REQUIRED_PLATFORMS="darwin-arm64" \
    BLUEY_PUBLIC_BASE="https://fixture.invalid" \
    BLUEY_TEST_EVENT_LOG="$EVENT_LOG" \
    BLUEY_VERSION="$VERSION" \
    PUBLISH_DO=1 \
    PUBLISH_HOST="fixture-host" \
    PUBLISH_PATH="$PUBLIC_DIR" \
    SOURCE_DATE_EPOCH=1700000000 \
    bash scripts/publish-bluey-release.sh >/dev/null

env \
    PATH="$FAKE_BIN:$PATH" \
    BLUEY_PUBLIC_BASE="https://fixture.invalid" \
    BLUEY_RELEASE_SIGNING_KEY_FILE="$SIGNING_KEY" \
    BLUEY_RELEASE_VERIFY_PLATFORM="windows-x86_64" \
    BLUEY_TEST_PUBLIC_DIR="$PUBLIC_DIR" \
    bash scripts/bluey-release-live-verify.sh "$VERSION" >/dev/null

cp -R "$STAGE_DIR" "$FIRST_STAGE"
env \
    BLUEY_DIST_DIR="$DIST_DIR" \
    BLUEY_RELEASE_STAGE="$STAGE_DIR" \
    BLUEY_RELEASE_SIGNING_KEY_FILE="$SIGNING_KEY" \
    BLUEY_PUBLIC_BASE="https://fixture.invalid" \
    BLUEY_VERSION="$VERSION" \
    SOURCE_DATE_EPOCH=1700000000 \
    bash scripts/publish-bluey-release.sh >/dev/null

diff -qr "$FIRST_STAGE" "$STAGE_DIR" >/dev/null
(
    cd "$STAGE_DIR/releases/v$VERSION"
    shasum -a 256 -c SHA256SUMS.txt >/dev/null
)

python3 - "$STAGE_DIR" "$PUBLIC_DIR" "$EVENT_LOG" "$VERSION" <<'PY'
import hashlib
import json
from pathlib import Path
import sys

stage = Path(sys.argv[1])
public = Path(sys.argv[2])
event_log = Path(sys.argv[3])
version = sys.argv[4]
release = stage / "releases" / f"v{version}"
manifest = json.loads((stage / "latest.json").read_text(encoding="utf-8"))

for key, filename in (("install", "install.sh"), ("windows_install", "install.ps1")):
    entry = manifest[key]
    expected_url = f"releases/v{version}/{filename}"
    assert entry["url"] == expected_url, (key, entry["url"])
    content = (release / filename).read_bytes()
    assert entry["sha256"] == hashlib.sha256(content).hexdigest()
    assert entry["size_bytes"] == len(content)
    assert (stage / filename).read_bytes() == content
    assert (public / filename).read_bytes() == content

checksums = (release / "SHA256SUMS.txt").read_text(encoding="utf-8")
assert "  install.sh\n" in checksums
assert "  install.ps1\n" in checksums

events = event_log.read_text(encoding="utf-8").splitlines()
signature = next(i for i, line in enumerate(events) if line.startswith("mv:") and line.endswith("/latest.json.sig"))
manifest_move = next(i for i, line in enumerate(events) if line.startswith("mv:") and line.endswith("/latest.json"))
unix_alias = next(i for i, line in enumerate(events) if line.startswith("mv:") and line.endswith("/install.sh"))
windows_alias = next(i for i, line in enumerate(events) if line.startswith("mv:") and line.endswith("/install.ps1"))
assert signature < manifest_move < unix_alias
assert signature < manifest_move < windows_alias
PY

echo "Release publisher fixture passed (deterministic stage, immutable installers, checksums, visibility order)."
