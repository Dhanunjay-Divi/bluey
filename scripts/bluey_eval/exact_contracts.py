"""Utilities for exact customer-visible answer contracts."""

from __future__ import annotations

import re


def has_visible_affirmative_contract_sentence(
    text: str,
    expected: str,
    *,
    allow_inline_code: bool = False,
) -> bool:
    """Return true only when expected appears as visible, affirmative prose."""
    prose = text.replace("\r\n", "\n").replace("\r", "\n")
    prose = re.sub(r"<!--.*?-->", " ", prose, flags=re.DOTALL)
    prose = re.sub(
        r"<(?:del|s)\b[^>]*>.*?</(?:del|s)>",
        " ",
        prose,
        flags=re.DOTALL | re.IGNORECASE,
    )
    prose = re.sub(
        r"(?ms)^\s*(?:```|~~~).*?^\s*(?:```|~~~)\s*$",
        " ",
        prose,
    )
    prose = re.sub(r"(?ms)^\s*(?:```|~~~).*\Z", " ", prose)
    prose = re.sub(r"~~.*?~~", " ", prose, flags=re.DOTALL)
    if allow_inline_code:
        prose = re.sub(r"`([^`\n]*)`", r"\1", prose)
    else:
        prose = re.sub(r"`[^`\n]*`", " ", prose)
    prose = re.sub(r"(?m)^\s*>.*$", " ", prose)
    prose = re.sub(r"(?m)^(?: {4}|\t).*$", " ", prose)
    normalized = re.sub(
        r"\s+",
        " ",
        re.sub(r"[*_]+", "", prose.casefold().replace("’", "'")),
    ).strip()
    target = re.sub(r"\s+", " ", expected.casefold().replace("’", "'")).rstrip(
        "."
    )

    for match in re.finditer(re.escape(target), normalized):
        prefix = normalized[max(0, match.start() - 180) : match.start()]
        clause_start = max(
            prefix.rfind(delimiter) for delimiter in (".", "!", "?", ";")
        )
        lead = prefix[clause_start + 1 :]
        rejected_before = bool(
            re.search(
                r"\b(?:ignore|reject|disregard|omit|exclude|avoid)\w*\b|"
                r"\b(?:it|this|that)\s+(?:is|was)\s+"
                r"(?:false|wrong|unsafe|inapplicable)\s*(?:that|:)?\s*$|"
                r"\b(?:it|this|that|the\s+following|(?:this|that|the)\s+claim)\s+"
                r"(?:(?:is|was)\s+(?:not\s+(?:true|correct|accurate|valid)|"
                r"false|incorrect|wrong|untrue|unsafe|inapplicable)|"
                r"(?:isn't|wasn't)\s+true|(?:cannot|can't|could\s+not|couldn't)\s+"
                r"be\s+true)\s*(?:that|:)?\s*$|"
                r"\b(?:false|incorrect|wrong|untrue|unsafe|inapplicable)\s*"
                r"(?:that|:)?\s*$|"
                r"\b(?:i|we)\s+(?:deny|reject|dispute|disagree)\s+(?:that\s*)?$|"
                r"\b(?:do\s+not|don't|never)\s+"
                r"(?:believe|accept|trust|claim|say|state|write|repeat)\s+"
                r"(?:that\s*)?$|"
                r"\b(?:not|never\s+(?:claim|say|state|write)(?:\s+that)?)\s*$|"
                r"\b(?:do\s+not|don't|never|"
                r"(?:i\s+)?(?:will|would|can|could|should|must)\s+not)\s+"
                r"(?:follow|use|apply|include|say|state|repeat|write|honor|enforce)\w*\b|"
                r"\b(?:i\s+)?refus\w*\s+to\s+"
                r"(?:follow|use|apply|include|say|state|repeat|write|honor|enforce)\w*\b|"
                r"\b(?:not\s+applicable|what\s+not\s+to\s+say|"
                r"(?:this\s+is\s+)?(?:an?\s+)?example\b.{0,40}\bnot\s+"
                r"(?:advice|guidance|policy|answer)|illustration\s+only)\b",
                lead,
            )
            or re.search(
                r"\b(?:this|that|the)\s+claim\s+"
                r"(?:(?:is|was)\s+(?:false|incorrect|wrong|untrue|unsafe|"
                r"inapplicable)|(?:isn't|wasn't)\s+true)\s*;\s*$",
                prefix,
            )
        )
        suffix = normalized[match.end() : match.end() + 160]
        rejected_after = bool(
            re.match(
                r"^[\s.,:;'\")\]-]*(?:"
                r"(?:is|was)\s+(?:false|wrong|unsafe|inapplicable)|"
                r"(?:do\s+not|don't|never|"
                r"(?:i\s+)?(?:will|would|can|could|should|must)\s+not)\s+"
                r"(?:follow|use|apply|include|say|state|repeat|honor|enforce)\w*\s+"
                r"(?:it|this)|"
                r"(?:i\s+)?refus\w*\s+to\s+"
                r"(?:follow|use|apply|include|say|state|repeat|honor|enforce)\w*\s+"
                r"(?:it|this)|"
                r"(?:ignore|reject|disregard)\w*\s+(?:it|this))\b",
                suffix,
            )
        )
        if not rejected_before and not rejected_after:
            return True
    return False
