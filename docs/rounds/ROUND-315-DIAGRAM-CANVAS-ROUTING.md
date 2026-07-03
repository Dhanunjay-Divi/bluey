# ROUND-315 Diagram Canvas Routing

Date: 2026-07-03
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Goal

Make system-design and explicit pictorial/diagram requests reliably open canvas/detail output, and avoid treating Mermaid/diagram blocks as normal code artifacts.

## Diagnosis

- System-design prompts already mapped to `AnswerOutput::CanvasDetail`.
- Explicit pictorial prompts such as `Give a pictorial representation of an LRU cache data flow` could still look like coding because `LRU` and `cache` are code/data-structure signals.
- The UI already recognizes `diagram` artifacts on macOS and Windows, but the server did not emit `diagram` artifacts from Mermaid or flowchart-shaped answers.
- The server system-design prompt asked for architecture sections, but did not strongly require a diagram section when the user specifically asked for pictorial output.

## Changes

- Added `looks_like_diagram_request` for:
  - `diagram`
  - `flowchart`
  - `sequence diagram`
  - `architecture diagram`
  - `data flow`
  - `pictorial`
  - `visual representation`
  - `draw`
  - `block diagram`
  - `box diagram`
- Diagram requests now win over generic coding signals unless the user explicitly asks for code generation.
- System-design prompt now says diagram/pictorial/flowchart requests should start canvas detail with `### Diagram` and include compact ASCII or Mermaid diagram output.
- Added `looks_like_diagram_artifact` so Mermaid/flowchart output becomes `artifact_type=diagram` before generic fenced-code detection can classify it as code.
- Added a canvas-detail artifact path so system-design answers prefer `system_design`, `diagram`, `screen`, `document`, or `structured` artifacts instead of accidentally becoming code artifacts.

## Verification

```bash
cargo fmt --all
cargo test --manifest-path server/Cargo.toml answer_plan_ -- --nocapture
cargo test --manifest-path server/Cargo.toml response_artifact -- --nocapture
cargo build --manifest-path server/Cargo.toml
git diff --check
```

New regression coverage:

- `answer_plan_pictorial_design_opens_canvas_detail`
- `response_artifact_detects_mermaid_diagram_before_code`
- `response_artifact_for_output_keeps_system_design_canvas_non_code`

## Live Smoke Finding

First live deploy smoke showed:

- `Give a pictorial representation of an LRU cache data flow` returned `artifact_type=diagram`, as intended.
- `Design a scalable notification system` returned useful system-design text but `artifact_type=code`, because generic code-shape detection was still allowed for canvas-detail output.

The follow-up fix added `response_canvas_detail_artifact` so `AnswerOutput::CanvasDetail` no longer falls through to code detection first.

## Deployment

Final production deploy:

- Commit: `64ca5334e22cf0013e944f6ef87ce5eeaa0c9aa8`
- Build tree: `/opt/bluey-build-codex-round315-diagram/server`
- Installed binary: `/usr/local/bin/bluey-server`
- Binary SHA256: `ec1d2d07a6405fae266457a446e8708296a48619f1092f6aaf44fb22671ef806`
- Previous binary backup: `/var/backups/bluey-api/bin/bluey-server.previous-20260703T071150Z`
- `bluey-api.service`: active
- `NRestarts`: `0`
- Public health reports commit `64ca5334e22cf0013e944f6ef87ce5eeaa0c9aa8`
- Recent warning logs: no entries.

Final live smoke:

- `Design a scalable notification system with queues...` returned `artifact_type=system_design`.
- `Give a pictorial representation of an LRU cache data flow...` returned `artifact_type=diagram` with a Mermaid flowchart.

## Expected Behavior

- `Design a scalable notification system` opens system-design canvas/detail.
- `Give a pictorial representation of LRU cache data flow` opens canvas/detail instead of code canvas.
- If Bluey emits Mermaid or flowchart-shaped output, the artifact type is `diagram`, not `code`.
- Explicit code requests such as `Build LRU cache in Python` still open code artifacts.
