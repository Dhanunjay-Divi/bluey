# Conversational Assist and private interview memory

Preflight: load `bluey-ops`, `pinky-ops` and `pinky-bluey-integration-ops`.
Updated 2026-10-09; integration feature branches only, not production or Jobs.

## Current conversational change

The trusted text Assist system instruction now requests natural, direct speech,
plain language, varied short sentences and minimal formatting. Default, Short
and STAR remain separate output choices. A second "humanizer" provider call is
not added: it would add latency and another opportunity to alter facts.
Source tests assert that every style retains the managed contract, truthful
personal-experience boundary and unavailable-source boundary. Real answer
quality and first-visible-token latency still require the live preprod test.
No detector-evasion or "indistinguishable from a human" guarantee is made.

## Actual baseline finding and bounded repair

The deployed `65ec499c` baseline delivered an authenticated Haiku4.5 answer,
but Short exceeded the requested 120-word shape and included literal Markdown
emphasis in Pinky's deliberately plain-text renderer. The provider token cap
was preserved; a token limit is not a word limit. Shared AnswerPlan instructions
are appended after the original selected style. This confirms a presentation
precedence/enforcement gap, not proof that any one planner branch caused this
particular answer.

The current source adds server-owned final presentation rules after the shared
planner, preserves its factual/safety context, and leaves standalone prompts
unchanged. Short explicitly requests <=120 words and no Markdown emphasis.
A full composed-prompt regression uses the representative database-index
question; HTTP tests retain pre-dispatch cancellation. Final local validation
passed 932 all-target tests, 35 focused tests, the real PostgreSQL transaction
test and strict all-target Clippy. These are prompt invariants, not deterministic
word-count enforcement. A fresh visible completion after exact-artifact cutover
is still required; see the [review](REVIEW-PHASE-626-VOICE-MODELS-PG.md).

## Owner's Otter transcript idea — next scoped knowledge-base slice

No transcript files have been supplied in this request; no local Messages,
Otter account, arbitrary Downloads files or private transcripts were read.
Implement on top of Bluey's existing document/RAG subsystem, not a QuillBot
dependency or a second model backend. Before mounting, audit its exact owned
account authorization, retention/deletion and retrieval contract.

1. Pinky: explicit import of owner-provided TXT/SRT/VTT/DOCX/PDF; preview
   participants and choose "my answers" versus interviewer questions. Require
   review for ambiguous speaker attribution and third-party sensitive content.
2. Bluey: preserve immutable source hash and timestamp/speaker/page provenance;
   extract projects, responsibilities and STAR stories as proposed facts, not
   automatically verified achievements. Owner confirmation binds fact IDs.
3. Keep style preferences separate from factual memory. Learn sentence length,
   tone and preferred terminology without copying other speakers' identity.
4. Retrieve a small bounded set per question with tenant/document permissions
   rechecked at dispatch. Treat transcripts as untrusted data, never system
   instructions. Cite source excerpts/timestamps privately in an expandable
   "Based on your experience" view; admit missing evidence honestly.
5. Add opt-in source toggles, export and deletion. Remove both text and vectors,
   fence deleted/revoked documents and isolate two users with adversarial tests.
6. Benchmark retrieval plus first-token latency against text-only baseline.
   Keep common voice instructions stable for caching, and avoid a rewrite pass.

Acceptance: no interviewer story becomes the candidate's; no fabricated metric,
employer or date; no cross-account retrieval; deleted sources cannot reappear;
prompt injection in a transcript cannot change authority or access tools.
This memory slice is planned, **not implemented or deployed**. Viewer audio,
microphone, screen and Otter integration consent remain separate and off.
