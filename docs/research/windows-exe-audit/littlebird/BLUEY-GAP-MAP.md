# Littlebird 0.81.10 Windows versus Bluey

| Capability | Status | Bluey evidence and decision |
|---|---|---|
| Durable assistant workspaces | Equivalent/Bluey narrower | Bluey now has owner-scoped revisioned workspaces and reference-only activity/context/artifacts (`workspace.rs:21-166`; `workspace_store.rs:66-321`). Continue UX cohesion, not data duplication. |
| Windows context/OCR | Missing | Littlebird's signed helper proves the value of window/OCR context. Build a Bluey Rust helper with exclusions, consent, bounds, and no raw-retention default. |
| Windows WASAPI audio | Implemented statically, runtime unvalidated | Bluey captures default mic/render loopback with event callbacks and a 100 ms buffer (`native/windows/cue-audio/main.c:285-352,366-445`) and has a host-tested 64-tap/256-phase polyphase resampler (`resampler.c:10-58,69-113`; `resampler_test.c:41-79`). Add device selection/automatic recovery and real-device tests. |
| Helper supervision | Partial | Bluey bounds readiness output/time (`app.rs:4675-4815`) and validates helper integrity (`app.rs:4986-5040+`). Add Littlebird-style critical-state replay and exponential crash budget to continuous capture. |
| Secret-safe logs | Equivalent/needs canary | Bluey has redaction-safe observability and secure token store; add synthetic no-content log tests. Adapt Littlebird's correlation hashes only for non-secret event IDs. |
| Local IPC | Bluey stronger | owner-only named pipe plus peer SID/session and bearer/replay controls (`ipc_auth.rs:589-871`). Littlebird child stdio is private by parentage but not capability-authenticated. |
| Jobs automation | Bluey stronger | ATS adapters, leases, encrypted profiles, submit authority, receipts. Littlebird has none. |
| Calendar/email outcomes | Partial | Littlebird has broader integrations; Bluey should first ship one read-only inbox connector with durable cursor/provenance/deletion. |
| Update trust | Partial | Both need release-quality signed manifests/rollback. Bluey should combine Final Round publisher pinning with a native verifier. |

P0: signed helper/publisher trust plus real-Windows proof of the implemented
minimal environment and bounded frames/restarts. P1: Windows
OCR/context with exclusions plus real-Windows validation and terminal wiring for
the implemented capture-session hints. P2: broader
workspace integrations only after deletion and content-free telemetry canaries.
Do not import Littlebird maps, UI code/assets, API contracts, or helper bytes.
