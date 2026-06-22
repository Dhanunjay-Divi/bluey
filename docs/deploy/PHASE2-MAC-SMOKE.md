# Phase 2: Mac Internal-Test Smoke Validation

> **Status:** durable doc; promoted from /tmp staging when the
> Observability Round was at 5/6 phases done. Use this as the operator
> playbook for the closed-alpha Mac smoke.


**Tip after Phase 1:** the next commit landed by `phase1_commit_build.sh`.
**Artifact:** `/tmp/bluey-internal-test/` on uno.
**Required:** uno is a Mac with `bluey-server` reachable, login flow set up.

This is the post-reboot validation script. Run each step in order. **STOP if any
step fails** and report the failure mode rather than continuing.

---

## Pre-flight: Activate the test build

```bash
# On uno, after reboot:
cd /Users/uno/Downloads/cue
bash /tmp/bluey-internal-test/run-internal-test.sh
export PATH=/tmp/bluey-internal-test/bin:$PATH
```

Verify version + binaries are the just-built ones:
```bash
which bluey
bluey --version  # should match crates/cue-cli/Cargo.toml version
```

Optional dev-only guard for Steps 1-3:
```bash
scripts/macos-overlay-visual-smoke.sh
```
This script launches Bluey with `BLUEY_DEV_OVERLAY=1`,
`BLUEY_LOCAL_VISIBLE_OVERLAY=1`, and `BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE=1`
so it can screenshot the overlay, click the pill, start simulated audio, and
assert the expanded window remains fixed-size while transcript text updates.
Do not use those env vars in customer launch paths.

---

## Step 1 — `bluey on` boots cleanly

```bash
bluey on
```

**Expect:**
- Daemon starts; logs go to stderr (no rotation yet — observability round)
- Boot lines printed to terminal include:
  - "Listening for hotkey F19" (or similar)
  - "Managed answers ready" if already signed in, OR "finish sign-in in your browser" if not
  - Session-history affordance, attach/analyse consent, transcript/answer behavior copy
- Native overlay pill appears centered as a compact Bluey-tinted badge

**Fail signals:**
- "Failed to spawn overlay helper" → check `/tmp/bluey-internal-test/bin/cue-overlay` exists + is executable; `codesign -dv` should show ad-hoc sign
- Daemon crashes immediately → check `RUST_LOG=debug bluey on` for panic stack
- No pill visible → check macOS Accessibility + Screen Recording permissions (System Settings → Privacy & Security)

---

## Step 2 — Pill → Expanded overlay

Click the pill or use the F19 hotkey.

**Expect:**
- Overlay expands to an approximately 820x520 panel that fits the visible screen
  and can be resized without cropping header/composer chrome
- Top header shows: Bluey label, balance (e.g. "Bluey · $4.98" or "Sign in to start"), Hide (eye-slash) and Close (X) icons
- Disguise menu accessible (click the Bluey icon header)
- Empty state explains the session surface; not a blank debug panel
- Composer row at bottom: opacity capsule, listening toggle, attach, analyse-screen, instructions, send button

**Fail signals:**
- Panel doesn't expand → IPC issue between daemon and overlay; check daemon log for "send_overlay" errors
- Expanded panel is blank/grey → SwiftUI render fault; check overlay process for stderr/crash
- Composer controls overlap or wrap → window-size or fixed-width regression

**Regression guard:** the header must never be cropped. If the panel is close to
an edge, it should reposition or shrink within the visible screen, not hide the
model picker, balance, Hide, or Close controls.

---

## Step 3 — Listen / Stop session

In the expanded overlay, click "Listen" (or the listening toggle).

**Expect:**
- Daemon starts capturing audio; overlay shows live transcript preview ("Live captions preview" strip)
- Live transcription text scrolls as you speak
- Transcript snippets stay inside the fixed bottom preview strip; they do not render as chat cards
- Expanded overlay remains at the user-selected size while captions update
- "Stop" button replaces "Listen"
- Session is created in `~/Library/Application Support/Bluey/active-meeting.json` (perms 0600)

Click Stop. **Expect:**
- Capture stops; transcript freezes
- A new entry appears in the recordings drawer (if drawer was open)
- Final session JSON saved to archive directory (perms 0600)

**Fail signals:**
- "No microphone access" → System Settings → Privacy & Security → Microphone → check "Bluey" or "cue-daemon"
- Transcript appears as repeated right-side bubbles → transcript card routing regression
- Overlay grows taller/wider while captions arrive → fixed-size/layout regression
- Capture starts but no transcription → STT routing: if logged in, should be using managed `/router/transcribe`; check daemon log for `router/transcribe` HTTP errors
- "STT not configured" with developer keys NOT set → confirm logged in (`bluey usage` should show balance)

**Send behavior:** live captions are context, not automatic questions. Bluey
should not auto-send an answer just because speech paused. The user sends by
pressing Enter/Send or by clicking Answer/Screen. Future voice-auto-submit can
be a separate explicit mode, but default live-call UX is "listen continuously,
answer on command".

---

## Step 4 — Ask / Answer streaming

In the composer, type a question (e.g. "Write a Python function that sorts a list").

**Expect:**
- Question bubble appears right-aligned in chat
- Bluey response streams in left-aligned (synthesized SSE — chunks appear after server completion, not token-by-token; this is documented as N-1 in the review)
- After the answer arrives:
  - Cost label visible (e.g. "$0.03 · balance $29.97")
  - Model + provider pill visible (e.g. "openai · gpt-4o-mini")
  - Code in the answer is removed from the chat bubble and shown in the right-side canvas
  - Canvas auto-opens because artifact_type = "code"

**Fail signals:**
- Question hangs / no response → check daemon log for `/router/complete` or `/router/complete/stream` errors
- 402 Payment Required → balance is exhausted; topup via `bluey portal`
- 401 Unauthorized → run `bluey on` again so Bluey can reopen sign-in if needed
- Answer arrives but no canvas → either fallback heuristics aren't detecting code, OR the LlmArtifactMetadata is not threading through correctly; check daemon log

---

## Step 5 — Attach docs

Click the "Attach" button in the composer. Select a small `.txt` or `.pdf`.

**Expect:**
- Attached file shows as a chip above the composer (horizontally scrollable if multiple)
- Asking a question now uses the document as context; answer references the doc
- The picker allows only readable context formats: text, Markdown, code, PDF,
  DOC/DOCX, CSV/TSV, JSON/YAML/TOML, HTML/CSS, shell/SQL, and RTF.
- Video/audio/keychain/certificate files such as `.mp4`, `.mov`, `.wav`, `.p12`
  must be disabled by the native picker or skipped by the daemon with a visible
  warning card.

**Fail signals:**
- "File picker doesn't open" → Tauri dialog plugin issue; check dashboard log
- File attaches but answer doesn't reference it → context-bridge issue; verify the file content reaches the LLM request (check daemon `request_cue` logs)
- Unsupported file appears as an attached chip → context validation regression

---

## Step 6 — Analyse Screen

Click the "Analyse Screen" button.

**Expect:**
- macOS Screen Recording permission prompt (first time)
- Screenshot captured (overlay window itself excluded from capture per `sharingType=.none`)
- Screenshot becomes a context artifact for the next question
- Asking "what do you see on my screen?" answers based on the capture

**Fail signals:**
- Permission prompt appears every time → permissions weren't actually granted; verify in System Settings
- Screenshot includes the overlay → capture-exclusion regression; check Swift main.swift `sharingType` setting
- Screenshot taken but answer doesn't reference it → vision-model route is misconfigured

---

## Step 7 — Style prompt ("How Bluey should answer")

In the expanded overlay, find the inline style textbox. Type:
```
Answer in 2 sentences max, casual tone.
```

Click save. Ask a question.

**Expect:**
- Save succeeds with token validation (no separate modal — inline)
- Next answer respects the style: short, casual

**Fail signals:**
- Save button does nothing → check daemon log for SessionRenameRequested or instruction-update event
- Style ignored → instruction text isn't reaching the LLM system prompt; check daemon `build_request`

---

## Step 8 — Session history

Open the recordings drawer in the overlay (header button or arrow).

**Expect:**
- Recent sessions listed with timestamp + title
- Click a session → loads its transcript + responses into the chat view
- Per-row rename button → click → inline editor; type new name, Enter saves

**Fail signals:**
- Drawer empty → no past sessions in `MeetingStore`; expected on a fresh install
- Click does nothing → SessionOpenRequested event isn't being emitted; check overlay log
- Rename opens an alert modal instead of inline → regression on the senior UX pass

---

## Step 9 — Balance / cost labels

Trigger several questions to use balance. After each:

```bash
bluey usage
bluey credits
```

**Expect:**
- Balance decrements visibly in the overlay header AND in `bluey usage` output
- Cost labels on responses match (e.g. "$0.03" decrement matches usage event)
- After exhausting balance, next request returns 402; overlay shows "Sign in to topup" or similar

```bash
bluey portal  # open Stripe billing portal
```

**Expect:**
- Browser opens to Stripe customer portal at the URL returned by server `/billing/portal`
- Topup completes → next `bluey usage` shows refreshed balance (within auto-refresh interval)

**Fail signals:**
- Balance doesn't update after question → check `push_overlay_balance_snapshot` / `SetBalance` IPC
- Cost label missing → metadata didn't thread through; check `cue_response_chunk` payload includes `cost_label`
- 402 after topup → webhook didn't fire or balance migration not triggered; check server `/billing/webhook` log

---

## Step 10 — Cloud sync

```bash
bluey cloud sync
bluey cloud sessions --limit 10
bluey cloud show <session_id_from_above>
bluey cloud rag "test query words"
```

**Expect:**
- `cloud sync` reports records uploaded count > 0 (unless empty cache)
- `cloud sessions` shows the sessions you just created
- `cloud show` returns transcript, responses, cost metadata for that session
- `cloud rag` returns relevant chunks if the query matches transcript content

**Fail signals:**
- 401 / 403 → re-login; account token expired
- Empty result for known content → check server `cloud_rag_chunks` table; verify upload included rag_chunks (`tracing::debug` on daemon CloudSyncNow)

---

## If ALL 10 steps pass

Move to Phase 3 (server test deploy). Otherwise, capture:
- The exact step that failed
- `bluey doctor` output (NOTE: not yet implemented; manually grep `~/Library/Logs/Bluey/` if it exists, or pipe daemon stderr to a file)
- The daemon log fragment around the failure timestamp
- macOS version + arch (`sw_vers` + `uname -m`)

Report and we iterate.
