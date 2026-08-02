# Windows installer audit index

## Scope and evidence standard

This audit covers the five owner-supplied Windows installers in
`/Users/uno/Downloads/exe_backtrack_code`. It is a static audit: no installer,
application, bundled helper, package hook, or recovered program was executed.
Installers were parsed as NSIS/7z containers; PE metadata and Authenticode were
inspected; `app.asar` archives were mechanically expanded with the repository's
already-installed `@electron/asar` parser. Network addresses and behavior below
come from packaged resources, not live traffic.

`Observed` means a concrete packaged file, code path, PE field, signature result,
or updater manifest supports the statement. `Inferred` is explicitly marked.
Server implementations, live authorization, latency, backend retention, and
effective UI behavior remain unknown. Marketing text is not evidence.

The exact retained trees are under
`/Users/uno/Downloads/exe_backtrack_code/recovered`. The recovery agent's
top-level [INDEX](/Users/uno/Downloads/exe_backtrack_code/recovered/INDEX.md) and
[PROVENANCE](/Users/uno/Downloads/exe_backtrack_code/recovered/PROVENANCE.md)
bind each exact payload, ASAR, and source-map reconstruction to its installer.

## Installer and payload identity

| Product | Installer SHA-256 | Installer | App payloads | ASAR SHA-256 | Authenticode result |
|---|---|---:|---|---|---|
| [Cluely 2.0.193](cluely/MANIFEST.md) | `827b46fe…e62` | 222,071,256 bytes | Windows x64 + ARM64 | `607c9294…3e7` on both architectures | Installer and app digest/chain verified; Cluely Inc |
| [Littlebird 0.81.10](littlebird/MANIFEST.md) | `2e202cde…25f` | 334,080,984 bytes | Windows x64 | `dc51fb4f…1a0` | Installer, app, capture helper, and bundled `rg.exe` verified; LITTLE BIRD SOFTWARE LLC |
| [LockedIn 1.8.8](lockedin/MANIFEST.md) | `0240d187…cb1` | 172,231,944 bytes | Windows x64 | `e9d560c8…d5b` | Installer, app, and WinKeyServer verified; Cyber Gravity LLC |
| [ParakeetAI 3.7.0](parakeetai/MANIFEST.md) | `63b29b10…977` | 224,467,424 bytes | Windows x64 + ARM64 | `c46e9111…26c` on both architectures | Authenticode digest matches; Microsoft ID Verified chain is unavailable in the local macOS CA bundle, so trust remains unverified |
| [Final Round 2.4.0](final-round/MANIFEST.md) | `74410be0…ffe` | 209,350,488 bytes | Windows x64 | `6ffc3dbd…3f0` | Installer and app digest/chain verified; Final Round AI, Inc |

All five installers are PE32 NSIS self-extracting installers. Every main
application is an Electron/Chromium PE32+ GUI with subsystem version 10.0.
Cluely and ParakeetAI contain separate x64 and ARM64 app archives whose ASAR
bytes are identical across architectures. The other three are x64-only in the
supplied installers. See each app's `hashes.txt` and `evidence/STATIC-EVIDENCE.md`.

## Windows-only findings that should shape Bluey

1. **ParakeetAI contains the clearest reusable architecture idea.** Its exact
   packaged Rust source enumerates active Windows capture sessions through
   MMDevice/`IAudioSessionManager2`, and its WebRTC/Sonora AEC uses 10 ms frames,
   an 80 ms microphone alignment delay, a 200 ms bounded queue, drop-oldest
   backpressure, and a two-second microphone-only bypass. This is a pattern to
   independently implement and benchmark, not code to import.
2. **Littlebird has the best helper supervision.** Its signed .NET 8 helper owns
   Windows window/screen/OCR capture and WASAPI mic/loopback. Electron exchanges
   framed, schema-validated JSON over child stdin/stdout, verifies stale PIDs by
   executable name, replays critical state after a crash, and stops after five
   exponentially delayed restarts. Weaknesses: no evident frame byte ceiling and
   the helper inherits the entire parent environment.
3. **Final Round has the strongest updater check.** It verifies the downloaded
   Windows installer with `Get-AuthenticodeSignature`, pins the publisher, fails
   closed, and defers update dialogs while a live session is busy. Its current
   PowerShell command interpolates the path into script text, so Bluey should use
   a native WinVerifyTrust path or argument-safe verifier instead.
4. **Native-addon trust is inconsistent.** Final Round's three high-privilege
   `.node` addons and ParakeetAI's N-API binaries are unsigned even though their
   enclosing app is signed. LockedIn exposes remote mouse/keyboard injection to
   a broad renderer bridge. Bluey should retain an independently signed/hashed
   helper boundary, narrow commands, sender authentication, and no renderer path
   to arbitrary input injection.
5. **Bluey is already stronger on local IPC.** The current tree uses an
   owner-only, remote-client-rejecting Windows named pipe, per-boot bearer,
   request IDs/replay detection, peer SID and session validation, 256 KiB bounds,
   three-second deadlines, and a 64-connection limit
   (`crates/cue-core/src/ipc_auth.rs:21-29,589-871`;
   `crates/cue-daemon/src/app.rs:1816-1831,2080-2235`). None of the five products
   provides evidence of a stronger local command boundary.
6. **Bluey's Windows audio path has advanced since the static product audit.**
   This is Bluey implementation evidence, not an observation about a recovered
   product: the helper targets Windows 10, uses event-driven shared-mode WASAPI
   for microphone or render-loopback capture with a 100 ms jitter buffer, drains
   all available packets per wakeup, and classifies endpoint/service invalidation
   errors as recoverable (`native/windows/cue-audio/main.c:3,233-245,285-352,366-445`).
   Its 64-tap, 256-phase Blackman-windowed polyphase-sinc resampler is implemented
   in `native/windows/cue-audio/resampler.h:7-29` and
   `native/windows/cue-audio/resampler.c:10-58,69-113`; coefficients are
   precomputed at initialization, so the live push path has no trigonometry or
   heap allocation. The host DSP test
   (`resampler_test.c:41-79`) produced 15,979 samples from 48 kHz, 15,977 from
   44.1 kHz, 0.353556 passband RMS, and 0.000022 RMS for a 12 kHz stopband tone;
   a MinGW x64 compile produced a PE32+ console binary with subsystem 10.0.
   Those compile/unit results do not establish real-device behavior, automatic
   endpoint rebinding, suspend/resume recovery, long-run stability, or AEC.

## Cross-product capability matrix

| Capability | Cluely | Littlebird | LockedIn | ParakeetAI | Final Round | Bluey position |
|---|---|---|---|---|---|---|
| Windows mic + system audio | SoX mic + Chromium loopback | Signed .NET WASAPI mic/loopback | Renderer mic + Chromium loopback | Chromium loopback + native AEC | Native WASAPI mic/loopback + WebRTC APM | Implemented statically: event-driven WASAPI mic/render-loopback, 100 ms buffer, structured device-loss errors, and tested 64-tap/256-phase polyphase resampling; real-device Windows canaries, automatic recovery, and AEC remain |
| Meeting detection | Calendar/session driven | Helper context + meeting events | Session/manual flows | Active capture-session detection | Native capture-device polling | Implemented statically for the dashboard consumer: Windows eCapture sessions, PID deduplication, exact process allowlist, native/browser debounce, and ambiguous-scan fail-safe (`meeting_detect.rs:41-188,269-446`); terminal/daemon wiring and real-Windows proof remain |
| Context/OCR | Screenshot | Window/screen/OCR and contextual memory | Screenshots/documents | Screenshot | Screenshot, coding/system-design | Partial; Bluey has screenshot/page context but no Windows OCR helper |
| Helper crash recovery | Basic process lifecycle | Strong bounded-retry state replay | Audio watchdog is macOS-only | Utility-process stop/restart boundary | Session state machine + audio pause/resume | Partial; Bluey supervises its audio helper but needs Windows long-run canaries |
| Credential protection | Chromium session; no app `safeStorage` found | Electron/Chromium stores; redacted logging | Firebase/Clerk/browser stores | `safeStorage`, fail-closed when unavailable | `safeStorage` with legacy fallback paths | Bluey stronger: secure account store, CAS refresh, stable account scope (`tokens.rs:57-65,272-504`) |
| Local IPC privilege | Broad generic preload | Typed but large preload + child protocol | Very broad generic bridge; remote input | Generic invoke/on bridge | Typed gateway, identical preload surface | Bluey stronger: authenticated owner-only named pipe |
| Signed update enforcement | electron-updater + signed installer | electron-updater + signed installer | publisher-configured updater | GitHub updater; local chain unverified | Publisher-pinned post-download verification | Partial: Bluey needs a signed manifest plus native publisher verification |
| Job discovery/ATS/browser automation | Not observed | Not observed | Not observed | Not observed | Not observed | Bluey stronger |
| Resume tailoring/diff/export | Not observed | General artifacts only | Resume/document context only | Resume context only | Interview documents only | Bluey stronger |
| Durable job leases/idempotency/receipts | Not observed | Not observed | Not observed | Not observed | Not observed | Bluey stronger |

Bluey's job-system evidence includes provider adapters
(`jobs/automation/src/standard-adapters.ts:47-98,237-309`), CAPTCHA/takeover
(`jobs/automation/src/challenge-handling.ts:55-84`), exclusive submit authority
(`jobs/browser/src/irreversible-submit.ts:63-171`), execution leases
(`jobs/runner/src/execution-lease.ts:167-230`), encrypted profiles
(`jobs/runner/src/profile-store.ts:14-66`), and evidence-backed receipts
(`jobs/automation/src/receipts.ts:101-199`).

## Recommended Bluey implementation order

### P0

- Finish and run Windows x64/ARM64 named-pipe ACL, peer-SID/session, stale-boot,
  replay, slowloris, oversized-request, and wrong-user canaries on real Windows.
- Sign the Windows audio helper; verify publisher plus SHA-256 from a signed
  release manifest before every launch. Do not trust an adjacent unsigned JSON
  file as the release root of trust.
- Promote the new event-driven WASAPI/resampler path only after real-Windows
  validation (`native/windows/cue-audio/main.c:3,233-245,285-352,360-445`;
  `resampler.h:7-29`; `resampler.c:10-58,69-113`; `resampler_test.c:41-79`).
  The host DSP test and MinGW subsystem-10 compile pass; still exercise physical
  endpoints, device changes, Bluetooth, suspend/resume, silence continuity,
  teardown, backpressure, and long runs. Structured recoverable errors are
  implemented, but endpoint rebinding/restart after those errors is not proven.
- Add content-free logs and strict bounded stdout/stderr for every helper. Never
  inherit unrelated secrets into child processes.

### P1

- Run the new Windows capture-session hints on real Windows and wire the watcher
  into the terminal/daemon runtime before claiming terminal benefit. Preserve
  the exact allowlist, multi-sample start/stop debounce, ambiguous-scan fail-safe,
  user-visible confirmation, and stricter browser policy
  (`meeting_detect.rs:41-188,269-446`).
- Benchmark an optional WebRTC AEC stage using bounded 10 ms frames against the
  existing raw helper; preserve a bypass for headset/mic-only cases.
- Add Windows window/OCR context behind explicit per-app/domain exclusions and
  visible capture controls; retain only derived references by default.
- Add fail-closed WinVerifyTrust publisher verification for updates and helpers,
  monotonic versions, staged rollout, rollback, and in-session deferral.

### P2

- Add Windows ARM64 builds after native dependency/SBOM/signing parity is proven.
- Consider phone/helper collaboration only with explicit pairing, expiry,
  consent, authenticated commands, and no general remote-input bridge.

## Unknowns requiring controlled Windows validation

- Real capture latency, device switching, Bluetooth behavior, suspend/resume,
  multi-monitor DPI, and screen-share exclusion on supported Windows versions.
- Windows SmartScreen reputation and revocation results. The offline verifier
  established exact Authenticode digests; only four chains resolved through the
  local macOS trust bundle.
- Server authorization, storage, deletion, billing enforcement, and telemetry
  scrubbing for all five products.
- Comparative speed. Static bundle size and architecture do not prove latency.

No recovered product code was copied into Bluey by this audit.
