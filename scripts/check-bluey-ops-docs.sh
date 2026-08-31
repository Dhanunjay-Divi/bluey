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

test_launcher="scripts/run-bluey-tests.sh"
test_launcher_docs=(
  "AGENTS.md"
  "AGENT-ONBOARDING.md"
  "AGENT-HANDOFF.md"
  "docs/HANDOFF.md"
  "docs/DELIVERY-LIFECYCLE.md"
  "docs/MODEL-ROUTING.md"
  "docs/RELEASE-RUNBOOK.md"
  "docs/SECURITY-HARDENING.md"
  "docs/TESTING-RUNBOOK.md"
  "docs/work/TEMPLATE-IMPL.md"
  "docs/work/TEMPLATE-REVIEW.md"
  ".github/PULL_REQUEST_TEMPLATE.md"
)
test_launcher_consumers=(
  "scripts/bluey-e2e-staging-smoke.sh"
  "scripts/observability-acceptance-smoke.sh"
  "scripts/smoke-test.sh"
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

if [[ ! -x "$test_launcher" ]]; then
  echo "bluey-ops docs check: test launcher is missing or not executable: $test_launcher" >&2
  missing=1
fi

for file in "${test_launcher_docs[@]}"; do
  if [[ ! -f "$file" ]]; then
    echo "bluey-ops docs check: test-launcher entry point is missing: $file" >&2
    missing=1
    continue
  fi
  if ! grep -Fq "$test_launcher" "$file"; then
    echo "bluey-ops docs check: isolated test launcher is not documented in $file" >&2
    missing=1
  fi
done

for file in "${test_launcher_consumers[@]}"; do
  if [[ ! -f "$file" ]] || ! grep -Fq "$test_launcher" "$file"; then
    echo "bluey-ops docs check: Rust test entry point bypasses $test_launcher: $file" >&2
    missing=1
  fi
done

if [[ "$missing" -ne 0 ]]; then
  exit 1
fi

echo "bluey-ops docs check: all agent entry points, work templates, and runbooks are covered"
