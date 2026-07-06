# ROUND-402-ANSWERPLAN-REGRESSION-CLOSURE

Date: 2026-07-06
Branch: `main`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Goal

Close the concrete AnswerPlan issues from `ROUND-399-LIVE-ANSWER-LATENCY-EVAL` so Bluey stops answering with the wrong route/style before more UI polish is added.

## Fixed

- Quick technical concept comparisons now stay `quick`/`instant` and do not trigger managed web-search status.
- Common system-design prompts like `Design a URL shortener` now route as `system_design` with canvas detail instead of falling through to quick/general/behavioral behavior.
- Generic screen-context prompts that include OCR/planning-context code now route as `coding` with `code_artifact`, even if the request does not include an image data payload.
- Screen-context coding no longer gets downgraded to the simple/balanced code path; it uses the deeper code-artifact lane so the canvas has enough budget for complete code.
- Missing-image detection now treats OCR/planning context as real screen evidence, avoiding false `missing_context`.
- Web-search status no longer says `Searching web...` when the search was skipped before provider work started.
- `short_observability_ref` now keeps UUID session refs stable but uses the entropy-bearing suffix for prefixed request IDs like `live-eval-...`, so support refs are unique enough to search logs.

## Tests Added

- `answer_plan_round399_quick_concept_does_not_trigger_research`
- `answer_plan_round399_url_shortener_is_system_design`
- `answer_plan_round399_screen_context_ocr_code_without_image_is_code_artifact`
- `retrieval_status_does_not_show_searching_when_search_was_skipped`
- `short_observability_ref_uses_entropy_suffix_for_prefixed_request_ids`

## Verification

- `cargo test --manifest-path server/Cargo.toml answer_plan_round399 --quiet`
- `cargo test --manifest-path server/Cargo.toml answer_plan --quiet`
- `cargo test --manifest-path server/Cargo.toml retrieval_status_does_not_show_searching_when_search_was_skipped --quiet`
- `cargo test --manifest-path server/Cargo.toml --lib --quiet`
- `cargo test --manifest-path crates/cue-core/Cargo.toml short_observability_ref --quiet`
- `cargo test --manifest-path crates/cue-core/Cargo.toml --lib --quiet`

## Deploy Status

Not deployed at the time this doc was first written. Deploy after commit if the branch remains clean.
