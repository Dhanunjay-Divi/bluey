# ROUND-311 Otter Interview Transcript Retry

Date: 2026-07-02

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

Branch: `codex/bluey-overlay-spacing-20260626`

## Goal

Retry the Otter/ChatGPT interview-review pass and make Bluey answer more like a strong human interview coach across BIE, SDE, data engineering, data science, AI/ML, autonomy/perception, and related roles.

## Otter Retry Notes

- Retried the Otter folder path after earlier Chrome lazy-loading timeouts.
- Confirmed the folder exposes 76 recording links after scrolling/waiting, not just the first 12 visible rows.
- Sampled shared Otter speech endpoints for representative recordings without copying private transcript text into docs.
- Observed role/domain coverage from metadata and summaries: BIE/Tableau/SQL/ETL, AI/ML perception/localization/sensor calibration, data science, data engineering, Python/AWS/Power BI, and interview self-introduction flows.
- Some Otter pages still lazy-load slowly in Chrome, so the product fix is intentionally resilient to partial page loads and messy live transcript wrappers.

## Changes

- Managed AnswerPlan now lets generic live-caption prompts inherit intent from the hidden transcript/planning context.
  - If the visible question is `Answer the latest live captions...` but the context is coding, it routes as coding.
  - If the context is interview coaching, it routes as behavioral/interview coaching.
  - If the context is system design, it routes as system design.
  - Generic live-caption prompts are only treated as missing context when no planning context exists.
- Interview answer mode now explicitly says:
  - infer the latest interviewer question from messy live transcript input;
  - do not summarize the transcript or repeat the live-caption wrapper;
  - repair a rough candidate draft into a clean answer the user can say;
  - preserve supplied facts and avoid inventing employers, metrics, tools, or claims.
- Added role/domain depth for:
  - AI/ML, autonomy, perception, robotics, object detection, segmentation, localization, sensor calibration, evals, latency, safety, traces, and cost;
  - BIE/data analyst/data engineer work: SQL, ETL/PySpark/dbt/Airflow, validation, freshness, reconciliation, dashboard choices, KPIs, lineage, stakeholder impact, and production verification;
  - SDE/system work: ownership, APIs, data flow, concurrency, failure modes, tests, and rollout.
- Native/direct daemon prompt path now mirrors these interview transcript rules.
- Native prompt helper now detects role/domain signals inside attached transcript context, not only in the visible question.

## Regression Coverage

- Generic live-caption wrapper plus an AI/perception interview transcript is recognized as interview coaching.
- Amazon BIE/Tableau/backend-refresh/source-table question stays behavioral/interview coaching instead of being stolen by the coding classifier.
- AI/ML production interview style still includes RAG/MCP depth plus autonomy/perception terms.
- Native/direct prompt path enables autonomy/BIE transcript interview mode from attached transcript context.

## Verification

```bash
cargo fmt --all
cargo test --manifest-path server/Cargo.toml answer_plan -- --nocapture
cargo test -p cue-daemon provider_messages_enable -- --nocapture
cargo build -p cue-daemon --bin bluey-daemon
cargo build --manifest-path server/Cargo.toml
```

## Remaining Follow-Up

- Build a safe, owner-visible benchmark set from redacted interview prompts and expected answer qualities.
- Add product telemetry that records answer intent/lane/provider and transcript hashes, not transcript text, so failures can be debugged without exposing user content.
- Add a UI affordance for "interview mode active" only if it helps users understand why Bluey is speaking in candidate voice.
