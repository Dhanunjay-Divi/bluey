# Static-analysis limitations and runtime unknowns

- The DMG was mounted read-only. After static analysis, one coordinator-approved unauthenticated launch ran from the mounted app with a disposable HOME/profile and blocked external egress. The app was not installed; no credentials, permissions, updater/install, helper, or remote-control action was used. See `runtime-unauthenticated.md`.
- The renderer is minified and lacks a first-party source map. UI and workflow statements are therefore tied to literals and call shapes, not original component names/lines.
- Server code, Firestore security rules, backend queueing, data-retention jobs, billing enforcement, and authorization policy were not supplied.
- Static resource references do not prove an endpoint is reachable, a feature is enabled for this account, or a UI path succeeds.
- A remote-control WebRTC data channel and main-process input executor are observed. User-facing consent, helper authentication, session binding, revocation, replay protection, and audit logging remain unknown.
- The account-delete UI proves Firebase Auth user deletion is attempted; cascading deletion of Firestore, files, backend data, logs, analytics profiles, and payment data is unknown.
- The isolated pass confirmed creation of default Electron Cookies, caches, IndexedDB, Local/Session Storage, WebStorage, Crashpad settings, updater ID, and a main log. Contents, authenticated persistence, Keychain interaction after sign-in, and normal-profile cleanup remain unknown.
- Runtime confirmed an automatic check was attempted and created `.updaterId` even with `ELECTRON_NO_UPDATER=1`; the closed proxy blocked it before any remote response/download/install. Signature enforcement, downgrade behavior, successful update recovery, and normal network payloads remain unknown.
- Screen/audio/accessibility permission prompts, stealth effectiveness, content protection, the x86_64-only key helper on arm64, and arm64-only PDF native dependencies require controlled platform validation.
- Pricing text and payment request shapes are observed; current plan availability, quotas, overage enforcement, refunds, and cancellation behavior are server-side unknowns.
- No marketing-only statement was used as proof of implementation.
