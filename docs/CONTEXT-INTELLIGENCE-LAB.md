# Context Intelligence Lab

Bluey should not treat the research-paper work as a side project. It becomes a
product capability: Bluey can measure what context it has, explain what is
missing, and guide the user to add the right approved source.

## Product Goal

Give users the feeling that Bluey can work with any legitimate screen or work
surface while keeping access explicit, auditable, and reliable.

The feature answers four questions:

1. What can Bluey currently understand?
2. What is missing for a high-quality answer?
3. What approved action can the user take to add that context?
4. How confident is Bluey that the answer is grounded in the right sources?

## Customer-Facing Surface

### Context Coverage Meter

The overlay should show a compact coverage strip or popover:

```text
Audio: live
Screen: captured 12s ago
Docs: 3 attached
Repo: not attached
Page: readable
Agent: none
Cloud memory: synced
```

Each source has a state:

- `missing`
- `available`
- `needs permission`
- `indexing`
- `ready`
- `stale`
- `failed`

### Safe Context Upgrade Prompts

When Bluey cannot answer well from the current context, it should say exactly
what it needs:

```text
I can see the screen, but not the underlying files.
Attach the project folder or connect the repo for exact file-level help.
```

Examples:

- Screen mentions a file path -> suggest attaching the repo/folder.
- Meeting mentions a ticket -> suggest connecting the approved ticket system or
  adding the ticket text.
- Browser app shows partial table -> suggest reading the page or attaching an
  export.
- Code answer needs full project context -> suggest Attach Project.

### Source Cards

Every answer should expose the sources it used:

```text
Used: transcript, screen OCR, AGENT-HANDOFF.md, src/router.rs
Missing: full repo dependency graph
Confidence: medium
```

This keeps the product trustworthy and makes it clear when Bluey is guessing
from a screenshot versus grounded in real files.

## Research-To-Product Loop

The research paper becomes the measurement harness for this feature.

Lab surfaces:

- Mock Monaco/VS Code-style browser IDE with fake projects.
- Mock dashboard/SaaS pages with tables, charts, and hidden details.
- Mock meeting transcript with ticket/branch/project references.
- Adapter tests for screenshot/OCR, clipboard/selection, browser-page text,
  accessibility tree, local folder, repo index, and approved connector export.

Metrics:

- Context coverage percentage.
- File/tree recovery accuracy.
- Answer grounding accuracy.
- Time-to-useful-answer.
- Privacy risk level.
- Consent clarity.
- Failure mode classification.

The output should feed both:

- product decisions, such as which context prompt to show first;
- the public research/trust paper explaining Bluey's consent-based architecture.

## Implementation Phases

### CIL-1: Local Coverage Model

- Add a `ContextSource` model covering audio, screen, docs, repo, page, memory,
  cloud, and agent.
- Add coverage state, freshness timestamp, confidence, and user-visible label.
- Surface coverage in the overlay header/popover.

### CIL-2: Missing Context Detector

- Extend routing/classification to emit `missing_context`.
- Trigger safe prompts: attach project, attach docs, read page, analyse screen,
  connect repo, or continue with limited context.

### CIL-3: Project/Repo Context

- First-class Attach Project flow.
- Build file tree, summaries, dependency hints, and RAG chunks.
- Cite files in answers.

### CIL-4: Research Harness

- Build controlled mock IDE and mock work-app fixtures.
- Run repeatable context-channel benchmarks.
- Store reports under `docs/research/`.

### CIL-5: Agent/Connector Bridge

- Discover approved local agent/workspace context where feasible.
- Prefer MCP-style/context-export APIs.
- Keep raw session cookies and hidden third-party scraping out of scope.

### CIL-6: Team Context

- Merge multiple participants' approved context into one meeting.
- Enforce workspace boundaries.
- Add audit logs and source attribution.

## Non-Goals

- Do not depend on raw browser cookie capture.
- Do not replay hidden third-party APIs as a product feature.
- Do not market or design around bypassing assessment/proctoring controls.
- Do not silently read broad local folders. User selects folders/connectors.

## Why This Matters

Screenshots alone make Bluey feel magical for shallow tasks and weak for deep
work. Context Intelligence turns Bluey into a real work companion: it knows
when it has enough context, asks for the right missing source, and proves what
it used.
