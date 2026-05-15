#!/usr/bin/env bash
# Usage: ./infra/scripts/bump-formulae.sh <version> <sha256-darwin-arm64> <sha256-darwin-x86_64> <sha256-windows>
# Updates Homebrew formula and Scoop manifest with new version and hashes.
set -euo pipefail

VERSION="${1:?Usage: $0 VERSION SHA_ARM64 SHA_X86_64 SHA_WIN}"
SHA_ARM64="${2:?}"
SHA_X86_64="${3:?}"
SHA_WIN="${4:?}"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Update Homebrew formula version
sed -i.bak "s/version \".*\"/version \"$VERSION\"/" "$REPO_ROOT/infra/homebrew/bluey.rb"
rm -f "$REPO_ROOT/infra/homebrew/bluey.rb.bak"

# Update sha256 - arm64 is first occurrence, x86_64 is second
python3 -c "
import re, sys
content = open('$REPO_ROOT/infra/homebrew/bluey.rb').read()
shas = ['$SHA_ARM64', '$SHA_X86_64']
i = [0]
def repl(m):
    s = shas[min(i[0], 1)]
    i[0] += 1
    return f'sha256 \"{s}\"'
content = re.sub(r'sha256 \"[^\"]+\"', repl, content)
open('$REPO_ROOT/infra/homebrew/bluey.rb', 'w').write(content)
"

# Update Scoop manifest
cd "$REPO_ROOT"
jq --arg v "$VERSION" --arg h "$SHA_WIN" \
  '.version = $v | .hash = $h | .url = (.url | gsub("v[0-9.]+"; "v" + $v))' \
  infra/scoop/bluey.json > infra/scoop/bluey.json.tmp
mv infra/scoop/bluey.json.tmp infra/scoop/bluey.json

echo "Done: updated formulae to v$VERSION"
