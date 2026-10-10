# FIX: SQLite selected-artifact list lifetime

Preflight: `bluey-ops`; Phase626 integration ownership only.

Symptom: queued Rust build failed E0597 in `store::list_artifacts` because a
tail-expression `query_map(...)?` temporary could outlive its statement/connection.

Cause: wrapping the expression in an inner scope did not end its borrow; the
same tail-expression issue remained inside that scope.

Fix: collect into a local `artifacts` variable before returning it from the
scope. Commit `edb2f089`. No query, ownership or storage semantics changed.

Verification: final code revision `7a288472` passed 44 focused tests, the
projector regression and strict all-target Clippy. Failed and successful build
temporary roots were cleaned. Live PostgreSQL/document gates remain separate.
