"""Payment contract detector self-checks for the Bluey live evaluator."""

from __future__ import annotations

from .payment_contracts import (
    Q39_INGRESS_IDEMPOTENCY_SENTENCE,
    Q39_LEDGER_IDEMPOTENCY_SENTENCE,
    Q39_PARTIAL_ACTION_BOUNDARY_SENTENCE,
    has_exactly_once_processing_overclaim,
    has_safe_payment_same_operation_replay_condition,
    has_unsafe_ambiguous_payment_outcome,
    large_fk_migration_safety_issues,
    payment_operation_semantic_issues,
    payment_platform_safety_issues,
    payment_q39_completeness_issues,
)


def self_check_large_fk_migration_safety() -> None:
    failed_live_answer = (
        "You should tell the junior engineer that they should not add the constraint "
        "directly to the production table. Instead, they should create a new table "
        "with the desired schema, copy the data into it, rename the old table to a "
        "backup name, and rename the new table to the original name. This approach "
        "avoids locking the production table for the "
        "duration of the constraint creation, which would block all reads and writes. "
        "This validation process locks the table, preventing any reads or writes."
    )
    assert set(large_fk_migration_safety_issues(failed_live_answer)) == {
        "unsafe_whole_table_copy_swap_advice",
        "unsafe_universal_fk_read_write_block_claim",
    }
    assert large_fk_migration_safety_issues(
        "Foreign-key validation blocks all reads and writes."
    ) == ["unsafe_universal_fk_read_write_block_claim"]
    assert large_fk_migration_safety_issues(
        "Validation can always block all reads and writes."
    ) == ["unsafe_universal_fk_read_write_block_claim"]
    second_failed_live_answer = (
        "For PostgreSQL 9.2 or later, add the foreign key as NOT VALID. The NOT "
        "VALID approach still takes an ACCESS EXCLUSIVE lock briefly, which blocks "
        "all reads and writes. Use pg_repack or pt-online-schema-change if the lock "
        "is unacceptable."
    )
    second_failed_issues = set(
        large_fk_migration_safety_issues(second_failed_live_answer)
    )
    assert "unsafe_postgres_fk_access_exclusive_claim" in second_failed_issues
    assert "unsafe_universal_fk_read_write_block_claim" in second_failed_issues
    assert "unsafe_mysql_tool_in_postgres_migration_advice" in second_failed_issues
    assert "unsupported_eol_postgresql_migration_baseline" in second_failed_issues
    safe_answers = (
        "I would not create a new table, copy all rows, and rename it as the default. "
        "For PostgreSQL 15, use NOT VALID and validate separately in a monitored window.",
        "PostgreSQL constraint creation can require a brief lock window, but concurrent "
        "reads continue; exact write conflicts depend on the engine and version.",
        "Only if native online DDL is unavailable would I consider a shadow table as a "
        "last resort with a vetted online schema change tool and CDC for concurrent "
        "write sync: create a shadow table, copy the rows, then cut over.",
        "Never claim that foreign-key validation universally blocks all reads and writes.",
        "For PostgreSQL 17, ADD FOREIGN KEY uses SHARE ROW EXCLUSIVE on both tables, "
        "not ACCESS EXCLUSIVE; ordinary SELECT queries can continue.",
        "Most ALTER TABLE forms use ACCESS EXCLUSIVE, but ADD FOREIGN KEY NOT VALID "
        "uses only SHARE ROW EXCLUSIVE.",
        "Unlike ACCESS EXCLUSIVE, the SHARE ROW EXCLUSIVE lock used by ADD FOREIGN "
        "KEY still permits ordinary SELECT queries.",
        "For PostgreSQL 17, adding the foreign key takes SHARE ROW EXCLUSIVE, which "
        "is compatible with ordinary SELECTs but conflicts with ACCESS EXCLUSIVE operations.",
        "In PostgreSQL 17, ADD FOREIGN KEY NOT VALID takes a SHARE ROW EXCLUSIVE lock, "
        "which blocks conflicting ACCESS EXCLUSIVE DDL while ordinary reads continue.",
        "PostgreSQL 17 uses SHARE ROW EXCLUSIVE, so it doesn't require ACCESS EXCLUSIVE "
        "for ADD FOREIGN KEY NOT VALID.",
        "For a tested MySQL version, a vetted pt-online-schema-change workflow may "
        "be an explicit fallback with CDC and cutover monitoring.",
        "pt-online-schema-change is inappropriate for PostgreSQL foreign keys; use "
        "native NOT VALID and VALIDATE CONSTRAINT instead.",
        "Unlike pt-online-schema-change, PostgreSQL should use its native NOT VALID "
        "constraint workflow.",
        "Do not use PostgreSQL 13 as a migration baseline because it is end-of-life.",
        "I wouldn't use PostgreSQL 13 as a migration baseline because it's end-of-life.",
    )
    for answer in safe_answers:
        assert not large_fk_migration_safety_issues(answer), answer
    unsafe_mixed_engine = (
        "PostgreSQL supports NOT VALID. MySQL has different online DDL. For PostgreSQL, "
        "use pt-online-schema-change to avoid locking."
    )
    assert "unsafe_mysql_tool_in_postgres_migration_advice" in (
        large_fk_migration_safety_issues(unsafe_mixed_engine)
    )
    unsafe_orphan_query = (
        "Run SELECT COUNT(*) FROM child_table WHERE parent_id NOT IN "
        "(SELECT id FROM parent_table) to find orphaned rows."
    )
    assert set(large_fk_migration_safety_issues(unsafe_orphan_query)) == {
        "unsafe_not_in_orphan_preflight",
        "unsafe_unbounded_count_orphan_preflight",
    }
    assert "unsafe_unbounded_count_orphan_preflight" in (
        large_fk_migration_safety_issues(
            "Run SELECT COUNT(*) FROM child WHERE NOT EXISTS "
            "(SELECT 1 FROM parent WHERE parent.id = child.parent_id) LIMIT 1."
        )
    )
    for unsafe_count in (
        "Run SELECT COUNT(1) FROM child WHERE parent_id IS NOT NULL LIMIT 1 as the orphan preflight.",
        "Run SELECT COUNT(id) FROM child WHERE NOT EXISTS "
        "(SELECT 1 FROM parent WHERE parent.id = child.parent_id) LIMIT 1.",
    ):
        assert "unsafe_unbounded_count_orphan_preflight" in (
            large_fk_migration_safety_issues(unsafe_count)
        ), unsafe_count
    safe_orphan_queries = (
        "Scan child rows in bounded primary-key ranges and use NOT EXISTS against the "
        "parent primary key, writing violations to a review table.",
        "Use SELECT 1 FROM child c WHERE NOT EXISTS (SELECT 1 FROM parent p WHERE "
        "p.id = c.parent_id) LIMIT 1 for a bounded existence probe.",
        "Do not run SELECT COUNT(*) FROM child c LEFT JOIN parent p ON p.id = "
        "c.parent_id WHERE p.id IS NULL; scan bounded primary-key ranges instead.",
        "I would not use SELECT COUNT(*) FROM child WHERE parent_id NOT IN "
        "(SELECT id FROM parent). I would scan bounded ranges with NOT EXISTS.",
    )
    for answer in safe_orphan_queries:
        assert not large_fk_migration_safety_issues(answer), answer
    assert "unsafe_unbounded_count_orphan_preflight" in (
        large_fk_migration_safety_issues(
            "Run SELECT COUNT(*) FROM public.child c WHERE NOT EXISTS "
            "(SELECT 1 FROM public.parent p WHERE p.id=c.parent_id)."
        )
    )
    for safe_warning in (
        "I wouldn't run SELECT COUNT(*) FROM child WHERE NOT EXISTS "
        "(SELECT 1 FROM parent WHERE parent.id=child.parent_id); scan bounded ranges instead.",
        "SELECT COUNT(*) FROM child WHERE NOT EXISTS "
        "(SELECT 1 FROM parent WHERE parent.id=child.parent_id) is unsafe; avoid it.",
        "NOT IN (SELECT id FROM parent) is unsafe; use NOT EXISTS.",
    ):
        assert not large_fk_migration_safety_issues(safe_warning), safe_warning
    assert "unsafe_unbounded_count_orphan_preflight" in (
        large_fk_migration_safety_issues(
            "Do not avoid SELECT COUNT(*) FROM child WHERE NOT EXISTS "
            "(SELECT 1 FROM parent WHERE parent.id=child.parent_id)."
        )
    )
    assert "unsafe_not_in_orphan_preflight" in (
        large_fk_migration_safety_issues(
            "Do not avoid SELECT 1 FROM child WHERE parent_id NOT IN "
            "(SELECT id FROM parent)."
        )
    )

def self_check_exactly_once_processing_detector() -> None:
    unsafe = (
        "The platform provides exactly-once processing per idempotency key.",
        "We guarantee exactly-once processing per idempotency key, yielding "
        "exactly-once effects.",
        "Exactly-once effects are the goal, and the platform provides exactly-once "
        "processing per key.",
    )
    assert all(has_exactly_once_processing_overclaim(text) for text in unsafe)
    safe = (
        "We cannot guarantee exactly-once processing across the provider boundary; "
        "we provide idempotent exactly-once effects for each logical operation.",
        "Exactly-once processing is impossible across an external provider boundary.",
        "Exactly-once processing is not guaranteed end to end.",
        "I would not claim exactly-once processing across independent systems.",
        "Do not promise exactly-once processing; use idempotent effects.",
        "There is no global exactly-once processing guarantee.",
    )
    assert all(not has_exactly_once_processing_overclaim(text) for text in safe)

def self_check_ambiguous_payment_detector() -> None:
    safe = (
        "A timeout leaves the outcome UNKNOWN, not failed. "
        "Move it to PENDING_RECONCILIATION; do not mark it FAILED.",
        "After a timeout, never retry the charge; retry only the status lookup.",
        "After a timeout, never automatically retry the charge once per minute; "
        "reconcile provider status instead.",
        "Provider timeout after the charge attempt. Do not mark the payment FAILED or "
        "retry the charge; keep it UNKNOWN and reconcile. If the provider confirms no "
        "charge, transition to FAILED.",
        "The outcome is ambiguous. Only after the payment processor definitively "
        "confirms that the card was not charged, transition it to FAILED.",
        "The result remains PENDING_RECONCILIATION. Once receiving definitive "
        "confirmation from the gateway that no charge occurred, set it FAILED.",
        "The result is UNKNOWN. Only after the processor status lookup "
        "authoritatively confirms no authorization exists, transition it to FAILED.",
        "The outcome is ambiguous. Once a signed webhook from the gateway reports "
        "that the request was declined before authorization, mark it FAILED.",
        "Keep the payment UNKNOWN until the provider confirms no charge occurred, "
        "then mark it FAILED. Do not retry the charge with a new key.",
        "I would transition the payment intent from PROCESSING to UNKNOWN and stop "
        "automatic charge retries. Then I would reconcile using authoritative "
        "provider evidence, specifically a status check by payment ID or client "
        "reference, and webhooks persisted under a database uniqueness constraint "
        "on provider event ID. Those signals can move UNKNOWN to SUCCEEDED, FAILED, "
        "or CANCELED only when the provider evidence is conclusive. If the result is "
        "still inconclusive, I would keep it UNKNOWN and escalate to manual "
        "reconciliation. Only if the provider contract explicitly guarantees "
        "idempotent replay may I retry the exact same provider command under a "
        "bounded policy with the original operation's idempotency key, never a new "
        "key, and never a second charge.",
    )
    unsafe = (
        "After a timeout, mark the payment FAILED and retry the charge.",
        "The outcome is unknown. After maximum retries, transition it to FAILED.",
        "The outcome is unknown. If the provider has not confirmed whether a charge "
        "occurred, transition it to FAILED.",
        "After max retries, if the provider still has not confirmed a charge, "
        "transition to FAILED.",
        "After a timeout, if our database shows no charge, mark it FAILED.",
        "The provider confirms the timeout. Transition the payment to FAILED.",
        "After a timeout, our database shows no charge, so transition it to FAILED.",
        "After a timeout, do not mark it failed. Retry the charge immediately.",
        "We do not know whether the provider charged the card, and retry the payment.",
        "We don't know whether the charge succeeded, so retry the payment.",
        "UNKNOWN automatically becomes FAILED after 24 hours.",
        "After 24 hours, expire UNKNOWN as FAILED.",
        "UNKNOWN is considered FAILED when the reconciliation deadline expires.",
        "A scheduled job converts UNKNOWN into FAILED after one day.",
        "Those signals move UNKNOWN to FAILED even when provider evidence remains "
        "inconclusive.",
    )
    assert not any(has_unsafe_ambiguous_payment_outcome(text) for text in safe)
    assert all(has_unsafe_ambiguous_payment_outcome(text) for text in unsafe)

def self_check_payment_operation_semantics() -> None:
    round548_webhook_event_store = (
        "The inbound_event table is a dedupe store for webhook event IDs. "
        "A duplicate webhook is ignored by event ID uniqueness."
    )
    assert "missing_provider_event_id_webhook_dedup" not in (
        payment_operation_semantic_issues(
            round548_webhook_event_store,
            require_webhook_event_dedup=True,
            require_complete_idempotency_semantics=False,
        )
    )
    for wrong_webhook_dedup_identity in (
        "The inbound_event table is a dedupe store for payment IDs.",
        "The inbound_event table is a dedupe store for operation idempotency keys.",
    ):
        assert "missing_provider_event_id_webhook_dedup" in (
            payment_operation_semantic_issues(
                wrong_webhook_dedup_identity,
                require_webhook_event_dedup=True,
                require_complete_idempotency_semantics=False,
            )
        ), wrong_webhook_dedup_identity
    round548_saved_canary_wording = (
        "Webhook dedupe uses provider event ID, then transitions only from "
        "authoritative evidence."
    )
    assert "missing_provider_event_id_webhook_dedup" not in (
        payment_operation_semantic_issues(
            round548_saved_canary_wording,
            require_webhook_event_dedup=True,
            require_complete_idempotency_semantics=False,
        )
    )
    for negated_provider_event_id in (
        "Webhook dedupe does not use provider event ID.",
        "Webhook dedupe uses no provider event ID.",
        "Webhook dedupe uses provider event ID only for logging, not deduplication.",
    ):
        assert "missing_provider_event_id_webhook_dedup" in (
            payment_operation_semantic_issues(
                negated_provider_event_id,
                require_webhook_event_dedup=True,
                require_complete_idempotency_semantics=False,
            )
        ), negated_provider_event_id
    for wrong_identifier in ("payment ID", "operation key", "operation ID"):
        unsafe_webhook_identifier = (
            f"Webhook dedupe uses {wrong_identifier}, then transitions only from "
            "authoritative evidence."
        )
        assert "missing_provider_event_id_webhook_dedup" in (
            payment_operation_semantic_issues(
                unsafe_webhook_identifier,
                require_webhook_event_dedup=True,
                require_complete_idempotency_semantics=False,
            )
        ), unsafe_webhook_identifier
        assert "unsafe_webhook_dedup_by_operation_key" in (
            payment_operation_semantic_issues(
                unsafe_webhook_identifier,
                require_webhook_event_dedup=True,
                require_complete_idempotency_semantics=False,
            )
        ), unsafe_webhook_identifier
    complete_local_payment_boundaries = " ".join(
        (
            Q39_INGRESS_IDEMPOTENCY_SENTENCE,
            Q39_LEDGER_IDEMPOTENCY_SENTENCE,
            Q39_PARTIAL_ACTION_BOUNDARY_SENTENCE,
        )
    )
    assert not payment_q39_completeness_issues(complete_local_payment_boundaries)
    complete_payment_contract = (
        "Each logical provider operation instance gets its own stable idempotency "
        "key. Authorize, capture, and refund use distinct keys. Retries of the same "
        "logical operation reuse its original stable key. Deduplicate webhooks by "
        "provider event ID. Reconcile UNKNOWN outcomes from authoritative provider "
        f"status. {complete_local_payment_boundaries}"
    )
    assert not payment_operation_semantic_issues(
        complete_payment_contract,
        require_webhook_event_dedup=True,
    )
    line_wrapped_contract = complete_payment_contract.replace(
        "account and client idempotency key",
        "account and client\nidempotency key",
    )
    assert not payment_q39_completeness_issues(line_wrapped_contract)
    assert "unsafe_shared_idempotency_key_across_payment_operations" not in (
        payment_operation_semantic_issues(
            line_wrapped_contract,
            require_webhook_event_dedup=True,
        )
    )
    for hidden_sentence, expected_issue in (
        (
            f"<!-- {Q39_INGRESS_IDEMPOTENCY_SENTENCE} -->",
            "missing_client_idempotency_intent_mapping",
        ),
        (
            f"~~{Q39_LEDGER_IDEMPOTENCY_SENTENCE}~~",
            "missing_idempotent_local_ledger_posting",
        ),
        (
            f"```text\n{Q39_PARTIAL_ACTION_BOUNDARY_SENTENCE}\n```",
            "missing_partial_action_child_operation_boundary",
        ),
    ):
        assert expected_issue in payment_q39_completeness_issues(hidden_sentence)
    unsafe_client_key_reuse = (
        "Authorize, capture, and refund reuse the client idempotency key."
    )
    assert "unsafe_shared_idempotency_key_across_payment_operations" in (
        payment_operation_semantic_issues(
            unsafe_client_key_reuse,
            require_webhook_event_dedup=False,
        )
    )
    safe_partial_action_bullets = (
        "- New partial capture or partial refund = new logical action, new key.\n"
        "- Retry of that exact partial action = same key."
    )
    assert "unsafe_shared_idempotency_key_across_payment_operations" not in (
        payment_operation_semantic_issues(
            safe_partial_action_bullets,
            require_webhook_event_dedup=False,
        )
    )
    contradicted_partial_action_bullets = (
        f"{safe_partial_action_bullets}\n"
        "- The same key is used for both operations."
    )
    assert "unsafe_shared_idempotency_key_across_payment_operations" in (
        payment_operation_semantic_issues(
            contradicted_partial_action_bullets,
            require_webhook_event_dedup=False,
        )
    )
    contradicted_partial_action_clause = (
        "New partial capture or partial refund is a new logical action with a new "
        "key, retry of that exact partial action reuses the same key, capture and "
        "refund reuse an idempotency key."
    )
    assert "unsafe_shared_idempotency_key_across_payment_operations" in (
        payment_operation_semantic_issues(
            contradicted_partial_action_clause,
            require_webhook_event_dedup=False,
        )
    )
    for omitted_sentence, expected_issue in (
        (
            Q39_INGRESS_IDEMPOTENCY_SENTENCE,
            "missing_client_idempotency_intent_mapping",
        ),
        (
            Q39_LEDGER_IDEMPOTENCY_SENTENCE,
            "missing_idempotent_local_ledger_posting",
        ),
        (
            Q39_PARTIAL_ACTION_BOUNDARY_SENTENCE,
            "missing_partial_action_child_operation_boundary",
        ),
    ):
        incomplete = complete_local_payment_boundaries.replace(omitted_sentence, "")
        assert payment_q39_completeness_issues(incomplete) == [expected_issue]

    safe = (
        "Give each logical provider operation, such as authorize, capture, or refund, "
        "its own stable idempotency key, and reuse that same key only when replaying "
        "that same operation. Deduplicate webhooks by provider event ID under a "
        "unique constraint. UNKNOWN blocks a new charge.",
        "Retry the same authorization operation with the same stable idempotency key. "
        "Derive separate keys for authorize, capture, and refund. Deduplicate signed "
        "webhooks by the provider event ID stored under a unique constraint. If the "
        "outcome is UNKNOWN, block another charge and reconcile by provider status.",
        "Never generate a fresh idempotency key on retry, and never share one key "
        "across authorize, capture, and refund. Reuse the same stable key only when "
        "replaying that same logical operation. Deduplicate webhooks by the processor "
        "event ID stored under a unique constraint. Do not use the operation key for "
        "webhook deduplication.",
        "Each authorize, capture, and refund operation gets a unique idempotency key; "
        "every retry reuses that operation's same stable key. Deduplicate webhooks by "
        "provider_event_id under a unique constraint. A timeout remains UNKNOWN and "
        "blocks any new charge.",
        "Every logical operation instance, including each partial capture and refund, "
        "gets its own durable idempotency key. Authorize, capture, and refund have "
        "separate keys, and the exact command preserves its key across retry attempts. "
        "Never rotate the provider key and never retry without it. Do not use a global "
        "constant key. Deduplicate webhooks by provider event ID.",
        "Namespace the idempotency key by merchant ID, payment ID, operation kind, "
        "and operation ID. A retry deterministically recomputes the identical key. "
        "Authorizations, partial captures, and refunds have different keys. "
        "Deduplicate webhooks by provider event ID under a unique constraint.",
        "Each logical provider-operation instance gets its own stable idempotency "
        "key scoped to owning account, payment, operation type, and operation "
        "instance. A new partial capture or partial refund is a new logical action "
        "with a new key. A retry of that exact partial action reuses its original "
        "key. Authorization, capture, and refund use separate keys. Retries of the "
        "same authorization, capture, or refund reuse the same key and must not "
        "create a second effect. Deduplicate webhooks by provider event ID under a "
        "unique constraint.",
    )
    for value in safe:
        assert not payment_operation_semantic_issues(
            value, require_webhook_event_dedup=True
        ), value

    # The grouped retry summary is safe because the preceding rule scopes a
    # stable key to each provider-operation instance. It must not be confused
    # with a single key shared across authorize, capture, and refund.
    grouped_same_operation_retry_summary = (
        "Each logical provider-operation instance gets its own stable idempotency key "
        "scoped to owning account, payment, operation type, and operation instance. "
        "A new partial capture or partial refund is a new logical action with a new "
        "key. A retry of that exact partial action reuses the original key. Retries "
        "of the same authorization, capture, or refund reuse the same key and must "
        "not create a second effect. Deduplicate webhooks by provider event ID."
    )
    assert "unsafe_shared_idempotency_key_across_payment_operations" not in (
        payment_operation_semantic_issues(
            grouped_same_operation_retry_summary,
            require_webhook_event_dedup=True,
        )
    )

    unsafe = (
        (
            "Generate a fresh idempotency key for every retry. Deduplicate webhooks "
            "using provider event ID under a unique constraint.",
            "unsafe_new_idempotency_key_on_retry",
        ),
        (
            "Replay the payment with a different idempotency key. Deduplicate webhooks "
            "using provider event ID under a unique constraint.",
            "unsafe_new_idempotency_key_on_retry",
        ),
        (
            "On every replay, select new idempotency keys. Deduplicate webhooks using "
            "provider event ID under a unique constraint.",
            "unsafe_new_idempotency_key_on_retry",
        ),
        (
            "Rotate the provider idempotency key between retry attempts. Deduplicate "
            "webhooks using provider event ID.",
            "unsafe_new_idempotency_key_on_retry",
        ),
        (
            "Each provider attempt gets a fresh random UUID as its idempotency key. "
            "Deduplicate webhooks using provider event ID.",
            "unsafe_new_idempotency_key_on_retry",
        ),
        (
            "The provider key is not stable across retries and we rotate it.",
            "unsafe_new_idempotency_key_on_retry",
        ),
        (
            "After timeout regenerate the key.",
            "unsafe_new_idempotency_key_on_retry",
        ),
        (
            "Derive the retry key by appending the attempt number.",
            "unsafe_new_idempotency_key_on_retry",
        ),
        (
            "Derive the retry idempotency key by appending the attempt number to the "
            "original key.",
            "unsafe_new_idempotency_key_on_retry",
        ),
        (
            "Refresh the provider key after timeout.",
            "unsafe_new_idempotency_key_on_retry",
        ),
        (
            "Discard the old key and mint a successor.",
            "unsafe_new_idempotency_key_on_retry",
        ),
        (
            "Use an attempt-specific nonce.",
            "unsafe_new_idempotency_key_on_retry",
        ),
        (
            "Retry the provider request without an idempotency key. Deduplicate "
            "webhooks using provider event ID.",
            "unsafe_missing_idempotency_key_on_retry",
        ),
        (
            "The idempotency key may be omitted on retries. Deduplicate webhooks "
            "using provider event ID.",
            "unsafe_missing_idempotency_key_on_retry",
        ),
        (
            "Omit idempotency after the first attempt.",
            "unsafe_missing_idempotency_key_on_retry",
        ),
        (
            "Use one global constant idempotency key across all customer payments. "
            "Deduplicate webhooks using provider event ID.",
            "unsafe_constant_idempotency_key",
        ),
        (
            "Every tenant shares the same fixed key for all payment requests. "
            "Deduplicate webhooks using provider event ID.",
            "unsafe_constant_idempotency_key",
        ),
        (
            "Every provider call uses the payment's constant key across all operations.",
            "unsafe_constant_idempotency_key",
        ),
        (
            "Use one shared idempotency key across authorize, capture, and refund. "
            "Deduplicate webhooks using provider event ID under a unique constraint.",
            "unsafe_shared_idempotency_key_across_payment_operations",
        ),
        (
            "Each logical provider-operation instance gets its own stable idempotency "
            "key. Retries of the same authorization, capture, or refund reuse the same "
            "key; that same key is used across all three operations. Deduplicate "
            "webhooks by provider event ID.",
            "unsafe_shared_idempotency_key_across_payment_operations",
        ),
        (
            "Each logical provider-operation instance gets its own stable idempotency "
            "key. Retries of the same authorization, capture, or refund reuse the same "
            "key. Authorization, capture, and refund all use that key. Deduplicate "
            "webhooks by provider event ID.",
            "unsafe_shared_idempotency_key_across_payment_operations",
        ),
        (
            "Each logical provider-operation instance gets its own stable idempotency "
            "key. Retries of the same authorization, capture, or refund reuse the same "
            "key. The key applies to all three operations. Deduplicate webhooks by "
            "provider event ID.",
            "unsafe_shared_idempotency_key_across_payment_operations",
        ),
        (
            "Each logical provider-operation instance gets its own stable idempotency "
            "key. Authorization, capture, and refund use separate keys. Retries of the "
            "same authorization, capture, or refund reuse the same key; one key covers "
            "all three operations. Deduplicate webhooks by provider event ID.",
            "unsafe_shared_idempotency_key_across_payment_operations",
        ),
        (
            "Each logical provider-operation instance gets its own stable idempotency "
            "key. Authorization, capture, and refund use separate keys. Retries of the "
            "same authorization, capture, or refund reuse the same key; a single key "
            "spans all three operations. Deduplicate webhooks by provider event ID.",
            "unsafe_shared_idempotency_key_across_payment_operations",
        ),
        (
            "Each logical provider-operation instance gets its own stable idempotency "
            "key. Authorization, capture, and refund use separate keys. Retries of the "
            "same authorization, capture, or refund reuse the same key; the same key "
            "spans all three operations. Deduplicate webhooks by provider event ID.",
            "unsafe_shared_idempotency_key_across_payment_operations",
        ),
        (
            "Each logical provider-operation instance gets its own stable idempotency "
            "key. Authorization, capture, and refund use separate keys. Retries of the "
            "same authorization, capture, or refund reuse the same key; one shared key "
            "is used for all operations. Deduplicate webhooks by provider event ID.",
            "unsafe_shared_idempotency_key_across_payment_operations",
        ),
        (
            "Reuse the same key for authorization and capture. Deduplicate webhooks "
            "using provider event ID under a unique constraint.",
            "unsafe_shared_idempotency_key_across_payment_operations",
        ),
        (
            "Use one shared key. Authorization happens first. Capture reuses it, and "
            "refund reuses it too. Deduplicate webhooks using provider event ID.",
            "unsafe_shared_idempotency_key_across_payment_operations",
        ),
        (
            "Authorization uses key A. Capture reuses it in the next step. Refund "
            "gets a separate key. Deduplicate webhooks using provider event ID.",
            "unsafe_shared_idempotency_key_across_payment_operations",
        ),
        (
            "Deduplicate webhooks using the operation idempotency key.",
            "unsafe_webhook_dedup_by_operation_key",
        ),
        (
            "Webhook deduplication is keyed by payment_id.",
            "unsafe_webhook_dedup_by_operation_key",
        ),
        (
            "Process signed webhooks and update the payment state.",
            "missing_provider_event_id_webhook_dedup",
        ),
        (
            "After a timeout leaves the outcome UNKNOWN, create another charge with "
            "a fresh idempotency key.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
        (
            "After the outcome becomes UNKNOWN, issue another authorization with a "
            "fresh key.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
        (
            "If the payment remains UNKNOWN, send a new payment under a different "
            "idempotency key.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
        (
            "Following a provider timeout, send a new payment request.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
        (
            "After the timeout leaves the result UNKNOWN, submit the charge using a "
            "fresh idempotency key.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
        (
            "After the timeout leaves the result UNKNOWN, use a fresh idempotency "
            "key; then submit the charge.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
        (
            "The timeout leaves the payment UNKNOWN. Submit the payment. Use a "
            "fresh idempotency key for that command.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
        (
            "The timeout leaves the payment UNKNOWN. Submit the payment. It gets "
            "a new idempotency key.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
        (
            "The timeout leaves the payment UNKNOWN. Submit the payment. Give it "
            "a fresh idempotency key.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
        (
            "The timeout leaves the payment UNKNOWN. Give that command a fresh "
            "idempotency key. Then submit the charge.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
        (
            "The timeout leaves the payment UNKNOWN. Select a fresh key. Record the "
            "retry reason. Then submit the charge.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
        (
            "The payment is UNKNOWN. Then put a different idempotency token on the "
            "command and submit the charge.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
        (
            "The payment is UNKNOWN. Then use another idempotency key and submit "
            "the charge.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
        (
            "The payment is UNKNOWN. Then generate a fresh key and charge the card.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
        (
            "The payment is UNKNOWN. Generate a fresh key and call the provider to "
            "charge the card.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
        (
            "The payment is UNKNOWN. Set a fresh idempotency key. Then submit the "
            "charge.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
        (
            "The payment is UNKNOWN. Rotate the idempotency key. Then submit the "
            "charge.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
        (
            "The payment is UNKNOWN. Do not keep the original idempotency key but "
            "use a fresh idempotency key and submit the charge.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
        (
            "The payment is UNKNOWN. Although we do not reuse the old key, we submit "
            "the charge with a fresh key.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
        (
            "The payment is UNKNOWN. We avoid the original key and submit the charge "
            "under a replacement idempotency key.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
        (
            "The payment is UNKNOWN. Apply a fresh idempotency key. Then submit the "
            "charge.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
        (
            "The payment is UNKNOWN. Attach a fresh idempotency key. Then submit the "
            "charge.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
        (
            "The payment is UNKNOWN. Associate a fresh key with the command. Then "
            "submit the charge.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
        (
            "The payment is UNKNOWN. Bind a fresh key to the command. Then submit "
            "the charge.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
        (
            "The payment is UNKNOWN. Tag the command with a fresh key. Then submit "
            "the charge.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
        (
            "The payment is UNKNOWN. Generate a fresh key and run the charge command.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
        (
            "The payment is UNKNOWN. Generate a fresh key and kick off the charge.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
        (
            "The payment is UNKNOWN. Generate a fresh key and invoke the payment "
            "command.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
        (
            "The payment is UNKNOWN. Generate a fresh key and enqueue the charge.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
        (
            "The payment is UNKNOWN. Generate a fresh key and perform the charge.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
        (
            "The payment is UNKNOWN. Generate a fresh key and fire the payment request.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
        (
            "The payment is UNKNOWN. Stamp the command with a fresh key. Then submit "
            "the charge.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
        (
            "The payment is UNKNOWN. Generate a fresh key and process the payment.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
        (
            "The payment is UNKNOWN. Generate a fresh key and reissue the payment "
            "request.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
        (
            "The payment is UNKNOWN. Generate a fresh key and rerun the provider "
            "command.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
        (
            "The payment is UNKNOWN. Generate a fresh key. Record one. Record two. "
            "Record three. Record four. Record five. Record six. Then submit the "
            "charge.",
            "unsafe_new_charge_after_ambiguous_outcome",
        ),
    )
    for value, expected in unsafe:
        found = payment_operation_semantic_issues(
            value, require_webhook_event_dedup=True
        )
        assert expected in found, (value, found)

    incomplete_positive_contracts = (
        (
            "Use idempotency keys for provider calls. Deduplicate webhooks by provider "
            "event ID and reconcile UNKNOWN payments.",
            {
                "missing_stable_idempotency_key_per_operation",
                "missing_distinct_authorize_capture_refund_keys",
                "missing_same_operation_idempotency_key_reuse",
            },
        ),
        (
            "Give every logical operation a stable idempotency key and reuse it on "
            "same-operation retries. Deduplicate webhooks by provider event ID.",
            {"missing_distinct_authorize_capture_refund_keys"},
        ),
        (
            "Authorize, capture, and refund use distinct operation-scoped keys. "
            "Deduplicate webhooks by provider event ID.",
            {"missing_same_operation_idempotency_key_reuse"},
        ),
        (
            "Give every logical operation a stable idempotency key and reuse the same "
            "key only for same-operation retries. Authorize, capture, and refund do "
            "not use distinct keys. Deduplicate webhooks by provider event ID.",
            {"missing_distinct_authorize_capture_refund_keys"},
        ),
    )
    for value, expected in incomplete_positive_contracts:
        found = set(
            payment_operation_semantic_issues(
                value,
                require_webhook_event_dedup=True,
            )
        )
        assert expected.issubset(found), (value, found)

    query_only = (
        "Keep the result UNKNOWN, do not submit the charge again, block another "
        "charge, and query provider status. Reuse the same stable idempotency key only "
        "when replaying that same original logical operation. Do not retry the charge "
        "with a new idempotency key."
    )
    assert not payment_operation_semantic_issues(
        query_only,
        require_webhook_event_dedup=False,
        require_complete_idempotency_semantics=False,
        require_same_operation_retry_reuse=True,
    )
    live_q39_wording = (
        "Authorize, capture, and refund each get a unique idempotency key. Provider "
        "calls reuse the same idempotency key for retries of the same operation. "
        "Deduplicate webhooks by provider event ID."
    )
    assert not payment_operation_semantic_issues(
        live_q39_wording,
        require_webhook_event_dedup=True,
    )
    deployed_q39_wording = (
        "Each authorize, capture, or refund gets its own stable idempotency key. "
        "The worker uses the same idempotency key for the same logical operation "
        "replay. Deduplicate webhook events by provider event ID."
    )
    assert not payment_operation_semantic_issues(
        deployed_q39_wording,
        require_webhook_event_dedup=True,
    )
    round538_q39_wording = (
        "Support payment operations like authorize, capture, and refund. The API "
        "normalizes the logical operation, then generates or accepts a client "
        "idempotency key. In a single DB transaction it records operation state and "
        "an outbox command. Provider retries use the same logical operation and the "
        "same stable provider idempotency key. Deduplicate webhooks by provider event "
        "ID under a unique constraint."
    )
    assert set(
        payment_operation_semantic_issues(
            round538_q39_wording,
            require_webhook_event_dedup=True,
        )
    ) == {"missing_distinct_authorize_capture_refund_keys"}
    saved_round539_q39_spoken = (
        "I would build the system so every payment action is persisted first, then sent "
        "to the provider through a transactional outbox, with a stable idempotency key "
        "per account, payment, operation type, and operation instance. I would treat "
        "provider timeouts as UNKNOWN or PENDING_RECONCILIATION, never as terminal "
        "failure, and I would block any second effect until reconciliation proves the "
        "outcome. I would keep an immutable double-entry ledger and append authorization, "
        "capture, and refund movements only from authoritative provider evidence."
    )
    assert set(
        payment_operation_semantic_issues(
            saved_round539_q39_spoken,
            require_webhook_event_dedup=False,
        )
    ) == {
        "missing_distinct_authorize_capture_refund_keys",
        "missing_same_operation_idempotency_key_reuse",
    }
    saved_round539_q39_canvas = (
        "- Before any provider call, atomically persist a payment intent and transactional "
        "outbox command.\n"
        "- Each logical provider-operation instance gets its own stable idempotency key, "
        "scoped to owning account, payment, operation type, and operation instance.\n"
        "- Reuse the same key for retries of the same logical operation instance.\n"
        "- Use different keys for authorization, each capture, and each refund.\n"
        "- Append confirmed money movements to an immutable double-entry ledger only "
        "after authoritative provider evidence.\n"
        "- Deduplicate webhooks by provider event ID.\n"
        "- Reconcile an ambiguous timeout by provider payment ID, client reference, or "
        "webhook while the outcome remains UNKNOWN.\n"
        "- Do not use a distributed lock, or Redis lock, as the correctness boundary.\n"
        "4. If provider responds synchronously, persist result and append ledger movement.\n"
        "- Partial capture or refund retry\n"
        "  - new logical operation instance, new idempotency key"
    )
    assert payment_operation_semantic_issues(
        saved_round539_q39_canvas,
        require_webhook_event_dedup=True,
    ) == ["unsafe_new_idempotency_key_on_retry"]
    assert payment_platform_safety_issues(saved_round539_q39_canvas) == [
        "unsafe_unqualified_payment_ledger_movement"
    ]
    # Exact idempotency-rule shape from the Q39 live canvas.  These adjacent
    # bullets deliberately mention partial capture/refund and a same-key retry,
    # but establish a new operation instance for each new partial action.
    canary_q39_canvas_idempotency_rules = (
        "One stable key per logical provider-operation instance.\n"
        "New partial capture or partial refund = new logical action, new key.\n"
        "Retry of that exact partial action = same key.\n"
        "Never share one key across different operation instances."
    )
    assert "unsafe_shared_idempotency_key_across_payment_operations" not in (
        payment_operation_semantic_issues(
            canary_q39_canvas_idempotency_rules,
            require_webhook_event_dedup=False,
            require_complete_idempotency_semantics=False,
        )
    )
    round544_live_q39_rules = (
        "Each logical provider-operation instance gets its own idempotency key, "
        "scoped to owning account, payment, operation type, and operation instance. "
        "A new partial capture or partial refund is a new logical action with a new "
        "key. A retry of that exact partial action reuses the original key. Do not "
        "create a second charge command after a timeout. Keep ambiguous outcomes as "
        "UNKNOWN or PENDING_RECONCILIATION until reconciled. Client submits payment "
        "request. Webhook consumer deduplicates by provider event ID. Partial "
        "capture/refund retry: reuse the same partial-action key, but a new partial "
        "action gets a new key."
    )
    assert not payment_operation_semantic_issues(
        round544_live_q39_rules,
        require_webhook_event_dedup=True,
        require_complete_idempotency_semantics=False,
    )
    round546_live_q39_partial_boundary = (
        "Each logical provider-operation instance gets its own stable idempotency "
        "key. I give each authorization, capture, and refund, including each partial "
        "capture or refund, its own stable idempotency key; retries of that same "
        "operation reuse the original key. A new partial capture or refund is a new "
        "child provider-operation row under the existing payment intent. A retry of "
        "that exact partial action reuses the original key and same child row. Duplicate "
        "client submission, same account plus client idempotency key: return the "
        "original stored intent. Retry "
        "of the same provider operation: reuse the original provider-operation key. "
        "New partial capture or refund: create a new child operation with a new key. "
        "Retry of that same partial action: reuse the child operation key."
    )
    assert "unsafe_shared_idempotency_key_across_payment_operations" not in (
        payment_operation_semantic_issues(
            round546_live_q39_partial_boundary,
            require_webhook_event_dedup=False,
            require_complete_idempotency_semantics=False,
        )
    )
    for unsafe_round546_partial_boundary in (
        "A new partial capture or refund is a new child provider-operation row. "
        "A retry of that exact partial action reuses the original key and same child row.",
        f"{round546_live_q39_partial_boundary} Authorization, capture, and refund "
        "share one idempotency key.",
    ):
        assert "unsafe_shared_idempotency_key_across_payment_operations" in (
            payment_operation_semantic_issues(
                unsafe_round546_partial_boundary,
                require_webhook_event_dedup=False,
                require_complete_idempotency_semantics=False,
            )
        ), unsafe_round546_partial_boundary
    for unsafe_round546_child_key_sharing in (
        "A partial capture shares the child operation key of a partial refund.",
        "The child key is common between partial captures and refunds.",
        "Every partial action uses the same original key.",
        "A partial refund and partial capture have a common child operation key.",
        "The child operation key for partial capture is reused by partial refund.",
        "The original key is common to every partial action.",
        "All partial actions retain one original idempotency key.",
        "A partial capture and refund share their original key.",
        "Partial capture and refund must not share a child key, but they share one "
        "child operation key here.",
        "Do not reuse a partial capture key for a partial refund, but the partial "
        "refund inherits it in this recovery flow.",
        "A partial capture does not share its child key with a refund, yet the refund "
        "uses the capture child key after retry.",
        "The key assigned to a partial capture is also assigned to a partial refund.",
        "Both partial actions retain their common original key.",
        "The original idempotency key spans every partial capture and refund.",
    ):
        assert "unsafe_shared_idempotency_key_across_payment_operations" in (
            payment_operation_semantic_issues(
                f"{round546_live_q39_partial_boundary} "
                f"{unsafe_round546_child_key_sharing}",
                require_webhook_event_dedup=False,
                require_complete_idempotency_semantics=False,
            )
        ), unsafe_round546_child_key_sharing
    for safe_partial_retry_summary in (
        "Partial capture/refund retry: reuse the same partial-action key, and a new "
        "partial action gets a new key.",
        "Partial capture/refund retry: reuse the same partial-action key; a new "
        "partial action gets a new key.",
        "Partial capture/refund retry: reuse the same partial-action key. A new "
        "partial action gets a new key.",
        "For a retry of the exact partial capture or refund, reuse its original key; "
        "for a new partial action, mint a new key.",
        "Partial capture/refund retry reuses the same partial-action key, but any new "
        "partial action receives a fresh key.",
    ):
        assert "unsafe_shared_idempotency_key_across_payment_operations" not in (
            payment_operation_semantic_issues(
                "Each logical provider-operation instance gets its own idempotency "
                "key. "
                + safe_partial_retry_summary,
                require_webhook_event_dedup=False,
                require_complete_idempotency_semantics=False,
            )
        ), safe_partial_retry_summary
    for safe_cross_operation_negation in (
        "A capture retry does not carry the refund token; it keeps its own original "
        "key.",
        "A refund retry never borrows the capture key; it reuses its own.",
    ):
        assert "unsafe_shared_idempotency_key_across_payment_operations" not in (
            payment_operation_semantic_issues(
                "Each logical provider-operation instance gets its own idempotency "
                "key. "
                + safe_cross_operation_negation,
                require_webhook_event_dedup=False,
                require_complete_idempotency_semantics=False,
            )
        ), safe_cross_operation_negation
    safe_negated_adjacent_key = (
        "After a timeout leaves the result UNKNOWN, do not use a fresh key. "
        "Submit a payment status query instead of a second charge."
    )
    assert "unsafe_new_charge_after_ambiguous_outcome" not in (
        payment_operation_semantic_issues(
            safe_negated_adjacent_key,
            require_webhook_event_dedup=False,
            require_complete_idempotency_semantics=False,
        )
    )
    safe_initial_flow_before_ambiguity = (
        "A new payment gets a new idempotency key. Client submits the payment. "
        "If the provider later times out, mark the result UNKNOWN."
    )
    assert "unsafe_new_charge_after_ambiguous_outcome" not in (
        payment_operation_semantic_issues(
            safe_initial_flow_before_ambiguity,
            require_webhook_event_dedup=False,
            require_complete_idempotency_semantics=False,
        )
    )
    for safe_separate_flow in (
        "The timeout path leaves UNKNOWN and blocks a second charge. A new partial "
        "refund is a separate action and gets a new key. In the normal initial "
        "flow, the client submits payment to the API.",
        "UNKNOWN outcomes stay pending reconciliation. Separately, each new partial "
        "capture gets a new key. For an unrelated new customer purchase, the client "
        "submits payment.",
        "The payment is UNKNOWN. Do not attach a fresh key or submit another charge; "
        "run a status query.",
        "The payment is UNKNOWN. In the normal initial flow for an unrelated new "
        "purchase, attach its new operation key and submit the payment.",
    ):
        assert "unsafe_new_charge_after_ambiguous_outcome" not in (
            payment_operation_semantic_issues(
                safe_separate_flow,
                require_webhook_event_dedup=False,
                require_complete_idempotency_semantics=False,
            )
        ), safe_separate_flow
    for unsafe_partial_action_contrast in (
        "Partial capture/refund retry: reuse the same partial-action key, and a new "
        "partial action also gets the same key.",
        "Partial capture/refund retry: reuse the same partial-action key for both "
        "operations, but a new partial action gets a new key.",
        "Partial capture/refund retry: reuse the same partial-action key across the "
        "two operation types, but a new partial action gets a new key.",
        "Partial capture/refund retries reuse the same partial-action key across the "
        "two operation types, but a new partial action gets a new key.",
        "Partial capture/refund retry: reuse the same partial-action key regardless "
        "of whether it is capture or refund, but a new partial action gets a new key.",
        "Partial capture/refund retry: reuse the same partial-action key for either "
        "operation, but a new partial action gets a new key.",
        "Partial capture/refund retry: reuse the same partial-action key between "
        "them, but a new partial action gets a new key.",
        "Partial capture and refund retries carry an identical idempotency token, "
        "but a new partial action gets a new key.",
        "A refund retry borrows the capture idempotency key, but a new partial "
        "action gets a new key.",
        "Capture does not use its own key; instead it shares the refund idempotency "
        "key.",
        "Capture does not use a separate key and instead it shares the refund "
        "idempotency key.",
        "A capture retry carries the refund idempotency token, but a new partial "
        "action gets a new key.",
        "A refund retry takes the capture idempotency key, but a new partial action "
        "gets a new key.",
        "Capture does not keep its key; it borrows the refund idempotency key.",
        "Capture does not map to a distinct token; rather, it uses the refund "
        "idempotency token.",
        "A capture retry runs under the refund idempotency key.",
        "A capture retry is keyed with the refund idempotency token.",
        "Capture does not use a separate key and instead shares the refund key.",
    ):
        assert "unsafe_shared_idempotency_key_across_payment_operations" in (
            payment_operation_semantic_issues(
                "Each logical provider-operation instance gets its own idempotency key. "
                + unsafe_partial_action_contrast,
                require_webhook_event_dedup=False,
                require_complete_idempotency_semantics=False,
            )
        ), unsafe_partial_action_contrast
    canary_q39_cross_operation_contradictions = (
        "Never share one key between authorization and refund, but authorization "
        "and capture share the same idempotency key.",
        "Capture and refund map to a common idempotency key.",
        "Capture inherits the authorization idempotency key.",
        "The authorization idempotency key is also used for capture.",
        "Capture and refund share one idempotency token.",
        "Although authorization and refund have separate keys, authorization and "
        "capture have a common key.",
        "Even though capture and refund do not map to a common key, capture uses "
        "authorization's key.",
        "Except for refund, authorization and capture have a common idempotency token.",
        "Capture and refund have a common key.",
        "Capture uses authorization's key.",
        "The authorization idempotency key doubles as the capture key.",
        "Never share one key between authorization and refund but authorization and "
        "capture share the same idempotency key.",
        "Never share one key between authorization and refund although authorization "
        "and capture share the same idempotency key.",
        "Never share one key between authorization and refund even though authorization "
        "and capture share the same idempotency key.",
        "Never share one key between authorization and refund except authorization and "
        "capture share the same idempotency key.",
        "Capture maps to authorization's idempotency key.",
        "The capture token aliases the authorization token.",
    )
    for contradiction in canary_q39_cross_operation_contradictions:
        assert "unsafe_shared_idempotency_key_across_payment_operations" in (
            payment_operation_semantic_issues(
                f"{canary_q39_canvas_idempotency_rules} {contradiction}",
                require_webhook_event_dedup=False,
                require_complete_idempotency_semantics=False,
            )
        ), contradiction
    for rejected_sharing_claim in (
        "Capture and refund do not map to a common key.",
        "Capture must not inherit the authorization idempotency key.",
    ):
        assert "unsafe_shared_idempotency_key_across_payment_operations" not in (
            payment_operation_semantic_issues(
                f"{canary_q39_canvas_idempotency_rules} {rejected_sharing_claim}",
                require_webhook_event_dedup=False,
                require_complete_idempotency_semantics=False,
            )
        ), rejected_sharing_claim
    negated_retry_rules = (
        "Do not reuse the same idempotency key for retries of the same operation.",
        "Avoid reusing the same idempotency key for retries of the same operation.",
        "Do not retry the same operation with the same idempotency key.",
        "Never replay the original charge operation using the original idempotency key.",
        "The same operation must not keep its idempotency key when replayed.",
        "Do not use the same idempotency key for the same logical operation replay.",
        "Never use the same idempotency key for the same logical operation replay.",
    )
    for negated_rule in negated_retry_rules:
        negated_retry_reuse = (
            "Authorize, capture, and refund each get a unique idempotency key. "
            + negated_rule
            + " Deduplicate webhooks by provider event ID."
        )
        assert "missing_same_operation_idempotency_key_reuse" in (
            payment_operation_semantic_issues(
                negated_retry_reuse,
                require_webhook_event_dedup=True,
            )
        ), negated_rule
    live_q40_wording = (
        "The original charge operation keeps its idempotency key so it can be "
        "replayed if needed. Keep the intent UNKNOWN, block a new charge, query "
        "provider status, and deduplicate webhooks by provider event ID."
    )
    assert not payment_operation_semantic_issues(
        live_q40_wording,
        require_webhook_event_dedup=False,
        require_complete_idempotency_semantics=False,
        require_same_operation_retry_reuse=True,
    )
    deployed_q40_wording = (
        "The provider status checks by payment ID or client reference and "
        "deduplicated webhook events determine the confirmed terminal state. The "
        "original operation's idempotency key is reused only if the same provider "
        "command must be replayed; it is not the webhook deduplication key."
    )
    assert payment_operation_semantic_issues(
        deployed_q40_wording,
        require_webhook_event_dedup=False,
        require_complete_idempotency_semantics=False,
        require_same_operation_retry_reuse=True,
    ) == ["missing_provider_event_id_webhook_dedup"]
    assert not payment_operation_semantic_issues(
        deployed_q40_wording.replace(
            "deduplicated webhook events",
            "webhook events deduplicated by provider event ID",
        ),
        require_webhook_event_dedup=False,
        require_complete_idempotency_semantics=False,
        require_same_operation_retry_reuse=True,
    )
    # Exact Q40 policy shape: a replay is a provider-guaranteed exception to a
    # fail-closed timeout path, not an unbounded automatic retry loop.
    canary_q40_replay_policy = (
        "Transition the payment to UNKNOWN and stop automatic charge retries. "
        "Provider status checks and webhooks reconcile it first. Only if the result "
        "remains inconclusive and the provider contract guarantees idempotent replay "
        "may the exact same provider command be retried under the original operation's "
        "idempotency key, never a new key. If unresolved, escalate to manual "
        "reconciliation. Webhooks are deduplicated by provider event ID; the operation "
        "key is not the webhook deduplication key. "
        "The idempotency key is scoped to the specific logical action, including a "
        "partial capture or refund, and its instance; a new partial action gets a new "
        "key, while only the exact same action can reuse its original key."
    )
    assert has_safe_payment_same_operation_replay_condition(canary_q40_replay_policy)
    assert not payment_operation_semantic_issues(
        canary_q40_replay_policy,
        require_webhook_event_dedup=False,
        require_complete_idempotency_semantics=False,
        require_same_operation_retry_reuse=True,
    )
    round546_live_q40 = (
        "I would transition the payment intent from PROCESSING to UNKNOWN and stop "
        "automatic charge retries. I would reconcile by provider payment ID or client "
        "reference and authoritative webhooks persisted under a database uniqueness "
        "constraint on provider event ID. If the result still stays inconclusive, "
        "I would not create a new charge. Only if the provider contract explicitly "
        "guarantees idempotent replay would I retry the exact same provider command "
        "under a bounded policy with the original operation's idempotency key, not a "
        "new key. If it remains unresolved, I keep it UNKNOWN and escalate to manual "
        "reconciliation."
    )
    assert has_safe_payment_same_operation_replay_condition(round546_live_q40)
    assert not payment_operation_semantic_issues(
        round546_live_q40,
        require_webhook_event_dedup=False,
        require_complete_idempotency_semantics=False,
        require_same_operation_retry_reuse=True,
    )
    # Correct scope language must never mask an affirmative pairwise share.
    for unsafe_pairwise_share in (
        "Each logical provider-operation instance gets its own stable idempotency key. "
        "Capture and refund share one idempotency key.",
        "One stable key per logical provider-operation instance. New partial capture "
        "or partial refund is a new logical action with a new key. Retry of that exact "
        "partial action reuses its original key. Capture and refund use the same key.",
    ):
        assert "unsafe_shared_idempotency_key_across_payment_operations" in (
            payment_operation_semantic_issues(
                unsafe_pairwise_share,
                require_webhook_event_dedup=False,
                require_complete_idempotency_semantics=False,
            )
        ), unsafe_pairwise_share
    assert not has_safe_payment_same_operation_replay_condition(
        "Keep the payment UNKNOWN and retry the same command forever; the provider "
        "does not guarantee idempotency."
    )
    status_query_guarantee_does_not_cover_charge_replay = (
        "Keep the payment UNKNOWN and reconcile first. Only if the result remains "
        "inconclusive, the provider guarantees idempotency for status queries. Run "
        "at most 3 bounded status queries. Use an automatic charge retry every minute "
        "with the original operation's idempotency key. If unresolved, escalate to "
        "manual reconciliation."
    )
    assert not has_safe_payment_same_operation_replay_condition(
        status_query_guarantee_does_not_cover_charge_replay
    )
    replay_context = (
        "Keep the outcome UNKNOWN and reconcile provider status first. Only if "
        "reconciliation remains inconclusive and {guarantee} may the exact same "
        "provider command be retried {bound} with the original operation's "
        "idempotency key."
    )
    assert not has_safe_payment_same_operation_replay_condition(
        replay_context.format(
            guarantee="the provider guarantees idempotency for status queries",
            bound="at most once",
        )
    )
    for status_only_guarantee in (
        "the provider guarantees idempotency only for status queries",
        "the provider guarantees idempotent status queries only",
        "the provider guarantees status-query idempotency and nothing for charge replay",
    ):
        assert not has_safe_payment_same_operation_replay_condition(
            replay_context.format(guarantee=status_only_guarantee, bound="at most once")
        ), status_only_guarantee
    for explicitly_denied_money_replay in (
        "the provider contract explicitly guarantees idempotent replay but not for charges",
        "the provider contract explicitly guarantees idempotent replay but not for payments",
        "the provider contract explicitly guarantees idempotent replay but not covering charges",
        "the provider contract explicitly guarantees idempotent replay but not for the payment command",
        "the provider contract explicitly guarantees idempotent replay but not for money movement",
        "the provider contract explicitly guarantees idempotent replay but no charges are covered",
        "the provider contract explicitly guarantees idempotent replay but payments are not covered",
    ):
        assert not has_safe_payment_same_operation_replay_condition(
            replay_context.format(
                guarantee=explicitly_denied_money_replay,
                bound="at most once",
            )
        ), explicitly_denied_money_replay
    for money_guarantee in (
        "the provider guarantees charge replay idempotency, not status-query idempotency",
        "the provider guarantees idempotency for charge replay, not status queries",
        "the provider guarantees idempotency for status queries and charge replay",
        "the provider guarantees charge replay idempotency",
    ):
        assert has_safe_payment_same_operation_replay_condition(
            replay_context.format(guarantee=money_guarantee, bound="at most once")
        ), money_guarantee
    for replay_bound in (
        "at most once",
        "no more than two times",
        "a maximum of one replay",
    ):
        assert has_safe_payment_same_operation_replay_condition(
            replay_context.format(
                guarantee="the provider guarantees idempotent replay",
                bound=replay_bound,
            )
        ), replay_bound
    for recurring_cadence in (
        "once per minute",
        "hourly",
        "periodically",
        "on a timer",
        "at 30-second intervals",
        "with exponential backoff",
        "every scheduler tick",
    ):
        assert not has_safe_payment_same_operation_replay_condition(
            replay_context.format(
                guarantee="the provider guarantees idempotent replay",
                bound=recurring_cadence,
            )
        ), recurring_cadence
    assert not has_safe_payment_same_operation_replay_condition(
        "Never automatically retry the charge once per minute."
    )
    assert "missing_same_operation_idempotency_key_reuse" in payment_operation_semantic_issues(
        "Keep the payment UNKNOWN, block the original charge, and query provider status.",
        require_webhook_event_dedup=False,
        require_complete_idempotency_semantics=False,
        require_same_operation_retry_reuse=True,
    )

    distinct_later_payment = (
        "Authorize, capture, and refund use distinct operation-scoped idempotency "
        "keys. Reuse the same stable idempotency key only when replaying that same "
        "logical operation. A timeout leaves the original payment UNKNOWN and blocks "
        "that charge. Reconcile the original payment until the provider status lookup "
        "confirms it FAILED with no charge. The customer then explicitly authorizes a "
        "distinct later purchase, so create a new payment intent with its own new "
        "operation-scoped key for that separate purchase."
    )
    assert not payment_operation_semantic_issues(
        distinct_later_payment,
        require_webhook_event_dedup=False,
        require_complete_idempotency_semantics=False,
        require_same_operation_retry_reuse=True,
    )
    reverse_authorization_later_payment = (
        "A timeout leaves the original payment UNKNOWN. Provider status confirms "
        "the original payment FAILED with no charge. For a distinct later purchase "
        "explicitly requested by the customer, submit a new payment with a fresh "
        "idempotency key."
    )
    assert "unsafe_new_charge_after_ambiguous_outcome" not in (
        payment_operation_semantic_issues(
            reverse_authorization_later_payment,
            require_webhook_event_dedup=False,
            require_complete_idempotency_semantics=False,
        )
    )
    ambiguous_same_purchase = (
        "Authorize, capture, and refund use distinct operation-scoped idempotency "
        "keys. Reuse the same stable idempotency key only when replaying that same "
        "logical operation. A timeout leaves the original payment UNKNOWN. The "
        "customer asks us to try the same purchase again, so create a new payment "
        "with a fresh key."
    )
    assert "unsafe_new_charge_after_ambiguous_outcome" in payment_operation_semantic_issues(
        ambiguous_same_purchase,
        require_webhook_event_dedup=False,
        require_complete_idempotency_semantics=False,
        require_same_operation_retry_reuse=True,
    )

def self_check_payment_platform_safety_detector() -> None:
    safe = (
        "Record every state transition in a durable double-entry ledger, then "
        "reconcile unknown outcomes with provider status queries and webhooks.",
        "PostgreSQL holds the append-only ledger. Redis is only a cache and is not "
        "the correctness boundary; processor webhook reconciliation resolves UNKNOWN.",
        "Persist a durable payment intent and append-only ledger before calling the "
        "provider. Reconcile ambiguous outcomes through status checks and webhooks.",
        "Never call the provider first; persist the durable payment intent and ledger "
        "before dispatch. Reconcile through provider status and webhooks.",
        "Persist the payment intent. After commit, call the provider. Use a durable "
        "double-entry ledger and reconcile through provider status and webhooks.",
        "Only after the database transaction commits do we call the provider. The "
        "transaction writes a durable payment intent and ledger; provider status and "
        "webhooks reconcile ambiguous outcomes.",
        "Use a durable double-entry ledger and reconcile through provider status and "
        "webhooks. Do not rely on Redis locks or distributed locks for correctness. "
        "Use durable DB constraints and ledger immutability as the source of truth. "
        "Read models can be eventually consistent; the ledger remains authoritative.",
        "Use a durable double-entry ledger and reconcile through provider status and "
        "webhooks. Ambiguous provider response, do not post ledger movement until "
        "authoritative evidence exists.",
        "Use a durable double-entry ledger and reconcile through provider status and "
        "webhooks. On any provider response, do not post ledger movement until "
        "authoritative evidence exists.",
        "Use a durable immutable double-entry ledger and reconcile through provider "
        "status and webhooks. No distributed lock is the correctness boundary, only "
        "a performance aid around durable writes.",
    )
    unsafe = (
        (
            "Store idempotency results durably and reconcile through provider status.",
            ["missing_durable_payment_ledger"],
        ),
        (
            "Write an append-only PostgreSQL ledger and return the stored result.",
            ["missing_payment_reconciliation_path"],
        ),
        (
            "Use a durable double-entry ledger and provider webhook reconciliation. "
            "Redis SETNX is the source of truth and guarantees no duplicate charge.",
            ["unsafe_volatile_payment_correctness_boundary"],
        ),
        (
            "Use a durable double-entry ledger and reconcile through provider status "
            "and webhooks. Do not rely on Redis locks for correctness. For duplicate "
            "charges, Redis locks are the correctness boundary.",
            ["unsafe_volatile_payment_correctness_boundary"],
        ),
        (
            "Use a durable double-entry ledger and reconcile provider status. The "
            "authoritative source of truth is a volatile distributed lock.",
            ["unsafe_volatile_payment_correctness_boundary"],
        ),
        (
            "Use a durable double-entry ledger and reconcile provider status. Redis "
            "locks guarantee no duplicate charges.",
            ["unsafe_volatile_payment_correctness_boundary"],
        ),
        (
            "Use a durable immutable double-entry ledger and reconcile through provider "
            "status and webhooks. No problem: a distributed lock is the correctness "
            "boundary for duplicate prevention.",
            ["unsafe_volatile_payment_correctness_boundary"],
        ),
        (
            "Use a durable double-entry ledger and reconcile provider status and "
            "webhooks. Redis locks provide the correctness boundary.",
            ["unsafe_volatile_payment_correctness_boundary"],
        ),
        (
            "Use a transactional ledger and gateway status reconciliation. Check Redis; "
            "if the key is missing, charge the card, then write the result.",
            ["unsafe_volatile_payment_correctness_boundary"],
        ),
        (
            "Do not use a durable ledger. Reconcile unknown outcomes through provider "
            "status checks and webhooks.",
            ["missing_durable_payment_ledger"],
        ),
        (
            "Use a non-durable ledger. Reconcile unknown outcomes through provider "
            "status checks and webhooks.",
            ["missing_durable_payment_ledger"],
        ),
        (
            "Use a durable append-only ledger, but do not reconcile unknown outcomes "
            "with the provider or accept webhooks.",
            ["missing_payment_reconciliation_path"],
        ),
        (
            "Use a durable append-only ledger. Provider reconciliation is not "
            "performed, and webhooks are not accepted.",
            ["missing_payment_reconciliation_path"],
        ),
        (
            "Call the provider first, then persist the payment intent in a durable "
            "append-only ledger. Reconcile through provider status and webhooks.",
            ["unsafe_provider_before_durable_persistence"],
        ),
        (
            "Send the payment to the gateway, then create the idempotency record and "
            "append-only ledger. Reconcile through the gateway webhook.",
            ["unsafe_provider_before_durable_persistence"],
        ),
        (
            "Persist the payment intent and durable double-entry ledger before calling "
            "the provider, and reconcile through status checks and webhooks. If the "
            "provider responds synchronously, persist the result and append a ledger movement.",
            ["unsafe_unqualified_payment_ledger_movement"],
        ),
        (
            "Persist the intent and outbox first. On any synchronous response, append "
            "a ledger movement, then reconcile by webhook.",
            ["unsafe_unqualified_payment_ledger_movement"],
        ),
        (
            "Persist the intent and outbox first. Any synchronous provider response "
            "posts the corresponding money movement to the ledger.",
            ["unsafe_unqualified_payment_ledger_movement"],
        ),
        (
            "Persist the intent and outbox first. Any synchronous provider reply "
            "records the corresponding financial effect in the double-entry ledger.",
            ["unsafe_unqualified_payment_ledger_movement"],
        ),
    )
    assert all(not payment_platform_safety_issues(text) for text in safe)
    for text, expected in unsafe:
        issues = payment_platform_safety_issues(text)
        assert all(issue in issues for issue in expected), (text, issues)
    assert not payment_platform_safety_issues(
        "Persist the payment intent and durable double-entry ledger before calling the "
        "provider, and reconcile through status checks and webhooks. If the provider "
        "response authoritatively confirms that funds moved, append the ledger movement."
    )
    assert not payment_platform_safety_issues(
        "Persist a durable payment intent and ledger and reconcile through provider "
        "status and webhooks. Do not use a distributed lock, or Redis lock, as the "
        "correctness boundary."
    )
    # Exact durable-ledger evidence from the live Q39 canvas.  The schema uses a
    # snake_case table name, which must still count as a ledger when its rows are
    # explicitly immutable and double-entry.
    live_q39_canvas_ledger = (
        "- Webhook ingester deduplicates by provider event ID, then updates state "
        "from authoritative evidence.\n"
        "- Ledger service appends confirmed hold or movement entries only after "
        "authoritative confirmation.\n"
        "- `ledger_entries`: immutable double-entry rows for confirmed auth holds, "
        "captures, refunds.\n"
        "- Reconciliation matches by provider payment ID or client reference."
    )
    assert not payment_platform_safety_issues(live_q39_canvas_ledger)
    # A ledger-like table name alone remains insufficient: the answer must state
    # durable/immutable/transactional ledger semantics, not merely name a table.
    incomplete_ledger_schema = (
        "The ledger_entries table stores rows. Reconcile UNKNOWN outcomes through "
        "provider status and webhooks."
    )
    assert "missing_durable_payment_ledger" in payment_platform_safety_issues(
        incomplete_ledger_schema
    )
    explicitly_nondurable_ledger_schema = (
        "The ledger_entries: not durable rows. Reconcile UNKNOWN outcomes through "
        "provider status and webhooks."
    )
    assert "missing_durable_payment_ledger" in payment_platform_safety_issues(
        explicitly_nondurable_ledger_schema
    )
    contradictory_ledger_schemas = (
        "`ledger_entries`: immutable double-entry rows but not durable. Reconcile "
        "UNKNOWN outcomes through provider status and webhooks.",
        "The ledger_entries are immutable double-entry, however volatile. Reconcile "
        "UNKNOWN outcomes through provider status and webhooks.",
        "`ledger_entries`: immutable double-entry rows. These entries are not durable. "
        "Reconcile UNKNOWN outcomes through provider status and webhooks.",
    )
    for contradictory_schema in contradictory_ledger_schemas:
        assert "missing_durable_payment_ledger" in payment_platform_safety_issues(
            contradictory_schema
        ), contradictory_schema
    complete_boundary = (
        "Use a durable double-entry ledger and reconcile by provider status and webhooks. "
    )
    for unsafe_effect in (
        "On any gateway reply, debit the ledger.",
        "Apply a ledger movement on every response from the provider.",
        "The provider response is not confirmed; record a ledger movement.",
    ):
        assert "unsafe_unqualified_payment_ledger_movement" in (
            payment_platform_safety_issues(complete_boundary + unsafe_effect)
        ), unsafe_effect
    assert not payment_platform_safety_issues(
        complete_boundary
        + "On an authenticated provider response that explicitly approves the capture, "
        "record the ledger movement."
    )
    adjacent_safe_deferral_does_not_hide_unsafe_effect = (
        complete_boundary
        + "On any provider response, post a ledger movement. "
        "For an ambiguous provider response, do not post ledger movement until "
        "authoritative evidence exists."
    )
    assert "unsafe_unqualified_payment_ledger_movement" in (
        payment_platform_safety_issues(adjacent_safe_deferral_does_not_hide_unsafe_effect)
    )
