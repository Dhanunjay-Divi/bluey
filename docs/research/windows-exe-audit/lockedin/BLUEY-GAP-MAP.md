# LockedIn 1.8.8 Windows versus Bluey

| Capability | Status | Bluey evidence and decision |
|---|---|---|
| Windows loopback audio | Equivalent design, runtime unvalidated | Bluey's event-driven native WASAPI helper (`native/windows/cue-audio/main.c:285-352,366-445`) avoids renderer display-media dependence; its host-tested 64-tap/256-phase polyphase resampler is in `resampler.c:10-58,69-113`. Finish real-device and automatic-recovery canaries. |
| Screenshot/document interview context | Equivalent/Partial UX | Bluey has screenshot context and Coach artifacts; continue structured coding/system-design presentation without importing LockedIn assets. |
| Session recovery | Equivalent | Bluey meeting/workspace records are durable and revisioned (`workspace_store.rs:66-321`). Test crash/restart on Windows. |
| Helper collaboration | Reject implementation | LockedIn's renderer-to-robotjs path is too broad. Bluey takeover remains scoped to an application browser with explicit user intervention. |
| Global shortcuts | Partial | Implement narrow Windows shortcuts in a signed helper; do not expose arbitrary key injection. |
| Local IPC | Bluey stronger | named-pipe owner ACL, peer SID/session, capability, replay, bounds (`ipc_auth.rs:589-871`). |
| Secret storage | Bluey stronger | secure account store and refresh CAS (`tokens.rs:57-65,272-504`). |
| ATS/job execution | Bluey stronger | standard adapters, leases, submit markers, encrypted profile, receipts. |
| Signed addons | Missing release proof | Bluey plans helper hash metadata but needs a signed root manifest and publisher verification before launch. |

P0: forbid general remote input, sign/pin Bluey helpers, exercise Windows daemon
and audio recovery. P1: improve interview document/screenshot UI and narrow
hotkeys. Direct reuse of LockedIn's renderer, extension, robotjs modules, native
bytes, backend contracts, or configuration is rejected.
