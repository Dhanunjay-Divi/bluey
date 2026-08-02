# Littlebird 0.81.10 Windows security assessment

## Positive observations

- Installer, application, observer, and ripgrep helper pass Authenticode digest
  and chain verification to LITTLE BIRD SOFTWARE LLC.
- BrowserWindows explicitly enable context isolation and disable node integration.
- Helper events are framed, parsed, schema-validated, correlated by callback ID,
  and subject to request timeouts.
- Orphan termination validates the PID's image name before killing it.
- Sensitive logging uses structured redaction and opaque-value hashing.
- App/domain exclusions and sensitive categories are first-class product state.

## Risks

- The preload/IPC domain is large. A renderer compromise can request privileged
  context, screenshot, observer, integration, and update operations.
- The child receives the entire Electron environment. This can leak unrelated
  credentials to a high-privilege capture process.
- Delimiter framing has no obvious maximum accumulated byte count. A corrupt or
  compromised helper could grow the parser buffer or send huge validated fields.
- The PID file and executable-name check prevent accidental kills but do not
  cryptographically authenticate the child after launch.
- Snapshot dumps, OCR, full-window context, and telemetry create high privacy
  stakes; authenticated runtime defaults and retention are unknown.
- S3 alpha updater config proves a feed, not independent manifest signature,
  rollback, or publisher pinning in app code.

Bluey should adapt Littlebird's child-state replay, schema validation, bounded
restart policy, and redaction. It should improve them with signed-helper checks,
strict frame limits, minimal environment, per-command capability, and explicit
retention controls.
