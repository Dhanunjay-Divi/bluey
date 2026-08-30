# REVIEW: PHASE-620A1 — Jobs Business Messaging No-Egress Kernel

> **Codex preflight:** Loaded `$bluey-ops`, reconciled the isolated worktree, Round, implementation
> commit, retained test output, and independent source/security review. The SSD archive was not
> used.

**Commit range:**
`f9cd591e57ba9ef4541fd54982f7db8b6ba55b08..8707778be27e1940682af5c763a7ccd107bc50a0`

**Reviewer:** Codex independent correctness/security review, reconciled by the primary agent

**Date:** 2026-08-30

**Status:** 🟢 local source accepted; 🟡 messaging production/release capability intentionally absent

## Verdict

The Phase 620A1 source is accepted as a bounded, test-only no-egress kernel. The independent first
pass found four substantive classes of defect: production TypeScript emission, TypeScript/Rust raw
rejection mismatch, permissive synthetic plaintext identifiers, and provider/action truth
overstatement. Corrective review verified each repair and found no remaining P0, P1, or P2.

Acceptance does not authorize a provider connection, route, worker, UI, durable schema, production
flag, customer cohort, deployment, message, application, or email. Those boundaries are absent by
design and remain successor work.

## Per-Task Review

### Cross-runtime command and canonicalization contract

| Field | Value |
| --- | --- |
| Files | TypeScript support/test/fixture and Rust verifier/test |
| Verdict | 🟢 accept |

The closed ASCII grammar, canonical newline JSON, SHA-256 bindings, exact Jobs read set, safe
integer limits, unknown-field rejection, STOP precedence, step-up-only approval, and deterministic
ambiguity projections agree across both runtimes. The shared fixture freezes successful canonical
bytes; corresponding adversarial tests cover raw rejection parity.

### Privacy and channel identity

| Field | Value |
| --- | --- |
| Files | TypeScript/Rust kernels and adversarial tests |
| Verdict | 🟢 accept |

Only exact closed synthetic IDs and field-specific revision stems enter canonical evidence.
Disguised phone, bearer, secret, token, URL, email, and provider identifiers fail closed. Personal
WhatsApp, QR/device sessions, personal iMessage, and unattended SMS have typed denials and cannot be
reinterpreted as business channels.

### Provider and action truth ceilings

| Field | Value |
| --- | --- |
| Files | TypeScript/Rust outcome projection and policy tests |
| Verdict | 🟢 accept |

`PREPARE` can only produce `simulated_no_effect`. Provider-accepted and provider-closed outcomes
require the exact provider-specific gateway/close evidence. WhatsApp delivered/read requires its
exact synthetic status schema; Apple cannot claim delivered/read. Cross-provider evidence is
rejected.

### No-egress and release containment

| Field | Value |
| --- | --- |
| Files | Dedicated TypeScript config, Cargo/lib gating, containment guard, CI/release workflows |
| Verdict | 🟢 accept |

The TypeScript support is outside production compilation and exports. Rust support is default-off
and release-rejected. Exact import allowlists plus deny guards exclude transport, filesystem,
process, browser, credentials, ambient time APIs, randomness, database, and Jobs effect seams. Post-build guards
prove no emitted simulator artifact, production import, Docker consumer, managed-release feature,
or release-binary identifier.

## Corrective Finding Reconciliation

| Initial finding | Severity | Resolution |
| --- | --- | --- |
| Non-exported TypeScript source still emitted into production artifacts | P1 | Moved to `tests/support`; dedicated strict no-emit config; post-build artifact guard |
| Rust closed enum could not match typed personal-channel raw rejection | P1 | Added strict raw `Value` boundary with alias-first typed denial |
| Generic synthetic grammar admitted privacy-shaped plaintext | P1 | Closed per-field ID registries, exact revision stems, adversarial privacy matrix |
| `PREPARE` and provider outcomes could exceed truthful evidence | P1 | Action truth ceiling and exact provider evidence families |
| Provider accepted/closed evidence was not provider-specific | P2 | Exact WhatsApp/Apple gateway and close revisions plus cross-provider denial |
| Deny-only import checks admitted alternate or multiline seams | P2 | Exact static import/`use` allowlists, count assertions, time/randomness bans |

All corrections were rebuilt and independently rereviewed.

## Build & Test Verification

| Evidence | Result |
| --- | --- |
| TypeScript focused | 71/71 |
| Full Jobs JavaScript | 1,967 passed / 1 skipped |
| Full Jobs typecheck/build | Green |
| Rust verifier | 14/14 |
| Full Rust library aggregate with feature | 1,590/1,590 |
| Strict Clippy/check/fmt | Green |
| Default release binaries | Green and identifier-free; exact SHA-256 values recorded in IMPL |
| Release feature negative test | Expected exit 101 compile rejection |
| Privacy/schema/provenance | Green: 2,712/2,437; 102/86; 663/631/1/14 |
| Browser/managed release authority | 10/10 and 17/17 |
| Portal freshness/account deletion | Green and 3/3 |
| Independent final review | No remaining P0–P2 |

## Residual Limitations

- This is a deterministic contract simulator, not an OS-level sandbox.
- It has no durable state, ingress authentication, consent/suppression persistence, leases,
  reconciliation, provider adapter, route, worker, or UI.
- Provider evidence revisions are synthetic contracts, not live provider evidence.
- Shared golden fixtures emphasize success bytes; raw rejection parity is covered in parallel
  TypeScript/Rust tests.
- WhatsApp/Apple eligibility and approvals remain external prerequisites.
- The public limited-beta V1 release is a separate review-first production-boundary effort; this
  implementation must not be presented as a shipped messaging feature.

## Overall Verdict

🟢 **ACCEPT** — Ready for local documentation/PR review as a no-egress test-only kernel.

🟡 **HOLD PRODUCTION CAPABILITY** — No provider integration, messaging launch, deployment, or
production flag is authorized by this review.

## Follow-ups

- Design durable Phase 620A2 authority as a separate bounded phase.
- Complete provider eligibility and sandbox evidence before adapter work.
- Implement and verify the separate public limited-beta cohort gate for the existing review-first
  Bluey Jobs V1.
