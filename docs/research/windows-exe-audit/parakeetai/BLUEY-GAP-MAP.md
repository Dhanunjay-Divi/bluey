# ParakeetAI 3.7.0 Windows versus Bluey

| Capability | Status | Bluey evidence and decision |
|---|---|---|
| WASAPI mic/system capture | Equivalent design, runtime unvalidated | Bluey's helper captures both default endpoints using event callbacks and a 100 ms buffer (`native/windows/cue-audio/main.c:285-352,366-445`). Its 64-tap/256-phase polyphase resampler passes host 48 kHz/44.1 kHz passband and 12 kHz stopband tests (`resampler.c:10-58,69-113`; `resampler_test.c:41-79`). Real-device, drift, device-change, and automatic-recovery tests remain. |
| Bounded AEC | Missing | Independently implement WebRTC AEC with Parakeet's evidenced 10 ms/80 ms/200 ms bounds as benchmark hypotheses, not copied source. |
| Active-mic meeting hints | Implemented statically for dashboard; terminal partial | Bluey-owned Rust now enumerates active eCapture sessions, deduplicates PIDs, resolves process identities read-only, emits content-free allowlisted labels, and applies stricter browser debounce plus ambiguous-scan fail-safe behavior (`meeting_detect.rs:41-188,269-446`). Dashboard consumption exists; terminal/daemon wiring and real-Windows canaries remain. |
| Native trust boundary | Bluey stronger design | Bluey uses a separate helper and rejects helpers without matching integrity metadata (`app.rs:4986-5040+`); finish signed-manifest/publisher verification. |
| Secure tokens | Bluey stronger | Bluey secure account store, stable account ID and refresh CAS (`tokens.rs:57-65,272-504`). |
| Local IPC | Bluey stronger | owner-only named pipe + SID/session + bearer/replay/bounds (`ipc_auth.rs:589-871`). |
| Workspaces/Coach | Bluey stronger | owner/revision model and seven modes (`workspace.rs:21-166`; `Coach.tsx:85-489`). |
| Job automation | Bluey stronger | ATS adapters, browser identity, leases, irreversible marker, receipts; Parakeet has none. |
| Updater publisher pin | Partial | add native publisher verification and signed manifest/rollback, following Final Round's safer concept. |

P0: helper signing and real Windows audio canaries. P1: validate and wire the
new debounced capture-session detector into the terminal runtime, then benchmark
AEC. Reject direct reuse of Parakeet's source/binaries,
renderer, endpoints, updater identity, or model assets pending separate provenance
and dependency review.
