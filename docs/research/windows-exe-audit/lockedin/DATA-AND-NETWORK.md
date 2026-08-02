# LockedIn 1.8.8 Windows data and network map

## Local data

- Electron/Chromium `userData`: cookies, local/session storage, caches, logs.
- Firebase/Clerk authentication state and custom token exchange.
- Packaged `.env.local` and Firebase configuration: build-time/public client
  configuration is present; values are deliberately not reproduced here.
- Renderer session/document state and crash dumps/log files. The preload exposes
  log path, log read, crash-dump path, and show-in-folder operations.
- Packaged browser extension ZIP and PDF/canvas dependencies.

No Windows Credential Manager or `safeStorage` use was found in first-party
application code. Treat browser/firebase tokens and local logs as sensitive.

## Client-visible endpoints

- Production/preproduction APIs: `prod-us-east-1.lockedinai.com`,
  `preprod-us-east-1.lockedinai.com`.
- Auth/data: Clerk, Firebase, Google token services.
- Realtime: Socket.IO client and WebRTC helper channel.
- Storage: LockedIn S3 image store and CloudFront resources.
- Billing/telemetry: Stripe, Microsoft Clarity, LinkedIn/Google analytics assets.
- Updates: `desktop-app-updates-lockedin-ai.s3.amazonaws.com`.

Static files expose client shapes only. Server authentication of the WebRTC
helper, remote-input authorization, storage encryption, and retention are
unknown and require controlled runtime/backend evidence.
