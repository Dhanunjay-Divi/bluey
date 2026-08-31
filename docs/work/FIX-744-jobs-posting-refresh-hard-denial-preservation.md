# FIX-744: Jobs Posting Refresh Hard-denial Preservation

**Severity:** P2 fail-closed representation

**Status:** Implemented; expanded focused evidence rerun pending; independent review pending

## Issue

Posting refresh sanitized incoming evidence but preserved only identity/timestamps from an existing
row. Existing mismatch, impersonation, blocked-risk, invalid, or original-source hard denials could
therefore be replaced by a new mutable positive or unknown projection.

The same weakening remained at the current-authority read boundary: non-positive relational source
heads projected no evidence, and the absent-projection merge cleared a persisted hard source denial
to generic unknown/revalidation.

## Required Fix

- Merge existing hard denials fail closed before scoring or persistence.
- Cover existing-denied plus incoming-positive and incoming-unknown refresh paths.
- Project current closed, mismatch, untrusted, and expired relational heads as exact negative
  evidence without minting an execution binding.
- Preserve a persisted hard source denial when no trustworthy relational projection is available;
  only a current positive relational head may supersede it.
- Never let a mutable refresh erase signed or independently derived denial signals.

## Evidence

- `upsert_cannot_replace_existing_hard_denials_with_mutable_positive_labels`: pass.
- `refresh_cannot_replace_existing_hard_denials_with_unknown_labels`: pass.
- `current_nonpositive_source_head_projects_exact_denial_without_execution_binding`,
  `absent_relational_projection_never_erases_a_persisted_source_denial`, and
  `relational_negative_source_projection_reaches_the_hard_eligibility_gate`: rerun pending.
- The earlier five-test profile-posting representation slice passes. Aggregate gates and post-fix
  independent review remain pending.
