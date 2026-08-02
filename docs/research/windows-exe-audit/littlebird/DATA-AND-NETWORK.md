# Littlebird 0.81.10 Windows data and network map

## Local data

| Store/path | Observed purpose | Security boundary |
|---|---|---|
| Electron `userData` | settings, window state, PID, diagnostics, observer state | user-local files; exact ACLs/runtime contents unvalidated |
| `contextkit.pid` | helper PID | rechecked against `tasklist` executable name before kill |
| `SnapshotDumps/` | diagnostic context snapshots | potentially sensitive; production retention/consent needs validation |
| `category-seed.sqlite` | exclusion/category seed | packaged static database |
| IndexedDB/Dexie/electron-store | renderer records, cache/state | schemas recoverable in maps; Windows storage encryption not established |

The main contains a deliberate redaction layer for access/refresh/ID tokens,
authorization/password/secrets, JWT-shaped strings, and long opaque values
(`dist-electron/main/index.js:3038-3110`). It hashes redacted values for
correlation. IPC logging is policy-driven and truncates formatted payloads, a
useful pattern, though raw parse-error previews and snapshot diagnostics still
need privacy canaries.

## Client-visible network boundaries

- API/auth: `api.littlebird.app`, `app.littlebird.ai`, `app.lilbird.co`.
- MCP: `mcp.littlebird.ai` and packaged MCP SDK/toolkit clients.
- Downloads/support/trust: Littlebird domains in packaged renderer resources.
- Telemetry: Sentry, PostHog, Axiom, Singular, Product Fruits dependencies.
- Updater: S3 `little-bird-releases`, region `us-east-2`, `x64`, `alpha`.

The helper inherits all of `process.env` (`index.js:9130-9163`). Even with
application logging redaction, Bluey now passes a minimal child environment so
unrelated credentials never cross into a capture helper.
