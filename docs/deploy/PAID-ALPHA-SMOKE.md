# Paid Alpha Smoke

Run this before taking real paid users. It proves the complete money, caption,
answer, memory, and support path on a clean Mac using the deployed server.

## Preconditions

- Latest pushed code is deployed manually to the droplet/site/release artifacts.
- `https://bluey.sh/health` and `https://bluey.sh/pricing/tiers` return 200.
- `latest.json` and `latest.json.sig` are byte-identical static files served from
  the release host.
- Square sandbox and production webhook deliveries show 2xx in Square dashboard.
- OpenAI, Anthropic, Gemini if enabled, and Deepgram accounts are funded or have
  verified free credits.
- Resend/SMTP is verified for `hello@bluey.sh`.

## Fresh Install

```bash
rm -rf "$HOME/.bluey"
curl -fsSL https://bluey.sh/install.sh | bash
bluey --version
bluey doctor --json
bluey on
```

Pass:

- `bluey on` opens the pill without dev flags.
- Missing macOS permissions are shown clearly before use.
- No mock transcript chunks appear in normal mode.
- `bluey status` shows production host and no debug capture-visible flag.

## Account And Credits

1. Create a new test account from the browser page opened by `bluey on`.
2. Verify email if the account requires it.
3. Add credits through Square hosted checkout.
4. Confirm the account balance changes in the web account page and overlay.

Pass:

- Square webhook logs show verified signature and a 2xx response.
- Balance is credited within 30 seconds.
- Replaying the same webhook is idempotent.
- Low/empty-balance test account cannot run paid cloud work.

## Live Captions

1. Click `Listen`.
2. Speak a short mic sentence.
3. Play a short system-audio sentence.
4. Stop listening.

Pass:

- Live captions appear near the composer quickly.
- Mic/system source labels change only when the source changes.
- Final transcript is saved once; interim hypotheses do not duplicate into the
  saved conversation.
- Balance changes match STT reserve/settle logs.
- No Deepgram key is present on the desktop.

## Answers

1. Type a question and press `Cmd+Enter`.
2. Ask from recent transcript context.
3. Ask a coding/system-design question that should open canvas.
4. Ask a quick general question that should stay chat-only.

Pass:

- First visible answer starts quickly.
- Final answer includes cost/latency metadata.
- User/transcript text appears on the right; Bluey answer appears on the left.
- Canvas opens only for code/system-design/screen-heavy answers.
- Request IDs and trace IDs tie daemon, cloud-client, server, and provider logs.

## Screen And Documents

1. Click `Screen`; ask what is visible.
2. Attach a supported file (`.txt`, `.md`, `.pdf`, `.docx`, `.rtf`, code).
3. Drag and drop a supported file.
4. Try an unsupported file such as `.mp4`.
5. Remove an attached document.

Pass:

- Overlay is excluded from normal screen capture.
- Supported files convert to Markdown/text preview and index locally.
- Unsupported files are rejected clearly.
- Answers cite/use attached document context.
- Attachment state says loaded or failed, not stuck loading.

## Sessions And Memory

1. End the current session.
2. Reopen an old session.
3. Rename it.
4. Delete it and confirm.
5. Ask a question that needs older-session memory.

Pass:

- Session drawer opens within the overlay bounds.
- Delete stops active audio/screen capture before removing the session.
- Deleted sessions do not resurrect RAG chunks.
- Old-session recall uses local RAG without requiring local provider keys.
- Cloud sync uploads session metadata/artifacts when logged in.

## Support Bundle

```bash
bluey logs export
```

Pass:

- Zip is created.
- Logs are redacted for provider keys, bearer tokens, link codes, auth links, and
  raw reset/verify URLs.
- Support can find the smoke run by trace ID.

## Fail-Fast Rule

Stop on the first failed paid-path step. Capture:

- screenshot or screen recording,
- `bluey doctor --json`,
- `bluey logs export` zip path,
- server `journalctl -u bluey-api.service` lines around the timestamp,
- Square event ID or provider request ID if relevant.
