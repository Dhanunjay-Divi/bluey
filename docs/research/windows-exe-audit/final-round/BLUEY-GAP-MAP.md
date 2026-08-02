# Final Round 2.4.0 Windows versus Bluey

| Capability | Status | Bluey evidence and decision |
|---|---|---|
| Native Windows mic/loopback | Implemented statically, runtime unvalidated | Bluey has event-driven direct WASAPI with a 100 ms buffer (`native/windows/cue-audio/main.c:285-352,366-445`) and a host-tested 64-tap/256-phase polyphase-sinc resampler (`resampler.c:10-58,69-113`; `resampler_test.c:41-79`). Add automatic device-change/suspend recovery and real latency tests. |
| AEC/silence continuity | Missing | Benchmark WebRTC APM/AEC and bounded silence filling; preserve bypass and content-free metrics. |
| Structured coding/system design UI | Partial | Bluey Coach/artifacts exist (`Coach.tsx:85-489`); add Approach/Code/Complexity/Tests panels over existing contracts. |
| Session lifecycle/reconnect | Equivalent/Partial Windows proof | Bluey durable meeting/workspace model plus supervised daemon; run crash/network/update canaries. |
| Publisher-pinned updates | Missing/Partial | Adapt concept, using native WinVerifyTrust plus signed manifest and rollback rather than interpolated PowerShell. |
| Native helper trust | Bluey stronger design | Bluey is out-of-process and integrity-gated (`app.rs:4986-5040+`); must make the manifest signed and verify publisher. |
| Local IPC | Bluey stronger | owner-only named pipe, SID/session, bearer/replay, hard bounds (`ipc_auth.rs:589-871`; `app.rs:2080-2235`). |
| Jobs automation | Bluey stronger | adapters, profile isolation/encryption, leases, submit marker, receipts; Final Round has none. |
| Secure account scope | Bluey stronger | OS secure account store with CAS/stable ID (`tokens.rs:57-65,272-504`). |
| Video/phone coaching | Missing/optional | Begin with audio-first mock interview using existing Jobs prep context; add external audio only with pairing/consent/expiry. |

P0: signed helper/update trust and Windows audio recovery. P1: AEC benchmark,
structured interview presentation, and real-Windows/terminal integration for the
implemented meeting hints. P2: audio-first mock coach.
Reject direct reuse of Final Round renderer/addon/model bytes, service contracts,
telemetry identifiers, or updater code.
