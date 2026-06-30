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

mkdir -p "$STAGE_DIR/releases/$VERSION_TAG"
rm -f "$STAGE_DIR/latest.json" "$STAGE_DIR/latest.json.sig" "$STAGE_DIR/install.sh" "$STAGE_DIR/install.ps1"
rm -f "$STAGE_DIR/releases/$VERSION_TAG"/bluey-*.tar.gz
rm -f "$STAGE_DIR/releases/$VERSION_TAG"/SHA256SUMS.txt

cp ops/install/install.sh "$STAGE_DIR/install.sh"
chmod 0644 "$STAGE_DIR/install.sh"
cp ops/install/install.ps1 "$STAGE_DIR/install.ps1"
chmod 0644 "$STAGE_DIR/install.ps1"

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

python3 - "$DIST_DIR" "${artifacts[@]}" <<'PY'
import os
import sys
import tarfile
import zipfile
from pathlib import Path

static_patterns = [
    ("dev flag BLUEY_OVERLAY_CAPTURE_VISIBLE", b"BLUEY_OVERLAY_CAPTURE_VISIBLE"),
    ("dev flag BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE", b"BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE"),
    ("dev flag BLUEY_LOCAL_VISIBLE_OVERLAY", b"BLUEY_LOCAL_VISIBLE_OVERLAY"),
    ("dev flag BLUEY_ALLOW_CAPTURE_VISIBLE_LOCAL", b"BLUEY_ALLOW_CAPTURE_VISIBLE_LOCAL"),
    ("dev flag BLUEY_DEV_OVERLAY", b"BLUEY_DEV_OVERLAY"),
    ("dev flag bluey-local-visible-overlay", b"bluey-local-visible-overlay"),
    ("dev flag bluey-overlay-capture-visible", b"bluey-overlay-capture-visible"),
    ("dev flag bluey-dev-overlay", b"bluey-dev-overlay"),
]

secret_env_names = [
    "OPENAI_API_KEYS",
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEYS",
    "ANTHROPIC_API_KEY",
    "GEMINI_API_KEYS",
    "GEMINI_API_KEY",
    "GOOGLE_API_KEYS",
    "GOOGLE_API_KEY",
    "DEEPSEEK_API_KEYS",
    "DEEPSEEK_API_KEY",
    "ZAI_API_KEYS",
    "ZAI_API_KEY",
    "ZHIPU_API_KEYS",
    "ZHIPU_API_KEY",
    "DEEPGRAM_API_KEYS",
    "DEEPGRAM_API_KEY",
    "BLUEY_WEB_SEARCH_API_KEY",
    "TAVILY_API_KEY",
    "BRAVE_SEARCH_API_KEY",
    "BLUEY_OBJECT_SECRET_ACCESS_KEY",
    "BLUEY_R2_SECRET_ACCESS_KEY",
    "AWS_SECRET_ACCESS_KEY",
    "BLUEY_SQUARE_ACCESS_TOKEN",
    "SQUARE_ACCESS_TOKEN",
    "STRIPE_SECRET_KEY",
]


def secret_patterns_from_env():
    patterns = []
    seen = set()
    for name in secret_env_names:
        raw = os.environ.get(name, "")
        for token in raw.replace("\n", ",").split(","):
            secret = token.strip()
            if len(secret) < 16 or secret in seen:
                continue
            seen.add(secret)
            patterns.append((f"secret value from {name}", secret.encode()))
    return patterns


patterns = static_patterns + secret_patterns_from_env()

dist_dir = Path(sys.argv[1])
failures = []
checked = 0
for pair in sys.argv[2:]:
    _platform, filename = pair.split(":", 1)
    path = dist_dir / filename
    if not path.exists():
        failures.append(f"{filename}: missing artifact")
        continue
    if path.suffix == ".zip":
        with zipfile.ZipFile(path) as zf:
            for info in zf.infolist():
                if info.is_dir():
                    continue
                data = zf.read(info.filename)
                checked += 1
                for label, pattern in patterns:
                    if pattern in data:
                        failures.append(f"{filename}:{info.filename}: {label}")
    elif path.name.endswith(".tar.gz"):
        with tarfile.open(path, "r:gz") as tf:
            for member in tf.getmembers():
                if not member.isfile():
                    continue
                extracted = tf.extractfile(member)
                if extracted is None:
                    continue
                data = extracted.read()
                checked += 1
                for label, pattern in patterns:
                    if pattern in data:
                        failures.append(f"{filename}:{member.name}: {label}")
    else:
        data = path.read_bytes()
        checked += 1
        for label, pattern in patterns:
            if pattern in data:
                failures.append(f"{filename}: {label}")

if failures:
    print("Refusing to publish release artifacts with forbidden dev flags or secret material:", file=sys.stderr)
    for failure in failures:
        print(f"  - {failure}", file=sys.stderr)
    sys.exit(1)

secret_count = len(patterns) - len(static_patterns)
if secret_count:
    print(
        f"Release artifact dev-flag/secret scan passed "
        f"({checked} files checked, {secret_count} configured secret value(s) covered)."
    )
else:
    print(f"Release artifact dev-flag/secret scan passed ({checked} files checked, no configured secrets present).")
PY

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
install_path = os.path.join(stage_dir, "install.sh")
windows_install_path = os.path.join(stage_dir, "install.ps1")

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
    "released_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
    "base_url": public_base.rstrip("/"),
    "install": {
        "url": "install.sh",
        "sha256": install_digest,
        "size_bytes": os.path.getsize(install_path),
    },
    "windows_install": {
        "url": "install.ps1",
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
    ssh "$PUBLISH_HOST" "mkdir -p '$PUBLISH_PATH/releases/$VERSION_TAG'"
    root_files=(
        "$STAGE_DIR/install.sh"
        "$STAGE_DIR/install.ps1"
        "$STAGE_DIR/latest.json"
    )
    if [ -f "$STAGE_DIR/latest.json.sig" ]; then
        root_files+=("$STAGE_DIR/latest.json.sig")
    fi
    rsync -av --chmod=Fu=rw,Fgo=r,Du=rwx,Dgo=rx \
        "${root_files[@]}" \
        "$PUBLISH_HOST:$PUBLISH_PATH/"
    rsync -av --chmod=Fu=rw,Fgo=r,Du=rwx,Dgo=rx \
        "$STAGE_DIR/releases/$VERSION_TAG/" \
        "$PUBLISH_HOST:$PUBLISH_PATH/releases/$VERSION_TAG/"
    ssh "$PUBLISH_HOST" "chmod -R u=rwX,go=rX '$PUBLISH_PATH/install.sh' '$PUBLISH_PATH/install.ps1' '$PUBLISH_PATH/latest.json' '$PUBLISH_PATH/latest.json.sig' '$PUBLISH_PATH/releases/$VERSION_TAG'"
    echo "Published to $PUBLISH_HOST:$PUBLISH_PATH"
fi
