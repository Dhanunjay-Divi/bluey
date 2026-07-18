"""Detect unsafe idempotency-key sharing between payment operations.

The detector intentionally models a small set of payment relations instead of
trying to infer intent from arbitrary word proximity.  It keeps bounded context
so a policy denial cannot hide a later contradictory behavior claim, while
same-operation retries and metadata-only references remain safe.
"""

from __future__ import annotations

from dataclasses import dataclass
import re
from typing import Iterable, Optional, Tuple


_OPERATION = r"(?:partial\s+)?(?:authoriz\w*|captur\w*|refund\w*)"
_CREDENTIAL = (
    r"(?:(?:child|provider|operation|partial[- ]action)\s+)?"
    r"(?:(?:idempotency\s+)?(?:key|token|credential)|idempotency\s+value)s?"
)
_SHARED_QUALIFIER = (
    r"(?:one|single|same|common|shared|identical|matching|equal|equivalent|original)"
)
_SHARING_VERB = (
    r"(?:share|use|reuse|retain|keep|have|map|alias|inherit|borrow|take|reference|"
    r"assign|issue|run|key|carry|bear|span|cover|apply)\w*"
)
_NEGATION = (
    r"(?:do\s+not|don't|does\s+not|doesn't|did\s+not|didn't|must\s+not|"
    r"should\s+not|cannot|can't|never|would\s+not|wouldn't|will\s+not|won't|"
    r"is\s+not|isn't|are\s+not|aren't|reject\w*|forbid\w*|avoid\w*|neither|no)"
)
_DISCOURSE_BOUNDARY = re.compile(
    r"(?<=[.!?;])\s+|\s*[—–]\s*|"
    r"\b(?:but|however|yet|nevertheless|although|even\s+though|except|"
    r"instead(?:\s+of)?|rather(?:\s+than)?|while|whereas)\b"
)
_NON_OPERATIONAL_REFERENCE = re.compile(
    r"\b(?:audit|metadata|log|logging|trace|traceability|correlation|diagnostic)\w*\b"
)
_NON_OPERATIONAL_SIMILARITY = re.compile(
    r"\b(?:audit|metadata|log|logging|trace|traceability|correlation|diagnostic|"
    r"format|schema|length|prefix|shape|encoding|naming|header\s+name|field\s+name|"
    r"generator|key[- ]generation|derivation|helper|function|algorithm|library|"
    r"database\s+(?:table|column)|table|column|hmac|encryption|signing|"
    r"cryptographic)\w*\b"
)
_OWN_PROVIDER_KEY = re.compile(
    rf"\b(?:own|distinct|different|separate|new|fresh|unique)\b[^.!?;]{{0,25}}"
    rf"\b{_CREDENTIAL}\b|"
    rf"\b{_CREDENTIAL}\b[^.!?;]{{0,25}}"
    r"\b(?:own|distinct|different|separate|new|fresh|unique)\b"
)
_PER_OPERATION_CREDENTIAL = re.compile(
    rf"\b(?:one|single|a)\b[^.!?;]{{0,15}}\b{_CREDENTIAL}\b"
    r"[^.!?;]{0,20}\bper\b[^.!?;]{0,15}"
    r"\b(?:(?:logical|provider)\s+)?operation(?:[- ]instance)?\b"
)
_SHARED_TRANSPORT_DISTINCT_VALUES = re.compile(
    r"\b(?:same|shared|common)\b[^.!?;]{0,20}"
    r"\bidempotency[- ]key\b[^.!?;]{0,12}\b(?:header|field)\b"
    r"(?:\s*,?\s*(?:but|while|yet|and|with)\b)?"
    r"[^.!?;]{0,20}\b(?:distinct|different|separate|unique)\b"
    r"[^.!?;]{0,18}\b(?:values?|tokens?|keys?)\b|"
    r"\b(?:distinct|different|separate|unique)\b[^.!?;]{0,18}"
    r"\b(?:values?|tokens?|keys?)\b[^.!?;]{0,20}"
    r"\b(?:through|via|in|using)\b[^.!?;]{0,12}"
    r"\b(?:the\s+)?(?:same|shared|common)\b[^.!?;]{0,20}"
    r"\bidempotency[- ]key\b[^.!?;]{0,12}\b(?:header|field)\b"
)


@dataclass
class _Context:
    """Only the recent bindings needed for narrow payment coreference."""

    active_pair: Tuple[str, ...] = ()
    last_subject: Optional[str] = None
    key_owner: Optional[str] = None
    key_age: int = 99
    partial_new_key_boundary: bool = False


def _canonical_operation(value: str) -> str:
    lower = re.sub(r"\s+", " ", value.casefold()).strip()
    partial = lower.startswith("partial ")
    if "authoriz" in lower:
        kind = "authorize"
    elif "captur" in lower:
        kind = "capture"
    else:
        kind = "refund"
    return f"partial_{kind}" if partial and kind != "authorize" else kind


def _operation_mentions(segment: str) -> Tuple[str, ...]:
    mentions = [_canonical_operation(match.group()) for match in re.finditer(_OPERATION, segment)]
    if re.search(
        r"\bpartial\s+captur\w*\s*(?:/|\bor\b|\band\b)\s*refund\w*\b",
        segment,
    ):
        mentions.extend(("partial_capture", "partial_refund"))
    ordered = []
    for mention in mentions:
        if mention not in ordered:
            ordered.append(mention)
    return tuple(ordered)


def _segments(text: str) -> Iterable[str]:
    normalized = re.sub(r"\s+", " ", text.casefold().replace("’", "'")).strip()
    # Keep a transport-header/value contrast in one segment.  The shared header
    # name is conventional; the distinct transmitted values are the safety
    # boundary that the relation detector must see before evaluating the clause.
    normalized = re.sub(
        r"(\b(?:same|shared|common)\b[^.!?;]{0,20}"
        r"\bidempotency[- ]key\b[^.!?;]{0,12}\b(?:header|field)\b"
        r"[^.!?;]{0,20})\b(?:but|while|yet)\b"
        r"(?=[^.!?;]{0,30}\b(?:distinct|different|separate|unique)\b"
        r"[^.!?;]{0,18}\b(?:values?|tokens?|keys?)\b)",
        r"\1and",
        normalized,
    )
    for discourse_segment in _DISCOURSE_BOUNDARY.split(normalized):
        discourse_segment = discourse_segment.strip(" ,:-")
        if not discourse_segment:
            continue
        # A negated policy followed by a comma and a new subject is two claims.
        # Splitting only that shape preserves ordinary operation lists.
        marked = re.sub(
            rf"(\b{_NEGATION}\b[^,]{{0,80}}),\s*"
            rf"(?=(?:{_OPERATION}|it\b|they\b|both\b|the\s+two\b))",
            lambda match: f"{match.group(1)}\x00",
            discourse_segment,
        )
        marked = re.sub(
            rf",\s*(?=(?:{_OPERATION})[^,]{{0,35}}\b(?:and|or)\b"
            rf"[^,]{{0,25}}(?:{_OPERATION})[^,]{{0,55}}"
            rf"\b(?:{_SHARING_VERB})\b)",
            "\x00",
            marked,
        )
        comma_parts = marked.split("\x00")
        for part in comma_parts:
            part = part.strip(" ,:-")
            if part:
                yield part


def _claim_is_negated(segment: str, claim_start: int) -> bool:
    prefix = segment[max(0, claim_start - 85) : claim_start]
    return bool(re.search(rf"\b{_NEGATION}\b[^.!?;,]{{0,60}}$", prefix))


def _same_operation_retry_summary(segment: str) -> bool:
    if not re.search(r"\b(?:retr(?:y|ies|ied|ying)|replay\w*|resubmit\w*)\b", segment):
        return False
    cross_scope = bool(
        re.search(
            r"\b(?:across|between|for\s+(?:both|either)|both\s+operations?|"
            r"two\s+operation[- ]types?|regardless\s+of\s+whether)\b|"
            r"\bnew\s+partial\s+action\b[^.!?;]{0,35}"
            r"\b(?:also\s+)?(?:gets?|has|uses?|receives?)\b[^.!?;]{0,25}"
            r"\b(?:the\s+)?same\b[^.!?;]{0,15}\b(?:key|token)\b",
            segment,
        )
    )
    same_scope = bool(
        re.search(
            r"\b(?:exact|that\s+same|same)\b[^.!?;]{0,30}"
            r"\b(?:partial\s+)?(?:action|operation|authorization|capture|refund)\b|"
            r"\bretries\s+of\s+the\s+same\b[^.!?;]{0,45}"
            r"\b(?:authorization|capture|refund)\b|"
            r"\bpartial\s+capture\s*/\s*refund\s+retr\w*\b[^.!?;]{0,45}"
            r"\bpartial[- ]action\s+key\b",
            segment,
        )
    )
    return same_scope and not cross_scope


def _explicit_key_owner(segment: str) -> Optional[str]:
    owner_patterns = (
        re.compile(
            rf"\b(?P<owner>{_OPERATION})\b[^.!?;]{{0,30}}"
            r"\b(?:owns?|gets?|has|receives?|mints?|creates?|is\s+assigned)\b"
            rf"[^.!?;]{{0,30}}\b{_CREDENTIAL}\b"
        ),
        re.compile(
            rf"\b{_CREDENTIAL}\b[^.!?;]{{0,25}}\b(?:for|owned\s+by)\b"
            rf"[^.!?;]{{0,15}}\b(?P<owner>{_OPERATION})\b"
        ),
        re.compile(
            rf"\b(?P<owner>{_OPERATION})\b[^.!?;]{{0,25}}"
            r"\b(?:records?|stores?|logs?)\b[^.!?;]{0,20}\bits\b"
            rf"[^.!?;]{{0,15}}\b{_CREDENTIAL}\b"
        ),
        re.compile(
            rf"\b(?P<owner>{_OPERATION})\b[^.!?;]{{0,20}}\buses?\b"
            r"(?![^.!?;]{0,18}\b(?:inherited|that|this|another|"
            r"authoriz\w*|captur\w*|refund\w*)\b)"
            rf"[^.!?;]{{0,25}}\b{_CREDENTIAL}\b"
        ),
    )
    for pattern in owner_patterns:
        match = pattern.search(segment)
        if match and not _claim_is_negated(segment, match.start()):
            return _canonical_operation(match.group("owner"))
    return None


def _mentioned_key_owner(segment: str) -> Optional[str]:
    """Resolve a directly named key owner even inside a denied relation."""
    direct_owner = re.compile(
        rf"\b(?P<owner>{_OPERATION})(?:'s)?\b\s+"
        r"(?:(?:own|original|stable|child|provider|operation|partial[- ]action|"
        r"idempotency)\s+){0,4}(?:key|token|credential)s?\b"
    )
    key_for_owner = re.compile(
        rf"\b{_CREDENTIAL}\b[^.!?;]{{0,12}}\b(?:for|of)\b"
        rf"[^.!?;]{{0,12}}\b(?P<owner>{_OPERATION})\b"
    )
    matches = [*direct_owner.finditer(segment), *key_for_owner.finditer(segment)]
    matches.sort(key=lambda match: match.start())
    if not matches:
        return None
    return _canonical_operation(matches[-1].group("owner"))


def _has_collective_shared_key(segment: str, context: _Context) -> bool:
    segment = _PER_OPERATION_CREDENTIAL.sub("operation-scoped identifier", segment)
    if _same_operation_retry_summary(segment):
        return False
    if re.search(r"\b(?:each|every)\s+(?:retry|replay|attempt)\b", segment):
        return False
    inverse_group_claim = re.search(
        rf"\b(?:the\s+|one\s+|same\s+|shared\s+)?{_CREDENTIAL}\b"
        r"[^.!?;]{0,35}\b(?:applies?|covers?|spans?|is\s+used)\b"
        r"[^.!?;]{0,35}\b(?:all\s+(?:three|3)|both|the\s+two)\b"
        r"[^.!?;]{0,20}\b(?:operations?|actions?|rows?)\b",
        segment,
    )
    if inverse_group_claim and not _claim_is_negated(
        segment, inverse_group_claim.start()
    ):
        return True
    pair_anaphor = re.search(
        rf"\b(?:both|they|the\s+two|these|those)\b[^.!?;]{{0,30}}"
        rf"\b(?:share|use|reuse|retain|keep|reference|have)\w*\b"
        rf"[^.!?;]{{0,25}}(?:\b(?:it|that\s+(?:key|token|credential)|"
        rf"this\s+key)\b|\b(?:one|same|common|shared|identical|matching)\b"
        rf"[^.!?;]{{0,15}}\b{_CREDENTIAL}\b)",
        segment,
    )
    current_operations = _operation_mentions(segment)
    has_active_pair = bool(context.active_pair or len(current_operations) >= 2)
    if (
        pair_anaphor
        and has_active_pair
        and (
            not re.search(r"\bit\b", pair_anaphor.group())
            or (context.key_owner is not None and context.key_age <= 1)
        )
        and not _claim_is_negated(segment, pair_anaphor.start())
    ):
        return True
    collective = re.search(
        r"\b(?:both|the\s+two|all|every|each)\b[^.!?;]{0,30}"
        r"\b(?:partial[- ]operation\s+rows?|partial\s+actions?|actions?|rows?|operations?)\b",
        segment,
    )
    if not collective:
        return False
    claim = re.search(
        rf"\b(?:{_SHARING_VERB})\b[^.!?;]{{0,35}}"
        rf"\b(?:the\s+|their\s+)?{_SHARED_QUALIFIER}\b[^.!?;]{{0,20}}"
        rf"\b{_CREDENTIAL}\b|"
        rf"\b{_SHARED_QUALIFIER}\b[^.!?;]{{0,20}}\b{_CREDENTIAL}\b"
        rf"[^.!?;]{{0,35}}\b(?:for|between|across|to)\b[^.!?;]{{0,25}}"
        r"\b(?:both|the\s+two|all|every|each)\b",
        segment,
    )
    if claim and not _claim_is_negated(segment, claim.start()):
        return True
    return False


def _has_directional_sharing(segment: str) -> bool:
    """Return true when one named operation is assigned another operation's key."""
    forward_patterns = (
        re.compile(
            rf"\b(?P<target>{_OPERATION})\b[^.!?;]{{0,40}}"
            r"\b(?P<verb>use|reuse|inherit|borrow|take|keep|retain|attach|pass|put|"
            r"place|inject|populate|write|"
            r"cop(?:y|ies|ied|ying)|"
            r"carr(?:y|ies|ied|ying)|reference|"
            r"run|issue|key|map|submit|dispatch|forward|send|set)\w*\b"
            r"(?:\s+(?:to|under|with|by|into|in))?[^.!?;]{0,28}"
            rf"\b(?P<owner>{_OPERATION})(?:'s)?\b[^.!?;]{{0,20}}\b{_CREDENTIAL}\b"
        ),
        re.compile(
            rf"\b(?P<target>{_OPERATION})\b[^.!?;]{{0,35}}"
            r"\b(?:is\s+)?(?:issued|run|keyed)\s+under\b[^.!?;]{0,25}"
            rf"\b(?P<owner>{_OPERATION})(?:'s)?\b[^.!?;]{{0,20}}\b{_CREDENTIAL}\b"
        ),
    )
    for pattern in forward_patterns:
        for match in pattern.finditer(segment):
            if _canonical_operation(match.group("target")) == _canonical_operation(
                match.group("owner")
            ):
                continue
            verb_start = match.start("verb") if "verb" in match.groupdict() else match.start()
            if _NON_OPERATIONAL_REFERENCE.search(segment) and re.search(
                r"\b(?:reference|store|log|record|trace)\w*\b",
                match.group(),
            ):
                continue
            if _NON_OPERATIONAL_REFERENCE.search(segment) and re.search(
                r"\b(?:only\s+as|solely\s+as)\b[^.!?;]{0,25}"
                r"\b(?:correlation|metadata|audit|trace)\b|"
                r"\bnot\s+(?:sent\s+|attached\s+|passed\s+|copied\s+)?to\b"
                r"[^.!?;]{0,20}\bprovider\s+request\b",
                segment,
            ):
                continue
            if not _claim_is_negated(segment, verb_start):
                return True

    inverse_patterns = (
        re.compile(
            rf"\b(?:the\s+)?(?P<owner>{_OPERATION})(?:'s)?\b[^.!?;]{{0,20}}"
            rf"\b{_CREDENTIAL}\b[^.!?;]{{0,30}}"
            r"\b(?:is\s+)?(?:cop(?:y|ies|ied|ying)|placed|put|written|injected)\b"
            r"[^.!?;]{0,25}\b(?:into|in|onto|to)\b[^.!?;]{0,20}"
            rf"\b(?:the\s+)?(?P<target>{_OPERATION})\b"
            r"[^.!?;]{0,25}\b(?:provider\s+)?(?:request|header|command)\b"
        ),
        re.compile(
            rf"\b{_CREDENTIAL}\b[^.!?;]{{0,20}}\bfor\b[^.!?;]{{0,15}}"
            rf"\b(?P<owner>{_OPERATION})\b[^.!?;]{{0,30}}"
            r"\b(?:is\s+)?(?:also\s+)?(?:used|reused|assigned|applied|shared|inherited)\b"
            r"[^.!?;]{0,20}\b(?:by|for|to)\b[^.!?;]{0,18}"
            rf"\b(?P<target>{_OPERATION})\b"
        ),
        re.compile(
            rf"\b(?:the\s+)?(?P<owner>{_OPERATION})(?:'s)?\b[^.!?;]{{0,20}}"
            rf"\b{_CREDENTIAL}\b[^.!?;]{{0,35}}"
            r"\b(?:is\s+)?(?:also\s+)?(?:used|reused|assigned|applied|shared)\b"
            r"[^.!?;]{0,20}\b(?:by|for|to|as)\b[^.!?;]{0,18}"
            rf"\b(?P<target>{_OPERATION})\b"
        ),
        re.compile(
            rf"\b{_CREDENTIAL}\b[^.!?;]{{0,25}}\bassigned\s+to\b"
            rf"[^.!?;]{{0,18}}\b(?P<owner>{_OPERATION})\b[^.!?;]{{0,35}}"
            r"\b(?:is\s+)?also\s+assigned\s+to\b[^.!?;]{0,18}"
            rf"\b(?P<target>{_OPERATION})\b"
        ),
        re.compile(
            rf"\b(?:the\s+)?(?P<owner>{_OPERATION})(?:'s)?\b[^.!?;]{{0,20}}"
            rf"\b{_CREDENTIAL}\b[^.!?;]{{0,28}}\b(?:aliases?|doubles?)\b"
            r"[^.!?;]{0,20}\b(?:as|for)\b[^.!?;]{0,15}"
            rf"\b(?P<target>{_OPERATION})\b"
        ),
    )
    for pattern in inverse_patterns:
        for match in pattern.finditer(segment):
            if _canonical_operation(match.group("target")) == _canonical_operation(
                match.group("owner")
            ):
                continue
            if not _claim_is_negated(segment, match.start()):
                return True

    token_alias = re.search(
        rf"\b(?P<target>{_OPERATION})\b[^.!?;]{{0,20}}\b{_CREDENTIAL}\b"
        r"[^.!?;]{0,25}\b(?:aliases?|maps?\s+to)\b[^.!?;]{0,20}"
        rf"\b(?P<owner>{_OPERATION})\b[^.!?;]{{0,20}}\b{_CREDENTIAL}\b",
        segment,
    )
    return bool(
        token_alias
        and _canonical_operation(token_alias.group("target"))
        != _canonical_operation(token_alias.group("owner"))
        and not _claim_is_negated(segment, token_alias.start())
    )


def _has_pairwise_shared_key(segment: str, operations: Tuple[str, ...]) -> bool:
    segment = _PER_OPERATION_CREDENTIAL.sub("operation-scoped identifier", segment)
    if len(operations) < 2 or not re.search(rf"\b{_CREDENTIAL}\b", segment):
        return False
    if _same_operation_retry_summary(segment):
        return False
    direct_idempotency_value_share = re.search(
        r"\b(?:one|single|same|common|shared|identical|matching|equal|equivalent)\b"
        r"[^.!?;]{0,18}\bidempotency[- ](?:key|token|credential|value)s?\b"
        r"(?![^.!?;]{0,12}\b(?:header\s+name|field\s+name|database\s+column|"
        r"schema|format|length|prefix)\b)|"
        r"\bidempotency[- ](?:key|token|credential|value)s?\b"
        r"[^.!?;]{0,25}\b(?:are|is|must\s+be|should\s+be)?\s*"
        r"(?:the\s+)?(?:same|common|shared|identical|matching|equal|equivalent)\b",
        segment,
    )
    denied_equivalence = re.search(
        r"\bidempotency[- ](?:key|token|credential|value)s?\b"
        r"[^.!?;]{0,25}\b(?:are|is|must\s+be|should\s+be)?\s*not\s+"
        r"(?:the\s+)?(?:same|common|shared|identical|matching|equal|equivalent)\b",
        segment,
    )
    if (
        direct_idempotency_value_share
        and not _NON_OPERATIONAL_SIMILARITY.search(
            segment[
                direct_idempotency_value_share.start() :
                direct_idempotency_value_share.end() + 18
            ]
        )
        and not denied_equivalence
        and not _claim_is_negated(segment, direct_idempotency_value_share.start())
    ):
        return True
    if _NON_OPERATIONAL_SIMILARITY.search(segment):
        return False

    qualifier = re.search(
        rf"\b{_SHARED_QUALIFIER}\b[^.!?;]{{0,25}}\b{_CREDENTIAL}\b|"
        rf"\bmatching\b[^.!?;]{{0,15}}\b{_CREDENTIAL}\b|"
        rf"\b{_CREDENTIAL}\b[^.!?;]{{0,30}}"
        rf"\b(?:(?:is|are|remains?|must\s+be|should\s+be)\s+)?"
        rf"(?:the\s+)?{_SHARED_QUALIFIER}\b",
        segment,
    )
    if (
        qualifier
        and not denied_equivalence
        and not _claim_is_negated(segment, qualifier.start())
    ):
        return True

    shared_action = re.search(
        rf"\b(?:share|use|reuse|retain|keep|map|carry|have)\w*\b"
        rf"[^.!?;]{{0,35}}\b(?:a|the|their|one)?\s*{_CREDENTIAL}\b",
        segment,
    )
    if shared_action and not _claim_is_negated(segment, shared_action.start()):
        if not re.search(
            rf"\b(?:distinct|different|separate|own|unique|new|fresh)\b"
            rf"[^.!?;]{{0,20}}\b{_CREDENTIAL}\b",
            segment,
        ):
            return True
    return False


def _has_contextual_sharing(
    segment: str,
    operations: Tuple[str, ...],
    context: _Context,
) -> bool:
    segment = _PER_OPERATION_CREDENTIAL.sub("operation-scoped identifier", segment)
    local_owner = _explicit_key_owner(segment) or _mentioned_key_owner(segment)
    if not local_owner and context.key_age > 12:
        return False

    claim = re.search(
        r"\b(?:it\s+)?(?:does\s+)?"
        r"(?:use|reuse|inherit|borrow|take|keep|retain|carry)\w*\b"
        r"[^.!?;]{0,25}\b(?:it|that\s+(?:key|token|credential)|"
        r"the\s+inherited\s+token|inherited\s+token)\b|"
        r"\b(?:uses?|reuses?|keeps?|takes?)\b[^.!?;]{0,20}"
        r"\bthe\s+inherited\s+token\b|"
        rf"\bit\b[^.!?;]{{0,15}}"
        r"\b(?:share|use|reuse|inherit|borrow|take|keep|retain)\w*\b"
        rf"[^.!?;]{{0,25}}\b{_CREDENTIAL}\b|"
        r"^\s*(?:share|use|reuse|inherit|borrow|take|keep|retain)\w*\b"
        rf"[^.!?;]{{0,25}}\b{_CREDENTIAL}\b|"
        r"\b(?:send|forward)\w*\b[^.!?;]{0,30}"
        r"\b(?:it|that\s+(?:key|token|credential))\b[^.!?;]{0,35}"
        r"\b(?:provider|request|header|money\s+command)\b|"
        r"\b(?:attach|pass|cop(?:y|ies|ied|ying))\w*\b[^.!?;]{0,25}"
        r"\b(?:it|that\s+(?:key|token|credential))\b[^.!?;]{0,35}"
        r"\b(?:provider|request|header|money\s+command)\b|"
        r"\b(?:call|invoke)\w*\b[^.!?;]{0,25}\bprovider\b"
        r"[^.!?;]{0,20}\bwith\s+(?:it|that\s+(?:key|token))\b|"
        r"\b(?:submit|dispatch)\w*\b[^.!?;]{0,20}\bunder\b"
        r"[^.!?;]{0,15}\b(?:it|that\s+(?:key|token|credential))\b|"
        r"\bset\w*\b[^.!?;]{0,35}"
        r"\b(?:provider\s+)?(?:idempotency[- ]key\s+)?header\b"
        r"[^.!?;]{0,20}\bto\s+(?:it|that\s+(?:key|token))\b",
        segment,
    )
    if not claim or _claim_is_negated(segment, claim.start()):
        return False
    if _OWN_PROVIDER_KEY.search(claim.group()):
        return False

    pronoun_subject = bool(
        re.match(r"\s*(?:in\s+\w+\s*,\s*)?it\b", segment)
    )
    implicit_subject = bool(
        re.match(
            r"\s*(?:share|use|reuse|inherit|borrow|take|keep|retain)\w*\b",
            segment,
        )
    )
    explicit_subjects = list(re.finditer(_OPERATION, segment[: claim.start()]))
    if pronoun_subject or implicit_subject:
        target = context.last_subject
    elif explicit_subjects:
        target = _canonical_operation(explicit_subjects[-1].group())
    else:
        target = operations[0] if operations else context.last_subject

    owner = local_owner or context.key_owner
    return bool(target and owner and target != owner)


def has_affirmative_cross_operation_key_sharing(text: str) -> bool:
    """Recognize an affirmed shared credential across distinct money operations."""
    normalized = re.sub(r"\s+", " ", text.casefold().replace("’", "'"))
    partial_new_key_boundary = bool(
        re.search(
            r"\bnew\s+partial\s+(?:capture|refund)\b[^.!?;]{0,90}"
            r"\bnew\s+(?:(?:logical|provider)\s+)?(?:action|operation|key)\b"
            r"[^.!?;]{0,35}\bnew\s+(?:idempotency\s+)?key\b|"
            r"\bnew\s+partial\s+(?:capture|refund)\b[^.!?;]{0,110}"
            r"\b(?:gets?|has|uses?|receives?|mints?|creates?)\b[^.!?;]{0,20}"
            r"\b(?:a\s+)?(?:new|fresh|unique|different)\b[^.!?;]{0,15}"
            r"\b(?:idempotency\s+)?key\b|"
            r"\b(?:each|every)\b[^.!?;]{0,80}\b(?:logical\s+)?"
            r"(?:provider[- ]?)?operation[- ]instance\b[^.!?;]{0,80}"
            r"\b(?:its|their)\s+own\b[^.!?;]{0,35}"
            r"\b(?:stable\s+)?idempotency\s+key\b|"
            r"\beach\s+authoriz\w*\s*,\s*captur\w*\s*,?\s+and\s+"
            r"refund\w*(?:\s*,\s*including\s+each\s+partial\s+captur\w*"
            r"\s+or\s+refund\w*)?[^.!?;]{0,35}"
            r"\b(?:its|their)\s+own\b[^.!?;]{0,20}"
            r"\b(?:stable\s+)?idempotency\s+key\b",
            normalized,
        )
    )
    context = _Context(partial_new_key_boundary=partial_new_key_boundary)
    for segment in _segments(text):
        context.key_age += 1
        operations = _operation_mentions(segment)
        relation_segment = _SHARED_TRANSPORT_DISTINCT_VALUES.sub(
            "common transport field with operation-distinct values",
            segment,
        )

        if (
            _same_operation_retry_summary(segment)
            and not context.partial_new_key_boundary
            and {"partial_capture", "partial_refund"}.issubset(context.active_pair)
            and re.search(r"\boriginal\b[^.!?;]{0,20}\b(?:key|token)\b", segment)
        ):
            return True

        if _has_directional_sharing(relation_segment):
            return True
        if _has_collective_shared_key(relation_segment, context):
            return True
        if _has_pairwise_shared_key(relation_segment, operations):
            return True
        if _has_contextual_sharing(relation_segment, operations, context):
            return True

        if len(operations) >= 2:
            context.active_pair = operations
        elif re.search(
            r"\b(?:one|common|shared|same)\s+partial\s+(?:idempotency\s+)?key\b"
            r"[^.!?;]{0,30}\b(?:both|the\s+two)\s+actions\b",
            segment,
        ):
            context.active_pair = ("partial_action_1", "partial_action_2")
        if operations:
            context.last_subject = operations[0]

        owner = _explicit_key_owner(segment)
        if owner:
            context.key_owner = owner
            context.key_age = 0
            context.last_subject = owner
        elif mentioned_owner := _mentioned_key_owner(segment):
            context.key_owner = mentioned_owner
            context.key_age = 0
        elif (
            not _PER_OPERATION_CREDENTIAL.search(segment)
            and not _same_operation_retry_summary(segment)
            and not re.search(
                r"\b(?:distinct|different|separate|unique|own)\b"
                r"[^.!?;]{0,25}\b(?:idempotency\s+)?keys?\b",
                segment,
            )
        ) and (
            shared_key := re.search(
                rf"\b(?:one|single|same|shared|common)\b[^.!?;]{{0,20}}"
                rf"\b{_CREDENTIAL}\b",
                segment,
            )
        ):
            if not _claim_is_negated(segment, shared_key.start()):
                context.key_owner = "shared"
                context.key_age = 0
        elif re.search(r"\b(?:new|unrelated)\s+(?:payment|intent|purchase)\b", segment):
            context.key_owner = None
            context.key_age = 99

    return False
