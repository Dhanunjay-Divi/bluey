# ParakeetAI 3.7.0 Windows security assessment

## Positive observations

- Installer/main Authenticode content digests match the signed value. Trust-chain
  resolution is recorded as unverified on this macOS host, not invalid.
- `safeStorage` secret handling fails closed rather than silently persisting
  plaintext.
- Rust AEC queues are explicitly bounded and drop stale audio under pressure.
- Native process enumeration requests limited process-query rights and closes
  handles/COM memory.
- Meeting detection runs in an Electron utility process and has a stop/kill path.
- Content protection is enabled by default through private mode.

## Risks

- The Windows N-API binaries are unsigned and loaded in-process. A replaced addon
  has the full Electron main-process privilege.
- BrowserWindow security flags are not explicit; sandboxing is not established.
- Permission handling grants `media` without an observed request-origin check.
- External-window handling calls `shell.openExternal` without a visible scheme
  allowlist.
- The default display-media handler automatically selects the first screen and
  loopback source.
- One-sample meeting start/stop transitions can be noisy, especially for browsers
  that use microphones for unrelated pages.
- Update feed is GitHub-based but no additional publisher pin is visible in the
  app updater path.

Bluey should adapt the AEC/backpressure and MMDevice session-enumeration concepts,
but keep native code out of the main process, sign it, authenticate commands,
and make meeting transitions advisory/debounced.
