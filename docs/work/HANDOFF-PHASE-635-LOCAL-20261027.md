# Jobs-only Phase 635 local handoff

Date: 2026-10-27 (America/New_York). Current Rust qualification passed locally;
overall Jobs production/parity goal remains active and unfinished.

## Authority

- Checkout: `/Users/uno/Downloads/cue-jobs-source-verifier-release-v3-20260831`.
- Branch: `feat/phase-635-jobs-current-rust-qualification`.
- Exact tested source: `863042a02afbdce4a898b8bc77978845612a569f`.
- Resolve docs-only closeout subject `docs(jobs): qualify current Rust and PostgreSQL gates`.
  That successor is not a rebuilt release artifact or new source qualification.
- Read `bluey-ops`, checkout AGENTS.md, Round/IMPL/REVIEW635, the completion ledger
  and Phase634 handoff for exact Linux-image/VM limitations.
- Current code/local handoffs are authoritative. SSD was not used. Preserve
  inherited untracked REVIEW628; do not edit/start meeting-owned
  `/Users/uno/Downloads/cue`, desktop/overlay/Pinky or unrelated VM profiles.
- Project-scoped Ruflo task: `task-1793142594980-hr7v60`.

## Actual qualification

Session61600 exited0 on unchanged source. The existing PG17/pgvector wrapper
ran the exact30 configured selections from IMPL629, each in a fresh private
database:30 passed, none failed. It stopped PostgreSQL cleanly, and the parent
controller asserted the postmaster PID file absent before continuing.

The same owned private compiler root/HOME then ran server formatting, strict
warnings-denied all-target Clippy and all-target tests offline/locked with both
integration/simulator support features. The full test target total is1,845:
library1,705, server binary1, integration134, guarded example5; Jobs API binary0.
All reported zero failed/ignored. Library took2,471.54s and HTTP113 took539.80s.
Configured-only early returns in the SQLite/full stage are not additional real
PostgreSQL proof. Installed Rust/Cargo1.95.0 was native macOS/ARM64.

The full exact command, private environment, dependency-test optimization flags
and target table are in IMPL635. Source was frozen until terminal; no tests,
timeouts, authority rules or production settings were weakened to pass.

The canonical root was absent, its specific parent empty and observed command/
Cargo/library PIDs absent after completion; no owned process path remained.
Fresh launcher self-test84754 passed including deliberate negative cleanup
coverage. No runtime artifact is retained for promotion. Final privacy and
review evidence is recorded in IMPL/REVIEW635 rather than inferred here.

## Next bounded work

1. Fix the confirmed company-identity gap: active uniqueness is currently
   `(account_id, normalize_company_key(posting.company))`. Different normalized
   display aliases for the same signed employer can evade that guard. Preserve
   the name guard; add canonical employer binding/constraints, safe legacy
   handling and own-reservation packet/pre-click validation. Reproduce with two
   simultaneously Verified postings, not stale/fabricated source authority.
2. Before any further VM reuse, qualify live host-listener policy. Phase634
   inspected its no-network/read-only container but not host listeners while
   the VM was alive. Official VZ source needs one loopback SSH management
   listener; literal zero is not a valid claim. Deny public/DNS/application
   listeners, inventory permitted management plus hostagent/driver/usernet PIDs,
   and prove absence before deleting private VM state.
3. Stitch the existing private queue to actual HTTP runner claim/heartbeat/
   safe-release and controlled browser/receipt proof. Do not reuse local ATS
   authority as cloud authority or relax employer network restrictions.

Shared fixture audit found the alias regression can use existing helpers: two
Tracks/verified identities, distinct Greenhouse tenant/job URLs, one managed
source runtime, the same canonical employer domain, and reuse the first ATS
runtime target/surface for the second job. Re-resolve BOTH composed authorities
as Verified before the second admission; assert equal signed employer ID/domain
and distinct display keys. This setup has not yet been executed.

Legacy unknown bindings must not silently fall back to display names or rewrite
history. Preserve reconciliation/terminal recording and authorized safe release;
new admission/pre-click should fail closed until historical identity is proven.
Do not blindly change frozen v3 schema/release/metering hash contracts.

Hosted object/model/OAuth/billing, verified source coverage, Linux server build,
exact-tip CI/signed artifacts, capacity/rollback and MCP/C2C/business messaging/
client parity remain unfinished in the ledger. Passing local tests is not a
production release. No push/PR/merge, deployment, registry/signing publication,
paid resource, production flag or real application/email/calendar is authorized.
