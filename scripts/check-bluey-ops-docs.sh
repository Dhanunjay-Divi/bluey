#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

required_files=(
  "AGENTS.md"
  "AGENT-ONBOARDING.md"
  "AGENT-HANDOFF.md"
  "README.md"
  "docs/work/README.md"
  "docs/work/TEMPLATE-FIX.md"
  "docs/work/TEMPLATE-IMPL.md"
  "docs/work/TEMPLATE-REVIEW.md"
)

while IFS= read -r runbook; do
  required_files+=("$runbook")
done < <(
  find docs -type f -iname '*runbook*.md' \
    ! -path 'docs/reviews/*' \
    ! -path 'docs/rounds/*' \
    | sort
)

missing=0
for file in "${required_files[@]}"; do
  if [[ ! -f "$file" ]]; then
    echo "bluey-ops docs check: required file is missing: $file" >&2
    missing=1
    continue
  fi

  if ! grep -Fq '$bluey-ops' "$file"; then
    echo "bluey-ops docs check: missing preflight in $file" >&2
    missing=1
  fi
done

if [[ "$missing" -ne 0 ]]; then
  exit 1
fi

echo "bluey-ops docs check: all agent entry points, work templates, and runbooks are covered"
