# ROUND-476 Medha Otter Context QA

Date: 2026-07-11
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Goal

Run Bluey against a real resume plus four Otter transcript exports and verify
whether saved conversation files are actually useful for interview answers,
without repeatedly resending every full document on each answer.

## Inputs

- Resume: `/Users/uno/Downloads/Medha Reddy ResumeSE (6).pdf`
- Extracted resume text: `tmp/bluey-resume-test/resume.txt`
- Otter transcript exports:
  - `tmp/bluey-otter-test/otter-1-visible.md`
  - `tmp/bluey-otter-test/otter-2-visible.md`
  - `tmp/bluey-otter-test/otter-3-visible.md`
  - `tmp/bluey-otter-test/otter-4-visible.md`

## What Failed Before This Round

- Bluey attached the saved files to the session, but answers sometimes used only
  shallow context from the beginning of each file.
- Interview questions about the Amazon Just Walk Out camera incident missed the
  deeper transcript facts: fan RPM, heartbeat stops, overheating, CloudWatch,
  Athena, Lambda reboot guardrails, and false-alarm avoidance.
- The first self-introduction answer invented unsupported details instead of
  grounding itself in the resume and Otter transcript.
- The audit trail showed prompt and UI events, but it did not clearly report how
  many final answer contexts were used.

## What Changed

- Saved session attachments now build query-focused excerpts instead of always
  sending only the first slice of the converted preview.
- Interview-style questions get a small relevance boost for resume and Otter
  transcript attachments.
- Resume-like documents include a short profile preview plus matched excerpts,
  so Bluey keeps candidate identity while still staying cheap.
- Final answer diagnostics now log context counts:
  - total contexts
  - document contexts
  - memory contexts
  - screenshot contexts
  - transcript contexts
- Existing behavior remains: saved docs are background context, not pending
  one-shot chips, unless the user explicitly attaches them for that answer.

## Expected Product Behavior

- First attach: convert and save docs locally, then use summaries and relevant
  excerpts for future answers.
- Each answer: send the question, recent transcript/chat, and tiny relevant
  snippets, not every full file again.
- Resume/interview questions should find deep transcript facts even when they
  appear far below the first page of the converted Otter text.
- Unrelated questions should not drag in saved documents.
- The audit log should make it obvious whether documents/screenshots/memory were
  used for a given answer.

## Verification So Far

```bash
cargo fmt --all
cargo check -p cue-daemon
cargo build -p cue-cli -p cue-daemon
cargo test -p cue-daemon interview_
cargo test -p cue-daemon relevant_current_attachment_context
cargo test -p cue-daemon sanitize_answer_text
```

## Live QA Results

- Visible Bluey was restarted with `scripts/bluey-visible-local.sh --title
  "Medha Resume Otter QA"`.
- Active session id: `9d6c6ae7-4c46-4a90-87d8-adc5e1bfe282`.
- Attached context count: 5.
  - Medha resume PDF.
  - Four Otter transcript markdown exports.
- Final visual screenshot: `/tmp/bluey-medha-qa-final.png`.

### Prompt: Amazon Just Walk Out Camera Incident

Question:

```text
What happened in the Amazon Just Walk Out camera incident and how did Medha resolve it? Use the Otter transcript context.
```

After the camera-anchor fix, Bluey answered from the right transcript section:

- Amazon Just Walk Out camera failures.
- Fan RPM issue caused cameras to overheat.
- Lambda monitored fan RPM and triggered reboot only after sustained threshold breach.
- Normal fan-speed fluctuations caused false triggers in early testing.
- Validation compared failed-camera telemetry against healthy-camera telemetry.
- Guardrail prevented repeated or cascading reboot instances.
- SLA detail was preserved: couple of minutes for initial response, 15 to 20 minutes for resolution.

QA verdict: good enough for interview use. One wording still has mild polish
inference: "kept us within our SLA window consistently." That is plausible from
the transcript but could be tightened to "helped us stay within the SLA window"
to avoid overstating.

### Prompt: Outside Comfort Area

Question:

```text
Describe a time Medha took on work outside her comfort area. Keep it in STAR format and use the Otter transcript context.
```

Final answer correctly used the Fannie Mae style transcript section:

- Current AWS developer scope was testing and automation.
- Work involved validating data pipelines and business logic.
- Migration was from a legacy SaaS platform to AWS.
- Output differences appeared between legacy SaaS and new AWS pipelines.
- Medha had no prior SaaS transformation or financial-domain experience.
- She worked daily with business analysts and senior engineers.
- She wrote SQL and Python validation scripts.
- Result stayed qualitative, without invented Glue, Step Functions, dashboards,
  regulatory deadlines, or numeric metrics.

QA verdict: good.

### Prompt: Self Introduction

The live self-introduction answer grounded itself in the resume and interview
context, but it used an AI-sounding filler word. The sanitizer now removes
leading and inline filler such as `genuinely`, `honestly`, and
`straightforwardly`. This exact self-introduction was not rerun after the
sanitizer to avoid extra paid calls.

## Audit Log Check

Audit file:

```text
~/Library/Application Support/cue/session-audit-events/9d6c6ae7-4c46-4a90-87d8-adc5e1bfe282/events.jsonl
```

For the two final Otter QA answers, the audit trail recorded:

```json
{
  "answer_context_total": 1,
  "answer_context_documents": 1,
  "answer_context_memory": 0,
  "answer_context_screenshots": 0,
  "answer_context_transcripts": 0,
  "visible_context_count": 0
}
```

This means Bluey used one focused saved document/transcript excerpt and did not
pull stale visible context or RAG memory into those answers.

Start latency remained high:

- Camera incident: first visible text in about 9.9 seconds.
- Outside comfort area: first visible text in about 8.2 seconds.
- Local context preparation was only about 23 ms, so the delay is dominated by
  the managed route/provider path, not local file retrieval.

## Web UI Check

Chrome signed-in dashboard check:

- `https://bluey.sh/account` loaded successfully.
- Balance, My Computers, Session History, Usage Summary, Billing, and Trial Ops
  were visible.
- Linked device list showed two Bluey desktops.
- `https://bluey.sh/download` loaded and showed macOS, Windows beta, and Linux
  coming soon sections.
- `https://bluey.sh/login` renders the account dashboard when the browser is
  already signed in. That is functional, but it can feel confusing if a user
  expects a login form.

## Remaining Work

- Make answer start latency feel closer to sub-second for small interview
  prompts. The focused-context path is fast locally, but the managed route still
  starts slowly.
- Tighten phrasing for transcript-grounded stories so qualitative outcomes do
  not sound stronger than the transcript evidence.
- Finish the browser-side web UI pass for Session History detail loading and
  saved file visibility if we want to fully close the dashboard QA loop.
- If the user means every helper environment should be created during install,
  verify installer coverage beyond the recent local document-tools bootstrap.
