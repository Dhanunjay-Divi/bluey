"""Payment and database answer-safety contracts for the Bluey live evaluator."""

from __future__ import annotations

import re
from typing import List

def has_mysql_not_valid_portability_claim(text: str) -> bool:
    lower = re.sub(r"\s+", " ", text.casefold())
    if "mysql" not in lower or "not valid" not in lower:
        return False
    if re.search(
        r"mysql.{0,100}(?:does not|doesn't|doesn’t|cannot|can't|can’t|lacks).{0,80}(?:support|have).{0,60}not valid",
        lower,
    ) or re.search(
        r"not valid.{0,120}(?:is not|isn't|isn’t|not).{0,60}(?:supported|available).{0,50}mysql",
        lower,
    ):
        return False
    return bool(
        re.search(
            r"not valid.{0,240}(?:works?|supported|available|introduced).{0,100}mysql",
            lower,
        )
        or re.search(
            r"mysql.{0,160}(?:supports?|has|offers|allows).{0,100}not valid",
            lower,
        )
        or re.search(r"works?.{0,100}(?:recent|current|newer).{0,40}mysql", lower)
    )


def large_fk_migration_safety_issues(text: str) -> List[str]:
    """Reject unsafe migration claims and table-wide orphan preflight queries."""
    lower = re.sub(
        r"\s+",
        " ",
        re.sub(r"[*_`~]+", "", text.casefold().replace("’", "'")).strip(),
    )
    issues: List[str] = []

    def sql_claim_is_rejected(start: int, end: int, snippet: str) -> bool:
        prefix = lower[max(0, start - 120) : start]
        suffix = lower[end : min(len(lower), end + 120)]
        double_negative = bool(
            re.search(
                r"\b(?:do\s+not|don't|never|would\s+not|wouldn't)\s+"
                r"(?:avoid|reject|skip)\w*\b.{0,70}$",
                prefix,
            )
        )
        prefix_rejection = bool(
            re.search(
                r"\b(?:do\s+not|don't|never|avoid|reject|would\s+not|wouldn't|"
                r"should\s+not|must\s+not)\b.{0,85}$",
                prefix,
            )
        ) and not double_negative
        suffix_rejection = bool(
            re.search(
                r"\b(?:is|would\s+be|remains?)\s+(?:unsafe|dangerous|unbounded)\b|"
                r"\b(?:should|must)\s+(?:not\s+be\s+used|be\s+avoided)\b|"
                r"\b(?:avoid|reject)\s+(?:it|this|that\s+query)\b",
                f"{snippet} {suffix}",
            )
        )
        return prefix_rejection or suffix_rejection

    unsafe_not_in_subquery = False
    for match in re.finditer(
        r"\bnot\s+in\s*\(\s*select\b.{0,240}?\bfrom\b.{0,160}?\)",
        lower,
    ):
        safely_rejected = sql_claim_is_rejected(
            match.start(), match.end(), match.group()
        )
        if not safely_rejected:
            unsafe_not_in_subquery = True
            break
    if unsafe_not_in_subquery:
        issues.append("unsafe_not_in_orphan_preflight")

    unbounded_count_preflight = False
    count_scan = re.sub(r"(?<=\w)\.(?=\w)", "_", lower)
    for match in re.finditer(
        # Markdown normalization removes `*`, so COUNT(*) arrives here as COUNT().
        r"\bselect\s+count\s*\(\s*[^)]{0,80}\)\s+from\b[^.!?;]{0,500}",
        count_scan,
    ):
        query = lower[match.start() : match.end()]
        safely_rejected = sql_claim_is_rejected(match.start(), match.end(), query)
        bounded = bool(
            re.search(
                r"\b(?:bounded|batch(?:ed|es)?|chunk(?:ed|s)?|primary[- ]key\s+range|"
                r"partition(?:ed|s)?|page(?:d|s)?)\b",
                query,
            )
        )
        orphan_context = bool(
            re.search(
                r"\b(?:child|parent|orphan|foreign\s+key|referential)\w*\b|"
                r"\bnot\s+(?:exists|in)\b|\bleft\s+join\b",
                query,
            )
        )
        if orphan_context and not safely_rejected and not bounded:
            unbounded_count_preflight = True
            break
    if unbounded_count_preflight:
        issues.append("unsafe_unbounded_count_orphan_preflight")

    table_start = r"(?:create|build|make|provision)\w*\s+(?:a\s+)?(?:new|shadow|replacement)\s+table"
    copy_data = r"(?:copy|move|migrate)\w*\s+(?:the\s+|all\s+|entire\s+)?(?:data|rows)"
    table_swap = r"(?:rename|swap|cut\s*over)\w*"
    for match in re.finditer(
        rf"{table_start}.{{0,500}}{copy_data}.{{0,500}}{table_swap}",
        lower,
    ):
        window = lower[max(0, match.start() - 120) : min(len(lower), match.end() + 120)]
        prescriptive = bool(
            re.search(
                r"\b(?:should|recommend|instead|proposed solution|approach is|"
                r"i would|we would|standard pattern)\b",
                window,
            )
        )
        rejected = bool(
            re.search(
                r"\b(?:do not|don't|never|avoid|reject|unsafe|not the default|"
                r"should not|would not)\b\s+(?:(?:use|recommend)\s+)?" + table_start,
                window,
            )
            or re.search(
                table_start + r".{0,220}\b(?:is|as)\s+not\s+(?:the\s+)?default\b",
                window,
            )
        )
        controlled_fallback = bool(
            re.search(
                r"\b(?:last resort|fallback|only if)\b.{0,180}"
                r"\b(?:online schema change|vetted tool|cdc|change data capture|"
                r"dual write|concurrent write sync)\b",
                window,
            )
        )
        if prescriptive and not rejected and not controlled_fallback:
            issues.append("unsafe_whole_table_copy_swap_advice")
            break

    for sentence in re.split(r"(?<=[.!?])\s+|\n+", lower):
        if not re.search(
            r"\b(?:foreign key|constraint|validation|validate|not valid)\w*\b",
            sentence,
        ):
            continue
        categorical = bool(
            re.search(
                r"\b(?:will|would|does|always)\b.{0,100}\b(?:block|prevent|stop)\w*\b|"
                r"\b(?:lock|validation)\w*\b.{0,100}\bprevent\w*\b|"
                r"\b(?:block|prevent|stop)s?\b",
                sentence,
            )
        )
        all_io = bool(
            re.search(
                r"\b(?:all|any|both)\b.{0,25}\breads?\b.{0,25}\b(?:and|or)\b"
                r".{0,25}\bwrites?\b|"
                r"\b(?:all|any|both)\b.{0,25}\bwrites?\b.{0,25}\b(?:and|or)\b"
                r".{0,25}\breads?\b",
                sentence,
            )
        )
        explicit_universal = bool(re.search(r"\b(?:always|universally)\b", sentence))
        qualified = bool(
            re.search(
                r"\b(?:can|could|may|might|risk|depending|"
                r"not universally|does not universally|doesn't universally)\b",
                sentence,
            )
            and not explicit_universal
            or re.search(
                r"\b(?:do not|don't|never)\b.{0,80}\b(?:claim|assume|say)\b",
                sentence,
            )
            or re.search(
                r"\b(?:does not|doesn't|will not|won't|would not|wouldn't|never)\b"
                r".{0,30}\b(?:block|prevent|stop)\w*\b",
                sentence,
            )
        )
        if categorical and all_io and not qualified:
            issues.append("unsafe_universal_fk_read_write_block_claim")
            break

    for sentence in re.split(r"(?<=[.!?])\s+|\n+", lower):
        if "access exclusive" not in sentence:
            continue
        if not re.search(r"\b(?:foreign key|not valid|validate constraint)\b", sentence):
            continue
        safely_rejected = bool(
            re.search(
                r"\b(?:not|never)\s+(?:an?\s+)?access exclusive\b|"
                r"\b(?:does\s+not|doesn't)\s+(?:take|require|use)\b.{0,25}"
                r"\baccess exclusive\b|"
                r"\brather\s+than\s+(?:an?\s+)?access exclusive\b|"
                r"\bunlike\s+access exclusive\b|"
                r"\baccess exclusive\b.{0,90}\b(?:but|whereas|while)\b"
                r".{0,90}\bshare row exclusive\b|"
                r"\bconflicts?\s+with\s+access exclusive\s+(?:operations?|locks?)\b|"
                r"\bblocks?\s+conflicting\s+access exclusive\s+"
                r"(?:ddl|operations?|locks?|requests?)\b",
                sentence,
            )
        )
        if not safely_rejected:
            issues.append("unsafe_postgres_fk_access_exclusive_claim")
            break

    normalized_text = re.sub(r"\s+", " ", text.casefold().replace("’", "'"))
    for match in re.finditer(r"\bpt-online-schema-change\b", normalized_text):
        clause_start = max(
            normalized_text.rfind(delimiter, 0, match.start())
            for delimiter in (".", "!", "?", ";")
        )
        clause_ends = [
            index
            for delimiter in (".", "!", "?", ";")
            if (index := normalized_text.find(delimiter, match.end())) >= 0
        ]
        clause_end = min(clause_ends) if clause_ends else len(normalized_text)
        tool_clause = normalized_text[clause_start + 1 : clause_end]
        normalized_paragraph = normalized_text[
            max(0, match.start() - 180) : min(len(normalized_text), match.end() + 180)
        ]
        postgres_context = bool(
            re.search(
                r"\b(?:postgres(?:ql)?|not valid|validate constraint|pg_repack)\b",
                normalized_paragraph,
            )
        )
        mysql_applicability = bool(
            re.search(
                r"\b(?:for|on|with)\s+(?:a\s+)?(?:tested\s+)?mysql\b.{0,100}"
                r"\bpt-online-schema-change\b|"
                r"\bpt-online-schema-change\b.{0,100}"
                r"\b(?:for|on|with)\s+(?:a\s+)?(?:tested\s+)?mysql\b",
                tool_clause,
            )
        )
        rejected = bool(
            re.search(
                r"\b(?:do not|don't|never|avoid)\b.{0,80}"
                r"\bpt-online-schema-change\b",
                normalized_paragraph,
            )
            or re.search(
                r"\b(?:inappropriate|unsuitable|wrong)\b.{0,60}"
                r"\bpt-online-schema-change\b|"
                r"\bpt-online-schema-change\b.{0,60}"
                r"\b(?:inappropriate|unsuitable|not\s+(?:a\s+)?postgresql\s+tool)\b|"
                r"\bunlike\s+pt-online-schema-change\b|"
                r"\bpt-online-schema-change\b.{0,40}\b(?:is|does)\s+not\b"
                r".{0,40}\b(?:for|support|apply\w*\s+to)\b.{0,30}\bpostgres(?:ql)?\b",
                normalized_paragraph,
            )
        )
        if postgres_context and not mysql_applicability and not rejected:
            issues.append("unsafe_mysql_tool_in_postgres_migration_advice")
            break

    eol_version = re.compile(r"\bpostgres(?:ql)?\s+(?:9\.\d+|10|11|12|13)\b")
    for match in eol_version.finditer(lower):
        window = lower[max(0, match.start() - 90) : match.end() + 90]
        safely_rejected = bool(
            re.search(
                r"\b(?:do not|don't|never|avoid|reject|wouldn't|would\s+not|unsupported|"
                r"end[- ]of[- ]life|eol|obsolete)\b.{0,80}"
                + re.escape(match.group()),
                window,
            )
            or re.search(
                re.escape(match.group())
                + r".{0,80}\b(?:is|isn't|it's|as)\s+(?:now\s+)?(?:unsupported|"
                r"end[- ]of[- ]life|eol|obsolete)\b",
                window,
            )
        )
        if not safely_rejected:
            issues.append("unsupported_eol_postgresql_migration_baseline")
            break
    return issues


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


def has_exactly_once_processing_overclaim(text: str) -> bool:
    for sentence in re.split(r"(?<=[.!?])\s+|\n+", text.casefold()):
        if not re.search(r"exactly[- ]once", sentence):
            continue
        caveated = bool(
            re.search(
                r"(?:cannot|can't|can’t|not possible|not truly|no true|"
                r"limited to|only within)",
                sentence,
            )
            or re.search(
                r"\b(?:do not|don't|never|would not|should not)\b.{0,45}"
                r"\b(?:claim|promise|guarantee)\w*\b.{0,45}\bexactly[- ]once\b",
                sentence,
            )
            or re.search(
                r"\bno\s+(?:global|end[- ]to[- ]end)\b.{0,45}\bexactly[- ]once\b|"
                r"\bexactly[- ]once\b.{0,60}\b(?:is\s+impossible|"
                r"is\s+not\s+guaranteed|cannot\s+be\s+guaranteed)\b",
                sentence,
            )
        )
        if caveated:
            continue
        if re.search(
            r"(?:guarantee|guarantees|guaranteed|ensure|ensures|achieve|achieves)"
            r".{0,80}exactly[- ]once|exactly[- ]once.{0,50}processing(?:\s+semantics)?",
            sentence,
        ):
            return True
    return False


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


def has_explicit_no_provider_command_replay(text: str) -> bool:
    """Recognize a fail-closed policy that never resubmits the money effect."""
    lower = re.sub(r"\s+", " ", text.casefold().replace("’", "'"))
    return bool(
        re.search(
            r"\b(?:never|do\s+not|don't|must\s+not|cannot|can't)\s+"
            r"(?:ever\s+)?(?:retry|replay|resubmit)\w*\b.{0,55}"
            r"\b(?:charge|payment|provider\s+command|provider\s+operation|"
            r"money\s+movement|effect)\b|"
            r"\b(?:never|do\s+not|don't|must\s+not|cannot|can't)\s+"
            r"(?:ever\s+)?(?:retry\s+the\s+charge|resubmit\s+(?:the\s+)?"
            r"provider\s+command)\b",
            lower,
        )
    )


def has_safe_payment_same_operation_replay_condition(text: str) -> bool:
    """Recognize a reconciled, provider-guaranteed replay of the original command."""
    lower = re.sub(
        r"\s+",
        " ",
        re.sub(r"[*_`~]+", "", text.casefold().replace("’", "'")),
    )
    conditional_replay = bool(
        re.search(
            r"\b(?:only\s+if|if|after|unless)\b.{0,220}"
            r"\b(?:replay|retry|resubmit)\w*\b",
            lower,
        )
    )
    explicit_provider_capability = bool(
        re.search(
            r"\bprovider(?:'s)?\s+(?:contract\s+)?"
            r"(?:guarantees?|supports?|honors?|deduplicates?)\b.{0,50}"
            r"\bidempoten\w*\b|"
            r"\bidempoten\w*\b.{0,50}\b(?:guaranteed|supported|honored|"
            r"deduplicated)\b.{0,30}\bby\s+(?:the\s+)?provider\b",
            lower,
        )
    )
    negated_provider_capability = bool(
        re.search(
            r"\bprovider(?:'s)?\s+(?:contract\s+)?"
            r"(?:guarantees?|states?|confirms?)\b.{0,65}\bidempoten\w*\b"
            r".{0,35}\b(?:is\s+)?not\s+(?:supported|honored|guaranteed)|"
            r"\bprovider\b.{0,55}\b(?:does\s+not|doesn't|cannot|can't)\s+"
            r"(?:support|honor|guarantee)\w*\b.{0,35}\bidempoten\w*\b",
            lower,
        )
    )
    if negated_provider_capability:
        explicit_provider_capability = False
    inconclusive_reconciliation = bool(
        re.search(
            r"\b(?:reconcil\w*|status\s+(?:check|lookup|query)|webhooks?)\b"
            r".{0,110}\b(?:inconclusive|unresolved|unknown|no\s+terminal\s+outcome)\b|"
            r"\b(?:inconclusive|unresolved|unknown|no\s+terminal\s+outcome)\b"
            r".{0,110}\b(?:reconcil\w*|status\s+(?:check|lookup|query)|webhooks?)\b",
            lower,
        )
    )
    original_key = bool(
        re.search(r"\b(?:same|original)\b.{0,35}\bidempotency\s+key\b", lower)
    )
    original_command = bool(
        re.search(
            r"\b(?:exact\s+same|same|original)\b.{0,35}"
            r"\b(?:provider\s+)?(?:operation|command|charge|authorization|payment)\b|"
            r"\b(?:retry|replay|resubmit)\w*\s+(?:the\s+)?"
            r"(?:same|original)?\s*(?:provider\s+)?"
            r"(?:operation|command|charge|authorization|payment)\b",
            lower,
        )
    )
    bounded_policy = bool(
        re.search(
            r"\bbounded\s+(?:policy|attempts?|retries|retry|replay|budget|window)\b|"
            r"\b(?:retry|replay)\s+(?:budget|deadline|limit)\b|"
            r"\b(?:at\s+most|no\s+more\s+than|maximum)\s+\d+\b",
            lower,
        )
    )
    unsafe_unbounded = bool(
        re.search(
            r"\b(?:forever|indefinitely|until\s+it\s+succeeds?|until\s+success|"
            r"without\s+(?:a\s+)?(?:limit|deadline|bound))\b|"
            r"\bevery\s+(?:\d+\s+)?(?:seconds?|minutes?|hours?)\b.{0,80}"
            r"\b(?:forever|until\s+it\s+succeeds?|until\s+success)\b",
            lower,
        )
    )
    return bool(
        conditional_replay
        and explicit_provider_capability
        and inconclusive_reconciliation
        and original_key
        and original_command
        and bounded_policy
        and not unsafe_unbounded
    )


def has_unsafe_ambiguous_payment_outcome(text: str) -> bool:
    lower = re.sub(r"\s+", " ", re.sub(r"[*_`~]+", "", text.casefold()))
    action_words = (
        r"mark(?:ed|s|ing)?|move(?:d|s|ing)?|transition(?:ed|s|ing)?|set|"
        r"become(?:s)?|consider(?:ed|s)?|convert(?:ed|s)?|"
        r"finaliz(?:e|ed|es|ing)|expir(?:e|ed|es|ing)|classif(?:y|ied|ies|ying)|"
        r"treat(?:ed|s|ing)?|flip(?:ped|s|ping)?|clos(?:e|ed|es|ing)"
    )

    def provider_definitively_confirmed_no_charge(prefix: str) -> bool:
        """Allow FAILED only when a provider-confirmed no-charge clause gates it."""
        actor = r"(?:the\s+)?(?:payment\s+)?(?:provider|processor|gateway|acquirer)"
        source = (
            rf"(?:{actor}(?:'s)?(?:\s+(?:authoritative|definitive))?"
            r"(?:\s+(?:status(?:\s+(?:api|lookup|query|response|result))?|"
            r"signed\s+webhook))?|"
            r"(?:an?\s+)?(?:authoritative\s+|definitive\s+)?"
            r"(?:status(?:\s+(?:api|lookup|query|response|result))?|signed\s+webhook)"
            rf"\s+from\s+{actor})"
        )
        evidence_verb = r"(?:confirms?|verifies?|certifies?|reports?|returns?|shows?|indicates?|states?)"
        no_charge = (
            r"(?:no\s+(?:charge|authorization|capture|payment|debit|funds?\s+movement)"
            r"(?:\s+(?:occurred|exists?|was\s+(?:made|created|submitted|recorded)))?|"
            r"(?:the\s+)?(?:card|account|customer|payment)\s+(?:was|is)\s+not\s+"
            r"(?:charged|debited|authorized)|"
            r"(?:request|attempt|payment)\s+(?:was|is)\s+(?:declined|rejected)\s+"
            r"before\s+(?:authorization|capture|funds?\s+movement)|"
            r"zero\s+(?:funds?|dollars?)\s+(?:moved|captured|authorized)|"
            r"no\s+(?:payment|authorization|capture)\s+record\s+(?:exists?|was\s+created))"
        )
        for gate in re.finditer(
            r"\b(?:only\s+)?(?:if|when|once|after|until)\b", prefix
        ):
            clause = prefix[gate.start() :]
            if len(clause) > 360:
                clause = clause[-360:]
            direct_evidence = re.search(
                rf"\b{source}\b.{{0,80}}\b{evidence_verb}\b"
                rf".{{0,110}}\b(?:that\s+)?{no_charge}\b",
                clause,
            )
            received_confirmation = re.search(
                rf"\b(?:receiving|obtaining)\b.{{0,40}}"
                rf"\b(?:explicit|definitive|authoritative)\b.{{0,30}}"
                rf"\bconfirmation\b.{{0,50}}\bfrom\s+{actor}\b"
                rf".{{0,100}}\b(?:that\s+)?{no_charge}\b",
                clause,
            )
            negated_evidence = re.search(
                rf"\b{source}\b.{{0,40}}\b(?:does|did|has|had|is|was)\s+not\b"
                rf".{{0,30}}\b{evidence_verb}\b",
                clause,
            )
            if (direct_evidence or received_confirmation) and not negated_evidence:
                return True
        return False

    def provider_authoritatively_resolves_terminal_state(context: str) -> bool:
        provider_source = bool(
            re.search(r"\b(?:provider|processor|gateway|acquirer)\b", context)
            and re.search(r"\b(?:status|lookup|query|webhook|evidence)\b", context)
        )
        authoritative_gate = bool(
            re.search(
                r"\b(?:only\s+from|after|based\s+on|when)\b.{0,80}"
                r"\b(?:authoritative|confirmed|definitive)\b.{0,30}"
                r"\b(?:provider\s+)?(?:status|webhook|evidence|response)\b|"
                r"\b(?:authoritative|confirmed|definitive)\b.{0,30}"
                r"\b(?:provider\s+)?(?:status|webhook|evidence|response)\b"
                r".{0,80}\b(?:moves?|transitions?|resolves?|sets?)\b",
                context,
            )
            or re.search(
                r"\b(?:moves?|transitions?|resolves?|sets?)\b.{0,120}"
                r"\bonly\s+from\s+authoritative\s+evidence\b",
                context,
            )
            or re.search(
                r"\b(?:those|these|such)\s+authoritative\s+"
                r"(?:signals?|results?|responses?|sources?|evidence)\b.{0,100}"
                r"\b(?:moves?|transitions?|resolves?|sets?)\b.{0,80}"
                r"\bunknown\b.{0,80}\b(?:succeeded|failed|canceled|cancelled)\b",
                context,
            )
        )
        negated = bool(
            re.search(
                r"\b(?:without|before|not\s+waiting\s+for|despite\s+missing)\b"
                r".{0,50}\b(?:authoritative|confirmed|definitive)\b",
                context,
            )
        )
        return provider_source and authoritative_gate and not negated

    terminal_failure = False
    for outcome in re.finditer(
        r"\b(?:timeout|timed out|unknown|ambiguous|no record|maximum retries|max retries)\b",
        lower,
    ):
        window = lower[outcome.start() : outcome.end() + 320]
        actions = re.finditer(
            rf"\b(?:{action_words})\b"
            rf"(?:(?!\b(?:{action_words})\b).){{0,80}}\bfailed\b",
            window,
        )
        for action in actions:
            action_start = outcome.start() + action.start()
            prefix = lower[max(0, action_start - 60) : action_start]
            if re.search(
                r"(?:do not|don't|don’t|does not|doesn't|is not|never|must not|"
                r"should not|cannot|can't|can’t)"
                r"(?:\s+(?:ever|be))?\s*$",
                prefix,
            ) or re.search(r"\bnot\b.{0,20}\bfailed\b", action.group()):
                continue
            confirmation_prefix = lower[max(outcome.end(), action_start - 220) : action_start]
            if provider_definitively_confirmed_no_charge(confirmation_prefix):
                continue
            transition_context = lower[
                max(0, outcome.start() - 100) : min(
                    len(lower), outcome.start() + action.end() + 160
                )
            ]
            if provider_authoritatively_resolves_terminal_state(transition_context):
                continue
            terminal_failure = True
            break
        if terminal_failure:
            break
    if not terminal_failure:
        for sentence_match in re.finditer(r"[^.!?;]+(?:[.!?;]|$)", lower):
            sentence = sentence_match.group().strip()
            reverse_transition = bool(
                "unknown" in sentence
                and "failed" in sentence
                and re.search(
                    rf"\b(?:{action_words})\b.{{0,80}}\bunknown\b"
                    r".{0,80}\b(?:as|to|into|becomes?|considered)?\s*failed\b",
                    sentence,
                )
            )
            if not reverse_transition:
                continue
            if re.search(
                r"\b(?:do\s+not|don't|does\s+not|doesn't|is\s+not|never|"
                r"must\s+not|should\s+not|cannot|can't)\b"
                r".{0,70}\b(?:unknown|failed|mark|move|transition|convert|expire|"
                r"become|consider|treat|flip|close)\b",
                sentence,
            ):
                continue
            transition_context = lower[
                max(0, sentence_match.start() - 320) : sentence_match.end()
            ]
            if provider_authoritatively_resolves_terminal_state(transition_context):
                continue
            terminal_failure = True
            break
    charge_retry = False
    retry_pattern = re.compile(
        r"\b(?:retry|retries|retrying|resubmit|resubmits|resubmitting|re-submit|re-submits)"
        r"\s+(?:the\s+|a\s+)?(?:charge|payment|gateway call|charge submission|payment submission)\b"
    )
    for retry in retry_pattern.finditer(lower):
        sentence_start = max(
            lower.rfind(delimiter, 0, retry.start()) for delimiter in (".", "!", "?", ";")
        )
        sentence_ends = [
            index
            for delimiter in (".", "!", "?", ";")
            if (index := lower.find(delimiter, retry.end())) >= 0
        ]
        sentence_end = min(sentence_ends) if sentence_ends else len(lower)
        retry_sentence = lower[sentence_start + 1 : sentence_end]
        local_start = retry.start() - sentence_start - 1
        prefix = retry_sentence[max(0, local_start - 120) : local_start]
        direct_negation = re.search(
            r"(?:do not|don't|don’t|never|must not|cannot|can't|can’t)\s*$", prefix
        )
        coordinated_negation = re.search(
            r"(?:do not|don't|don’t|never|must not|cannot|can't|can’t)\b"
            r"\s+(?:mark|set|move|transition|submit|resubmit|charge|call|send|attempt)\b"
            r"(?:(?![.!?]).){0,80}\b(?:or|and)\s*$",
            prefix,
        )
        if direct_negation or coordinated_negation:
            continue
        retry_context = retry_sentence
        if re.search(r"\b(?:only\s+if|after|unless)\b", retry_sentence):
            previous_start = max(
                lower.rfind(delimiter, 0, max(0, sentence_start))
                for delimiter in (".", "!", "?", ";")
            )
            retry_context = lower[previous_start + 1 : sentence_end]
        if has_safe_payment_same_operation_replay_condition(retry_context):
            continue
        charge_retry = True
        break
    return bool(terminal_failure or charge_retry)


def self_check_ambiguous_payment_detector() -> None:
    safe = (
        "A timeout leaves the outcome UNKNOWN, not failed. "
        "Move it to PENDING_RECONCILIATION; do not mark it FAILED.",
        "After a timeout, never retry the charge; retry only the status lookup.",
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
    )
    assert not any(has_unsafe_ambiguous_payment_outcome(text) for text in safe)
    assert all(has_unsafe_ambiguous_payment_outcome(text) for text in unsafe)


def payment_operation_semantic_issues(
    text: str,
    *,
    require_webhook_event_dedup: bool,
    require_complete_idempotency_semantics: bool = True,
    require_same_operation_retry_reuse: bool = False,
) -> List[str]:
    """Enforce operation-scoped idempotency and webhook correctness."""
    raw_lower = re.sub(
        r"[*`~]+", "", text.casefold().replace("’", "'")
    ).replace("_", " ")
    # Preserve Markdown list boundaries before whitespace normalization. Without
    # this, unrelated bullets such as "same key on retry" and "capture/refund"
    # collapse into one semantic clause and can manufacture a shared-key claim.
    clause_text = re.sub(
        r"(?m)^\s*(?:#{1,6}\s*|[-+]\s+|\d+[.)]\s+)",
        ". ",
        raw_lower,
    )
    clause_text = re.sub(r"\n+", ". ", clause_text)
    lower = re.sub(r"\s+", " ", raw_lower)
    clauses = [
        clause.strip()
        for clause in re.split(r"(?<=[.!?;])\s+", clause_text)
        if clause.strip()
    ]
    semantic_windows = list(clauses)
    semantic_windows.extend(
        " ".join(clauses[index : index + width])
        for width in (2, 3)
        for index in range(0, max(0, len(clauses) - width + 1))
    )
    issues: List[str] = []

    retry_signal = r"retry|retries|retrying|replay|replays|replaying|resubmit|attempt"
    new_key_signal = (
        r"(?:fresh|new|different|rotated|replacement|unique)\s+"
        r"(?:operation\s+)?(?:idempotency\s+)?keys?|"
        r"(?:rotate|change|replace)\w*\s+(?:the\s+)?idempotency\s+keys?"
    )
    unsafe_new_key = False
    for clause in clauses:
        direct_new_key_on_retry = bool(
            re.search(
                rf"\b(?:{retry_signal})\w*\b.{{0,55}}"
                rf"\b(?:use|using|with|under|generate|create|mint|get|receive|"
                rf"choose|select|assign|derive|switch|rotate|change)\w*"
                rf"\b.{{0,20}}\b(?:{new_key_signal})\b",
                clause,
            )
            or re.search(
                rf"\b(?:{new_key_signal})\b.{{0,55}}"
                rf"\b(?:for|on|per|upon|when|each|every)\b.{{0,25}}"
                rf"\b(?:{retry_signal})\w*\b",
                clause,
            )
            or re.search(
                r"\b(?:partial\s+)?(?:capture|refund)\b.{0,45}"
                r"\bretr(?:y|ies|ied|ying)\b.{0,100}"
                r"\bnew\s+logical\s+operation\s+instance\b.{0,70}"
                r"\bnew\s+idempotency\s+key\b|"
                r"\bretr(?:y|ies|ied|ying)\b.{0,45}"
                r"\b(?:partial\s+)?(?:capture|refund)\b.{0,100}"
                r"\bnew\s+logical\s+operation\s+instance\b.{0,70}"
                r"\bnew\s+idempotency\s+key\b",
                clause,
            )
        )
        if not direct_new_key_on_retry:
            continue
        safely_rejected = bool(
            re.search(
                r"\b(?:do not|don't|never|must not|should not|cannot|can't)\s+"
                r"(?:(?:generate|create|mint|rotate|replace|change|use|get|issue)\w*\s+)?"
                rf"(?:a\s+|the\s+)?(?:{new_key_signal})\b",
                clause,
            )
            or re.search(
                rf"\b(?:{new_key_signal})\b.{{0,35}}\b(?:must|should|can)\s+not\s+"
                r"be\s+(?:generated|created|used|issued|rotated)",
                clause,
            )
            or (
                re.search(r"\b(?:rather than|instead of)\b", clause)
                and re.search(r"\b(?:reuse|preserve|keep)\w*\b.{0,35}\b(?:same|stable)\b", clause)
            )
            or re.search(
                rf"\b(?:not|never)\s+(?:a\s+|the\s+)?(?:{new_key_signal})\b",
                clause,
            )
            or re.search(
                r"\b(?:do not|don't|never|must not|should not|cannot|can't)\s+"
                rf"(?:{retry_signal})\w*\b.{{0,80}}\b(?:{new_key_signal})\b",
                clause,
            )
        )
        if not safely_rejected:
            unsafe_new_key = True
            break
    if not unsafe_new_key:
        for window in semantic_windows:
            partial_retry_gets_new_operation_key = bool(
                re.search(
                    r"\b(?:partial\s+)?(?:capture|refund)\b.{0,45}"
                    r"\bretr(?:y|ies|ied|ying)\b.{0,100}"
                    r"\bnew\s+logical\s+operation\s+instance\b.{0,70}"
                    r"\bnew\s+idempotency\s+key\b|"
                    r"\bretr(?:y|ies|ied|ying)\b.{0,45}"
                    r"\b(?:partial\s+)?(?:capture|refund)\b.{0,100}"
                    r"\bnew\s+logical\s+operation\s+instance\b.{0,70}"
                    r"\bnew\s+idempotency\s+key\b",
                    window,
                )
            )
            safely_rejected = bool(
                re.search(
                    r"\b(?:do\s+not|don't|never|must\s+not|should\s+not|cannot|can't)"
                    r"\b.{0,80}\bnew\s+logical\s+operation\s+instance\b|"
                    r"\b(?:do\s+not|don't|never|must\s+not|should\s+not|cannot|can't)"
                    r"\b.{0,100}\bnew\s+idempotency\s+key\b",
                    window,
                )
            )
            if partial_retry_gets_new_operation_key and not safely_rejected:
                unsafe_new_key = True
                break
    if unsafe_new_key:
        issues.append("unsafe_new_idempotency_key_on_retry")

    explicitly_rotates_key = False
    for clause in clauses:
        rotates_or_replaces = bool(
            re.search(
                r"\b(?:rotate|change|replace|regenerate|remint|refresh)\w*\b.{0,45}"
                r"\b(?:provider\s+)?(?:idempotency\s+)?(?:key|token)\b.{0,55}"
                rf"\b(?:between|on|for|per|after|before)\b.{{0,25}}"
                rf"\b(?:{retry_signal})\w*\b|"
                rf"\b(?:{retry_signal})\w*\b.{{0,45}}"
                r"\b(?:rotate|change|replace|regenerate|remint|refresh)\w*\b.{0,35}"
                r"\b(?:provider\s+)?(?:idempotency\s+)?(?:key|token)\b|"
                rf"\b(?:each|every)\s+(?:provider\s+)?(?:call|{retry_signal})\b"
                r".{0,35}\b(?:gets?|uses?|receives?|mints?|generates?)\b.{0,25}"
                r"\b(?:a\s+)?(?:new|fresh|different|random|rotated)\b.{0,20}"
                r"\b(?:uuid|(?:idempotency\s+)?(?:key|token))\b|"
                r"\b(?:new|fresh|different|random|rotated)\b.{0,20}"
                r"\b(?:uuid|(?:idempotency\s+)?(?:key|token))\b.{0,30}"
                rf"\bper\s+(?:provider\s+)?(?:call|{retry_signal})\b|"
                r"\bprovider\s+(?:idempotency\s+)?(?:key|token)\b.{0,30}"
                r"\b(?:is|remains?)\s+not\s+stable\b.{0,35}\b(?:retry|retries)\b|"
                r"\bafter\s+(?:a\s+)?(?:provider\s+|psp\s+)?timeout\b.{0,50}"
                r"\b(?:regenerate|rotate|replace|remint|refresh)\w*\b.{0,25}"
                r"\b(?:the\s+)?(?:provider\s+)?(?:idempotency\s+)?(?:key|token)\b|"
                r"\b(?:derive|compute)\w*\b.{0,30}"
                r"\bretry\s+(?:idempotency\s+)?key\b.{0,45}"
                r"\bappend\w*\b.{0,20}\battempt\s+(?:number|index)\b|"
                r"\brefresh\w*\b.{0,25}\bprovider\s+(?:key|token)\b.{0,35}"
                r"\bafter\s+(?:a\s+)?timeout\b|"
                r"\bdiscard\w*\b.{0,25}\b(?:old|original)\s+(?:key|token)\b"
                r".{0,35}\bmint\w*\b.{0,20}\b(?:a\s+)?successor\b|"
                r"\battempt[- ]specific\s+(?:nonce|key|token)\b",
                clause,
            )
        )
        if not rotates_or_replaces:
            continue
        safely_rejected = bool(
            re.search(
                r"\b(?:do not|don't|never|must not|should not|cannot|can't|avoid)\b"
                r".{0,35}\b(?:rotate|change|replace|regenerate|remint|refresh|generate|"
                r"mint|derive|compute|append|discard|use)\w*\b",
                clause,
            )
            or re.search(
                r"\b(?:rotating|changing|replacing|regenerating|using)\b.{0,45}"
                r"\b(?:new|fresh|different|random|rotated)\b.{0,25}"
                r"\b(?:key|token|uuid)\b.{0,30}\b(?:forbidden|disallowed|rejected)\b",
                clause,
            )
        )
        if not safely_rejected:
            explicitly_rotates_key = True
            break
    if explicitly_rotates_key and "unsafe_new_idempotency_key_on_retry" not in issues:
        issues.append("unsafe_new_idempotency_key_on_retry")

    missing_key_on_retry = False
    for clause in clauses:
        omits_key = bool(
            re.search(
                rf"\b(?:{retry_signal})\w*\b.{{0,55}}"
                r"\b(?:without|omit|omits|omitting|drop|drops|dropping|clear|clears|"
                r"clearing|remove|removes|removing|no)\b.{0,25}"
                r"\b(?:provider\s+)?idempotency\s+(?:key|token)\b|"
                r"\bidempotency\s+(?:key|token)\b.{0,45}"
                r"\b(?:is|becomes?|may\s+be|can\s+be)?\s*"
                r"(?:optional|omitted|missing|absent|cleared|dropped|removed|not\s+required)\b"
                rf".{{0,55}}\b(?:on|for|during)\b.{{0,20}}\b(?:{retry_signal})\w*\b|"
                r"\bomit\w*\b.{0,25}\bidempotency(?:\s+(?:key|token))?\b"
                r".{0,35}\bafter\s+(?:the\s+)?first\s+attempt\b",
                clause,
            )
        )
        if not omits_key:
            continue
        safely_rejected = bool(
            re.search(
                rf"\b(?:do not|don't|never|must not|should not|cannot|can't)\s+"
                rf"(?:allow\s+|perform\s+)?(?:{retry_signal})\w*\b.{{0,45}}\bwithout\b",
                clause,
            )
            or re.search(
                r"\b(?:without|missing|absent)\b.{0,30}\bidempotency\s+(?:key|token)\b"
                r".{0,35}\b(?:is|are)\s+(?:forbidden|disallowed|rejected|blocked)\b",
                clause,
            )
            or re.search(
                r"\b(?:do not|don't|never|must not|should not|cannot|can't|avoid)\b"
                r".{0,35}\b(?:omit|drop|clear|remove)\w*\b.{0,35}"
                r"\bidempotency\s+(?:key|token)\b",
                clause,
            )
        )
        if not safely_rejected:
            missing_key_on_retry = True
            break
    if missing_key_on_retry:
        issues.append("unsafe_missing_idempotency_key_on_retry")

    constant_key = False
    for clause in clauses:
        constant_claim = bool(
            re.search(
                r"\b(?:global|constant|static|hard[- ]?coded|fixed)\b.{0,35}"
                r"\bidempotency\s+(?:key|token)\b.{0,70}"
                r"\b(?:all|every|each|across|system[- ]wide|service[- ]wide)\b|"
                r"\b(?:all|every|each)\b.{0,55}"
                r"\b(?:payment|account|tenant|operation|request|customer)s?\b.{0,45}"
                r"\b(?:share|use|reuse|get)\w*\b.{0,25}"
                r"\b(?:one|the\s+same|a\s+single|global|constant|static|fixed)\b"
                r".{0,20}\b(?:idempotency\s+)?(?:key|token)\b|"
                r"\b(?:one|the\s+same|a\s+single)\b.{0,25}"
                r"\b(?:idempotency\s+)?(?:key|token)\b.{0,55}"
                r"\b(?:for|across)\s+(?:all|every)\b.{0,35}"
                r"\b(?:payment|account|tenant|operation|request|customer)s?\b|"
                r"\bevery\s+provider\s+call\b.{0,45}\buses?\b.{0,35}"
                r"\b(?:the\s+payment(?:'s)?\s+)?constant\s+(?:key|token)\b"
                r".{0,45}\bacross\s+all\s+operations\b",
                clause,
            )
        )
        if not constant_claim:
            continue
        safely_rejected = bool(
            re.search(
                r"\b(?:do not|don't|never|must not|should not|cannot|can't|avoid)\b"
                r".{0,45}\b(?:use|share|reuse|hard[- ]?code)\w*\b.{0,45}"
                r"\b(?:global|constant|static|fixed|same|single|one)\b",
                clause,
            )
            or re.search(
                r"\b(?:global|constant|static|fixed|shared)\b.{0,35}"
                r"\b(?:key|token)\b.{0,30}\b(?:is|are)\s+(?:forbidden|disallowed)\b",
                clause,
            )
        )
        if not safely_rejected:
            constant_key = True
            break
    if constant_key:
        issues.append("unsafe_constant_idempotency_key")

    operation_pattern = {
        "authorize": r"\bauthoriz\w*\b",
        "capture": r"\bcaptur\w*\b",
        "refund": r"\brefund\w*\b",
    }
    unsafe_shared_key = False
    for clause in semantic_windows:
        operations = {
            operation
            for operation, pattern in operation_pattern.items()
            if re.search(pattern, clause)
        }
        if len(operations) < 2 or "key" not in clause:
            continue
        if re.search(r"\b(?:encryption|signing|hmac|webhook\s+secret)\s+key\b", clause):
            continue
        shared = bool(
            re.search(
                r"\b(?:same|single|one|shared)\b[^.!?;]{0,35}"
                r"\b(?:idempotency\s+)?key\b|"
                r"\b(?:reuse|reused|reusing)\b[^.!?;]{0,35}"
                r"\b(?:idempotency\s+)?key\b|"
                r"\b(?:idempotency\s+)?key\b[^.!?;]{0,35}"
                r"\b(?:same|single|one|shared)\b|"
                r"\b(?:share|reuse|reuses|reused|reusing)\b[^.!?;]{0,25}"
                r"\b(?:it|that\s+key|this\s+key)\b",
                clause,
            )
        )
        if not shared:
            continue
        operation_scoped = bool(
            re.search(
                r"\b(?:one|a|distinct|separate|derived)\b.{0,30}"
                r"\b(?:idempotency\s+)?key\b"
                r".{0,20}\bper\s+(?:payment\s+)?operation\b|"
                r"\b(?:distinct|separate|different|unique|derived)\b.{0,30}\bkeys?\b"
                r".{0,25}\b(?:for|across)\b.{0,100}"
                r"\b(?:authoriz\w*|captur\w*|refund\w*)\b|"
                r"\b(?:authoriz\w*|captur\w*|refund\w*)\b.{0,140}"
                r"\b(?:use|uses|have|has|get|gets)\b.{0,30}"
                r"\b(?:distinct|separate|different|derived)\b.{0,50}\bkeys?\b|"
                r"\b(?:each|every)\b.{0,100}\boperation\b.{0,100}"
                r"\b(?:its\s+)?own\b.{0,35}\b(?:idempotency\s+)?key\b|"
                r"\b(?:each|every)\b.{0,100}\boperation\b.{0,40}"
                r"\b(?:gets?|has|uses?)\b.{0,20}\b(?:a\s+)?unique\b"
                r".{0,20}\b(?:idempotency\s+)?key\b|"
                r"\b(?:its|their)\s+own\b.{0,25}\bidempotency\s+key\b|"
                r"\b(?:same|stable)\b.{0,25}\bkey\b.{0,60}\bonly\b.{0,60}"
                r"\b(?:same|that)\s+operation\b",
                clause,
            )
            or re.search(
                r"\b(?:stable\s+)?idempotency\s+key\b.{0,35}\bper\b"
                r".{0,100}\boperation[- ]type\b.{0,70}"
                r"\boperation[- ]instance\b|"
                r"\bidempotency\s+key\b.{0,40}\bscoped\s+to\b.{0,140}"
                r"\boperation[- ]type\b.{0,80}\boperation[- ]instance\b",
                clause,
            )
            or re.search(
                r"\b(?:same|that)\s+(?:logical\s+|provider\s+)?operation\b"
                r".{0,45}\b(?:uses?|gets?|keeps?|reuses?)\b.{0,35}"
                r"\b(?:the\s+)?same\b.{0,20}\bstable\b.{0,25}"
                r"\b(?:provider\s+)?(?:idempotency\s+)?key\b",
                clause,
            )
            or re.search(
                r"\b(?:retr(?:y|ies|ied|ying)|replay\w*)\b.{0,30}"
                r"\b(?:use|uses|using|for)\b.{0,30}"
                r"\b(?:the\s+)?same\s+(?:logical\s+|provider\s+)?operation\b"
                r".{0,40}\b(?:the\s+)?same\b.{0,20}\bstable\b.{0,25}"
                r"\b(?:provider\s+)?(?:idempotency\s+)?key\b",
                clause,
            )
            or (
                len(operations) == 3
                and re.search(
                    r"\beach\b.{0,35}\b(?:gets?|has|uses?)\b.{0,20}"
                    r"\b(?:a\s+)?(?:unique|distinct|separate)\b.{0,20}"
                    r"\b(?:idempotency\s+)?key\b",
                    clause,
                )
            )
            or (
                len(operations) == 3
                and re.search(
                    r"\beach\b.{0,30}\bauthoriz\w*\b.{0,50}\bcaptur\w*\b"
                    r".{0,50}\brefund\w*\b.{0,35}\b(?:gets?|has|uses?)\b"
                    r".{0,20}\b(?:its\s+own|a\s+(?:unique|distinct|separate))\b"
                    r".{0,25}\b(?:stable\s+)?(?:idempotency\s+)?key\b",
                    clause,
                )
            )
        )
        safely_rejected = bool(
            re.search(
                r"\b(?:do not|don't|never|must not|should not|cannot|can't)\s+"
                r"(?:use|reuse|share)?\w*\b.{0,45}\b(?:same|single|one|shared)\b"
                r".{0,35}\b(?:idempotency\s+)?key\b",
                clause,
            )
            or re.search(
                r"\b(?:do not|don't|never|must not|should not|cannot|can't)\b"
                r".{0,45}\b(?:share|reuse|use)\w*\b.{0,30}"
                r"\b(?:key|it|that\s+key|this\s+key)\b",
                clause,
            )
        )
        if not operation_scoped and not safely_rejected:
            unsafe_shared_key = True
            break
    if unsafe_shared_key:
        issues.append("unsafe_shared_idempotency_key_across_payment_operations")

    has_all_payment_operations = all(
        re.search(pattern, lower) for pattern in operation_pattern.values()
    )
    distinct_operation_keys = bool(
        has_all_payment_operations
        and (
            re.search(
                r"\b(?:distinct|separate|different|independent|operation[- ]specific)\b"
                r".{0,50}\b(?:idempotency\s+)?keys?\b",
                lower,
            )
            or re.search(
                r"\b(?:each|every)\b.{0,180}\b(?:authoriz\w*|captur\w*|refund\w*)\b"
                r".{0,180}\b(?:its\s+own|their\s+own|a\s+unique|an\s+independent)\b"
                r".{0,35}\b(?:idempotency\s+)?key\b",
                lower,
            )
            or re.search(
                r"\b(?:do not|don't|never|must not|cannot|can't)\b.{0,60}"
                r"\b(?:share|reuse|use)\w*\b.{0,45}"
                r"\b(?:same|single|one|shared)?\s*(?:idempotency\s+)?key\b"
                r".{0,120}\b(?:across|between|for)\b",
                lower,
            )
            or re.search(
                r"\bauthoriz\w*\b.{0,80}\bcaptur\w*\b.{0,80}\brefund\w*\b"
                r".{0,45}\beach\b.{0,35}\b(?:gets?|has|uses?)\b.{0,20}"
                r"\b(?:a\s+)?(?:unique|distinct|separate)\b.{0,20}"
                r"\b(?:idempotency\s+)?key\b",
                lower,
            )
            or re.search(
                r"\beach\b.{0,30}\bauthoriz\w*\b.{0,50}\bcaptur\w*\b"
                r".{0,50}\brefund\w*\b.{0,35}\b(?:gets?|has|uses?)\b"
                r".{0,20}\b(?:its\s+own|a\s+(?:unique|distinct|separate))\b"
                r".{0,25}\b(?:stable\s+)?(?:idempotency\s+)?key\b",
                lower,
            )
            or re.search(
                r"\b(?:authoriz\w*|captur\w*|refund\w*)\b.{0,180}"
                r"\b(?:payment|intent|account)[- ]?id\b.{0,80}"
                r"\boperation[- ]?(?:type|kind)\b.{0,80}"
                r"\b(?:operation[- ]?(?:id|instance)|sequence|index|ordinal)\b",
                lower,
            )
        )
    )
    if re.search(
        r"\b(?:without|no|not|never|do\s+not|don't|does\s+not|doesn't|"
        r"must\s+not|cannot|can't|fails?\s+to)\b.{0,70}"
        r"\b(?:distinct|separate|different|independent|operation[- ]specific)\b"
        r".{0,50}\b(?:idempotency\s+)?keys?\b",
        lower,
    ):
        distinct_operation_keys = False
    same_operation_retry_reuse = bool(
        re.search(
            r"\b(?:retr(?:y|ies|ied|ying)|replay\w*|resubmit\w*)\b.{0,80}"
            r"\b(?:same|original)\b.{0,30}"
            r"\b(?:operation|command|authorization|capture|refund|charge|payment\s+request)\b"
            r".{0,80}\b(?:same|stable|original|existing)\b.{0,30}"
            r"\b(?:idempotency\s+)?key\b",
            lower,
        )
        or re.search(
            r"\b(?:reuse|preserve|keep)\w*\b.{0,35}"
            r"\b(?:same|stable|original|existing|that)\b.{0,20}"
            r"\b(?:idempotency\s+)?key\b.{0,80}"
            r"\b(?:retry|replay|resubmit)\w*\b.{0,50}"
            r"\b(?:same|that|original)\b.{0,20}"
            r"\b(?:operation|command|authorization|capture|refund|charge)\b",
            lower,
        )
        or re.search(
            r"\b(?:retry|replay|resubmit)\w*\b.{0,45}"
            r"\b(?:reuse|preserve|keep)\w*\b.{0,45}"
            r"\b(?:same|stable|original|existing|operation(?:'s)?)\b.{0,35}"
            r"\b(?:idempotency\s+)?key\b",
            lower,
        )
        or re.search(
            r"\b(?:idempotency\s+)?keys?\b.{0,35}\b(?:remain|stay|are)\b"
            r".{0,20}\bstable\b.{0,45}\b(?:retry|replay)\w*\b"
            r".{0,45}\b(?:same|original)\s+(?:operation|command)\b",
            lower,
        )
        or re.search(
            r"\b(?:reuse|reuses|reused|reusing)\b.{0,45}"
            r"\b(?:operation(?:'s)?|command(?:'s)?)\b.{0,30}"
            r"\b(?:same|stable|original|existing)\b.{0,25}\bkey\b"
            r".{0,50}\b(?:retry|replay)\w*\b",
            lower,
        )
        or re.search(
            r"\b(?:reuse|reuses|reused|reusing)\b.{0,35}"
            r"\b(?:the\s+)?same\b.{0,25}\b(?:idempotency\s+)?key\b"
            r".{0,60}\b(?:retr(?:y|ies)|replay)\w*\b.{0,25}\bof\b"
            r".{0,25}\b(?:the\s+)?same\b.{0,20}"
            r"\b(?:operation|command|authorization|capture|refund|charge)\b",
            lower,
        )
        or re.search(
            r"\b(?:same|original)\b.{0,30}"
            r"\b(?:charge\s+)?(?:operation|command|authorization|capture|refund|charge)\b"
            r".{0,45}\b(?:keep|keeps|preserve|preserves|reuse|reuses)\w*\b"
            r".{0,30}\b(?:its\s+|the\s+)?(?:idempotency\s+)?key\b"
            r".{0,70}\b(?:retr(?:y|ied)|replay)\w*\b",
            lower,
        )
        or re.search(
            r"\b(?:use|uses|reuse|reuses|reused|reusing)\b.{0,30}"
            r"\b(?:the\s+)?same\b.{0,20}\b(?:idempotency\s+)?key\b"
            r".{0,25}\bfor\b.{0,25}\b(?:the\s+)?same\b.{0,25}"
            r"\b(?:logical\s+|provider\s+)?(?:operation|command)\b"
            r".{0,25}\b(?:retry|replay)\w*\b",
            lower,
        )
        or re.search(
            r"\b(?:the\s+)?(?:same|original)\b.{0,25}"
            r"\b(?:logical\s+|provider\s+)?(?:operation|command)(?:'s)?\b"
            r".{0,20}\b(?:idempotency\s+)?key\b.{0,20}"
            r"\b(?:is|was)\s+(?:reused|kept|preserved)\b.{0,55}"
            r"\b(?:the\s+)?same\b.{0,25}"
            r"\b(?:logical\s+|provider\s+)?(?:operation|command)\b"
            r".{0,30}\b(?:retry|replay)\w*\b",
            lower,
        )
        or has_safe_payment_same_operation_replay_condition(lower)
        or re.search(
            r"\b(?:same|exact|original)\b.{0,25}\b(?:command|request|action)\b"
            r".{0,45}\b(?:keeps?|retains?|reuses?|preserves?)\b.{0,30}"
            r"\b(?:its\s+)?(?:same\s+|original\s+|stable\s+)?"
            r"(?:idempotency\s+)?(?:key|token)\b"
            r".{0,45}\b(?:across|on|for)\b.{0,20}\b(?:attempt|retry|replay)s?\b",
            lower,
        )
        or re.search(
            r"\bretr(?:y|ies)\b.{0,35}\bdeterministically\b.{0,35}"
            r"\b(?:recomputes?|derives?|recreates?)\b.{0,25}"
            r"\b(?:the\s+)?identical\b.{0,20}"
            r"\b(?:idempotency\s+)?(?:key|token)\b",
            lower,
        )
    )
    negated_same_operation_retry_reuse = bool(
        re.search(
            r"\b(?:do not|don't|never|must not|should not|avoid)\b\s+"
            r"(?:reuse|reusing|preserve|preserving|keep|keeping)\w*\b.{0,35}"
            r"\b(?:the\s+)?same\b.{0,25}\b(?:idempotency\s+)?key\b"
            r".{0,70}\b(?:retr(?:y|ies)|replay)\w*\b.{0,35}"
            r"\b(?:the\s+)?same\b.{0,25}"
            r"\b(?:operation|command|authorization|capture|refund|charge)\b",
            lower,
        )
        or re.search(
            r"\b(?:do not|don't|never|must not|should not|avoid)\b\s+"
            r"(?:retry|replay)\w*\b.{0,35}\b(?:the\s+)?(?:same|original)\b"
            r".{0,25}\b(?:charge\s+)?"
            r"(?:operation|command|authorization|capture|refund|charge)\b"
            r".{0,45}\b(?:with|using|under)\b.{0,25}"
            r"\b(?:the\s+)?(?:same|original)\b.{0,20}"
            r"\b(?:idempotency\s+)?key\b",
            lower,
        )
        or re.search(
            r"\b(?:the\s+)?(?:same|original)\b.{0,25}"
            r"\b(?:charge\s+)?(?:operation|command|authorization|capture|refund|charge)\b"
            r".{0,35}\b(?:must|should)\s+not\b.{0,20}"
            r"\b(?:keep|preserve|reuse)\w*\b.{0,30}"
            r"\b(?:its\s+|the\s+)?(?:idempotency\s+)?key\b"
            r".{0,55}\b(?:retr(?:y|ied)|replay)\w*\b",
            lower,
        )
        or re.search(
            r"\b(?:do not|don't|never|must not|should not|avoid)\b\s+"
            r"(?:use|using)\b.{0,30}\b(?:the\s+)?same\b.{0,20}"
            r"\b(?:idempotency\s+)?key\b.{0,25}\bfor\b.{0,25}"
            r"\b(?:the\s+)?same\b.{0,25}"
            r"\b(?:logical\s+|provider\s+)?(?:operation|command)\b"
            r".{0,25}\b(?:retry|replay)\w*\b",
            lower,
        )
    )
    if negated_same_operation_retry_reuse:
        same_operation_retry_reuse = False
    operation_scoped_stable_key = bool(
        re.search(
            r"\b(?:each|every)\b.{0,80}\b(?:logical\s+|provider\s+|payment\s+)?"
            r"(?:operation|command)\b.{0,80}"
            r"\b(?:its\s+own|their\s+own|a\s+(?:stable|unique|distinct))\b"
            r".{0,35}\bidempotency\s+key\b",
            lower,
        )
        or re.search(
            r"\b(?:stable|persistent|durable|operation[- ]scoped)\b.{0,25}"
            r"\bidempotency\s+key\b.{0,45}"
            r"\b(?:per|for\s+each|for\s+every)\b.{0,30}"
            r"\b(?:logical\s+|provider\s+|payment\s+)?(?:operation|command)\b",
            lower,
        )
        or re.search(
            r"\bidempotency\s+key\b.{0,25}\bper\b.{0,25}"
            r"\b(?:logical\s+|provider\s+|payment\s+)?(?:operation|command)\b",
            lower,
        )
        or re.search(
            r"\b(?:same|that)\s+(?:logical\s+|provider\s+)?operation\b"
            r".{0,45}\b(?:uses?|gets?|keeps?|reuses?)\b.{0,35}"
            r"\b(?:the\s+)?same\b.{0,20}\bstable\b.{0,25}"
            r"\b(?:provider\s+)?idempotency\s+key\b",
            lower,
        )
        or re.search(
            r"\b(?:retr(?:y|ies|ied|ying)|replay\w*)\b.{0,30}"
            r"\b(?:use|uses|using|for)\b.{0,30}"
            r"\b(?:the\s+)?same\s+(?:logical\s+|provider\s+)?operation\b"
            r".{0,40}\b(?:the\s+)?same\b.{0,20}\bstable\b.{0,25}"
            r"\b(?:provider\s+)?idempotency\s+key\b",
            lower,
        )
        or re.search(
            r"\beach\b.{0,30}\bauthoriz\w*\b.{0,50}\bcaptur\w*\b"
            r".{0,50}\brefund\w*\b.{0,35}\b(?:gets?|has|uses?)\b"
            r".{0,20}\b(?:its\s+own|a\s+(?:unique|distinct|separate))\b"
            r".{0,25}\b(?:stable\s+)?(?:idempotency\s+)?key\b",
            lower,
        )
        or (distinct_operation_keys and same_operation_retry_reuse)
        or re.search(
            r"\b(?:each|every)\b.{0,70}\b(?:logical\s+)?"
            r"(?:operation|command|request|action)\s+instance\b.{0,70}"
            r"\b(?:its\s+own|a\s+(?:stable|durable|unique|distinct))\b.{0,30}"
            r"\bidempotency\s+(?:key|token)\b",
            lower,
        )
        or re.search(
            r"\b(?:derive|namespace|compute)\w*\b.{0,45}"
            r"\bidempotency\s+(?:key|token)\b.{0,110}"
            r"\b(?:payment|intent|account)[- ]?id\b.{0,80}"
            r"\boperation[- ]?(?:type|kind)\b.{0,80}"
            r"\b(?:operation[- ]?(?:id|instance)|sequence|index|ordinal)\b",
            lower,
        )
    )
    if require_complete_idempotency_semantics:
        if not operation_scoped_stable_key:
            issues.append("missing_stable_idempotency_key_per_operation")
        if not distinct_operation_keys:
            issues.append("missing_distinct_authorize_capture_refund_keys")
        if not same_operation_retry_reuse:
            issues.append("missing_same_operation_idempotency_key_reuse")
    elif (
        require_same_operation_retry_reuse
        and not same_operation_retry_reuse
        and not has_explicit_no_provider_command_replay(lower)
    ):
        issues.append("missing_same_operation_idempotency_key_reuse")

    provider_event_id = (
        r"(?:provider|processor|gateway)(?:'s)?(?:[- ]supplied)?\s+"
        r"(?:event|notification|webhook)?\s*(?:id|identifier)|"
        r"webhook(?:[- ]supplied)?\s+event\s+(?:id|identifier)|"
        r"(?:provider|processor|gateway|webhook)[-_]event[-_]?id|"
        r"(?:event\s+(?:id|identifier))\s+(?:supplied|assigned|returned)\s+by\s+"
        r"(?:the\s+)?(?:provider|processor|gateway)"
    )
    provider_event_dedup_patterns = (
        rf"\bdedup\w*\b[^.!?;]{{0,45}}\bwebhooks?\b[^.!?;]{{0,45}}"
        rf"\b(?:by|using|with|on|keyed\s+by)\b[^.!?;]{{0,25}}"
        rf"\b(?:{provider_event_id})\b",
        rf"\bwebhooks?\b[^.!?;]{{0,55}}\bdedup\w*\b[^.!?;]{{0,45}}"
        rf"\b(?:by|using|with|on|keyed\s+by)\b[^.!?;]{{0,25}}"
        rf"\b(?:{provider_event_id})\b",
        rf"\b(?:use|using|persist|store|record|insert)\w*\b[^.!?;]{{0,35}}"
        rf"\b(?:{provider_event_id})\b[^.!?;]{{0,55}}"
        rf"\b(?:to\s+dedup\w*|unique\s+(?:constraint|index))\b"
        rf"[^.!?;]{{0,55}}\bwebhooks?\b",
        rf"\bwebhooks?\b[^.!?;]{{0,55}}\b(?:use|using|persist|store|record|insert)\w*\b"
        rf"[^.!?;]{{0,35}}\b(?:{provider_event_id})\b[^.!?;]{{0,55}}"
        rf"\b(?:to\s+dedup\w*|unique\s+(?:constraint|index))\b",
        rf"\b(?:{provider_event_id})\b[^.!?;]{{0,45}}"
        rf"\b(?:dedup\w*|unique\s+(?:constraint|index))\b"
        rf"[^.!?;]{{0,55}}\bwebhooks?\b",
        rf"\bwebhooks?\b[^.!?;]{{0,55}}\b(?:persist|store|record|insert)\w*\b"
        rf"[^.!?;]{{0,55}}\b(?:database\s+)?(?:uniqueness|unique)\s+"
        rf"(?:constraint|index)\b[^.!?;]{{0,25}}\b(?:on|for)\b"
        rf"[^.!?;]{{0,20}}\b(?:{provider_event_id})\b",
    )
    provider_event_dedup = any(
        re.search(pattern, lower) for pattern in provider_event_dedup_patterns
    )

    operation_key = (
        r"idempotency\s+key|operation\s+(?:key|id)|payment(?:\s+operation)?\s+id|"
        r"payment_id"
    )
    unsafe_webhook_operation_key = False
    bad_webhook_patterns = (
        rf"\bdedup\w*\b[^.!?;]{{0,45}}\bwebhooks?\b[^.!?;]{{0,45}}"
        rf"\b(?:by|using|with|on|keyed\s+by)\b[^.!?;]{{0,25}}"
        rf"\b(?:{operation_key})\b",
        rf"\bwebhooks?\b[^.!?;]{{0,55}}\b(?:dedup\w*|idempotent\w*)\b"
        rf"[^.!?;]{{0,45}}\b(?:by|using|with|on|keyed\s+by)\b"
        rf"[^.!?;]{{0,25}}\b(?:{operation_key})\b",
        rf"\b(?:use|using)\b[^.!?;]{{0,35}}\b(?:{operation_key})\b"
        rf"[^.!?;]{{0,45}}\b(?:as|to|for)\b[^.!?;]{{0,35}}"
        rf"\b(?:dedup\w*|idempotent\w*)\b[^.!?;]{{0,35}}\bwebhooks?\b",
        rf"\b(?:use|using)\b[^.!?;]{{0,35}}\b(?:{operation_key})\b"
        rf"[^.!?;]{{0,45}}\bas\b[^.!?;]{{0,25}}\bwebhook\b"
        rf"[^.!?;]{{0,25}}\bdedup\w*\b",
    )
    for pattern in bad_webhook_patterns:
        for match in re.finditer(pattern, lower):
            prefix = lower[max(0, match.start() - 80) : match.start()]
            if re.search(
                r"\b(?:do not|don't|never|must not|should not|cannot|can't)\s*$",
                prefix,
            ) or re.search(
                r"\b(?:not|rather\s+than|instead\s+of)\b.{0,40}"
                r"\b(?:idempotency|operation|payment)\b",
                match.group(),
            ):
                continue
            unsafe_webhook_operation_key = True
            break
        if unsafe_webhook_operation_key:
            break
    if unsafe_webhook_operation_key:
        issues.append("unsafe_webhook_dedup_by_operation_key")

    webhook_used = any(
        "webhook" in clause
        and not re.search(
            r"\b(?:do not|don't|never|without|instead of|not rely(?:ing)? on)\b"
            r".{0,40}\bwebhooks?\b",
            clause,
        )
        for clause in clauses
    )
    if (require_webhook_event_dedup or webhook_used) and not provider_event_dedup:
        issues.append("missing_provider_event_id_webhook_dedup")

    def is_distinct_user_authorized_payment_after_reconciliation(
        action_start: int,
        action_end: int,
    ) -> bool:
        """Allow only a new logical purchase after the ambiguous one is resolved."""
        prefix = lower[max(0, action_start - 700) : action_start]
        context = lower[max(0, action_start - 360) : action_end + 120]
        near_action = lower[max(0, action_start - 160) : action_end + 80]
        if re.search(
            r"\b(?:retry|replay|resubmit)\w*\b.{0,70}"
            r"\b(?:same|original|ambiguous)\b.{0,35}"
            r"\b(?:operation|charge|payment|request|purchase|order)\b",
            near_action,
        ):
            return False
        distinct_purchase = bool(
            re.search(
                r"\b(?:distinct|separate|unrelated)\b.{0,45}"
                r"\b(?:later|subsequent|new)?\s*"
                r"(?:payment|purchase|order|payment\s+intent|logical\s+operation)\b",
                context,
            )
            or re.search(
                r"\b(?:later|subsequent)\b.{0,35}\bnew\b.{0,25}"
                r"\b(?:purchase|order|payment\s+intent)\b",
                context,
            )
        )
        user_authorized = bool(
            re.search(
                r"\b(?:user|customer|cardholder)\b.{0,70}"
                r"\b(?:explicitly\s+)?(?:authoriz|approv|request|initiat|confirm)\w*\b",
                context,
            )
            or re.search(
                r"\bexplicit\s+(?:user|customer|cardholder)\s+"
                r"(?:authorization|approval|request|confirmation)\b",
                context,
            )
        )
        authoritative_resolution = bool(
            re.search(
                r"\b(?:provider|processor|gateway|acquirer)(?:'s)?\b.{0,80}"
                r"\b(?:status(?:\s+(?:api|query|lookup|response|result))?|"
                r"signed\s+webhook|webhook)\b.{0,100}"
                r"\b(?:confirm|report|return|show|verify|reconcil|resolv)\w*\b"
                r".{0,100}\b(?:succeeded|failed|canceled|cancelled|declined|"
                r"no\s+charge|charged|captured|terminal)\b",
                prefix,
            )
            or re.search(
                r"\b(?:signed\s+webhook|status(?:\s+(?:api|query|lookup|response|result))?)\b"
                r".{0,70}\bfrom\s+(?:the\s+)?(?:provider|processor|gateway|acquirer)\b"
                r".{0,100}\b(?:confirm|report|return|show|verify|reconcil|resolv)\w*\b"
                r".{0,100}\b(?:succeeded|failed|canceled|cancelled|declined|"
                r"no\s+charge|charged|captured|terminal)\b",
                prefix,
            )
        )
        original_resolved = bool(
            re.search(
                r"\b(?:original|previous|ambiguous|timed[- ]out)\b.{0,50}"
                r"\b(?:operation|charge|payment|request|attempt)\b.{0,100}"
                r"\b(?:reconcil|resolv|terminal|confirm)\w*\b",
                prefix,
            )
            or re.search(
                r"\b(?:reconcil|resolv|terminal|confirm)\w*\b.{0,100}"
                r"\b(?:original|previous|ambiguous|timed[- ]out)\b.{0,50}"
                r"\b(?:operation|charge|payment|request|attempt)\b",
                prefix,
            )
        )
        return bool(
            distinct_purchase
            and user_authorized
            and authoritative_resolution
            and original_resolved
        )

    ambiguous_outcome = (
        r"timeout|timed out|unknown|ambiguous|pending[-_ ]reconciliation|"
        r"outcome\s+(?:is|remains)\s+uncertain"
    )
    new_charge_action = (
        r"(?:issue|send|submit|create|initiate|make|start|dispatch|enqueue|attempt)\w*"
        r"\s+(?:a\s+|the\s+)?(?:another|new|fresh|replacement|second)\s+"
        r"(?:charge|payment|authorization|payment\s+request|processor\s+request)"
    )
    unsafe_new_charge = False
    for match in re.finditer(rf"\b(?:{new_charge_action})\b", lower):
        window = lower[max(0, match.start() - 260) : match.end() + 260]
        if not re.search(rf"\b(?:{ambiguous_outcome})\b", window):
            continue
        if is_distinct_user_authorized_payment_after_reconciliation(
            match.start(), match.end()
        ):
            continue
        prefix = lower[max(0, match.start() - 100) : match.start()]
        if re.search(
            r"\b(?:do not|don't|never|must not|should not|cannot|can't|without|"
            r"block|blocks|blocked|prevent|prevents|prevented)\s*$",
            prefix,
        ) or re.search(
            r"\b(?:block|prevent)\w*\s+(?:(?:a|the|any)\s+)?"
            r"(?:(?:worker|system|client)\s+)?(?:from\s+)?$",
            prefix,
        ):
            continue
        unsafe_new_charge = True
        break
    if not unsafe_new_charge:
        passive_new_charge = re.finditer(
            r"\b(?:another|new|fresh|replacement|second)\s+"
            r"(?:charge|payment|authorization)\b.{0,30}"
            r"\b(?:is|gets?)\s+(?:issued|sent|submitted|created|initiated|made)\b",
            lower,
        )
        for match in passive_new_charge:
            window = lower[max(0, match.start() - 260) : match.end() + 260]
            prefix = lower[max(0, match.start() - 30) : match.start()]
            if (
                re.search(rf"\b(?:{ambiguous_outcome})\b", window)
                and not re.search(r"\b(?:not|never)\b", match.group())
                and not re.search(r"\b(?:no|without)\s*$", prefix)
            ):
                if is_distinct_user_authorized_payment_after_reconciliation(
                    match.start(), match.end()
                ):
                    continue
                unsafe_new_charge = True
                break
    if not unsafe_new_charge:
        keyed_charge = re.finditer(
            r"\b(?:issue|send|submit|create|initiate|make|start|dispatch)\w*\s+"
            r"(?:a\s+|the\s+)?(?:charge|payment|authorization)\b",
            lower,
        )
        for match in keyed_charge:
            window = lower[max(0, match.start() - 260) : match.end() + 260]
            if not re.search(rf"\b(?:{ambiguous_outcome})\b", window):
                continue
            second_effect = bool(
                re.search(r"\b(?:again|anew)\b", window)
                or re.search(rf"\b(?:{new_key_signal})\b", window)
            )
            prefix = lower[max(0, match.start() - 100) : match.start()]
            if second_effect and not re.search(
                r"\b(?:do not|don't|never|must not|should not|cannot|can't|without|"
                r"block|blocks|blocked|prevent|prevents|prevented)\s*$",
                prefix,
            ):
                if is_distinct_user_authorized_payment_after_reconciliation(
                    match.start(), match.end()
                ):
                    continue
                unsafe_new_charge = True
                break
    if unsafe_new_charge:
        issues.append("unsafe_new_charge_after_ambiguous_outcome")

    return issues


def self_check_payment_operation_semantics() -> None:
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
    )
    for value in safe:
        assert not payment_operation_semantic_issues(
            value, require_webhook_event_dedup=True
        ), value

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


def payment_platform_safety_issues(text: str) -> List[str]:
    """Find missing durable boundaries or unsafe volatile ones in payment designs."""
    lower = re.sub(
        r"\s+",
        " ",
        re.sub(r"[*_`~]+", "", text.casefold().replace("’", "'")),
    )
    ledger_property = (
        r"durable|persistent|transactional|append[- ]only|double[- ]entry|"
        r"database[- ]backed|postgres(?:ql)?|relational\s+database"
    )
    clauses = [
        clause.strip()
        for clause in re.split(
            r"(?<=[.!?;:])\s+|\n+|\b(?:but|however|instead)\b",
            lower,
        )
        if clause.strip()
    ]

    def negates_ledger(clause: str) -> bool:
        return bool(
            re.search(r"\b(?:no|without)\s+(?:a\s+)?(?:\w+[- ]?){0,3}ledger\b", clause)
            or re.search(
                rf"\bnot\s+(?:a\s+)?(?:{ledger_property})\s+ledger\b",
                clause,
            )
            or re.search(
                r"\bnon[- ](?:durable|persistent|transactional)\b.{0,40}\bledger\b|"
                r"\bledger\b.{0,40}\bnon[- ](?:durable|persistent|transactional)\b",
                clause,
            )
            or re.search(
                r"\b(?:do not|don't|never|must not|should not|cannot|can't|"
                r"avoid|skip|omit)\s+(?:(?:use|write|persist|create|maintain|"
                r"record|keep|rely on)\s+)?(?:\w+[- ]?){0,4}ledger\b",
                clause,
            )
            or re.search(
                rf"\bledger\b.{{0,35}}\b(?:is|should be|must be|will be|remain)\s+"
                rf"(?:not|never)\s+(?:{ledger_property})\b",
                clause,
            )
        )

    durable_ledger = any(
        not negates_ledger(clause)
        and (
            re.search(rf"\b(?:{ledger_property})\b.{{0,100}}\bledger\b", clause)
            or re.search(rf"\bledger\b.{{0,100}}\b(?:{ledger_property})\b", clause)
        )
        for clause in clauses
    )

    def negates_reconciliation(clause: str) -> bool:
        return bool(
            re.search(r"\b(?:no|without)\s+(?:provider\s+)?reconcil\w*\b", clause)
            or re.search(
                r"\b(?:do not|don't|never|must not|should not|cannot|can't|"
                r"avoid|skip|omit)\s+(?:(?:perform|run|use|attempt)\s+)?"
                r"reconcil\w*\b",
                clause,
            )
            or re.search(
                r"\b(?:do not|don't|never|must not|should not|cannot|can't|"
                r"avoid|skip|omit)\s+(?:query|check|lookup|poll|accept|process|use)"
                r".{0,35}\b(?:provider|processor|gateway|status|webhooks?)\b",
                clause,
            )
            or re.search(
                r"\breconcil\w*\b.{0,30}\b(?:is|will be|should be|must be)\s+"
                r"(?:not|never)\s+(?:performed|used|run|required|supported|available)|"
                r"\breconcil\w*\b.{0,20}\b(?:is|becomes?)\s+(?:unnecessary|"
                r"disabled|omitted)",
                clause,
            )
            or re.search(
                r"\b(?:provider\s+status|webhooks?)\b.{0,30}\b(?:is|are|will be)\s+"
                r"(?:not|never)\s+(?:used|queried|checked|accepted|processed)",
                clause,
            )
        )

    reconciliation = any(
        not negates_reconciliation(clause)
        and (
            re.search(r"\breconcil\w*\b", clause)
            or re.search(
                r"\b(?:provider|processor|gateway)\b.{0,100}"
                r"\b(?:status(?:\s+(?:query|lookup|check))?|webhooks?)\b",
                clause,
            )
            or re.search(
                r"\b(?:status(?:\s+(?:query|lookup|check))?|webhooks?)\b.{0,100}"
                r"\b(?:provider|processor|gateway)\b",
                clause,
            )
        )
        for clause in clauses
    )

    provider_call = (
        r"(?:(?:call|invoke|contact)\w*\s+(?:the\s+)?(?:payment\s+)?"
        r"(?:provider|processor|gateway)|"
        r"(?:dispatch|send|submit)\w*\s+(?:the\s+)?(?:charge|payment|request)"
        r"\s+to\s+(?:the\s+)?(?:provider|processor|gateway)|"
        r"(?:charge|authorize|capture)\w*\s+(?:the\s+)?(?:card|payment))"
    )
    durable_persist = (
        r"(?:(?:persist|write|create|insert|store|save|commit|record)\w*\b.{0,60}"
        r"\b(?:payment\s+intent|idempotency\s+(?:key|record)|ledger|"
        r"durable\s+(?:record|state)))"
    )
    provider_first_patterns = (
        (
            rf"\b{provider_call}\b.{{0,80}}\b(?:first\b.{{0,50}}\bthen|then|before|"
            rf"and\s+(?:only\s+)?then)"
            rf"\b.{{0,100}}\b{durable_persist}\b"
        ),
        rf"\b{provider_call}\b.{{0,40}}\bfirst\b.{{0,130}}\b{durable_persist}\b",
        (
            rf"\b{provider_call}\b.{{0,120}}\b{durable_persist}\b.{{0,30}}"
            r"\b(?:afterward|afterwards)\b"
        ),
    )

    def has_unnegated_order(pattern: str) -> bool:
        for match in re.finditer(pattern, lower):
            prefix = lower[max(0, match.start() - 70) : match.start()]
            if re.search(
                r"\b(?:do not|don't|never|must not|should not|cannot|can't)"
                r"(?:\s+\w+){0,4}\s*$",
                prefix,
            ):
                continue
            return True
        return False

    provider_before_persist = any(has_unnegated_order(pattern) for pattern in provider_first_patterns)

    def has_affirmative_volatile_boundary(clause: str) -> bool:
        volatile_subject = (
            r"(?:redis(?:\s+(?:setnx|locks?))?|setnx|"
            r"(?:(?:short[- ]lived|ephemeral|volatile|distributed)\s+)+"
            r"(?:redis\s+)?locks?)"
        )
        authority = (
            r"(?:financial\s+correctness|correctness\s+boundary|"
            r"source\s+of\s+truth|authoritative)"
        )
        patterns = (
            rf"\b{volatile_subject}\b\s+(?:is|are|remains?|becomes?|provides?|"
            rf"acts?\s+as|serves?\s+as)\s+(?:the\s+|our\s+|a\s+)?{authority}\b",
            rf"\b{authority}\b\s+(?:is|are|remains?)\s+(?:the\s+|our\s+|a\s+)?"
            rf"{volatile_subject}\b",
            rf"\b(?:use|treat|make)\w*\s+(?:the\s+)?{volatile_subject}\b"
            rf"\s+(?:as\s+)?(?:the\s+|our\s+|a\s+)?{authority}\b",
            rf"\b(?:financial\s+correctness|duplicate\s+prevention)\b"
            rf"\s+(?:relies|depends)\s+on\s+(?:the\s+)?{volatile_subject}\b",
            rf"\b{volatile_subject}\b\s+(?:alone\s+)?"
            r"(?:guarantee\w*|prevent\w*)\b"
            r".{0,45}\b(?:duplicate|double[- ]charg\w*)\b",
        )
        for pattern in patterns:
            for candidate in re.finditer(pattern, clause):
                prefix = clause[max(0, candidate.start() - 40) : candidate.start()]
                if re.search(
                    r"\b(?:do\s+not|don't|never|must\s+not|should\s+not|"
                    r"cannot|can't|avoid)\b.{0,25}$",
                    prefix,
                ):
                    continue
                return True
        return False

    volatile_boundary = any(
        has_affirmative_volatile_boundary(clause) for clause in clauses
    )
    if not volatile_boundary:
        volatile_boundary = bool(
            re.search(
                r"\bcheck\w*\b.{0,50}\bredis\b.{0,100}\b(?:missing|absent|not\s+found)\b"
                r".{0,100}\bcharg\w*\b.{0,100}\b(?:write|put|set|store)\w*\b",
                lower,
            )
        )

    ledger_windows = list(clauses)
    ledger_windows.extend(
        f"{clauses[index]} {clauses[index + 1]}"
        for index in range(max(0, len(clauses) - 1))
    )
    effect_verb = r"(?:append|post|book|record|write|apply|create|debit|credit|mutate|update)"
    effect_target = r"(?:ledger|financial\s+effect|money\s+movement)"

    def has_authoritative_success(window: str) -> bool:
        negated = bool(
            re.search(
                r"\b(?:not|never|without|unconfirmed|unauthenticated)\b.{0,35}"
                r"\b(?:confirm|authoritative|approve|approved|settled|succeed|funds?\s+moved)\w*\b|"
                r"\b(?:confirm|authoritative|approve|approved|settled|succeed)\w*\b"
                r".{0,20}\bnot\b",
                window,
            )
        )
        positive = bool(
            re.search(
                r"\b(?:authoritative\w*|confirm(?:ed|s|ing)?|succeed(?:ed|s|ing)?|"
                r"explicitly\s+approv(?:e|ed|es|ing)|settled|funds?\s+moved|"
                r"money\s+moved|confirmed\s+(?:effect|movement|authorization|capture|refund))\b",
                window,
            )
        )
        return positive and not negated

    unqualified_ledger_movement = False
    for window in ledger_windows:
        provider_then_effect = bool(
            re.search(
                r"\b(?:provider|processor|gateway)\b.{0,55}"
                r"\b(?:respond\w*|response|reply|status)\b.{0,120}"
                rf"\b{effect_verb}\w*\b.{{0,55}}\b{effect_target}\b",
                window,
            )
            or re.search(
                r"\b(?:on|after|for|if)\b.{0,35}\b(?:any\s+|every\s+)?"
                r"(?:(?:synchronous|sync)\s+)?(?:provider\s+|processor\s+|gateway\s+)?"
                r"(?:response|reply|status)\b.{0,120}"
                rf"\b{effect_verb}\w*\b.{{0,55}}\b{effect_target}\b",
                window,
            )
        )
        effect_then_provider = bool(
            re.search(
                rf"\b{effect_verb}\w*\b.{{0,55}}\b{effect_target}\b.{{0,100}}"
                r"\b(?:on|after|for)\b.{0,30}\b(?:any\s+|every\s+)?"
                r"(?:response|reply|status)\b.{0,35}\bfrom\b.{0,20}"
                r"\b(?:provider|processor|gateway)\b",
                window,
            )
        )
        if (provider_then_effect or effect_then_provider) and not has_authoritative_success(
            window
        ):
            unqualified_ledger_movement = True
            break

    issues: List[str] = []
    if not durable_ledger:
        issues.append("missing_durable_payment_ledger")
    if not reconciliation:
        issues.append("missing_payment_reconciliation_path")
    if volatile_boundary:
        issues.append("unsafe_volatile_payment_correctness_boundary")
    if provider_before_persist:
        issues.append("unsafe_provider_before_durable_persistence")
    if unqualified_ledger_movement:
        issues.append("unsafe_unqualified_payment_ledger_movement")
    return issues


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
