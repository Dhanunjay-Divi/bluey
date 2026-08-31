#!/usr/bin/env bash
# Pre-commit hook for Bluey: runs fast policy gates before allowing
# a commit. Install with:
#
#   ln -s ../../scripts/pre-commit-observability.sh .git/hooks/pre-commit
#   chmod +x .git/hooks/pre-commit
#
# Or copy:
#   cp scripts/pre-commit-observability.sh .git/hooks/pre-commit
#
# Skips for non-Rust commits (docs-only, scripts-only) so doc rounds
# don't pay the build cost.

set -euo pipefail

WORKSPACE="$(git rev-parse --show-toplevel)"
cd "$WORKSPACE"

# ── Detect if any Rust source file is staged. If not, skip. ──────────
STAGED_RS="$(git diff --cached --name-only --diff-filter=ACMR | grep -E '\.rs$' || true)"

# ── Always run the observability tracing-call policy gate, even on
#    non-Rust commits, because tooling changes (scripts/) could relax
#    the policy. ───────────────────────────────────────────────────────
if [ -x "$WORKSPACE/scripts/analyze-tracing-calls.py" ]; then
    if ! python3 "$WORKSPACE/scripts/analyze-tracing-calls.py" --check-only; then
        echo ""
        echo "Observability policy gate FAILED. Fix the findings above"
        echo "before committing, or use --no-verify to bypass (NOT for"
        echo "commits that touch tracing call sites)."
        exit 1
    fi
fi

# ── If any Rust file is staged, run fmt + clippy gates. ──────────────
if [ -n "$STAGED_RS" ]; then
    echo "Running cargo fmt --all --check..."
    if ! cargo fmt --all --check >/dev/null 2>&1; then
        echo "cargo fmt FAILED. Run 'cargo fmt --all' to fix."
        exit 1
    fi
    echo "✅ fmt clean"

    # Clippy is slower; skip unless the user opts in via env var to
    # keep the pre-commit fast. CI catches clippy regressions.
    if [ "${BLUEY_PRECOMMIT_CLIPPY:-}" = "1" ]; then
        echo "Running cargo clippy -D warnings..."
        if ! bash "$WORKSPACE/scripts/run-bluey-tests.sh" -- \
            cargo clippy --all-targets -- -D warnings >/dev/null 2>&1; then
            echo "cargo clippy FAILED. Run it through scripts/run-bluey-tests.sh to inspect."
            exit 1
        fi
        echo "✅ clippy clean"
    fi
fi

echo "✅ pre-commit gates passed"
exit 0
