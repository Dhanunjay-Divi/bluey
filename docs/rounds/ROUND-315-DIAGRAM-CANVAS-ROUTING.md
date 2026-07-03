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

## Expected Behavior

- `Design a scalable notification system` opens system-design canvas/detail.
- `Give a pictorial representation of LRU cache data flow` opens canvas/detail instead of code canvas.
- If Bluey emits Mermaid or flowchart-shaped output, the artifact type is `diagram`, not `code`.
- Explicit code requests such as `Build LRU cache in Python` still open code artifacts.

