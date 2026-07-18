"""Leadership-answer safety contracts."""

from __future__ import annotations

import re
from typing import List

from .exact_contracts import has_visible_affirmative_contract_sentence


Q47_INCIDENT_CONTAINMENT_SENTENCE = (
    "The only exception is a policy-governed production, security, safety, or "
    "compliance incident: I take only the minimum reversible containment, notify "
    "both directors immediately, and leave the resource-priority decision to their "
    "shared agreement or accountable owner."
)


def q47_incident_containment_issues(text: str) -> List[str]:
    """Require the exact customer-facing incident exception promised by the prompt."""
    if has_visible_affirmative_contract_sentence(
        text, Q47_INCIDENT_CONTAINMENT_SENTENCE
    ):
        return []
    return ["missing_policy_governed_incident_containment_exception"]


def q47_director_alignment_issues(text: str) -> List[str]:
    """Require one transparent shared decision path, not private arbitration."""
    lower = re.sub(r"\s+", " ", text.casefold().replace("’", "'"))
    action = (
        r"(?:align|communicat|speak|meet|consult|discuss|share|present|explain|"
        r"escalat|bring|coordinat|review|email|send|ask)\w*"
    )
    directors = r"(?:(?:both|two|the)\s+directors?|the\s+requesting\s+directors?)"
    visible_shared_engagement = bool(
        re.search(
            rf"\b(?:one|same|shared)?\s*(?:comparison|tradeoff|matrix|decision)\b.{{0,80}}"
            rf"\bvisible\s+to\b.{{0,25}}\b{directors}\b.{{0,180}}"
            r"\bask\w*\s+them\b.{0,45}\b(?:agree|align)\w*\b.{0,60}"
            r"\b(?:priority|order|sequence|decision|tradeoff|shared\s+rule|"
            r"which\s+(?:request|one))\b",
            lower,
        )
        and not re.search(
            r"\b(?:do\s+not|don't|never|must\s+not|should\s+not|cannot|can't|"
            r"without)\b.{0,25}\bask\w*\s+them\b.{0,35}"
            r"\b(?:agree|align)\w*\b",
            lower,
        )
    )
    shared_side_by_side_engagement = bool(
        re.search(
            rf"\b{directors}\b.{{0,220}}\b(?:single|same|shared|objective)\b"
            r".{0,45}\b(?:comparison|tradeoff|picture|view)\b.{0,220}"
            r"\bask\w*\s+them\b.{0,45}\b(?:agree|align)\w*\b.{0,60}"
            r"\b(?:priority|order|sequence|decision|tradeoff|shared\s+rule|"
            r"which\s+(?:request|one))\b",
            lower,
        )
        and not re.search(
            r"\b(?:do\s+not|don't|never|must\s+not|should\s+not|cannot|can't|"
            r"without)\b.{0,25}\bask\w*\s+them\b.{0,35}"
            r"\b(?:agree|align)\w*\b",
            lower,
        )
    )
    engagement = bool(
        re.search(rf"\b{action}\b.{{0,80}}\b{directors}\b", lower)
        or re.search(
            rf"\b{directors}\b.{{0,80}}\b(?:align|agree|review|decid|resolve|"
            r"understand|confirm)\w*\b",
            lower,
        )
        or re.search(
            rf"\b{directors}\b.{{0,160}}\bask\w*\s+them\b.{{0,35}}"
            r"\b(?:agree|align)\w*\b",
            lower,
        )
        or visible_shared_engagement
        or shared_side_by_side_engagement
    )
    shared_resolution = bool(
        re.search(
            rf"\balign\w*\b.{{0,45}}\b{directors}\b.{{0,65}}"
            r"\b(?:one|same|shared)\b.{0,30}\b(?:priority|order|decision|tradeoff)\b|"
            r"\b(?:ask|seek|work\s+with|bring|invite)\w*\b.{0,90}"
            rf"\b{directors}\b.{{0,100}}\b(?:agree|align|resolve|choose|confirm)\w*\b"
            r".{0,45}\b(?:shared|one|same|the)\b.{0,25}"
            r"\b(?:priority|order|decision)\b|"
            rf"\b{directors}\b.{{0,100}}\b(?:agree|align|resolve)\w*\b"
            r".{0,45}\b(?:priority|order|decision|tradeoff)\b|"
            r"\b(?:same|one|shared)\b.{0,30}\b(?:tradeoff|comparison|matrix)\b"
            rf".{{0,90}}\b{directors}\b.{{0,90}}\b(?:agree|align)\w*\b"
            r".{0,45}\b(?:priority|order|decision|tradeoff|shared\s+rule|"
            r"which\s+(?:request|one))\b|"
            r"\b(?:same|one|shared)\b.{0,30}\b(?:tradeoff|comparison|matrix)\b"
            rf".{{0,90}}\b{directors}\b.{{0,90}}\b(?:ask|seek)\w*\b"
            r".{0,40}\b(?:one|same|shared)\b.{0,25}"
            r"\b(?:priority|order|decision)\b|"
            rf"\b(?:email|send|present|share)\w*\b.{{0,65}}\b{directors}\b"
            r".{0,65}\b(?:same|shared|one)\b.{0,35}\b(?:matrix|tradeoff)\b"
            r".{0,65}\b(?:ask|seek)\w*\b.{0,40}\b(?:one|same|shared)\b"
            r".{0,25}\b(?:priority|order|decision)\b|"
            r"\bescalat\w*\b.{0,70}\b(?:common|accountable|shared)\b.{0,30}"
            r"\b(?:owner|sponsor|leader|manager|director|vp)\b|"
            r"\bescalat\w*\b.{0,35}\b(?:leadership|common\s+owner|sponsor)\b"
            r".{0,35}\b(?:decid|resolve|priority|order)\w*\b|"
            r"\bask\w*\s+them\b.{0,35}\b(?:agree|align)\w*\b.{0,45}"
            r"\b(?:which\s+(?:one|request)|priority|order|sequence)\b|"
            r"\bask\w*\s+for\s+(?:a|one)\s+"
            r"(?:(?:single|agreed|shared|common)\s+){1,3}"
            r"(?:priority\s+)?order\b",
            lower,
        )
        or visible_shared_engagement
        or shared_side_by_side_engagement
    )
    negated_shared_alignment = bool(
        re.search(
            r"\b(?:do\s+not|don't|never|must\s+not|should\s+not|cannot|can't|"
            r"without)\b[^.!?;]{0,35}\b(?:ask|seek|invite)\w*\b[^.!?;]{0,35}"
            r"\b(?:them|directors?)\b[^.!?;]{0,30}\b(?:agree|align)\w*\b",
            lower,
        )
        or re.search(
            r"\bask\w*\s+(?:them|the\s+directors?)\b[^.!?;]{0,20}"
            r"\bnot\s+to\s+(?:agree|align)\w*\b",
            lower,
        )
    )
    affirmative = engagement and shared_resolution and not negated_shared_alignment
    ignored_director_input = bool(
        re.search(
            r"\b(?:ignore|disregard|dismiss)\w*\b.{0,40}"
            r"\b(?:their|directors?'?|the)?\s*(?:input|feedback|views?|priorit(?:y|ies))\b",
            lower,
        )
    ) and not bool(
        re.search(
            r"\b(?:do\s+not|don't|never|avoid)\s+"
            r"(?:ignore|disregard|dismiss)\w*\b.{0,40}"
            r"\b(?:input|feedback|views?|priorit(?:y|ies))\b",
            lower,
        )
    )
    # Remove only a *directly rejected* private-arbitration action before
    # looking for unsafe choices.  Binding the rejection prefix to the action
    # matters: "they do not agree, so I pick privately" is unsafe, while "the
    # job is not to pick privately" is safe.  The match ends at the first
    # private-choice action, so a later contrastive action remains visible.
    private_choice_marker = (
        r"(?:privately|alone|unilaterally|myself|personally|independently|"
        r"behind\s+closed\s+doors|on\s+my\s+own|by\s+myself)"
    )
    private_choice_object = (
        r"(?:(?:which|what|the|a|an|this|that|either|neither|one|two|"
        r"request|requests|priority|priorities|order|sequence|winner|wins|"
        r"conflict|tradeoff|choice|ranking|decision|call|final|first|next|"
        r"work|item|task|it|between|of|both|over|other|director|directors|"
        r"competing|conflicting|their|and|or|a's|b's)\s+){0,6}"
    )
    private_decision_noun = (
        r"(?:(?:a|the)\s+)?(?:(?:priority|final)\s+){0,2}"
        r"(?:decisions?|calls?|rankings?|orders?)"
    )
    private_choice_action = (
        rf"(?:(?:decide|choose|select|pick|rank|set|resolve)\w*\b"
        rf"\s+{private_choice_object}{private_choice_marker}\b|"
        rf"{private_choice_marker}\b\s+(?:(?:i|we|personally)\s+)?"
        rf"(?:(?:choose|select|pick|rank|set|decide|resolve)\w*\b|"
        rf"mak(?:e|es|ing|ed)\s+{private_decision_noun}\b)|"
        rf"mak(?:e|es|ing|ed)\s+{private_decision_noun}\s+"
        rf"{private_choice_marker}\b|"
        r"mak(?:e|es|ing|ed)\s+(?:a\s+)?(?:unilateral|private)\s+"
        r"(?:priority\s+)?(?:decisions?|calls?)\b|"
        r"(?:unilateral|private)\s+(?:priority\s+)?(?:decisions?|calls?)\b|"
        r"(?:a\s+)?(?:unilateral\s+or\s+off[- ]channel|"
        r"off[- ]channel\s+or\s+unilateral)\s+(?:decisions?|calls?)\b)"
    )
    private_choice_rejection = (
        rf"\b(?:(?:do\s+not|don't|will\s+not|won't|would\s+not|wouldn't|"
        rf"must\s+not|should\s+not|never|not\s+to|not)\s+"
        rf"(?:(?:ever|simply|just|merely|personally|quietly|silently)\s+)?"
        rf"(?:(?:try|attempt)\w*\s+to\s+)?{private_choice_action}|"
        rf"(?:avoid(?:s|ed|ing)?|without|rather\s+than|instead\s+of|"
        rf"refus(?:e|es|ed|ing)\s+to)\s+{private_choice_action})"
    )
    unsafe_scan = re.sub(private_choice_rejection, " ", lower)
    # A candidate may describe the harm of unilateral arbitration as a warning:
    # "if I choose unilaterally, I may create avoidable risk."  Treat that as a
    # rejection only when the warning is not followed by an affirmative choice,
    # execution, or delegation.  Keeping candidate recognition separate from the
    # bounded follow-through check avoids both broad sentence-level exemptions and
    # false positives for "choose neither", "decide not to act", or "wait".
    warning_choice_verb = (
        r"(?:choose|chose|decid(?:e|ed)|pick(?:ed)?|select(?:ed)?|"
        r"rank(?:ed)?|set|resolv(?:e|ed)|mak(?:e|es|ing|ed))"
    )
    warning_choice_phrase = (
        rf"(?:{warning_choice_verb}\b[^.!?;]{{0,65}}\b{private_choice_marker}\b|"
        rf"{private_choice_marker}\b[^.!?;]{{0,45}}\b{warning_choice_verb}\b)"
    )
    conditional_private_choice_warning = (
        r"\b(?:"
        rf"(?:(?:if\s+(?:i|we)\s+(?:were\s+to\s+)?|"
        rf"should\s+(?:i|we)\s+|were\s+(?:i|we)\s+to\s+)"
        rf"{warning_choice_phrase})|"
        rf"(?:if\s+(?:choosing|deciding|picking|selecting|ranking)\b"
        rf"[^.!?;]{{0,55}}\b{private_choice_marker}\b)|"
        r"(?:(?:a|the)\s+(?:unilateral|private|solo)\s+"
        r"(?:decision|choice|call|ranking)))"
        r"(?=[^.!?;]{0,220}\b(?:may|might|could|would|can)\b)"
        r"(?=[^.!?;]{0,240}\b(?:avoidable|risk|harm|bias|unfair|opaque|"
        r"hidden|wrong|undermin|misalign|conflict|breach|consequence)\w*\b)"
        r"[^.!?;]*"
    )
    followthrough_lead = (
        r"(?:\b(?:i|we)\b\s+|"
        r"\b(?:and|but|so|although|though|while|however|yet|still|then|"
        r"nevertheless|regardless|despite)\b[\s,:-]*(?:(?:i|we)\s+)?)"
        r"(?:(?:then|still|next|personally|ultimately|immediately|simply|"
        r"just|now|anyway)\s+){0,2}"
        r"(?:(?:will|would|can|could|may|might|shall|should)\s+)?"
        r"(?:(?:not|never)\s+)?"
    )
    followthrough_target = (
        r"(?:it|one|request|task|work|option|priority|preferred|favored|chosen|"
        r"higher[- ]impact|winner|former|latter|"
        r"[ab](?=\s*(?:[,.!?;]|$|and\b|but\b|so\b|while\b|before\b|after\b|"
        r"despite\b|regardless\b|anyway\b)))"
    )
    conditional_followthrough_action = (
        rf"{followthrough_lead}(?:"
        r"(?:choose|chose|decid(?:e|ed)|pick(?:ed)?|select(?:ed)?|rank(?:ed)?|set)"
        rf"\b[^.!?;]{{0,65}}\b{followthrough_target}\b|"
        r"(?:proceed(?:s|ed)?|continu(?:e|es|ed)|start(?:s|ed)?|launch(?:es|ed)?|"
        r"ship(?:s|ped)?|execut(?:e|es|ed)|begin(?:s|began)?|deliver(?:s|ed)?|"
        r"implement(?:s|ed)?|prioritiz(?:e|es|ed)|commit(?:s|ted)?|"
        r"go\s+ahead|move\s+forward|act\s+on|take\s+up|tackle(?:s|d)?|"
        r"work(?:s|ed)?\s+on|carry\s+on|go\s+with|kick\s+off|"
        r"resume(?:s|d)?|focus(?:es|ed)?\s+on|staff(?:s|ed)?|"
        r"schedul(?:e|es|ed)|expedit(?:e|es|ed)|advanc(?:e|es|ed)|"
        r"queue(?:s|d)?)\b[^.!?;]{0,70}"
        rf"\b{followthrough_target}\b|"
        rf"(?:do|does|did)\b\s+(?:it|that|this|(?:the\s+)?{followthrough_target})\b|"
        r"(?:assign|allocat(?:e|es|ed))\w*\b[^.!?;]{0,35}"
        r"\b(?:team|engineers?|resources?|people|staff|it)\b[^.!?;]{0,45}"
        rf"\b{followthrough_target}\b|"
        r"assign\w*\b[^.!?;]{0,25}"
        rf"\b{followthrough_target}\b[^.!?;]{{0,25}}\bto\b[^.!?;]{{0,20}}"
        r"\b(?:team|engineers?|resources?|people|staff|them)\b|"
        r"(?:direct|tell|have|let|order)\w*\b[^.!?;]{0,35}"
        r"\b(?:team|engineers?|resources?|people|staff|them|it)\b[^.!?;]{0,55}"
        rf"\b{followthrough_target}\b|"
        r"(?:instruct|ask|send)\w*\b[^.!?;]{0,30}"
        r"\b(?:team|engineers?|resources?|people|staff|them)\b[^.!?;]{0,35}"
        r"\b(?:start|begin|execute|launch|prioritize|tackle|take\s+up|work\s+on)"
        rf"\w*\b[^.!?;]{{0,35}}\b{followthrough_target}\b|"
        r"(?:put|route)\w*\b[^.!?;]{0,35}"
        r"\b(?:team|engineers?|resources?|people|staff|them|it)\b[^.!?;]{0,55}"
        rf"\b{followthrough_target}\b|"
        r"open\w*\b[^.!?;]{0,25}\b(?:ticket|work\s+item|task)\b"
        rf"[^.!?;]{{0,35}}\b{followthrough_target}\b|"
        rf"give\w*\b[^.!?;]{{0,25}}\b{followthrough_target}\b"
        r"[^.!?;]{0,25}\b(?:the\s+)?go[- ]ahead\b|"
        r"(?:authoriz(?:e|es|ed)|approve(?:s|d)?|green[- ]?light(?:s|ed)?)\b"
        rf"[^.!?;]{{0,45}}\b{followthrough_target}\b|"
        r"(?:make|made|mark(?:s|ed)?)\b[^.!?;]{0,35}"
        r"\b(?:request|task|work|option)\b[^.!?;]{0,30}"
        r"\b(?:the\s+)?(?:top\s+|first\s+|chosen\s+)?priority\b|"
        r"(?:put|bump)\w*\b[^.!?;]{0,25}"
        rf"\b{followthrough_target}\b[^.!?;]{{0,25}}"
        r"\b(?:first|to\s+the\s+top|ahead)\b)"
    )

    def has_affirmative_conditional_followthrough(scan: str) -> bool:
        for action_match in re.finditer(conditional_followthrough_action, scan):
            action_text = action_match.group()
            if re.search(r"\b(?:not|never|neither|no|against)\b", action_text):
                continue
            if re.search(
                r"\b(?:choose|decid(?:e|ed))\s+to\s+"
                r"(?:wait|defer|pause|ask|escalate)\b",
                action_text,
            ):
                continue
            prefix = scan[max(0, action_match.start() - 80) : action_match.start()]
            suffix = scan[action_match.end() : action_match.end() + 90]
            if re.search(r"\b(?:before|only\s+after)\s*$", prefix) or re.search(
                r"\b(?:after|once|when|following|until)\b.{0,45}"
                r"\b(?:agree|agreement|align|alignment|decid|decision|resolv|"
                r"resolution|rul|ruling|owner|sponsor)\w*\b"
                r".{0,30}[\s,]*$",
                prefix,
            ) or re.search(
                r"\b(?:wait|await)\w*\b.{0,45}"
                r"\b(?:agree|agreement|align|decision|resolution|owner|sponsor)"
                r"\w*\b.{0,15}$|"
                r"\b(?:owner|sponsor|directors?|they)\b.{0,30}"
                r"\b(?:agree|align|decid|resolv|rul)\w*\b[\s,]*$",
                prefix,
            ):
                continue
            if re.search(
                r"^\s*(?:[ab]\s+)?(?:only\s+)?"
                r"(?:after|once|when|following|until)\b.{0,60}"
                r"\b(?:agree|agreement|align|alignment|decid|decision|resolv|"
                r"resolution|rul|ruling|owner|sponsor)\w*\b",
                suffix,
            ):
                continue
            return True
        return False

    unsafe_conditional_followthrough = False

    def strip_safe_conditional_warning(match: re.Match[str]) -> str:
        nonlocal unsafe_conditional_followthrough
        warning = match.group()
        modal = re.search(r"\b(?:may|might|could|would|can)\b", warning)
        consequent_start = 0
        if modal:
            separator = warning.rfind(",", 0, modal.start())
            if separator >= 0:
                consequent_start = separator + 1
            else:
                actors = list(re.finditer(r"\b(?:i|we)\b", warning[: modal.end()]))
                consequent_start = actors[-1].start() if actors else modal.start()
        following = unsafe_scan[match.end() :]
        following_sentences = re.match(
            r"(?:[.!?;]\s*[^.!?;]{0,220}){0,2}", following
        )
        preceding_parts = re.split(
            r"(?<=[.!?;])\s+", unsafe_scan[: match.start()]
        )
        preceding_context = " ".join(preceding_parts[-2:])
        followthrough_scan = preceding_context + " " + warning[consequent_start:] + (
            following_sentences.group() if following_sentences else ""
        )
        followthrough_scan = re.sub(
            r"^(\s*)(?=(?:may|might|could|would|can)\b)",
            r"\1I ",
            followthrough_scan,
            count=1,
        )
        if has_affirmative_conditional_followthrough(followthrough_scan):
            unsafe_conditional_followthrough = True
            return warning
        return " "

    unsafe_scan = re.sub(
        conditional_private_choice_warning,
        strip_safe_conditional_warning,
        unsafe_scan,
    )
    explanatory_private_choice_rejection = (
        r"\b(?:because|since)\s+"
        r"(?:taking|choosing|picking|selecting|starting|prioritizing)\b"
        rf"[^.!?;]{{0,55}}\b{private_choice_marker}\b"
        r"[^.!?;]{0,35}\b(?:hide|hides|create|creates|cause|causes|risk|risks|"
        r"harm|harms|undermine|undermines)\b[^.!?;]{0,50}"
        r"\b(?:conflict|risk|harm|trust|problem|misalignment)\b"
    )

    def strip_explanatory_private_choice_rejection(match: re.Match[str]) -> str:
        nonlocal unsafe_conditional_followthrough
        following = unsafe_scan[match.end() :]
        same_sentence_tail = re.match(r"[^.!?;]{0,260}", following)
        if same_sentence_tail and has_affirmative_conditional_followthrough(
            same_sentence_tail.group()
        ):
            unsafe_conditional_followthrough = True
            return match.group()
        return " "

    unsafe_scan = re.sub(
        explanatory_private_choice_rejection,
        strip_explanatory_private_choice_rejection,
        unsafe_scan,
    )
    first_person_priority_decision = bool(
        re.search(
            r"\bi\s+(?:then\s+|ultimately\s+|personally\s+)?"
            r"(?:decide|choose|pick|select|set|rank|resolve)\w*\b.{0,45}"
            r"\b(?:which\s+(?:request|one)|priority|order|wins?|"
            r"conflict|(?:my\s+)?personal\s+preference)\b",
            unsafe_scan,
        )
    )
    private_decision = (
        unsafe_conditional_followthrough
        or first_person_priority_decision
        or bool(re.search(
            r"\b(?:choose|decide|select|pick|resolve|make\s+"
            r"(?:(?:a|the)\s+)?(?:(?:priority|final)\s+){0,2}"
            r"(?:decision|call|ranking|order))"
            r"\w*\b.{0,25}"
            r"\b(?:privately|alone|unilaterally|myself|personally|independently|"
            r"behind\s+closed\s+doors|"
            r"on\s+my\s+own|by\s+myself|based\s+on\s+(?:my\s+)?personal\s+preference)\b|"
            r"\b(?:privately|alone|unilaterally|myself|personally|independently|"
            r"behind\s+closed\s+doors|"
            r"on\s+my\s+own|by\s+myself|"
            r"based\s+on\s+(?:my\s+)?personal\s+preference)\b"
            r".{0,30}"
            r"\b(?:choose|decide|select|pick|resolve|set|rank|make\s+"
            r"(?:(?:a|the)\s+)?(?:(?:priority|final)\s+){0,2}"
            r"(?:decision|call|ranking|order))\w*\b",
            unsafe_scan,
        ))
    )
    concealed_tradeoff = bool(
        re.search(
            r"\b(?:conceal|hide|withhold|keep)\w*\b.{0,55}"
            r"\b(?:tradeoff|comparison|priority|request)\w*\b.{0,65}"
            r"\b(?:other|each\s+other|director)\b|"
            r"\b(?:separate|private)\s+versions?\b.{0,55}\b(?:directors?|tradeoff)\b",
            unsafe_scan,
        )
    ) and not bool(
        re.search(
            r"\b(?:do\s+not|don't|will\s+not|won't|would\s+not|wouldn't|"
            r"never|avoid(?:s|ing)?|refuse\s+to|without)\b.{0,35}"
            r"\b(?:conceal|hide|withhold|keep)\w*\b.{0,55}"
            r"\b(?:tradeoff|comparison|priority|request|selection|choice)\w*\b"
            r".{0,65}\b(?:other|either|each\s+other|director)\b",
            unsafe_scan,
        )
    )
    decision_windows = [
        window.strip()
        for window in re.split(r"(?<=[.!?;])\s+", unsafe_scan)
        if window.strip()
    ]
    decision_windows.extend(
        f"{decision_windows[index]} {decision_windows[index + 1]}"
        for index in range(len(decision_windows) - 1)
    )
    # Escalation must pause ordinary contested work.  A comparison may suggest a
    # likely winner, but it does not authorize the candidate to act before the
    # directors agree or their accountable owner rules.  Deliberately keep the
    # incident exception narrow: only the containment action itself is allowed;
    # mentioning containment never authorizes work on a contested request.
    ordinary_conflicting_work_while_waiting = False
    for window in decision_windows:
        # The consequence question "what is the risk if it waits?" describes a
        # request's delay cost; it is not the candidate waiting for a decision.
        # Remove only that bounded phrase so real third-person or first-person
        # unresolved-work bypasses remain visible to the original detector.
        awaiting_scan = re.sub(
            r"\b(?:risk|impact|consequence|cost)\b[^.!?;]{0,35}"
            r"\bif\s+(?:it|the\s+request)\s+waits?\b",
            " ",
            window,
        )
        awaiting_resolution = bool(
            re.search(
                r"\b(?:while|pending|before)\b.{0,55}"
                r"\b(?:wait|await|no\s+(?:decision|ruling|resolution)\s+yet)\w*\b"
                r".{0,50}\b(?:decision|ruling|resolution|owner|sponsor|leader|vp)\b|"
                r"\b(?:wait|await)\w*\b.{0,55}"
                r"\b(?:decision|ruling|resolution|owner|sponsor|leader|vp)\b|"
                r"\bpending\b.{0,55}\b(?:decision|ruling|resolution)\b|"
                r"\bbefore\b.{0,55}\b(?:owner|sponsor|leader|vp)\b.{0,35}"
                r"\b(?:decid|resolv|rul)\w*\b|"
                r"\bwhile\b.{0,55}\b(?:owner|sponsor|leader|vp)\b.{0,35}"
                r"\b(?:consider|review|decid|resolv|rul)\w*\b",
                awaiting_scan,
            )
        )
        contested_work_verb = (
            r"(?:keep|continue|start|begin|execute|proceed|prioritize|work|"
            r"implement|ship|deploy|release|carry\s+on|tackle)\w*"
        )
        contested_work_target = (
            r"(?:item|request|priority|work|one|option|task|change|deliverable|"
            r"feature|project|fix)"
        )
        # Remove only an explicitly prohibited action, rather than exempting the
        # whole sentence. This keeps contrastive phrases such as "I would not
        # normally start A, but I deploy B while waiting" visible to the gate.
        contested_action_scan = re.sub(
            rf"\b(?:do\s+not|don't|never|will\s+not|won't)\b.{{0,55}}"
            rf"\b{contested_work_verb}\b.{{0,80}}\b{contested_work_target}\b",
            " ",
            window,
        )
        contested_action_scan = re.sub(
            rf"\b(?:do\s+not|don't|never|will\s+not|won't)\b.{{0,55}}"
            rf"\b{contested_work_verb}\b.{{0,25}}"
            r"\b(?:immediately|now|right\s+away)\b",
            " ",
            contested_action_scan,
        )
        contested_action_scan = re.sub(
            rf"\b(?:wait|await)\w*\b.{{0,65}}\bbefore\b.{{0,45}}"
            rf"\b{contested_work_verb}\b.{{0,80}}\b{contested_work_target}\b",
            " ",
            contested_action_scan,
        )
        selects_or_works_contested_item = bool(
            re.search(
                rf"\b{contested_work_verb}\b.{{0,80}}\b{contested_work_target}\b|"
                r"\bkeep\s+working\b|\bwork(?:ing)?\s+on\b.{0,45}\b"
                rf"{contested_work_target}\b|\b{contested_work_verb}\b.{{0,25}}"
                r"\b(?:immediately|now|right\s+away)\b",
                contested_action_scan,
            )
        )
        if (
            awaiting_resolution
            and selects_or_works_contested_item
        ):
            ordinary_conflicting_work_while_waiting = True
            break
    opaque_private_selection = False
    for window in decision_windows:
        rejects_concealment = bool(
            re.search(
                r"\b(?:do\s+not|don't|will\s+not|won't|would\s+not|wouldn't|"
                r"never|avoid(?:s|ing)?|reject\w*|refuse\s+to|without)\b"
                r".{0,45}\b(?:keep\w*|leave\w*|hold\w*|mak(?:e|es|ing|ed)|"
                r"use\w*|bas(?:e|es|ing|ed)|conceal\w*|hide\w*|withhold\w*)\b"
                r".{0,70}\b(?:opaque|off[- ]channel|undisclosed|hidden|secret)\b",
                window,
            )
        )
        conceals_selection = bool(
            re.search(
                r"\b(?:preference|final\s+(?:selection|choice)|chosen\s+priority|"
                r"priority\s+decision)\b.{0,55}"
                r"\b(?:opaque|off[- ]channel|undisclosed|hidden|secret)\b|"
                r"\b(?:opaque|off[- ]channel|undisclosed|hidden|secret)\b.{0,55}"
                r"\b(?:preference|final\s+(?:selection|choice)|chosen\s+priority|"
                r"priority\s+decision)\b",
                window,
            )
        )
        contrastive_concealment = bool(
            re.search(
                r"\b(?:reject|avoid|oppose|forbid|do\s+not|don't|never)\w*\b"
                r".{0,80}\b(?:opaque|off[- ]channel|undisclosed|hidden|secret)\b"
                r".{0,55}\b(?:but|however|yet|nevertheless)\b.{0,100}"
                r"\b(?:preference|final\s+(?:selection|choice)|chosen\s+priority|"
                r"priority\s+decision)\b.{0,55}"
                r"\b(?:opaque|off[- ]channel|undisclosed|hidden|secret)\b",
                window,
            )
        )
        if conceals_selection and (
            not rejects_concealment or contrastive_concealment
        ):
            opaque_private_selection = True
            break

    premature_action = False
    for window in decision_windows:
        preserves_wait_boundary = bool(
            re.search(
                r"\b(?:do\s+not|don't|never|will\s+not|won't)\s+"
                r"(?:proceed|start|begin|execute|prioritize)\w*\b.{0,55}"
                r"\b(?:before|without)\b.{0,40}"
                r"\b(?:agree|agreement|align|alignment|decision|resolution)\w*\b|"
                r"\bwait\w*\b.{0,45}"
                r"\b(?:agree|agreement|align|alignment|decision|resolution)\w*\b"
                r".{0,55}\bbefore\b.{0,30}"
                r"\b(?:proceed|start|begin|execute|prioritize)\w*\b",
                window,
            )
        )
        bypasses_wait_boundary = bool(
            re.search(
                r"\b(?:(?:there\s+is\s+)?no\s+need\s+to|"
                r"(?:do\s+not|don't)\s+need\s+to|need\s+not|"
                r"will\s+not|won't)\s+wait\w*\b.{0,55}"
                r"\b(?:agree|agreement|align|alignment|decision|resolution)\w*\b|"
                r"\b(?:proceed|start|begin|execute|prioritize)\w*\b.{0,55}"
                r"\b(?:before|without\s+wait\w*\s+for)\b.{0,45}"
                r"\b(?:agree|agreement|align|alignment|decision|resolution)\w*\b",
                window,
            )
        )
        if (
            bypasses_wait_boundary
            and not preserves_wait_boundary
        ):
            premature_action = True
            break

    consensus_disclaimed = bool(
        re.search(
            r"\b(?:consensus|agreement|alignment|shared\s+decision)\b.{0,30}"
            r"\b(?:is|are)\s+(?:not\s+required|unnecessary|optional|"
            r"non[- ]?binding)\b|"
            r"\b(?:do\s+not|don't)\s+(?:require|need)\w*\b.{0,30}"
            r"\b(?:consensus|agreement|alignment|shared\s+decision)\b",
            lower,
        )
    )
    acts_while_discussion_continues = bool(
        re.search(
            r"\b(?:proceed|start|begin|execute|follow|prioritize|carry\s+on)\w*\b"
            r".{0,100}\bwhile\b.{0,50}"
            r"\b(?:discuss|deliberat|align|decid|debate|review)\w*\b|"
            r"\bwhile\b.{0,50}"
            r"\b(?:discuss|deliberat|align|decid|debate|review)\w*\b.{0,100}"
            r"\b(?:proceed|start|begin|execute|follow|prioritize|carry\s+on)\w*\b",
            lower,
        )
    )
    consensus_bypassed_during_action = (
        consensus_disclaimed
        and acts_while_discussion_continues
    )
    resolution_bypassed_before_favored_action = bool(
        re.search(
            r"\b(?:(?:do\s+not|don't|does\s+not|doesn't)\s+need|"
            r"(?:there\s+is\s+)?no\s+need\s+for|need\s+no)\b.{0,25}"
            r"\b(?:resolution|ruling|decision)\b.{0,30}\bbefore\b.{0,30}"
            r"\b(?:execut|proceed|start|begin|follow|prioritize)\w*\b.{0,55}"
            r"\b(?:request|priority|choice|selection)\b.{0,35}"
            r"\b(?:i\s+)?(?:favor|prefer|chose|choose|select)\w*\b|"
            r"\b(?:execut|proceed|start|begin|follow|prioritize)\w*\b.{0,55}"
            r"\b(?:my\s+)?(?:favored|preferred|chosen|original|preselected)\b"
            r".{0,30}\b(?:request|priority|choice|selection)\b.{0,65}"
            r"\b(?:owner|sponsor|leader|manager|vp)\b.{0,35}"
            r"\b(?:respond|decid|resolv|rule)\w*\b.{0,20}\bafterward\b",
            lower,
        )
    )

    immediate_personal_action_after_escalation = bool(
        re.search(
            r"\bescalat\w*\b.{0,100}\b(?:owner|sponsor|leader|leadership|manager|vp)\b"
            r".{0,100}\b(?:immediately|right\s+away|meanwhile)\b.{0,55}"
            r"\b(?:proceed|start|begin|follow|execute|prioritize)\w*\b"
            r".{0,65}\b(?:my|own|preferred|preference)\b",
            lower,
        )
    )
    unresolved_personal_action_after_escalation = bool(
        re.search(
            r"\bescalat\w*\b.{0,100}\b(?:owner|sponsor|leader|leadership|manager|vp)\b"
            r".{0,160}\b(?:proceed|start|begin|follow|execute|prioritize)\w*\b"
            r".{0,65}\b(?:my|own|preferred|preference)\b.{0,100}"
            r"\bwithout\s+wait\w*\b.{0,55}"
            r"\b(?:owner|sponsor|leader|manager|vp)?'?s?\s*"
            r"(?:decision|resolution|response|ruling)\b",
            lower,
        )
    )
    cosmetic_escalation = (
        immediate_personal_action_after_escalation
        or unresolved_personal_action_after_escalation
    )
    owner_only_notified = bool(
        re.search(
            r"\b(?:email|copy|cc|notify|message|send|page|forward)\w*\b.{0,55}"
            r"\b(?:accountable\s+)?(?:owner|sponsor|leader|manager|vp)\b",
            lower,
        )
    )
    chosen_action_precedes_owner_ruling = bool(
        re.search(
            r"\b(?:immediately\s+)?(?:execut|proceed|start|begin|follow|prioritize|"
            r"launch|act(?:\s+on)?)\w*\b.{0,45}"
            r"(?:\b(?:my\s+)?(?:chosen|original|preferred|preselected|favored)\b"
            r".{0,25}\b(?:priority|request|choice|selection|option)\b|"
            r"\bmy\s+(?:priority|request|choice|selection|option)\b|"
            r"\b(?:priority|request|choice|selection|option)\b.{0,25}"
            r"\b(?:i\s+)?(?:favor|prefer|chose|choose|select)\w*\b).{0,90}"
            r"(?:\b(?:without\s+(?:await|wait)\w*(?:\s+for)?|before|pending)\b"
            r".{0,35}\b(?:(?:that|the|any)\s+)?"
            r"(?:owner|sponsor|leader|manager|vp)?'?s?\s*"
            r"(?:rul\w*|decision|resolution|response|decid\w*|resolv\w*)\b|"
            r"\bwhile\b.{0,30}\b(?:owner|sponsor|leader|manager|vp)\b.{0,35}"
            r"\b(?:consider|review|decid|resolv|rule)\w*\b)",
            lower,
        )
    ) or bool(
        re.search(
            r"\bpending\b.{0,30}\b(?:that\s+)?"
            r"(?:owner|sponsor|leader|manager|vp)?'?s?\s*"
            r"(?:response|ruling|decision|resolution)\b.{0,45}"
            r"\b(?:execut|proceed|start|begin|follow|prioritize|launch|"
            r"act(?:\s+on)?)\w*\b.{0,45}"
            r"(?:\bmy\s+(?:priority|request|choice|selection|option)\b|"
            r"\b(?:priority|request|choice|selection|option)\b.{0,25}"
            r"\b(?:i\s+)?(?:favor|prefer|chose|choose|select)\w*\b)",
            lower,
        )
    )
    cosmetic_notification_before_personal_action = (
        owner_only_notified and chosen_action_precedes_owner_ruling
    )

    advisory_director_input = bool(
        re.search(
            r"\b(?:directors?'?|their)\s+(?:input|feedback|views?)\b.{0,35}"
            r"\b(?:advisory|non[- ]?binding|optional|"
            r"(?:as|for)\s+context\s+only)\b|"
            r"\b(?:input|feedback|views?)\b.{0,30}"
            r"\b(?:advisory\s+only|non[- ]?binding|optional)\b",
            lower,
        )
    ) and not bool(
        re.search(
            r"\b(?:do\s+not|don't|never)\b.{0,35}"
            r"\b(?:treat|consider|regard)\w*\b.{0,35}"
            r"\b(?:input|feedback|views?)\b.{0,25}\b(?:advisory|optional)\b|"
            r"\b(?:input|feedback|views?)\b.{0,25}"
            r"\b(?:is|are)\s+not\s+(?:merely\s+|only\s+)?advisory\b",
            lower,
        )
    )
    retains_original_priority = bool(
        re.search(
            r"\b(?:keep|retain|preserve|follow|use|continue\s+with)\w*\b.{0,40}"
            r"\b(?:(?:my|the)\s+)?(?:original|initial|preferred|preselected)\b"
            r".{0,25}\b(?:priority|choice|request|selection|order|sequence)\b|"
            r"\b(?:keep|retain|preserve|follow|use|continue\s+with)\w*\b.{0,40}"
            r"\b(?:priority|choice|request|selection|order|sequence)\b.{0,35}"
            r"\b(?:i\s+)?(?:originally\s+)?(?:select|choose|chose|set|pick)\w*\b",
            lower,
        )
    )
    advisory_only_override = advisory_director_input and retains_original_priority

    disagreement_is_only_informational = bool(
        re.search(
            r"\b(?:treat|regard|consider|view)\w*\b.{0,30}"
            r"\b(?:their\s+)?disagreement\b.{0,30}"
            r"\b(?:merely\s+|only\s+)?(?:informational|advisory|non[- ]?binding)\b|"
            r"\b(?:their\s+)?disagreement\b.{0,30}\b(?:is|as)\b.{0,15}"
            r"\b(?:merely\s+|only\s+)?(?:informational|advisory|non[- ]?binding)\b",
            lower,
        )
    ) and not bool(
        re.search(
            r"\b(?:do\s+not|don't|never)\b.{0,30}"
            r"\b(?:treat|regard|consider|view)\w*\b.{0,30}"
            r"\b(?:their\s+)?disagreement\b.{0,30}"
            r"\b(?:informational|advisory|non[- ]?binding)\b",
            lower,
        )
    )
    carries_on_with_preselected_priority = bool(
        re.search(
            r"\b(?:carry\s+on|continue|proceed|follow|use|keep)\w*\b.{0,55}"
            r"\b(?:priority|request|choice|selection)\b.{0,45}"
            r"\b(?:i\s+)?(?:intend|preset|preselect|original|initial|preferred)\w*\b"
            r"(?:.{0,25}\bfrom\s+the\s+start\b)?",
            lower,
        )
    )
    disagreement_dismissed_for_preselected_priority = (
        disagreement_is_only_informational and carries_on_with_preselected_priority
    )
    joint_result_differs_from_preselected_choice = bool(
        re.search(
            r"\b(?:continue(?:\s+with)?|carry\s+on|proceed|follow)\w*\b.{0,55}"
            r"\b(?:request|priority|choice|selection|option)\b.{0,40}"
            r"\b(?:i\s+)?(?:preselect|select|choose|chose|set|pick)\w*\b.{0,30}"
            r"\b(?:from\s+the\s+start|beforehand|in\s+advance)\b.{0,55}"
            r"\b(?:even\s+if|although|despite)\b.{0,35}"
            r"\b(?:their\s+)?(?:joint|shared|common)\b.{0,20}"
            r"\b(?:result|outcome|decision|agreement|order)\b.{0,20}"
            r"\b(?:differ|change|conflict|disagree|oppose)\w*\b",
            lower,
        )
    )
    prior_selection_immune_to_director_views = bool(
        re.search(
            r"\b(?:views?|feedback|input)\b.{0,35}"
            r"\b(?:do\s+not|don't|cannot|can't|will\s+not|won't)\b.{0,15}"
            r"\b(?:change|alter|revise|affect|influence)\w*\b.{0,45}"
            r"\b(?:priority|selection|choice|request)\b.{0,45}"
            r"\b(?:i\s+)?(?:select|choose|chose|set|pick|determin)\w*\b.{0,25}"
            r"\b(?:beforehand|already|earlier|in\s+advance|from\s+the\s+start)\b|"
            r"\b(?:priority|selection|choice|request)\b.{0,40}"
            r"\b(?:select|choose|chose|set|pick|determin)\w*\b.{0,25}"
            r"\b(?:beforehand|already|earlier|in\s+advance)\b.{0,55}"
            r"\b(?:views?|feedback|input)\b.{0,35}"
            r"\b(?:cannot|can't|will\s+not|won't)\b.{0,15}"
            r"\b(?:change|alter|revise|affect|influence)\w*\b",
            lower,
        )
    )

    withholds_critical_context = bool(
        re.search(
            r"\b(?:withhold|hide|conceal|omit|keep|leave\s+out|suppress)\w*\b.{0,45}"
            r"\b(?:(?:critical|material)\s+)?"
            r"(?:(?:operational|customer[- ]impact)\s+)?"
            r"(?:dependency|constraint|risk|fact|information|context)\w*\b"
            r".{0,55}\bfrom\s+(?:(?:one|either|a)\s+director|"
            r"one\s+of\s+the\s+directors|director\s+[a-z])\b",
            lower,
        )
    ) and not bool(
        re.search(
            r"\b(?:do\s+not|don't|never|avoid(?:s|ing)?|without)\b.{0,35}"
            r"\b(?:withhold|hide|conceal|omit|keep|leave\s+out|suppress)\w*\b.{0,45}"
            r"\b(?:dependency|constraint|risk|fact|information|context)\w*\b",
            lower,
        )
    )
    overrides_joint_discussion = bool(
        re.search(
            r"\b(?:proceed|start|begin|follow|execute|prioritize|"
            r"continue(?:\s+with)?)\w*\b.{0,65}"
            r"\b(?:regardless\s+of|despite)\b.{0,45}"
            r"\b(?:joint|shared|common)\b.{0,20}"
            r"\b(?:discussion|deliberation|review|agreement|alignment|decision|"
            r"result|outcome|input)\b|"
            r"\b(?:regardless\s+of|despite)\b.{0,45}"
            r"\b(?:joint|shared|common)\b.{0,20}"
            r"\b(?:discussion|deliberation|review|agreement|alignment|decision|"
            r"result|outcome|input)\b.{0,65}"
            r"\b(?:proceed|start|begin|follow|execute|prioritize|"
            r"continue(?:\s+with)?)\w*\b",
            lower,
        )
    )
    favored_action = (
        r"(?:execut|start|begin|launch|proceed|follow|continue|preserve)\w*"
    )
    favored_choice = (
        r"(?:(?:my|the)\s+(?:chosen|selected|picked|preferred|favored|preselected|original|initial)\s+"
        r"(?:request|priority|choice|option|sequence|order|work)|"
        r"(?:request|priority|choice|option|sequence|order|work)\s+"
        r"(?:i\s+)?(?:favor|prefer|preselect|select|pick)\w*)"
    )
    secret_self_ranking = (
        bool(
        re.search(
            r"\b(?:secret\w*|confidential(?:ity|ly)?|behind\s+closed\s+doors)\b"
            r".{0,65}\b(?:rank\w*|mak(?:e|es|ing|ed)\s+(?:a\s+)?"
            r"(?:confidential\s+)?final\s+(?:choice|ranking|selection))\b.{0,55}"
            r"\b(?:myself|by\s+myself|on\s+my\s+own)\b",
            lower,
        )
        )
        or bool(
            re.search(
                r"\b(?:rank|set|mak(?:e|es|ing|ed))\w*\b.{0,40}"
                r"\b(?:final\s+)?(?:choice|ranking|selection|order)\b.{0,50}"
                r"\b(?:closed[- ]door(?:s)?|behind\s+closed\s+doors|confidential)\b"
                r".{0,45}\b(?:myself|by\s+myself|on\s+my\s+own)\b",
                lower,
            )
        )
    ) and not bool(
        re.search(
            r"\b(?:do\s+not|don't|will\s+not|won't|would\s+not|wouldn't|"
            r"never|avoid(?:s|ing)?|refuse\s+to|without)\b.{0,45}"
            r"\b(?:secret\w*|confidential(?:ity|ly)?|behind\s+closed\s+doors)\b"
            r".{0,65}\b(?:rank\w*|mak(?:e|es|ing|ed)\s+(?:a\s+)?"
            r"(?:confidential\s+)?final\s+(?:choice|ranking|selection))\b",
            lower,
        )
    )
    cosmetic_owner_contact = bool(
        re.search(
            r"\b(?:escalat|page|notify|copy|cc|forward|email|message)\w*\b.{0,60}"
            r"\b(?:owner|sponsor|leader|leadership|manager|vp)\b",
            lower,
        )
    )
    action_while_owner_ruling_pending = bool(
        re.search(
            rf"\b(?:with\s+)?no\s+(?:ruling|decision|resolution)\s+yet\b.{{0,55}}"
            rf"\b{favored_action}\b.{{0,55}}\b{favored_choice}\b|"
            rf"\b{favored_action}\b.{{0,55}}\b{favored_choice}\b.{{0,90}}"
            r"(?:\bwhile\b.{0,45}\b(?:owner|sponsor|leader|manager|vp)\b.{0,35}"
            r"\b(?:evaluat|consider|review|decid|resolv)\w*\b|"
            r"\b(?:ruling|decision|resolution)\b.{0,30}\b(?:afterward|later)\b)",
            lower,
        )
    )
    cosmetic_owner_action = cosmetic_owner_contact and action_while_owner_ruling_pending
    action_before_shared_resolution = bool(
        re.search(
            rf"\b(?:consensus|agreement|alignment)\b.{{0,25}}\boptional\b.{{0,55}}"
            rf"\b{favored_action}\b.{{0,55}}\b{favored_choice}\b.{{0,65}}"
            r"\bwhile\b.{0,45}\b(?:debat|discuss|deliberat|review)\w*\b|"
            rf"\b{favored_action}\b.{{0,55}}\b{favored_choice}\b.{{0,20}}\bfirst\b"
            r".{0,75}\b(?:shared|joint)\b.{0,25}\b(?:decision|discussion)\b"
            r".{0,35}\b(?:continue|happen|finish)\w*\b.{0,20}\b(?:afterward|later)\b|"
            rf"\bwithout\s+agreement\b.{{0,55}}\b{favored_action}\b.{{0,55}}"
            rf"\b{favored_choice}\b.{{0,75}}\b(?:resolv|decid)\w*\b.{{0,30}}\blater\b",
            lower,
        )
    )
    optional_feedback_preserves_preset = bool(
        re.search(
            r"\b(?:directors?'?\s+)?(?:feedback|input|views?)\b.{0,35}"
            r"\b(?:optional|non[- ]?binding|context\s+only)\b",
            lower,
        )
    ) and bool(
        re.search(
            r"\b(?:preserve|retain|keep|follow)\w*\b.{0,55}"
            r"(?:\b(?:preferred|preselected|preset|original|initial)\b.{0,25}"
            r"\b(?:sequence|order|request|priority|choice)\b|"
            r"\b(?:sequence|order|request|priority|choice)\b.{0,35}"
            r"\b(?:i\s+)?(?:preselect|select|pick|set)\w*\b)",
            lower,
        )
    )
    prior_choice_overrides_joint_result = False
    for window in decision_windows:
        joint_result_conflicts = bool(
            re.search(
                r"\b(?:even\s+if|even\s+when|despite)\b.{0,55}"
                r"\b(?:joint|shared|common)\b.{0,25}"
                r"\b(?:result|outcome|decision|agreement)\b|"
                r"\b(?:joint|shared|common)\b.{0,25}"
                r"\b(?:result|outcome|decision|agreement)\b.{0,30}"
                r"\b(?:differ|change|conflict|oppose|different)\w*\b|"
                r"\bdifferent\w*\b.{0,25}\b(?:joint|shared|common)\b.{0,20}"
                r"\b(?:result|outcome|decision|agreement)\b",
                window,
            )
        )
        action_preserves_prior_choice = bool(
            re.search(
                r"\b(?:continue|proceed|follow|preserve|keep|retain)\w*\b.{0,55}"
                r"(?:\b(?:initial|favored|preferred|preselected|preset|original)\b"
                r".{0,25}\b(?:sequence|order|request|priority|choice)\b|"
                r"\b(?:sequence|order|request|priority|choice)\b.{0,35}"
                r"\b(?:i\s+)?(?:preselect|select|pick|set)\w*\b.{0,25}"
                r"\b(?:earlier|beforehand|in\s+advance|from\s+the\s+start)\b)",
                window,
            )
        )
        prior_choice_declared_immutable = bool(
            re.search(
                r"\b(?:joint|shared|common)\b.{0,25}"
                r"\b(?:result|outcome|decision|agreement)\b.{0,25}"
                r"\b(?:cannot|can't|does\s+not|doesn't)\b.{0,20}\bchange\w*\b"
                r".{0,35}\b(?:sequence|order|request|priority|choice)\b.{0,35}"
                r"\b(?:i\s+)?(?:select|pick|set)\w*\b.{0,25}"
                r"\b(?:earlier|beforehand|in\s+advance)\b",
                window,
            )
        )
        if joint_result_conflicts and (
            action_preserves_prior_choice or prior_choice_declared_immutable
        ):
            prior_choice_overrides_joint_result = True
            break
    explicit_action_before_resolution = bool(
        re.search(
            rf"\b(?:before\b.{{0,30}}\b(?:owner|sponsor)\b.{{0,20}}"
            rf"\b(?:rule|respond|decid|resolv)\w*|while\s+await\w*.{{0,25}}"
            rf"\b(?:decision|ruling|resolution)\b|pending\b.{{0,20}}"
            rf"\b(?:decision|ruling|resolution)\b|before\s+(?:any\s+)?"
            rf"(?:decision|ruling|resolution)).{{0,60}}"
            rf"\b{favored_action}\b.{{0,55}}\b{favored_choice}\b|"
            rf"\b{favored_action}\b.{{0,55}}\b{favored_choice}\b.{{0,70}}"
            r"\b(?:before\b.{0,25}\b(?:owner|sponsor)\b.{0,20}"
            r"\b(?:rule|respond|decid|resolv)\w*|"
            r"while\b.{0,35}\bconsensus\b.{0,20}\bform\w*|"
            r"as\b.{0,35}\b(?:discuss|deliberat|debat)\w*|"
            r"pending\b.{0,25}\b(?:decision|ruling|resolution)\b)",
            lower,
        )
    )
    advisory_preselection = bool(
        re.search(
            r"\b(?:directors?'?\s+)?(?:feedback|input|views?)\b.{0,40}"
            r"\b(?:context\s+(?:only|rather\s+than\s+binding)|optional|non[- ]?binding)\b"
            r".{0,70}\b(?:preserve|retain|keep|follow)\w*\b.{0,45}"
            r"\b(?:priority|sequence|order|request|choice)\b.{0,35}"
            r"\b(?:i\s+)?(?:preset|preselect|select|set|pick)\w*\b",
            lower,
        )
    )
    settled_result_overridden = bool(
        re.search(
            r"\b(?:joint|shared|common)\b.{0,25}"
            r"\b(?:result|outcome|decision|order)\b.{0,70}"
            r"\b(?:although|but|after)\b.{0,65}"
            r"\b(?:continue|proceed|follow|preserve|retain|keep)\w*\b.{0,55}"
            r"(?:\b(?:earlier|preferred|favored|initial|preselected)\b.{0,25}"
            r"\b(?:sequence|order|option|request|priority|choice)\b|"
            r"\b(?:sequence|order|option|request|priority|choice)\b.{0,35}"
            r"\b(?:i\s+)?(?:chose|select|pick|set)\w*\b.{0,20}\bearlier\b)|"
            r"\b(?:joint|shared|common)\b.{0,25}\b(?:result|outcome|decision)\b"
            r".{0,45}\b(?:but|although)\b.{0,45}"
            r"\b(?:preferred|favored|initial|earlier)\b.{0,25}"
            r"\b(?:sequence|order|option|request|priority|choice)\b.{0,30}"
            r"\b(?:remain|stay|continue)\w*\b.{0,20}\bin\s+force\b|"
            r"\bafter\b.{0,35}\b(?:directors?|they)\b.{0,25}"
            r"\b(?:settle|agree|resolve)\w*\b.{0,25}\b(?:common|shared|joint)\b"
            r".{0,20}\b(?:order|decision|result|outcome)\b.{0,55}"
            r"\b(?:continue|proceed|follow)\w*\b.{0,45}"
            r"\b(?:option|request|priority|choice|sequence|order)\b.{0,35}"
            r"\b(?:i\s+)?(?:chose|select|pick|set)\w*\b.{0,20}\bearlier\b",
            lower,
        )
    )
    negated = (
        ignored_director_input
        or private_decision
        or concealed_tradeoff
        or opaque_private_selection
        or premature_action
        or consensus_bypassed_during_action
        or resolution_bypassed_before_favored_action
        or cosmetic_escalation
        or cosmetic_notification_before_personal_action
        or advisory_only_override
        or disagreement_dismissed_for_preselected_priority
        or joint_result_differs_from_preselected_choice
        or prior_selection_immune_to_director_views
        or (withholds_critical_context and overrides_joint_discussion)
        or secret_self_ranking
        or cosmetic_owner_action
        or action_before_shared_resolution
        or optional_feedback_preserves_preset
        or prior_choice_overrides_joint_result
        or explicit_action_before_resolution
        or advisory_preselection
        or settled_result_overridden
        or ordinary_conflicting_work_while_waiting
        or bool(
            re.search(
                rf"\b(?:do\s+not|don't|never|won't|wouldn't|without|avoid(?:s|ing)?|"
                rf"refuse\s+to)\b"
                rf"[^.!?;]{{0,55}}\b{action}\b[^.!?;]{{0,65}}\b{directors}\b|"
                rf"\b{action}\b[^.!?;]{{0,55}}\b(?:not|never|without)\b"
                rf"[^.!?;]{{0,30}}"
                rf"\b{directors}\b|"
                r"\b(?:make|take|reach)\w*\b.{0,20}\bunilateral\w*\b"
                r".{0,20}\b(?:decision|call)\b|"
                r"\bunilateral\w*\b.{0,20}\b(?:decid|decision|call)\w*\b",
                unsafe_scan,
            )
        )
    )
    issues: List[str] = []
    if not affirmative:
        issues.append("missing_affirmative_director_alignment")
    if negated:
        issues.append("unsafe_negated_or_unilateral_director_alignment")
    if re.search(
        r"\b(?:in\s+practice(?:\s+at)?|in\s+my\s+(?:current|prior|previous|last)\s+"
        r"role|at\s+my\s+(?:current|prior|previous|last)\s+(?:company|job)|"
        r"a\s+time\s+when\s+i|i\s+once)\b",
        lower,
    ):
        issues.append("claimed_past_example_in_hypothetical_q47")
    return issues
