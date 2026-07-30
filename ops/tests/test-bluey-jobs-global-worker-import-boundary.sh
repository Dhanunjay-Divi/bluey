#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
AUTOMATION_DIST="$ROOT/jobs/automation/dist"
WORKFLOWS_DIST="$ROOT/jobs/workflows/dist"

if [ ! -f "$AUTOMATION_DIST/discovery-runtime.js" ] ||
    [ ! -f "$AUTOMATION_DIST/jobhive-runtime.js" ] ||
    [ ! -f "$WORKFLOWS_DIST/discovery-worker.js" ] ||
    [ ! -f "$WORKFLOWS_DIST/global-discovery-worker.js" ]; then
    printf 'Build the Jobs automation and workflows packages before this test.\n' >&2
    exit 1
fi

if ! grep -Fq \
    'from "@bluey/jobs-automation/discovery-runtime"' \
    "$WORKFLOWS_DIST/discovery-runtime.js"; then
    printf 'Direct discovery must use the discovery-only automation entry point.\n' >&2
    exit 1
fi

if ! grep -Fq \
    'from "@bluey/jobs-automation/discovery-runtime"' \
    "$WORKFLOWS_DIST/discovery-provider.js"; then
    printf 'ATS discovery must use the discovery-only automation entry point.\n' >&2
    exit 1
fi

if ! grep -Fq \
    'from "@bluey/jobs-automation/jobhive-runtime"' \
    "$WORKFLOWS_DIST/global-discovery-runtime.js"; then
    printf 'Global discovery must use the feed-only automation entry point.\n' >&2
    exit 1
fi

LOADER="$(mktemp "${TMPDIR:-/tmp}/bluey-jobs-import-boundary.XXXXXX.mjs")"
trap 'rm -f "$LOADER"' EXIT

cat > "$LOADER" <<'EOF'
export async function resolve(specifier, context, nextResolve) {
  if (
    specifier === "pdf-lib"
    || specifier === "@pdf-lib/fontkit"
    || specifier.startsWith("pdfjs-dist/")
  ) {
    throw new Error(`Discovery worker imported the document runtime: ${specifier}`);
  }
  return nextResolve(specifier, context);
}
EOF

NODE_NO_WARNINGS=1 node \
  --experimental-loader "$LOADER" \
  --input-type=module \
  --eval "await import('file://$WORKFLOWS_DIST/discovery-worker.js')"

NODE_NO_WARNINGS=1 node \
  --experimental-loader "$LOADER" \
  --input-type=module \
  --eval "await import('file://$WORKFLOWS_DIST/global-discovery-worker.js')"

printf 'Bluey Jobs discovery worker import boundaries: OK\n'
