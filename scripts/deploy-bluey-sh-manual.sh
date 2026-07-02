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

if ! command -v "$CURL_BIN" >/dev/null 2>&1; then
    if [ -x /usr/bin/curl ]; then
        CURL_BIN="/usr/bin/curl"
    else
        echo "[manual-deploy] curl not found; set CURL_BIN=/path/to/curl" >&2
        exit 1
    fi
fi

echo "[manual-deploy] host=$PUBLISH_HOST path=$PUBLISH_PATH"

echo "[manual-deploy] syncing static web"
rsync -av --delete \
    --exclude '.DS_Store' \
    --exclude '._*' \
    --exclude '/backups/***' \
    --exclude '/releases/***' \
    --exclude '/install.sh' \
    --exclude '/install.ps1' \
    --exclude '/latest.json' \
    --exclude '/latest.json.sig' \
    --exclude 'index.html.bak-*' \
    --exclude 'latest.json.bak-*' \
    web/ "$PUBLISH_HOST:$PUBLISH_PATH/"

echo "[manual-deploy] publishing install manifest + current release artifact"
PUBLISH_DO=1 \
PUBLISH_HOST="$PUBLISH_HOST" \
PUBLISH_PATH="$PUBLISH_PATH" \
scripts/publish-bluey-release.sh

echo "[manual-deploy] live checks"
"$CURL_BIN" -fsS "https://bluey.sh/health" >/dev/null
"$CURL_BIN" -fsS "https://bluey.sh/latest.json" >/dev/null
"$CURL_BIN" -fsS "https://bluey.sh/install.sh" >/dev/null
"$CURL_BIN" -fsS "https://bluey.sh/install.ps1" >/dev/null
for path in \
    "/llms.txt" \
    "/robots.txt" \
    "/sitemap.xml" \
    "/how-bluey-works/" \
    "/bluey-faq/" \
    "/engineering-meeting-copilot/"
do
    "$CURL_BIN" -fsS "https://bluey.sh${path}" >/dev/null
done

if [ -n "${BLUEY_RELEASE_PUBKEY_FILE:-}" ] || [ -n "${BLUEY_RELEASE_SIGNING_KEY_FILE:-}" ]; then
    scripts/bluey-release-live-verify.sh "$VERSION"
else
    echo "[manual-deploy] warning: release signature verification skipped; set BLUEY_RELEASE_PUBKEY_FILE or BLUEY_RELEASE_SIGNING_KEY_FILE" >&2
fi

echo "[manual-deploy] done"
