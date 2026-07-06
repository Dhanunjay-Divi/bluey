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

Deployed to production after commit `e6423245 Fix AnswerPlan live regressions`.

- Production host: `root@165.227.77.152`
- Service: `bluey-api.service`
- Build directory: `/opt/bluey-builds/round402-answerplan-e6423245a615`
- Build commit: `e6423245a615ddeda8b060addd1c1d6bcdcd38ac`
- Installed binary SHA256: `42f48013fdd5db5d4f974bd129d511ae7837382fb132039eddbcfd4d9e9b8dd9`
- Prior binary backup: `/var/backups/bluey-api/bin/bluey-server.previous-20260706T074532Z`
- Pre-restart DB backup: `/var/backups/bluey-api/hourly/bluey-postgres-20260706T074524Z.pgdump`
- Health check returned commit `e6423245a615ddeda8b060addd1c1d6bcdcd38ac`
- `journalctl -u bluey-api.service --since "20 minutes ago" -p warning..alert` returned no warning/error entries after the smoke checks.

Production env still has:

- `BLUEY_ANSWER_PLAN_ROUTING=1`
- `BLUEY_ROUTE_POLICY=provider_mix`

## Live Smoke

Ran authenticated streamed checks against `https://bluey.sh` using the local linked account token without printing the token or response text.

| Prompt shape | Lane | Provider/model | First token | Full stream | Result |
| --- | --- | --- | ---: | ---: | --- |
| Quick concept: event loop vs thread pool | `instant` | `zai` / `glm-5.2` | 4056.5 ms | 4329.3 ms | OK |
| System design: URL shortener | `deep` | `openai` / `gpt-5.5` | 5607.8 ms | 8988.8 ms | OK |
| Screen-context coding OCR text | `deep` | `zai` / `glm-5.2` | 3416.1 ms | 9540.7 ms | OK |

The live path is now traceable and completed cleanly, but first-token latency is still in the `watch` range. That is a separate latency/provider-warmup issue, not the Round 399 routing regression itself.
