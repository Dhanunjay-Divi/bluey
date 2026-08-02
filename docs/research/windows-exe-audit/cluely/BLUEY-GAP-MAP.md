# Cluely 2.0.193 Windows versus Bluey

| Capability | Status | Bluey evidence and decision |
|---|---|---|
| Windows x64/ARM64 distribution | Partial | Bluey has a Windows native helper (`native/windows/cue-audio/main.c:1-470`) and an x64 MinGW compile produces a PE32+ subsystem-10 console binary, but signed real-Windows x64/ARM64 release canaries remain. Add ARM64 after dependencies/signing are proven. |
| Mic/system capture | Equivalent design, runtime unvalidated | Bluey directly uses event-driven WASAPI mic/render-loopback with a 100 ms buffer and typed diagnostics (`native/windows/cue-audio/main.c:203-245,285-352,366-445`). Prefer this over SoX + renderer loopback after real-device tests. Structured endpoint-loss reporting exists; automatic rebinding is not proven. |
| Resampling | Implemented/unit-tested | Bluey's 64-tap/256-phase Blackman-windowed polyphase-sinc path (`resampler.h:7-29`; `resampler.c:10-58,69-113`) precomputes coefficients so the live push path has no trigonometry or heap allocation, and passes host 48 kHz/44.1 kHz count and passband/stopband tests (`resampler_test.c:41-79`). Real endpoint, drift, long-run, and backpressure tests remain. |
| AEC | Missing | Cluely does not provide a stronger Windows AEC implementation either. Benchmark WebRTC AEC using Parakeet/Final Round evidence. |
| Meeting detection | Implemented statically for dashboard; terminal partial | Bluey now enumerates Windows eCapture sessions, deduplicates process IDs, applies an exact native/browser allowlist, debounces start/stop, and treats inconclusive scans as non-evidence (`crates/cue-daemon/src/cloud/meeting_detect.rs:41-188,269-446`). The watcher is currently consumed by the dashboard, not terminal/daemon startup; real-Windows validation and terminal wiring remain. |
| Overlay/privacy | Equivalent | Bluey native overlay/daemon model avoids a broad Electron capture bridge; validate Windows capture exclusion separately. |
| Local IPC | Bluey stronger | owner-only named pipe, SID/session peer checks, bearer/replay/bounds/deadlines (`ipc_auth.rs:589-871`; `app.rs:2080-2235`). |
| Secure tokens | Bluey stronger | secure account store plus CAS and stable owner scope (`crates/cue-cloud-client/src/tokens.rs:57-65,272-504`). |
| Workspaces/modes | Bluey stronger | revisioned owner-scoped workspaces (`workspace.rs:21-166`; `workspace_store.rs:66-273`) and seven-mode Coach UI (`Coach.tsx:85-489`). |
| Jobs/ATS/browser profiles/receipts | Bluey stronger | provider adapters, encrypted profiles, irreversible submit authority, leases, and receipts; Cluely has none. |
| Updater publisher pin | Partial | Cluely's signed artifact is evidence, not an in-app pin. Implement Bluey WinVerifyTrust/signature manifest checks based on Final Round's concept. |

Smallest production-quality changes: sign and publisher-pin Bluey's helper; test
the implemented event-driven WASAPI/resampler path on real Windows; add automatic
device-rebind and suspend/resume recovery; then add debounced meeting-session
detection. Do not import Cluely's
Electron bundles, renderer assets, service identifiers, or helper bytes.
