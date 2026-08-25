# REVIEW: Phase 612B — Jobs Career Command Center

> **Codex preflight:** Loaded `$bluey-ops` and reconciled its Jobs authority,
> receipt, privacy, and release guidance against the current repository, local
> handoff, Round 612, and the complete Phase 612B working-tree change set. The
> SSD archive was not used.

**Commit range:** `bdfb77f9..WORKTREE`
**Reviewer:** Codex review agent
**Date:** 2026-08-25

## Per-Task Review

### 612B.1 — Authenticated Second-Pass Evidence Record

| Field | Value |
|-------|-------|
| Files | `docs/rounds/ROUND-612B-JOBS-GIRAFFYREACH-SECOND-PASS-AND-CAREER-COMMAND-CENTER.md`, `docs/work/IMPL-PHASE-612B-JOBS-CAREER-COMMAND-CENTER.md`, `CHANGELOG.md` |
| Verdict | 🟢 accept |

**Findings:**

- Observed, claimed, contradicted, unknown, Bluey fact, and Bluey decision are
  kept separate.
- The two bounded user-initiated synthetic mutation workflows and their known
  retained outcomes are recorded without claiming a private implementation or
  exact backend write count.
- OAuth, C2C chat/send, LinkedIn publication, employer submission, MCP
  credentials, billing, deployment, production, and SSD boundaries are
  explicit. No credential, real resume body, or user-specific sensitive value
  is recorded.
- Round 613 remains an explicit launch prerequisite for canonical role, skill,
  location, and Career Track policy enforcement.

---

### 612B.2 — Command Center State Derivation And Truth Boundary

| Field | Value |
|-------|-------|
| Files | `jobs/portal/src/lib/command-center.ts`, `jobs/portal/src/lib/command-center.test.ts` |
| Verdict | 🟢 accept |

**Findings:**

- Readiness distinguishes `ready`, `needs_action`, `reported`, and `optional`.
  Profile resume references and identity-bound legacy Career Tracks remain
  reported-only because this workspace projection lacks authoritative asset
  read-back and canonical policy revisions. Optional inbox correlation is not
  counted as a failed required check.
- Rolling 24-hour matches exclude expired, future-dated, passed, and
  inactive-Track postings. A later restore re-includes the job, while the view
  explicitly avoids turning freshness into an eligibility claim.
- Matches and Overview share one effective 12-hour source-health projection.
  Paused, stale, never-synced, and degraded sources take precedence over raw
  optimistic health; source categories do not overlap.
- Prepared, in-flight, submitted, interview, offer, and reconciliation
  projections remain separately labeled. Interview and offer outcomes are
  deduplicated per application, while eligibility, exact approval, and
  communication-send state remain explicitly unavailable without their
  required receipts.
- Cloud and local runner availability, all calendar providers, mailbox state,
  and LinkedIn unavailability derive from the typed workspace without granting
  execution or integration authority.

---

### 612B.3 — Read-Only Overview, Routing, And Accessibility

| Field | Value |
|-------|-------|
| Files | `jobs/portal/src/views/CareerCommandCenterView.tsx`, `jobs/portal/src/views/CareerCommandCenterView.css`, `jobs/portal/src/views/CareerCommandCenterView.test.tsx`, `jobs/portal/src/App.tsx`, `jobs/portal/src/App.test.ts`, `jobs/portal/src/components/AppShell.tsx` |
| Verdict | 🟢 accept |

**Findings:**

- Overview is the canonical home and fallback under `BrowserRouter`
  `basename="/jobs"`; internal links remain basename-relative and preserve the
  validated preview query state.
- The new surface is navigation-only: it introduces no button, mutation
  handler, provider command, queue action, approval, submit, send, or external
  account write.
- Explicit states cover no matches, no interventions, no sources, no candidate
  outcomes, disconnected inbox/calendar, unavailable LinkedIn, unverified
  source freshness, runner availability, and side-effect reconciliation.
- A nullable server `last_verified_at_ms` is rendered as verification
  unavailable, never as a successful verification label.
- Desktop navigation retains all existing destinations. Mobile remains at five
  tabs, keeps Overview and review/blocker access visible, and leaves Settings
  available through the account menu.
- Keyboard focus and reduced-motion behavior are inherited from the portal
  shell. The new surface has text status in addition to color, responsive
  1,040 px and 640 px layouts, and surface-scoped light-theme text colors that
  satisfy the WCAG AA 4.5:1 normal-text contrast threshold.

---

### 612B.4 — Packaged Portal Assets And Operational Handoff

| Field | Value |
|-------|-------|
| Files | `web/jobs/index.html`, `web/jobs/assets/**`, `docs/work/IMPL-PHASE-612B-JOBS-CAREER-COMMAND-CENTER.md` |
| Verdict | 🟢 accept |

**Findings:**

- The production build regenerated immutable hashed assets and `index.html`
  points at the current portal entry chunk.
- The final lazy Command Center JavaScript contains the settled fail-closed,
  unavailable-state, runner, integration, and recent-evidence copy; its CSS
  contains the final responsive and contrast tokens. No source maps are
  packaged.
- The existing Vite advisory for chunks above 500 kB is non-fatal and does not
  change the Phase 612B read-only authority boundary.

## Cross-Task Findings

- Independent logic and UI reviews identified optimistic source freshness,
  candidate pass/Track leakage, nullable verification rendering, resume/Track
  authority, and optional-readiness defects; the focused regressions below
  close each one without adding a write path.
- No unresolved correctness, strict TypeScript, `/jobs` routing,
  accessibility, privacy, security, documentation, or generated-asset blocker
  remains in the reviewed change set.
- The implementation and documentation consistently leave eligibility,
  exact-approval, communication-send, provider/OAuth, execution, and production
  authority unavailable unless a later reviewed contract supplies the required
  server evidence and receipts.

## Build & Test Verification

```bash
cd jobs && npm run typecheck
# ✅ all five Jobs workspaces passed

cd jobs && npm test
# ✅ 139 test files / 1,776 passed / 1 skipped

cd jobs/portal && npm run build
# ✅ 2,296 modules transformed; existing >500 kB advisory only

npm exec --workspace @bluey/jobs-portal vitest run \
  src/lib/command-center.test.ts \
  src/views/CareerCommandCenterView.test.tsx \
  src/App.test.ts
# ✅ independent review pass: 3 files / 34 tests

scripts/check-bluey-ops-docs.sh
# ✅ passed

git diff --check bdfb77f9 -- . ':(exclude)output/**'
# ✅ passed

# ✅ generated asset-string check, no-secret/no-personal-data documentation
#    scan, 1,280 x 720 desktop and 390 x 844 phone local-preview visual/DOM QA,
#    responsive contract checks, and light-theme WCAG AA contrast checks passed
#    as recorded in the IMPL document
```

No Rust file changed in this batch, so Rust format, clippy, build, and test were
not applicable. No deploy, provider write, production read, feature activation,
OAuth grant, credential action, payment, or external-account mutation occurred
during implementation or review.

## Overall Verdict

🟢 **ACCEPT** — Ready to merge after the normal feature-branch commit and PR
workflow.

## Follow-ups for Next Batch

- Complete Round 613 as a separate reviewed batch before treating canonical
  matching, eligibility, preparation, approval, queueing, or submission as
  launch-ready.
- Add named-policy eligibility, exact-approval, communication-send, and broader
  recent-evidence projections only when `JobsWorkspace` exposes their durable,
  source-labeled receipts.
- Treat future portal chunk-size optimization as a separate performance batch;
  do not mix it into Phase 612B authority or release scope.
