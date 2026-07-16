#!/usr/bin/env bash
# Manual Bluey.sh deploy from a local or cloud machine.
#
# This is intentionally not tied to GitHub Actions. It keeps the static web
# deploy and release-artifact deploy separate so a web rsync cannot delete
# installer manifests or immutable release directories.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

PUBLISH_HOST="${PUBLISH_HOST:-root@165.227.77.152}"
PUBLISH_PATH="${PUBLISH_PATH:-/var/www/bluey}"
CURL_BIN="${CURL_BIN:-curl}"
VERSION="${BLUEY_VERSION:-$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')}"
PUBLIC_BASE="${BLUEY_PUBLIC_BASE:-https://bluey.sh}"
RELEASE_SIGNING_KEY_FILE="${BLUEY_RELEASE_SIGNING_KEY_FILE:-}"
ALLOW_UNSIGNED="${BLUEY_RELEASE_ALLOW_UNSIGNED:-0}"

if [ -z "$RELEASE_SIGNING_KEY_FILE" ]; then
    if [ "$ALLOW_UNSIGNED" != "1" ]; then
        echo "[manual-deploy] production deploy requires BLUEY_RELEASE_SIGNING_KEY_FILE so the published release can be signed and verified live" >&2
        echo "[manual-deploy] Set BLUEY_RELEASE_ALLOW_UNSIGNED=1 for a local/dev-only publish only" >&2
        exit 1
    fi
    if [ "${PUBLIC_BASE%/}" = "https://bluey.sh" ] || [ "$PUBLISH_HOST" = "root@165.227.77.152" ]; then
        echo "[manual-deploy] refusing the unsigned local/dev override for the Bluey production host" >&2
        exit 1
    fi
fi

if ! command -v "$CURL_BIN" >/dev/null 2>&1; then
    if [ -x /usr/bin/curl ]; then
        CURL_BIN="/usr/bin/curl"
    else
        echo "[manual-deploy] curl not found; set CURL_BIN=/path/to/curl" >&2
        exit 1
    fi
fi

echo "[manual-deploy] host=$PUBLISH_HOST path=$PUBLISH_PATH"

echo "[manual-deploy] syncing versioned/static assets before HTML"
rsync -av \
    --exclude '.DS_Store' \
    --exclude '._*' \
    --exclude '*.html' \
    --exclude '/backups/***' \
    --exclude '/releases/***' \
    --exclude '/install.sh' \
    --exclude '/install.ps1' \
    --exclude '/latest.json' \
    --exclude '/latest.json.sig' \
    --exclude 'index.html.bak-*' \
    --exclude 'latest.json.bak-*' \
    web/ "$PUBLISH_HOST:$PUBLISH_PATH/"

# HTML is the visibility boundary for hashed frontend assets. Move it only
# after every referenced asset is present, and retain old hashed files so
# already-open pages do not break during a deployment.
echo "[manual-deploy] publishing HTML entrypoints last"
rsync -av \
    --exclude '/backups/***' \
    --exclude '/releases/***' \
    --include '*/' \
    --include '*.html' \
    --exclude '*' \
    web/ "$PUBLISH_HOST:$PUBLISH_PATH/"

echo "[manual-deploy] publishing install manifest + current release artifact"
PUBLISH_DO=1 \
PUBLISH_HOST="$PUBLISH_HOST" \
PUBLISH_PATH="$PUBLISH_PATH" \
BLUEY_PUBLIC_BASE="$PUBLIC_BASE" \
BLUEY_RELEASE_ALLOW_UNSIGNED="$ALLOW_UNSIGNED" \
BLUEY_RELEASE_SIGNING_KEY_FILE="$RELEASE_SIGNING_KEY_FILE" \
scripts/publish-bluey-release.sh

echo "[manual-deploy] live checks"
"$CURL_BIN" -fsS "${PUBLIC_BASE%/}/health" >/dev/null
"$CURL_BIN" -fsS "${PUBLIC_BASE%/}/latest.json" >/dev/null
"$CURL_BIN" -fsS "${PUBLIC_BASE%/}/install.sh" >/dev/null
"$CURL_BIN" -fsS "${PUBLIC_BASE%/}/install.ps1" >/dev/null
for path in \
    "/robots.txt" \
    "/sitemap.xml" \
    "/how-bluey-works/" \
    "/bluey-faq/" \
    "/engineering-meeting-copilot/"
do
    "$CURL_BIN" -fsS "${PUBLIC_BASE%/}${path}" >/dev/null
done

llms_status="$($CURL_BIN -sS -o /dev/null -w '%{http_code}' "${PUBLIC_BASE%/}/llms.txt")"
if [ "$llms_status" != "410" ]; then
    echo "[manual-deploy] expected /llms.txt to return 410, got $llms_status" >&2
    exit 1
fi

if [ -n "$RELEASE_SIGNING_KEY_FILE" ]; then
    BLUEY_PUBLIC_BASE="$PUBLIC_BASE" \
    BLUEY_RELEASE_SIGNING_KEY_FILE="$RELEASE_SIGNING_KEY_FILE" \
        scripts/bluey-release-live-verify.sh "$VERSION"
else
    echo "[manual-deploy] warning: explicit local/dev-only unsigned override; live signature verification is not available" >&2
fi

echo "[manual-deploy] done"
