#!/usr/bin/env bash
# Build the static files Bluey clients need for install + auto-update.
#
# Dry run:
#   scripts/publish-bluey-release.sh
#
# Publish:
#   BLUEY_RELEASE_SIGNING_KEY_FILE=/path/to/ed25519-release-key.pem \
#     PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 scripts/publish-bluey-release.sh
#
# Public key for CLI builds:
#   openssl pkey -in /path/to/ed25519-release-key.pem -pubout -outform DER \
#     | tail -c 32 | base64 | tr -d '\n'
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
SIGNING_KEY_FILE="${BLUEY_RELEASE_SIGNING_KEY_FILE:-}"
RELEASE_MIRROR_DESTINATION="${BLUEY_RELEASE_MIRROR_DESTINATION:-}"
RELEASE_MIRROR_ENDPOINT_URL="${BLUEY_RELEASE_MIRROR_ENDPOINT_URL:-${BLUEY_BACKUP_S3_ENDPOINT_URL:-}}"
SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-$(git show -s --format=%ct HEAD)}"

case "$SOURCE_DATE_EPOCH" in
    *[!0-9]* | "")
        echo "SOURCE_DATE_EPOCH must be an integer Unix timestamp" >&2
        exit 1
        ;;
esac
if [ -z "$STAGE_DIR" ] || [ "$STAGE_DIR" = "/" ] || [ "$STAGE_DIR" = "$ROOT" ]; then
    echo "Refusing unsafe BLUEY_RELEASE_STAGE: $STAGE_DIR" >&2
    exit 1
fi

# A release stage is an immutable snapshot, not an incremental cache. Starting
# empty prevents a removed platform ZIP or stale checksum from leaking into a
# later manifest.
rm -rf "$STAGE_DIR"
mkdir -p "$STAGE_DIR/releases/$VERSION_TAG"

cp ops/install/install.sh "$STAGE_DIR/install.sh"
chmod 0644 "$STAGE_DIR/install.sh"
cp ops/install/install.ps1 "$STAGE_DIR/install.ps1"
chmod 0644 "$STAGE_DIR/install.ps1"
cp "$STAGE_DIR/install.sh" "$STAGE_DIR/releases/$VERSION_TAG/install.sh"
cp "$STAGE_DIR/install.ps1" "$STAGE_DIR/releases/$VERSION_TAG/install.ps1"

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

required_platforms="${BLUEY_RELEASE_REQUIRED_PLATFORMS:-}"
if [ "${PUBLISH_DO:-0}" = "1" ] && [ -z "$required_platforms" ]; then
    required_platforms="darwin-arm64 darwin-universal darwin-x86_64 windows-x86_64"
fi
for required_platform in $required_platforms; do
    found=0
    for pair in "${artifacts[@]}"; do
        if [ "${pair%%:*}" = "$required_platform" ]; then
            found=1
            break
        fi
    done
    if [ "$found" != "1" ]; then
        echo "Required release platform is missing: $required_platform" >&2
        exit 1
    fi
done

artifact_paths=()
for pair in "${artifacts[@]}"; do
    filename="${pair#*:}"
    artifact_paths+=("$DIST_DIR/$filename")
done
python3 scripts/check-release-artifact-contents.py "${artifact_paths[@]}"

(
    cd "$STAGE_DIR/releases/$VERSION_TAG"
    for file in bluey-* install.sh install.ps1; do
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

python3 - "$VERSION" "$VERSION_TAG" "$STAGE_DIR" "$PUBLIC_BASE" "$SOURCE_DATE_EPOCH" "${artifacts[@]}" <<'PY'
import hashlib
import json
import os
import sys
from datetime import datetime, timezone

version, version_tag, stage_dir, public_base, source_date_epoch, *pairs = sys.argv[1:]
platforms = {}
release_dir = os.path.join(stage_dir, "releases", version_tag)
install_path = os.path.join(release_dir, "install.sh")
windows_install_path = os.path.join(release_dir, "install.ps1")

with open(install_path, "rb") as fh:
    install_digest = hashlib.sha256(fh.read()).hexdigest()
with open(windows_install_path, "rb") as fh:
    windows_install_digest = hashlib.sha256(fh.read()).hexdigest()

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
    "released_at": datetime.fromtimestamp(
        int(source_date_epoch), timezone.utc
    ).isoformat().replace("+00:00", "Z"),
    "base_url": public_base.rstrip("/"),
    "install": {
        "url": f"releases/{version_tag}/install.sh",
        "sha256": install_digest,
        "size_bytes": os.path.getsize(install_path),
    },
    "windows_install": {
        "url": f"releases/{version_tag}/install.ps1",
        "sha256": windows_install_digest,
        "size_bytes": os.path.getsize(windows_install_path),
    },
    "platforms": dict(sorted(platforms.items())),
    "release_notes_url": f"releases/{version_tag}/RELEASE.md",
}

with open(os.path.join(stage_dir, "latest.json"), "w", encoding="utf-8") as fh:
    json.dump(manifest, fh, indent=2)
    fh.write("\n")
PY

if [ -n "$SIGNING_KEY_FILE" ]; then
    if ! command -v openssl >/dev/null 2>&1; then
        echo "openssl is required to sign latest.json" >&2
        exit 1
    fi
    if [ ! -f "$SIGNING_KEY_FILE" ]; then
        echo "BLUEY_RELEASE_SIGNING_KEY_FILE does not exist: $SIGNING_KEY_FILE" >&2
        exit 1
    fi
    sig_raw="$(mktemp)"
    openssl pkeyutl -sign -rawin -inkey "$SIGNING_KEY_FILE" \
        -in "$STAGE_DIR/latest.json" -out "$sig_raw"
    base64 < "$sig_raw" | tr -d '\n' > "$STAGE_DIR/latest.json.sig"
    rm -f "$sig_raw"
elif [ "${PUBLISH_DO:-0}" = "1" ] && [ "${BLUEY_RELEASE_ALLOW_UNSIGNED:-0}" != "1" ]; then
    echo "Refusing to publish unsigned latest.json." >&2
    echo "Set BLUEY_RELEASE_SIGNING_KEY_FILE to an Ed25519 PEM key, or BLUEY_RELEASE_ALLOW_UNSIGNED=1 for a local/dev-only publish." >&2
    exit 1
else
    echo "warning: latest.json is not signed; publish is disabled unless BLUEY_RELEASE_ALLOW_UNSIGNED=1" >&2
fi

# Release artifacts may inherit restrictive local permissions, especially zips
# copied back from Windows machines. Keep staged files web-readable before rsync
# and repeat the chmod remotely after upload so Caddy never falls through to the
# SPA index for unreadable artifacts.
chmod -R u=rwX,go=rX "$STAGE_DIR"

echo "Prepared Bluey release web files:"
find "$STAGE_DIR" -maxdepth 3 -type f | sort | sed "s#^$ROOT/##"

if [ "${PUBLISH_DO:-0}" = "1" ]; then
    : "${PUBLISH_HOST:?Set PUBLISH_HOST, e.g. root@165.227.77.152}"
    publish_nonce="${VERSION_TAG#v}-$$"
    remote_release="$PUBLISH_PATH/releases/$VERSION_TAG"
    remote_incoming="$PUBLISH_PATH/releases/.${VERSION_TAG}.incoming-${publish_nonce}"

    # Publish and verify immutable artifacts before exposing a manifest that
    # references them. Existing version directories are immutable: an
    # idempotent re-publish must be byte-for-byte identical or fail closed.
    ssh "$PUBLISH_HOST" \
        "mkdir -p '$PUBLISH_PATH/releases' && rm -rf '$remote_incoming' && mkdir -p '$remote_incoming'"
    rsync -av --chmod=Fu=rw,Fgo=r,Du=rwx,Dgo=rx \
        "$STAGE_DIR/releases/$VERSION_TAG/" \
        "$PUBLISH_HOST:$remote_incoming/"
    ssh "$PUBLISH_HOST" bash -s -- "$remote_incoming" "$remote_release" <<'REMOTE_RELEASE'
set -euo pipefail
incoming="$1"
release="$2"
(
    cd "$incoming"
    sha256sum -c SHA256SUMS.txt
)
if [ -e "$release" ]; then
    (
        cd "$release"
        sha256sum -c SHA256SUMS.txt
    )
    if ! diff -qr "$incoming" "$release" >/dev/null; then
        echo "Refusing to mutate existing immutable release: $release" >&2
        exit 1
    fi
    rm -rf "$incoming"
else
    mv "$incoming" "$release"
fi
chmod -R u=rwX,go=rX "$release"
REMOTE_RELEASE

    # Stage every root file under a private temporary name. The signed manifest
    # references only the immutable installer copies above, never these mutable
    # root convenience aliases.
    install_tmp="$PUBLISH_PATH/.install.sh.${publish_nonce}.tmp"
    install_ps_tmp="$PUBLISH_PATH/.install.ps1.${publish_nonce}.tmp"
    manifest_tmp="$PUBLISH_PATH/.latest.json.${publish_nonce}.tmp"
    signature_tmp="$PUBLISH_PATH/.latest.json.sig.${publish_nonce}.tmp"
    rsync -a "$STAGE_DIR/install.sh" "$PUBLISH_HOST:$install_tmp"
    rsync -a "$STAGE_DIR/install.ps1" "$PUBLISH_HOST:$install_ps_tmp"
    rsync -a "$STAGE_DIR/latest.json" "$PUBLISH_HOST:$manifest_tmp"
    signature_present=0
    if [ -f "$STAGE_DIR/latest.json.sig" ]; then
        signature_present=1
        rsync -a "$STAGE_DIR/latest.json.sig" "$PUBLISH_HOST:$signature_tmp"
    fi
    ssh "$PUBLISH_HOST" bash -s -- \
        "$PUBLISH_PATH" \
        "$install_tmp" \
        "$install_ps_tmp" \
        "$manifest_tmp" \
        "$signature_tmp" \
        "$signature_present" <<'REMOTE_ROOT'
set -euo pipefail
root="$1"
install_tmp="$2"
install_ps_tmp="$3"
manifest_tmp="$4"
signature_tmp="$5"
signature_present="$6"
chmod 0644 "$install_tmp" "$install_ps_tmp" "$manifest_tmp"
if [ "$signature_present" = "1" ]; then
    chmod 0644 "$signature_tmp"
    mv -f "$signature_tmp" "$root/latest.json.sig"
else
    rm -f "$root/latest.json.sig"
fi
mv -f "$manifest_tmp" "$root/latest.json"
# Replace curl/irm convenience aliases only after latest.json no longer refers
# to the previous root bytes. A prior manifest can therefore never checksum-pin
# a newly replaced mutable installer.
mv -f "$install_tmp" "$root/install.sh"
mv -f "$install_ps_tmp" "$root/install.ps1"
REMOTE_ROOT
    echo "Published to $PUBLISH_HOST:$PUBLISH_PATH"

    if [ -n "$RELEASE_MIRROR_DESTINATION" ]; then
        if ! command -v aws >/dev/null 2>&1; then
            echo "BLUEY_RELEASE_MIRROR_DESTINATION is set but aws CLI is not installed" >&2
            exit 1
        fi
        mirror_args=()
        if [ -n "$RELEASE_MIRROR_ENDPOINT_URL" ]; then
            mirror_args+=(--endpoint-url "$RELEASE_MIRROR_ENDPOINT_URL")
        fi
        mirror_root="${RELEASE_MIRROR_DESTINATION%/}"
        # The mirror follows the same trust-boundary order as the primary host:
        # immutable release first, detached signature then manifest, and root
        # convenience aliases only after the old manifest is no longer live.
        aws "${mirror_args[@]}" s3 sync "$STAGE_DIR/releases/$VERSION_TAG/" "$mirror_root/releases/$VERSION_TAG/" --quiet
        if [ -f "$STAGE_DIR/latest.json.sig" ]; then
            aws "${mirror_args[@]}" s3 cp "$STAGE_DIR/latest.json.sig" "$mirror_root/latest.json.sig" --quiet
        fi
        aws "${mirror_args[@]}" s3 cp "$STAGE_DIR/latest.json" "$mirror_root/latest.json" --quiet
        aws "${mirror_args[@]}" s3 cp "$STAGE_DIR/install.sh" "$mirror_root/install.sh" --quiet
        aws "${mirror_args[@]}" s3 cp "$STAGE_DIR/install.ps1" "$mirror_root/install.ps1" --quiet
        echo "Mirrored release files to $mirror_root"
    fi
fi
