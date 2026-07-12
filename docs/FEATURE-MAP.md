# Feature Map

This translates the reference-app comparison language into Bluey's ethical product scope.

## Implemented In The Current Product

- Native overlay with best-effort capture exclusion: macOS requests exclusion
  through `NSWindow.sharingType = .none`; Windows uses
  `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)`. Neither path is an
  invisibility guarantee or security boundary.
- Focus-friendly overlay: non-activating, movable, resizable,
  opacity-adjustable, and collapsible through explicit show/hide controls.
- User-selected visual/code context: `bluey context add <path>` and `/attach <path>` for screenshots, diagrams, documents, and code files.
- Permissioned screen context: `bluey context capture` and live `/capture` on macOS use the OS capture picker, preview the result, and ask before attaching.
- Consent-based page analysis: the Analyse Screen chip can attach readable active browser page text and generate an answer, so long pages can be included without manual scrolling. macOS uses browser scripting; Windows uses UI Automation where browsers expose document text. If that fails, Bluey can attach one screenshot and send it to a configured vision route.
- Simple session setup: overlay paperclip attaches files or shows attached context, and overlay notepad sets answer instructions.
- Overlay ask flow: the speech-bubble button asks Bluey a question and renders the answer back into the overlay.
- Source-labeled transcript cards: audio/STT segments render as system-audio or microphone transcript cards in the overlay.
- Real audio/STT runtime: when STT is configured, Bluey captures mic/system audio through bundled native helpers, applies VAD, routes through managed or local transcription paths, deletes Bluey's temporary raw-audio chunk after transcription, and stores source-labeled transcript text. Release `0.1.99` lists macOS Apple silicon and Windows x86-64 artifacts; its Windows proof is on Windows 11.
- Live provider answer path: OpenAI, Groq, Cerebras, and OpenAI-compatible Bluey managed endpoints can answer when credentials are present; local deterministic answers remain available for offline tests.
- Built-in control legend: the overlay explains every icon, dot, quick action, and card type from the question-mark/Help control.
- Safe shutdown: overlay close button asks for confirmation before stopping Bluey.
- Meeting intelligence loop: transcript in, action items/decisions/questions/recap out.
- Provider readiness: `bluey providers` checks provider configuration without printing secrets.
- RAG memory surface: `bluey memory search` retrieves across meeting history and attached context.
- Commercial account path: managed provider routing, billing, and opt-in cloud
  session sync. New installs default sync off.

## Next Ethical Slices

- Vision analysis: provider trait for screenshot/diagram/code interpretation, using attached context artifacts as inputs.
- Provider routing: managed server-side keys, richer budgets, cost metering, and citations.
- Audio/STT hardening: long-session stress, sleep/wake, device hot-swap, permission UX, and Windows hardware QA.
- Cloud RAG: authenticated sync, embeddings, cross-meeting recall, and deletion/export controls.
- Active page capture parity: Windows has a user-level UI Automation backend, but it still needs Windows hardware QA and browser coverage testing.
- Inline overlay input: compact macOS composer is implemented; Windows parity and richer answer streaming remain.
- Overlay model/mode picker: macOS command bar can request Bluey Auto, OpenAI Direct, Groq Realtime, Cerebras Fast, or Local routes, plus General/Code/System Design/Meeting/Writing answer modes. Bluey Auto now lets the daemon own fallback routing.

## Explicit Non-Goals

- No product positioning for exams, interviews, or monitored assessments.
- No bypassing proctoring, anti-cheat, enterprise monitoring, or consent requirements.
- No passive screen scraping. Screen or image context should be user-selected or permissioned.
- No process-impersonation, covert, or undetectable product positioning.
- No customer-content model training without a separate implemented opt-in.
