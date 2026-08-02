# Bluey Windows paid-alpha readiness

Status: **not yet production-ready for Windows**. The current tree implements
the core Windows architecture and passes host-side static checks, but a paid
Windows release remains blocked on Authenticode publisher trust and real
Windows 10/11 canaries.

This is the Windows parallel track for the same Bluey account, billing,
provider-routing, release, support, and storage systems used on macOS. There is
no separate Windows backend.

## Product boundary

Bluey is a terminal product with a background daemon and small native helpers:

```text
bluey.exe
  -> owner-only authenticated named pipe
  -> bluey-daemon.exe
       -> integrity-checked WASAPI audio helper
       -> integrity-checked virtual-desktop capture helper
       -> existing Bluey cloud, Jobs, workspace, and Coach contracts
```

Bluey is not distributed as an Electron, WebView, WPF, or other desktop GUI
application. Therefore, GUI application packaging, GUI installer signing, and
Apple notarization are not Windows release requirements. Windows executable
trust is still mandatory: `bluey.exe`, `bluey-daemon.exe`, and every shipped
native `.exe` helper must be Authenticode-signed by the approved publisher and
timestamped.

This Round 510 slice adds no Keychain or Windows Credential Manager dependency.
The release gate in this document does not require either facility.

## Implemented and host-verified

The following exists in the current tree. “Host-verified” means inspected,
unit-tested, or cross-compiled on the macOS analysis host; it does not mean the
Windows-only branch has executed on Windows.

| Area | Current implementation | Verification boundary |
| --- | --- | --- |
| Local IPC | Per-user Windows named pipe, current-user-only capability-file DACL, remote-client rejection, peer SID and logon-session validation, per-boot bearer, request IDs, replay bound, 256 KiB framing bounds, deadlines, and connection capacity | Platform-neutral framing/auth tests and Windows-target Rust compilation passed; real Windows ACL and peer-process behavior remain blocked |
| Screen capture | Native C++ helper captures the bounded full virtual desktop, including negative monitor coordinates, with per-monitor DPI awareness and structured diagnostics | MinGW x64 warnings-as-errors cross-compile passed as a Windows 10-subsystem console PE; real multi-monitor, mixed-DPI, HDR, and secure-desktop behavior remain blocked |
| Audio | Native event-driven WASAPI microphone/render-loopback helper with structured device-loss errors, a 100 ms jitter buffer, and a 64-tap/256-phase windowed-sinc resampler | Host DSP tests and MinGW x64 cross-compile passed; physical-device latency, endpoint changes, Bluetooth, and suspend/resume remain blocked |
| Meeting hints | Windows eCapture session enumeration with PID deduplication, read-only process identity, exact Teams/Zoom/Webex/Slack/Discord/browser labels, conservative debounce, and ambiguous-scan fail-safe behavior | Pure classification/debounce tests and Windows-target strict Clippy passed; only the dashboard currently consumes the watcher, so terminal/daemon wiring and real-Windows MMDevice proof remain blocked |
| Package contents | Windows build collects CLI, daemon, native helpers, policy notice, aliases, and a bounded per-file SHA-256/size integrity manifest | Build script and manifest writer are present; PowerShell parsing and execution still need Windows validation |
| Install/update safety | Artifact hash check, inner file-manifest verification, required-file enforcement, owner-only bin ACL, staged replacement, startup canary, and previous-version rollback | Implementation is present; clean install/update/failure-injection tests still need Windows validation |
| Release hygiene | Publication rejects source maps, debug/source files, ASAR, unsafe archive paths, links/special files, case collisions, excessive expansion, configured secret bytes, and production-unsafe flags | Scanner self-tests, scoped hygiene scan, shell syntax checks, and scoped diff checks passed |

Primary implementation evidence:

- Windows IPC and owner verification:
  `crates/cue-core/src/ipc_auth.rs`, `crates/cue-daemon/src/app.rs`, and
  `crates/cue-cli/src/app.rs`.
- Native capture: `native/windows/cue-capture/main.cpp` and
  `native/windows/cue-capture/build.ps1`.
- Native audio and resampling: `native/windows/cue-audio/main.c`,
  `native/windows/cue-audio/resampler.c`, and
  `native/windows/cue-audio/resampler.h`.
- Packaging and install: `scripts/build-windows.ps1`,
  `scripts/write-windows-integrity.ps1`, and `ops/install/install.ps1`.
- Release hygiene: `scripts/check-release-artifact-contents.py` and
  `scripts/publish-bluey-release.sh`.

See [Round 510](../rounds/ROUND-510-BLUEY-WINDOWS-EXE-AUDIT-NATIVE-CAPTURE-IPC-AND-RELEASE-HARDENING.md)
for exact source locations, observed commands, and the implementation handoff.
The comparative evidence is in the
[Windows installer audit](../research/windows-exe-audit/INDEX.md).

## Hard P0 release gates

Every item below must pass before any paid Windows user is invited.

### Publisher and distribution trust

- Produce one canonical, source-free Windows x64 archive through
  `scripts/build-windows.ps1` and run the release artifact scanner on the final
  archive.
- Authenticode-sign `bluey.exe`, `bluey-daemon.exe`, and every shipped native
  executable/helper with the same approved publisher and a trusted timestamp.
- Verify the publisher and signed release-manifest digest before install and
  before launching a privileged helper. The adjacent integrity JSON is useful
  tamper evidence, but is not a signed root of trust by itself.
- Validate certificate chain, revocation and offline policy, timestamp behavior,
  helper replacement rejection, Defender results, and SmartScreen reputation.
- Publish the artifact SHA-256 and signed update-manifest entry. Update paths
  must fail closed when signed-manifest handoff is absent or invalid.

### Real Windows 10/11 canaries

Run on clean, standard-user Windows 10 and Windows 11 hosts. Record OS build,
architecture, artifact hashes, content-free pass/fail codes, and durations.

- **Named pipe and capability ACL:** prove current-user ownership with Windows
  APIs and `icacls`; reject another user, another logon session, remote clients,
  stale boot state, forged bearer, replay, oversized input/output, half-open
  clients, capacity saturation, reparse/replacement attempts, and alternate
  address transport.
- **Audio:** exercise real microphones and render-loopback devices, silence,
  device switching, endpoint loss, exclusive-mode conflicts, Bluetooth, Remote
  Desktop, sleep/resume, clean teardown, and long-run capture. Record
  audio-to-answer p50/p95/p99 plus CPU and memory.
- **Meeting hints:** validate active eCapture enumeration, process identities,
  debounce, browser churn, ambiguous permission/device failures, and the future
  terminal/daemon confirmation UX without recording audio, paths, or PIDs.
- **Capture:** exercise single- and multi-monitor layouts, negative coordinates,
  mixed DPI at 100/125/150%, HDR/scaling, output bounds, locked/UAC/secure
  desktop behavior, protected windows, and the consent/integrity path.
- **Install and rollback:** exercise clean install, in-place update, running
  daemon replacement, forced failure at every staged transition, byte-for-byte
  previous-version restoration, PATH behavior, uninstall, non-admin operation,
  disk-full behavior, and antivirus file locks.
- **Lifecycle and privacy:** exercise start, status, sign-in, screen capture,
  audio readiness, listen, answer, stop, update, restart, and uninstall. Verify
  that logs, crash output, telemetry, temporary directories, and failed-install
  backups contain no screenshot/audio payloads, tokens, provider keys, device
  codes, or unnecessary user paths.
- **Soak and recovery:** run 8-, 24-, and 72-hour sessions; inject helper and
  daemon failures; verify bounded restart behavior, state recovery, and
  content-free diagnostics.

### Architecture support

- Windows x64 is the only concrete release target in the current build path.
- An ARM64 host may use the x64 compatibility build only after that fallback is
  tested on real ARM64 Windows hardware.
- Do not publish native Windows ARM64 until Rust binaries, every native helper,
  dependency/SBOM generation, Authenticode signing, install/update behavior, and
  the complete canary matrix have ARM64 parity.

## Explicitly not required for this release

- No Electron or other GUI runtime.
- No GUI-specific installer or application signing track.
- No Apple notarization; it does not apply to Windows.
- No new Keychain or Windows Credential Manager integration.
- No native ARM64 artifact until the parity gate above is complete.
- No claim that source-map removal or embedded policy text makes binaries
  impossible to analyze. Server-side authority, minimal shipped secrets,
  signed artifacts, and operational enforcement remain the effective boundary.

## P1 after the paid-alpha gate

- Wire the implemented Windows meeting-hint watcher into the terminal/daemon
  runtime with visible confirmation, then validate MMDevice/process behavior on
  real Windows without weakening its allowlist, debounce, or browser safeguards.
- Benchmark optional bounded-frame AEC against raw WASAPI capture; preserve a
  headset/mic-only bypass and ship only if measured quality improves.
- Add signed native window selection/OCR context with explicit exclusions,
  capture controls, strict byte/frame/deadline bounds, and derived-reference-only
  retention by default.
- Add automatic endpoint-change recovery, silence continuity, and bounded helper
  crash-budget/state replay after real-device evidence identifies the required
  behavior.

## Operator inputs

- Clean standard-user Windows 10 and Windows 11 x64 test hosts.
- A Windows ARM64 host for compatibility-fallback evidence, if ARM64 users are
  in the alpha cohort.
- Visual Studio Build Tools and the approved Rust toolchain.
- An approved Authenticode code-signing certificate and trusted timestamp
  service.
- A real Bluey test account with credits plus representative microphone,
  loopback, Bluetooth, multi-monitor, mixed-DPI, Defender, and SmartScreen test
  conditions.

## Launch rule

Windows may join the paid alpha only after every P0 gate above has concrete,
retained evidence. Until then, the Windows architecture is implemented and
host-checked, but the production release remains blocked. A macOS-only paid
alpha is unaffected by this Windows parallel track.
