# Final Round 2.4.0 Windows data and network map

## Local storage

- `electron-store` for settings, session state, update state, and cached client
  configuration.
- Chromium cookies/storage for auth and renderer state.
- `safeStorage` for secret material where available; bundled compatibility paths
  require validation for plaintext fallback/migration behavior.
- Optional recording configuration names a `recordings` directory, 30-minute
  splitting, and 48 kHz stereo output. Static presence does not prove recording
  is enabled by default.
- Native audio buffers and Silero VAD operate locally before cloud streaming.
- Electron logs/crash reports and telemetry SDK queues.

## Client-visible network boundaries

- Production APIs: `prod-finalroundai.frai.pro` and
  `prod-finalroundai.geofrai.pro` families.
- Realtime: Socket.IO and Daily/WebRTC domains.
- Reports/files: Google object-storage references and app API routes.
- Telemetry: Sentry, PostHog, Amplitude; main code sends platform/version/screen
  metadata and device model (Windows registry query).
- Updates: `releases.finalroundai.com/latest`.

Server schemas, queueing, authorization, storage encryption, retention/deletion,
and telemetry scrubbing are unknown. Packaged ingestion identifiers were not
reproduced.
