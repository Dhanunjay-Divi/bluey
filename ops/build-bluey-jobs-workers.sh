#!/usr/bin/env bash
# Build one immutable runtime artifact for both Bluey Jobs discovery workers.
#
# Usage:
#   ops/build-bluey-jobs-workers.sh [output-directory]

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
JOBS_ROOT="$ROOT/jobs"
OUTPUT_DIR="${1:-$ROOT/dist/bluey-jobs-workers}"
SOURCE_COMMIT="$(git -C "$ROOT" rev-parse HEAD)"
RELEASE_ID="${BLUEY_JOBS_WORKERS_RELEASE_ID:-jobs-workers-${SOURCE_COMMIT:0:12}}"
WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/bluey-jobs-workers.XXXXXX")"

cleanup() {
    rm -rf "$WORK_DIR"
}
trap cleanup EXIT

fail() {
    echo "build-bluey-jobs-workers: $*" >&2
    exit 1
}

sha256_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    else
        fail "sha256sum or shasum is required"
    fi
}

command -v node >/dev/null 2>&1 || fail "node is required"
command -v npm >/dev/null 2>&1 || fail "npm is required"
command -v tar >/dev/null 2>&1 || fail "tar is required"
[[ "$RELEASE_ID" =~ ^[A-Za-z0-9._-]+$ ]] || fail "release ID contains unsafe characters"

if [ "${BLUEY_ALLOW_DIRTY_WORKER_BUILD:-0}" != "1" ]; then
    git -C "$ROOT" diff --quiet
    git -C "$ROOT" diff --cached --quiet
fi

npm --prefix "$JOBS_ROOT" run build --workspace @bluey/jobs-automation
npm --prefix "$JOBS_ROOT" run build --workspace @bluey/jobs-workflows

PACK_DIR="$WORK_DIR/pack"
STAGE_ROOT="$WORK_DIR/$RELEASE_ID"
RUNTIME_ROOT="$STAGE_ROOT/jobs"
mkdir -p "$PACK_DIR" "$RUNTIME_ROOT/vendor" "$RUNTIME_ROOT/workflows"

AUTOMATION_TARBALL="$(
    npm --prefix "$JOBS_ROOT" pack \
        --workspace @bluey/jobs-automation \
        --pack-destination "$PACK_DIR" \
        --json |
        node -e '
            let data = "";
            process.stdin.on("data", (chunk) => { data += chunk; });
            process.stdin.on("end", () => {
              const result = JSON.parse(data);
              if (!Array.isArray(result) || !result[0]?.filename) process.exit(1);
              process.stdout.write(result[0].filename);
            });
        '
)"
cp "$PACK_DIR/$AUTOMATION_TARBALL" "$RUNTIME_ROOT/vendor/"
cp -R "$JOBS_ROOT/workflows/dist" "$RUNTIME_ROOT/workflows/dist"

cat > "$RUNTIME_ROOT/package.json" <<JSON
{
  "name": "bluey-jobs-discovery-runtime",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "dependencies": {
    "@bluey/jobs-automation": "file:vendor/$AUTOMATION_TARBALL"
  }
}
JSON

(
    cd "$RUNTIME_ROOT"
    PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1 \
        npm install --omit=dev --ignore-scripts --no-audit --no-fund
)

node --input-type=module -e \
    "await import('file://$RUNTIME_ROOT/workflows/dist/discovery-worker.js')"
node --input-type=module -e \
    "await import('file://$RUNTIME_ROOT/workflows/dist/global-discovery-worker.js')"

cat > "$STAGE_ROOT/manifest.json" <<JSON
{
  "release_id": "$RELEASE_ID",
  "source_commit": "$SOURCE_COMMIT",
  "built_at_utc": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "node_version": "$(node --version)",
  "npm_version": "$(npm --version)"
}
JSON

mkdir -p "$OUTPUT_DIR"
ARCHIVE="$OUTPUT_DIR/$RELEASE_ID.tar.gz"
tar -C "$WORK_DIR" -czf "$ARCHIVE" "$RELEASE_ID"
(
    cd "$OUTPUT_DIR"
    archive_name="$(basename "$ARCHIVE")"
    printf '%s  %s\n' "$(sha256_file "$archive_name")" "$archive_name" \
        > "$archive_name.sha256"
)

echo "$ARCHIVE"
echo "$ARCHIVE.sha256"
