# Delegated Short live length correction

Date: 2026-10-09. Preflight: `bluey-ops`, `pinky-ops`, and integration ops.

## Observed failure

The isolated exact-source `62f9ada0` backend delivered a real Haiku5.5 answer
through Pinky. The fixed synthetic database-index question produced 140 words
in two paragraphs, failing the Short <=120-word presentation check. First
nonempty SSE text arrived at 1124ms; total client time was 2236ms. Accounting
was durably settled and authoritative status confirmed completion. One
synthetic customer cent was debited; the separate provider/customer usage rows
are not two customer charges. No personal prompt or transcript was used.

The server already appended trusted presentation rules after shared planning
and kept the requested 384-token ceiling. This failure demonstrates that those
instructions alone did not reliably enforce the desired short presentation;
it does not prove a missing provider instruction or bypass of the final fence.

## Repair

The closed server-owned Short mode now targets 60–90 words, one paragraph of
three or four short sentences, preserving the absolute 120-word limit. It
explicitly omits optional examples, follow-ups, lists and excess explanation.
The output ceiling is 256 tokens. No second rewrite call, truncation, fabricated
experience, transcript access or hidden source collection is added. Default,
STAR and standalone presentation remain unchanged. Source tests bind these
rules and the token ceiling; full local and real-provider checks are separate.

Word counts are still model instructions, not a mathematically guaranteed
semantic output bound. Do not weaken the live harness or silently trim an
unfinished sentence to report a passing answer. Preserve the failed result
and qualify the rebuilt exact artifact with synthetic prompts.

Final local gate passed: PostgreSQL1/1, focused35/35, all-target932/932 and
strict all-target Clippy. The private PostgreSQL/Cargo root was verified
removed with no owned process remaining. These are source/transaction results;
the rebuilt artifact still needs the real provider acceptance run.
