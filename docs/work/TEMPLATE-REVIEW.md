# REVIEW: [Batch ID] — [Title]

> **Codex preflight:** Load `$bluey-ops` before review and verify its memory
> against the current repository state and commit range.

Use the branch skill at `.agents/skills/bluey-ops/SKILL.md` and
[agent round checklist](BLUEY-AGENT-ROUND-CHECKLIST.md).
**Round / IMPL / FIX / handoff links:**

**Commit range:** `abc1234..def5678`
**Reviewer:** [name/agent]
**Date:** YYYY-MM-DD

## Per-Task Review

### [Task ID] — [Title]

| Field | Value |
|-------|-------|
| Files | |
| Verdict | 🟢 accept / 🟡 minor nit / 🔴 blocker |

**Findings:**
-

---

## Cross-Task Findings

<!-- Issues that span multiple tasks: inconsistencies, missing integration, etc. -->
-

## Build & Test Verification

```bash
cargo fmt --check   # ✅ / ❌
cargo clippy        # ✅ / ❌
cargo build         # ✅ / ❌
bash scripts/run-bluey-tests.sh all  # ✅ / ❌ (X passed, Y failed; temp workspace cleaned)
```

## Overall Verdict

<!-- Pick one: -->
🟢 **ACCEPT** — Ready to merge.
🟡 **ACCEPT WITH NITS** — Merge after addressing minor items.
🔴 **REQUEST CHANGES** — Blockers must be resolved.

## Follow-ups for Next Batch

| Priority / finding | Next owner (or unassigned) | Files / required action | Re-review evidence |
|---|---|---|---|
| | | | |

**Unrun gates / merge blockers / deployment blockers:**
