# Final Round 2.4.0 Windows security assessment

## Positive observations

- Installer/main Authenticode digest and chain verify to Final Round AI, Inc.
- The app independently verifies each downloaded update's Authenticode status
  and publisher, fails closed, and defers update UI during a live session
  (`out/main/index.mjs:6767-7089`).
- BrowserWindows explicitly enable context isolation and disable node integration.
- Registered IPC validates the sender frame against localhost in development and
  exact packaged renderer entry directories in production (`index.mjs:6260-6408`).
- External URL handling uses a scheme allowlist
  (`index.mjs:6720-6744`).
- Permission handling allowlists media, display capture, and notifications.
- Audio queues/latency targets and VAD queues are bounded.

## Risks

- The three highest-privilege native addons are unsigned and loaded in-process.
- The updater verifier constructs PowerShell script text containing the download
  path (`index.mjs:6855-6881`); a native API avoids quoting/injection ambiguity.
- Permission allowlisting is not visibly restricted by requesting origin/window.
- Renderer sandboxing is not explicit.
- Shared preload capability across multiple windows still increases blast radius;
  origin/path validation is not per-window command authorization.
- Rich telemetry stacks handle session/report paths; authenticated content
  redaction and retention require runtime tests.
- Debug/PDB build paths and exported symbols remain in native addons, increasing
  reverse-engineering surface (not a security boundary but a release-hardening
  issue).

Bluey should adapt the fail-closed publisher check and in-session update deferral,
but use WinVerifyTrust, a signed release manifest, monotonic versions, rollback,
and signed/hash-checked helpers.
