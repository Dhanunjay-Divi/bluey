"""Coaching-answer ownership contracts for the Bluey live evaluator."""

from __future__ import annotations

import re
from typing import List


_TAKEOVER_ACTION = re.compile(
    r"\b(?:take\s+over(?:\s+the\s+work)?|"
    r"take\s+ownership\s+of\s+(?:it|the\s+work|their\s+code|the\s+code)"
    r"(?:\s+myself)?|"
    r"(?:rewrite|fix|correct|update)\w*\s+(?:their|the)\s+code(?:\s+myself)?|"
    r"(?:do|finish|complete|handle|make|implement|write|refactor|submit|apply|"
    r"perform|change|"
    r"solve|resolve|rewrite|fix|correct|update)\w*\s+"
    r"(?:it|the\s+work|the\s+fix|the\s+change|the\s+patch|their\s+code|"
    r"the\s+code)\s+myself|"
    r"step\s+in\s+and\s+(?:do|finish|complete|fix|write|implement)\s+"
    r"(?:it|the\s+work|the\s+fix|the\s+change|the\s+patch|their\s+code|"
    r"the\s+code)(?:\s+myself)?)\b"
)
_FIRST_PERSON_IN_CLAUSE = re.compile(
    r"\b(?:i|i'd|i'll|i\s+would|i\s+will|we|we'd|we'll|we\s+would|we\s+will)\b"
)
_SAFE_REJECTION_BEFORE = re.compile(
    r"\b(?:do\s+not|does\s+not|will\s+not|would\s+not|should\s+not|"
    r"must\s+not|don't|doesn't|won't|wouldn't|shouldn't|mustn't|never|without|"
    r"avoid(?:s|ed|ing)?|"
    r"rather\s+than|instead\s+of|refuse\s+to)\b[^.!?;]{0,55}$"
)
_LEARNER_DELEGATION_BEFORE = re.compile(
    r"\b(?:ask|tell|have|let|help)\w*\s+(?:them|the\s+learner|the\s+junior|"
    r"the\s+engineer)\s+to\s+$"
)


def q48_coaching_ownership_issues(text: str) -> List[str]:
    """Reject affirmative first-person takeover while allowing explicit rejection."""
    lower = re.sub(r"\s+", " ", text.casefold().replace("’", "'")).strip()
    for match in _TAKEOVER_ACTION.finditer(lower):
        clause_start = max(
            lower.rfind(delimiter, 0, match.start())
            for delimiter in (".", "!", "?", ";")
        )
        lead = lower[clause_start + 1 : match.start()]
        rejection = _SAFE_REJECTION_BEFORE.search(lead)
        safely_rejected = rejection is not None and not re.search(
            r"\b(?:but|however|yet)\b", rejection.group(0)
        )
        if safely_rejected or _LEARNER_DELEGATION_BEFORE.search(lead):
            continue
        if _FIRST_PERSON_IN_CLAUSE.search(lead) or lead.rstrip().endswith(
            ("then", "and then")
        ):
            return ["unsafe_q48_coach_takes_over"]
    return []


def self_check_q48_coaching_ownership() -> None:
    safe = (
        "I coach them with a concrete example and checklist, keeping the ownership "
        "with them. I ask them to update the code; they still do the work.",
        "I keep ownership with them and do not rewrite the code for them.",
        "I pair briefly to explain the pattern rather than rewriting their code.",
        "I would not take over and rewrite their code myself.",
    )
    unsafe = (
        "I ask them to update the code, then take over and rewrite their code myself.",
        "I explain the mistake, then I take over the work.",
        "I use a checklist, but I fix their code myself.",
        "I coach them with an example and checklist, keeping ownership with them. "
        "Then I fix it myself.",
        "I coach them and keep ownership with them. Then I rewrite it myself.",
        "I coach them and keep ownership with them. Then I correct the work myself.",
        "I do not take over, but I update it myself.",
        "I coach them and keep ownership with them. Then I take ownership of it myself.",
        "I coach them and keep ownership with them. Then I handle the work myself.",
        "I coach them and keep ownership with them. Then I make the fix myself.",
        "I coach them and keep ownership with them. Then I implement the change myself.",
        "I coach them and keep ownership with them. Then I write the patch myself.",
        "I coach them and keep ownership with them. Then I refactor it myself.",
        "I coach them and keep ownership with them. Then I perform the fix myself.",
        "I coach them and keep ownership with them. Then I change the code myself.",
        "I coach them and keep ownership with them. Then I step in and do it.",
    )
    assert all(not q48_coaching_ownership_issues(answer) for answer in safe)
    assert all(
        q48_coaching_ownership_issues(answer) == ["unsafe_q48_coach_takes_over"]
        for answer in unsafe
    )
