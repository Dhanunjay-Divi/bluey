#!/usr/bin/env bash
# Build the static files Bluey clients need for install + auto-update.
#
# Dry run:
#   scripts/publish-bluey-release.sh
#
# Publish:
#   PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 scripts/publish-bluey-release.sh
#
# The script intentionally stages first, then rsyncs. That keeps local review
# easy and avoids partially-generated manifests.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

VERSION="${BLUEY_VERSION:-$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')}"
VERSION_TAG="v${VERSION#v}"
DIST_DIR="${BLUEY_DIST_DIR:-$ROOT/dist}"
STAGE_DIR="${BLUEY_RELEASE_STAGE:-$DIST_DIR/publish-bluey-sh}"
PUBLIC_BASE="${BLUEY_PUBLIC_BASE:-https://bluey.sh}"
PUBLISH_PATH="${PUBLISH_PATH:-/var/www/bluey}"

mkdir -p "$STAGE_DIR/releases/$VERSION_TAG"
rm -f "$STAGE_DIR/latest.json" "$STAGE_DIR/install.sh"
rm -f "$STAGE_DIR/releases/$VERSION_TAG"/bluey-*.tar.gz
rm -f "$STAGE_DIR/releases/$VERSION_TAG"/SHA256SUMS.txt

cp ops/install/install.sh "$STAGE_DIR/install.sh"
chmod 0644 "$STAGE_DIR/install.sh"

artifacts=()
for platform in darwin-arm64 darwin-universal darwin-x86_64 windows-x86_64 linux-x86_64; do
    case "$platform" in
        windows-*) artifact="$DIST_DIR/bluey-${VERSION#v}-$platform.zip" ;;
        *) artifact="$DIST_DIR/bluey-${VERSION#v}-$platform.tar.gz" ;;
    esac
    if [ -f "$artifact" ]; then
        cp "$artifact" "$STAGE_DIR/releases/$VERSION_TAG/"
        artifacts+=("$platform:$(basename "$artifact")")
    fi
done

if [ "${#artifacts[@]}" -eq 0 ]; then
    echo "No release artifacts found in $DIST_DIR for $VERSION" >&2
    echo "Expected at least dist/bluey-${VERSION#v}-darwin-arm64.tar.gz" >&2
    exit 1
fi

(
    cd "$STAGE_DIR/releases/$VERSION_TAG"
    for file in bluey-*; do
        [ -f "$file" ] || continue
        shasum -a 256 "$file"
    done > SHA256SUMS.txt
)

RELEASE_NOTES="docs/release/RELEASE-${VERSION_TAG}.md"
if [ ! -f "$RELEASE_NOTES" ]; then
    RELEASE_NOTES="docs/release/RELEASE-${VERSION#v}.md"
fi
if [ -f "$RELEASE_NOTES" ]; then
    cp "$RELEASE_NOTES" "$STAGE_DIR/releases/$VERSION_TAG/RELEASE.md"
else
    cat > "$STAGE_DIR/releases/$VERSION_TAG/RELEASE.md" <<EOF
# Bluey ${VERSION#v}

See https://bluey.sh for the latest Bluey release notes.
EOF
fi

python3 - "$VERSION" "$VERSION_TAG" "$STAGE_DIR" "$PUBLIC_BASE" "${artifacts[@]}" <<'PY'
import hashlib
import json
import os
import sys
from datetime import datetime, timezone

version, version_tag, stage_dir, public_base, *pairs = sys.argv[1:]
platforms = {}
release_dir = os.path.join(stage_dir, "releases", version_tag)

for pair in pairs:
    platform, filename = pair.split(":", 1)
    path = os.path.join(release_dir, filename)
    with open(path, "rb") as fh:
        digest = hashlib.sha256(fh.read()).hexdigest()
    platforms[platform] = {
        "url": f"releases/{version_tag}/{filename}",
        "sha256": digest,
        "size_bytes": os.path.getsize(path),
    }

manifest = {
    "version": version.lstrip("v"),
    "released_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
    "base_url": public_base.rstrip("/"),
    "platforms": dict(sorted(platforms.items())),
    "release_notes_url": f"releases/{version_tag}/RELEASE.md",
}

with open(os.path.join(stage_dir, "latest.json"), "w", encoding="utf-8") as fh:
    json.dump(manifest, fh, indent=2)
    fh.write("\n")
PY

echo "Prepared Bluey release web files:"
find "$STAGE_DIR" -maxdepth 3 -type f | sort | sed "s#^$ROOT/##"

if [ "${PUBLISH_DO:-0}" = "1" ]; then
    : "${PUBLISH_HOST:?Set PUBLISH_HOST, e.g. root@165.227.77.152}"
    ssh "$PUBLISH_HOST" "mkdir -p '$PUBLISH_PATH/releases/$VERSION_TAG'"
    rsync -av --chmod=Fu=rw,Fgo=r,Du=rwx,Dgo=rx \
        "$STAGE_DIR/install.sh" \
        "$STAGE_DIR/latest.json" \
        "$PUBLISH_HOST:$PUBLISH_PATH/"
    rsync -av --chmod=Fu=rw,Fgo=r,Du=rwx,Dgo=rx \
        "$STAGE_DIR/releases/$VERSION_TAG/" \
        "$PUBLISH_HOST:$PUBLISH_PATH/releases/$VERSION_TAG/"
    echo "Published to $PUBLISH_HOST:$PUBLISH_PATH"
fi
