"""Utilities for exact customer-visible answer contracts."""

from __future__ import annotations

import re


def has_visible_affirmative_contract_sentence(text: str, expected: str) -> bool:
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
