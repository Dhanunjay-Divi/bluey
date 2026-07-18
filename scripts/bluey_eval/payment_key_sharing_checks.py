"""Adversarial self-checks for payment key ownership and coreference."""

from __future__ import annotations

from .payment_key_sharing import has_affirmative_cross_operation_key_sharing


def self_check_payment_key_sharing_detector() -> None:
    valid_context = (
        "Each logical provider-operation instance gets its own stable idempotency "
        "key. Authorization, capture, and refund use distinct keys. A new partial "
        "capture or refund is a new logical action with a new key. A retry of that "
        "exact partial action reuses its original key."
    )
    unsafe_claims = (
        "Partial capture and partial refund have matching idempotency keys.",
        "Both partial-operation rows reference the same idempotency credential.",
        "A partial refund is issued under the partial capture idempotency key.",
        "Partial capture must not reuse the refund key. In recovery, it does reuse "
        "that key.",
        "Never use one partial key for both actions — however, both actions retain "
        "that key.",
        "We reject a common partial key, except the partial refund keeps the capture "
        "key.",
        "Authorization owns a stable idempotency key. Reconciliation records the "
        "provider response. Ledger posting follows confirmation. Capture reuses it.",
        "Capture has a stable idempotency key. Reconciliation records the provider "
        "response. Ledger posting follows confirmation. Refund reuses it.",
        "Authorization owns a stable idempotency key. The durable outbox was "
        "committed. Provider status is recorded. Refund takes that key.",
        "Authorization owns an idempotency key. The worker persists the provider "
        "result. A reconciliation job stores the evidence. Capture uses the "
        "inherited token.",
        "Capture owns an idempotency key. The worker persists the provider result. "
        "A reconciliation job stores the evidence. Refund uses the inherited token.",
        "A partial capture owns its child operation key. The worker records the "
        "provider response. Reconciliation persists the evidence. A partial refund "
        "reuses it.",
        "Capture owns a stable idempotency key. Audit metadata records the provider "
        "response. The ledger records the result. Refund sends it to the provider.",
        "Capture owns a stable idempotency key. Audit metadata records the provider "
        "response. Refund calls the provider with it.",
        "Capture owns a stable idempotency key. Audit metadata records the provider "
        "response. Refund submits under that key.",
        "A capture owns its provider key. The refund worker forwards it in the money "
        "command.",
        "Capture owns a stable idempotency key. The scheduler records state. The "
        "outbox stores the attempt. The ledger records the result. Reconciliation "
        "stores evidence. The worker confirms completion. Refund reuses it.",
        "Capture owns a stable idempotency key. The worker records status. Refund "
        "attaches it to the provider request.",
        "Capture owns a stable idempotency key. The worker records status. Refund "
        "passes it in the Idempotency-Key header.",
        "Capture owns a stable idempotency key. The worker records status. Refund "
        "copies it into the provider request.",
        "Capture owns a stable idempotency key. The worker records status. Refund "
        "sets the provider Idempotency-Key header to it.",
        "The refund request copies the capture idempotency key into its provider "
        "header.",
        "Capture owns an idempotency key. Capture and refund both use it.",
        "Capture records its idempotency key in audit metadata and refund sends it "
        "to the provider.",
        "When issuing the refund, put the capture idempotency key in the provider "
        "request header.",
        "The capture idempotency key is copied into the refund provider request.",
        "Capture and refund send the identical idempotency value to the provider.",
        "Capture and refund use the same idempotency key and store it in the same "
        "database column.",
        "Capture and refund use the same Idempotency-Key header but send distinct "
        "values and also use the same idempotency key for recovery.",
    )
    for claim in unsafe_claims:
        assert has_affirmative_cross_operation_key_sharing(
            f"{valid_context} {claim}"
        ), claim

    safe_claims = (
        "Retry of the exact partial capture reuses its original key, but a new "
        "partial refund gets a new child key.",
        "Capture stores the authorization idempotency key as correlation metadata, "
        "but calls the provider with its own capture key.",
        "A refund audit record references the capture key, while the refund provider "
        "command uses its own key.",
        "Capture logs the authorization key for traceability; it does not send it to "
        "the provider.",
        "Capture owns a key. The worker records status. Ledger stores evidence. "
        "Capture reuses it.",
        "Authorization owns a key. The worker records status. A new unrelated "
        "payment starts. Capture reuses it.",
        "Capture owns a key. The worker records status. Refund gets a distinct key.",
        "Capture owns a key. The worker records status. Refund does not reuse it.",
        "Use a single idempotency key per operation: authorization, capture, and "
        "refund each use a different operation instance.",
        "Capture and refund use distinct idempotency keys. The service has a shared "
        "HTTP client. Both use it to submit provider calls.",
        "Capture and refund use distinct idempotency keys. The service has a shared "
        "HTTP client. Both operations use it to submit provider calls.",
        "Capture and refund include the same correlation key in metadata but use "
        "distinct provider idempotency keys.",
        "Capture and refund use matching formats for idempotency keys, but the "
        "generated values are distinct.",
        "Capture and refund have identical key lengths but separate idempotency keys.",
        "Capture and refund use the same key schema but distinct generated "
        "idempotency keys.",
        "Capture and refund idempotency keys share the same prefix but remain "
        "distinct values.",
        "Capture and refund use the same Idempotency-Key header name but send "
        "distinct values.",
        "Capture and refund use the same Idempotency-Key header but send different "
        "values.",
        "Capture and refund send distinct values through the same Idempotency-Key "
        "header.",
        "Capture and refund use a shared key generator that emits distinct "
        "operation-scoped idempotency keys.",
        "Capture and refund use the same HMAC key to derive distinct idempotency "
        "tokens.",
        "Capture and refund use the same encryption key but distinct provider "
        "idempotency keys.",
        "Capture and refund store their distinct values in the same idempotency-key "
        "database column.",
        "Capture and refund store separate idempotency keys in the same database "
        "table and column.",
        "Capture and refund call the same key-generation helper, which includes "
        "operation ID and emits distinct keys.",
        "Capture and refund idempotency keys are not equal.",
        "Capture attaches the refund idempotency key only as a correlation header, "
        "not to the provider request.",
        "I give each authorization, capture, and refund, including each partial "
        "capture or refund, its own stable idempotency key; retries of that same "
        "operation reuse the original key.",
    )
    for claim in safe_claims:
        assert not has_affirmative_cross_operation_key_sharing(
            f"{valid_context} {claim}"
        ), claim

    assert has_affirmative_cross_operation_key_sharing(
        "The capture and refund idempotency keys must be equal."
    )
    assert has_affirmative_cross_operation_key_sharing(
        "I give each authorization, capture, and refund, including each partial "
        "capture or refund, its own stable idempotency key; retries of that same "
        "operation reuse the original key. However, capture and refund then share "
        "the same idempotency key during recovery."
    )
