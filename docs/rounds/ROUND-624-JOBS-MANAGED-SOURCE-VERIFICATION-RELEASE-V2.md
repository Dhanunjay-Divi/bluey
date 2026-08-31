# Round 624 — Jobs Managed Source-Verification Release V2

**Date:** 2026-08-31

**Branch:** `feat/phase-624-jobs-managed-source-verification-v2`

**Status:** Bounded local source implementation; no candidate run, activation, deployment, or
production authority

> **Codex preflight:** Load `$bluey-ops` before implementation or review. Reconcile this round
> against Round 611, Round 614/614B, Phases 621/622, and the current repository. Use archived
> evidence only for one specifically missing historical fact.

## Goal

Close the minimum release-contract gap between the existing Phase 614 original-source verifier and
the managed-cloud candidate workflow. New candidates must use the already reviewed release-v2
contract with `sourceVerification=true`, the exact `original_source_verifier` capability, protocol,
entrypoint, measured runtime identity, role quorum, and activation-readback check. Direct and global
discovery remain false.

This round changes what an explicitly dispatched future candidate is allowed to represent. It does
not create a signed candidate, import or activate a release, start a worker, contact a provider,
open a customer cohort, or change a production flag.

## Predecessor Authority

- Phase 611 release v1 remains a valid historical contract and correctly rejects source
  verification.
- Phase 614 already supplies paired SQLite 057/PostgreSQL 035 source-verification authority,
  immutable observations and receipts, database-time assignment leases and heartbeats, the exact
  anonymous verifier entrypoint, and a separate release-v2 validator.
- Phase 614B supplies the independent signed employer-identity and job-risk authority needed for a
  production-representative positive application path. Provider presence alone remains
  insufficient.
- Phases 621/622 compose limited public admission with existing effect fences. Public admission is
  not source-verification, runner, queue, or Submit authority.

## Bounded Scope

### In Scope

- generate v2 migration and protocol contracts in the managed-cloud candidate job;
- emit a v2 descriptor with source verification true and both discovery modes false;
- measure the exact workflows image for the sorted roles
  `original_source_verifier`, `workflow_gateway`, and `workflow_worker`;
- require the compiled `original-source-verifier.js` bytes and source-verification protocol in the
  stored candidate;
- require `original-source-verifier-readiness` in the exact activation canary/readback set;
- add independent structural guards against v1 fallback, role omission, or discovery enablement;
- bind every release-evidence Node command to the pinned setup-node action, exact Node 22.23.2
  assertion, and repository default branch before processing;
- admit only the exact nonempty regular verifier entrypoint in the workflows OCI image, while
  aliases, renamed paths, and release-v1 assembly remain denied;
- document the separate rootless verifier-process command and protected per-role runtime settings;
  and
- retain the existing assignment/runtime/lease/replay fences unchanged.

### Out Of Scope

- direct or global discovery V3 authority, scheduling, source enrollment, rights, budgets, or SLOs;
- a new provider, authenticated retrieval, login, cookies, OAuth, CAPTCHA bypass, form write, or
  employer-facing effect;
- a schema migration, new lease protocol, job-integrity change, portal feature, model generation,
  mailbox, messaging, MCP, or C2C feature;
- registry publication, threshold signing, protected approval, hosted PostgreSQL/Temporal work,
  provider canary, customer admission, deployment, flag change, or rollback execution; and
- editing frozen `docs/reviews/` material.

## Exact Authority Chain

```text
clean source commit
  -> v2 contracts (SQLite 057 / PostgreSQL 035 / source_verification v1)
  -> one measured jobs-workflows image
  -> exact original_source_verifier role + compiled entrypoint bytes
  -> signed v2 manifest and v2 activation
  -> exact runtime grant/claim/heartbeat
  -> successful dependency lease poll
  -> original-source-verifier-readiness canary/readback
  -> account/cohort/source/hold-scoped assignment lease
  -> final pre-publication heartbeat
  -> immutable observation/receipt/head or typed fail-closed result
```

Any missing or mismatched link denies new source authority. Exact terminal replay remains
lookup-only, and changed bytes quarantine rather than reminting evidence.

## Acceptance Criteria

1. The candidate workflow generates contracts with `--version 2` and emits only descriptor v2.
2. Source verification is true only in that v2 descriptor; direct/global discovery remain false.
3. The workflows image measurement binds the exact sorted three-role set and the verifier
   entrypoint bytes.
4. The manifest requires source-verification protocol v1, exact capability mapping, and the
   role-separated runtime identity.
5. Activation evidence cannot validate without `original-source-verifier-readiness` and exact
   stored-byte, Temporal, failure-converter, portal, image, and rootfs readback.
6. Structural guards reject v1 fallback, a missing verifier role, enabled discovery, an old
   descriptor audience, ambient/wrong/late Node, or a non-default dispatch branch.
7. OCI inspection accepts only the exact nonempty regular verifier entrypoint; renamed and alias
   paths fail, and a three-role/verifier workflows image cannot enter release-v1 assembly.
8. Existing verifier lifecycle tests retain grant, runtime, assignment, heartbeat, terminal,
   response-loss replay, and changed-byte quarantine behavior.
9. Every checked-in production/provider-write flag remains `0`; every current production
   activation remains unchanged.
10. No external network/provider effect, release operation, deployment, or production mutation is
   performed by this round.

## External-Only Gates

Before any source-verification activation, separately prove immutable registry and portal readback,
threshold signatures and protected approvals, hosted migrations and lock behavior, Temporal task
queue behavior, exact image digest and read-only-rootfs policy, real verifier capacity, legally
approved anonymous provider canaries and rate/rights controls, monitoring/on-call, kill switch,
customer cohort, and higher-sequence rollback.

## Flag And Effect Ledger

```text
Production sourceVerification activation               not inspected or changed by this round
New candidate descriptor contract                      v2 / sourceVerification=true
directDiscovery                                        false
globalDiscovery                                        false
All checked-in production/provider-write flags         unchanged 0
Provider calls, applications, messages, deployment     not performed
```

## Decision

Round 624 is a bounded source and release-configuration phase. Local green evidence may make the
candidate path ready for exact-tip CI; it cannot by itself authorize a candidate run, activation,
deployment, customer effect, or production flag change.
