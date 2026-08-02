# Round 510 — Bluey Windows EXE audit, native capture, IPC, and release hardening

Date: 2026-07-12

Repository: `/Users/uno/Downloads/cue-bluey-jobs`

Branch: `codex/bluey-jobs-20260710`
Status: static Windows research and the current-tree implementation slice are complete; a Windows release remains blocked on signed artifacts and real Windows canaries

Research lineage: the complete [Windows installer audit](../research/windows-exe-audit/INDEX.md), the retained [recovery index](/Users/uno/Downloads/exe_backtrack_code/recovered/INDEX.md), and [recovery provenance](/Users/uno/Downloads/exe_backtrack_code/recovered/PROVENANCE.md) are the evidence base for this round. [Round 509](ROUND-509-BLUEY-WORKSPACES-READINESS-AND-LOCAL-SECURITY.md) records the preceding workspace, readiness, credential, and authenticated-loopback implementation.

## Executive summary

Five owner-supplied Windows installers were statically recovered and compared with Bluey: Cluely 2.0.193, Littlebird 0.81.10, LockedIn 1.8.8, ParakeetAI 3.7.0, and Final Round 2.4.0. All five are Electron applications delivered through NSIS. The recovery retained the exact installers and payload members, mechanically extracted ASAR contents, source-map material where present, hash ledgers, signing evidence, and a separate clean-room behavior layer. No recovered installer, app, helper, lifecycle hook, or bundled script was executed.

The useful Windows ideas are complementary:

1. ParakeetAI provides the clearest active-microphone-session and bounded-AEC design evidence.
2. Littlebird provides the strongest helper supervision, state replay, OCR/context, and coherent workspace model.
3. Final Round provides the strongest low-latency session lifecycle, narrow renderer sender checks, structured interview panels, and publisher-pinned update concept.
4. LockedIn demonstrates Windows DPI handling and collaboration mechanics, but its broad renderer-to-input-injection path should be rejected.
5. Cluely demonstrates a comparatively small atomic state layer and basic Windows audio/loopback lifecycle.

Bluey is already materially stronger in the area the five products do not cover: job discovery, ATS-specific execution, encrypted browser-profile isolation, human challenge takeover, irreversible-submit authority, durable leases and duplicate prevention, evidence-backed receipts, and Jobs-to-Coach provenance. This round therefore did not turn Bluey into another Electron desktop bundle. It kept the product's terminal/daemon/native-helper architecture and implemented the smallest Windows-native safety and capture slice:

```text
bluey.exe
  -> per-user authenticated named pipe
  -> bluey-daemon.exe
       -> integrity-checked WASAPI audio helper
       -> integrity-checked native virtual-desktop capture helper
       -> existing meeting / workspace / Coach / Jobs contracts
```

The current tree now contains owner-only Windows named-pipe IPC with SID/session peer validation, a native DPI-aware multi-monitor PNG capture helper, a windowed-sinc WASAPI resampler, package integrity metadata, staged install rollback, and a release-artifact scanner that rejects source maps/debug/source material and configured secrets. It also embeds a machine-readable provenance/policy notice. These controls reduce avoidable disclosure and local attack surface; they do not make native binaries impossible to inspect.

Bluey is a terminal application, so this round does not require signed/notarized GUI packaging. It does still require Authenticode-signed executable/helper artifacts, publisher verification, SmartScreen reputation work, and real Windows 10/11 runtime canaries before a production Windows release can be called ready. No new Keychain or Windows Credential Manager dependency was added in this slice.

## Evidence standard and scope

`Observed` below means that an exact installer/payload file, extracted bundle resource, PE/signature result, source-map entry, packaged native source file, or Bluey source location directly supports the claim. `Inferred` means an engineering conclusion derived from those observations and is labeled as such. Static bundle contents do not prove live latency, server authorization, backend retention, billing enforcement, or runtime UI quality.

The installer SHA-256 values are:

| Product | Version | Installer SHA-256 | Payload architecture |
| --- | ---: | --- | --- |
| Cluely | 2.0.193 | `827b46feccd68e18fa41ef840da03102c8c38f381626b1e6bb899d75a5582e62` | x64 + ARM64 |
| Littlebird | 0.81.10 | `2e202cde30b2d8cedf4295bf4d6160a044797086f2083f7afb07d01a9fcf525f` | x64 |
| LockedIn | 1.8.8 | `0240d1872560bf5c49a9372fc2919f8d698b667d80bd2e203b316669c3c9acb1` | x64 |
| ParakeetAI | 3.7.0 | `63b29b103609417d5b93370b415a1abcfe2c611439b60e659b37bab44a273977` | x64 + ARM64 |
| Final Round | 2.4.0 | `74410be0e746553441b9542fd5fc5c8719554139707f167a4fc51ee59de65ffe` | x64 |

The exact identity, ASAR hashes, PE subsystem, signer results, updater configuration, helper inventory, and evidence commands are in each application's `MANIFEST.md`, `hashes.txt`, and `evidence/STATIC-EVIDENCE.md` beneath the [audit index](../research/windows-exe-audit/INDEX.md).

## Observed facts

### Installer and recovery facts

- All five inputs are PE32 NSIS installers containing Electron/Chromium applications. Their main application executables are PE32+ GUI programs with subsystem version 10.0 ([audit index](../research/windows-exe-audit/INDEX.md)).
- Cluely and ParakeetAI contain x64 and ARM64 payloads whose ASAR bytes are identical across architectures. Littlebird, LockedIn, and Final Round are x64-only in the supplied installers.
- Authenticode digest and chain verification succeeded locally for the Cluely, Littlebird, LockedIn, and Final Round installer/main chains. ParakeetAI's embedded digest matched, but its Microsoft ID Verified chain could not be resolved with the macOS host's CA bundle; that result is unverified, not a demonstrated invalid signature.
- The versioned recovery contains 140,551 files and 8,197,738,496 bytes: 1,070 exact installer/payload members, 135,427 mechanically extracted ASAR files, and 3,990 source-map materializations. The separate reform tree contains 23 files and 3,465,216 bytes ([recovery index](/Users/uno/Downloads/exe_backtrack_code/recovered/INDEX.md)).
- Littlebird contains 705 first-party maps, 4,350 source relationships, and 3,980 unique materialized source files. The recovery validation syntax-checked 932 TS/TSX files. ParakeetAI contains the actual packaged Rust source for its native activity/audio package.
- Server implementations, private infrastructure, signing keys, absent native source/PDBs, deleted pre-minification types/comments/tests, Git history, and dependency source that was never packaged are not recoverable from these installers ([provenance](/Users/uno/Downloads/exe_backtrack_code/recovered/PROVENANCE.md)).

### Product and architecture facts

- ParakeetAI's packaged Rust enumerates active Windows capture sessions through MMDevice and `IAudioSessionManager2`, resolves process identities, and deduplicates process/PID/device observations. Its AEC wrapper uses 10 ms frames, an 80 ms microphone delay, 200 ms bounded queues with drop-oldest behavior, bounded per-call drains, and a two-second microphone-only bypass ([Parakeet architecture](../research/windows-exe-audit/parakeetai/ARCHITECTURE.md)).
- Littlebird's signed .NET 8 helper exposes window/screen/OCR context and WASAPI mic/loopback capture over framed stdin/stdout JSON. The Electron parent schema-validates messages, checks stale PIDs against process identity, replays critical state, and stops after five exponentially delayed restarts. Its helper inherits the full parent environment and no clear per-frame byte ceiling was observed ([Littlebird architecture](../research/windows-exe-audit/littlebird/ARCHITECTURE.md), [data map](../research/windows-exe-audit/littlebird/DATA-AND-NETWORK.md)).
- Final Round's native addons expose WASAPI mic/loopback, WebRTC APM/AEC, pause/resume, capture levels, MMDevice detection, and a global keyboard hook. The app defers updates during live sessions and publisher-checks the downloaded installer. The high-privilege `.node` addons themselves are unsigned, and the publisher check interpolates a path into PowerShell script text ([Final Round architecture](../research/windows-exe-audit/final-round/ARCHITECTURE.md), [security](../research/windows-exe-audit/final-round/SECURITY.md)).
- LockedIn forwards a renderer/WebRTC collaboration channel into broad robotjs mouse/keyboard injection. No sender-frame or session capability is visible at that handler. Bluey should reject that boundary rather than reproduce it ([LockedIn architecture](../research/windows-exe-audit/lockedin/ARCHITECTURE.md), [security](../research/windows-exe-audit/lockedin/SECURITY.md)).
- Cluely uses a re-signed 32-bit SoX helper for microphone input and Chromium display-media loopback for system audio. Its preload remains generic even though main-process handlers add origin checks ([Cluely architecture](../research/windows-exe-audit/cluely/ARCHITECTURE.md)).
- None of the five installers contains evidence of job discovery/ranking, ATS adapters, isolated automated application profiles, irreversible-submit idempotency, application receipts, or end-to-end application outcome correlation. Marketing text was not used to infer any such capability.

### Bluey facts in the current tree

- The IPC protocol keeps a 32-byte per-boot bearer, exhaustive authorization classification, request UUIDs, constant-time credential comparison, 256 KiB request/response constants, a 64-connection limit, and a 4,096-entry replay bound (`crates/cue-core/src/ipc_auth.rs:19-29,31-123,153-220`).
- On Windows, the capability file is created with a protected current-user-only DACL, opened with reparse-point awareness, atomically replaced with write-through, and revalidated (`crates/cue-core/src/ipc_auth.rs:239-264,475-603,826-899`).
- The named-pipe name is derived from a truncated SHA-256 of the current SID. Both daemon and CLI verify the peer process owner SID and logon session through Windows pipe APIs (`crates/cue-core/src/ipc_auth.rs:605-823`).
- The daemon rejects `--addr` on Windows, creates the first owner-only pipe instance with remote clients rejected, prepares the next instance before dispatch, validates every peer, and retains the existing capacity/read-deadline/request-bound/replay authorization path (`crates/cue-daemon/src/app.rs:1816-1835,2042-2060,2083-2232`).
- The CLI uses the per-user pipe, verifies the server SID/session, enforces request/response bounds and a six-second operation timeout, and retains one stale-boot refresh (`crates/cue-cli/src/app.rs:2853-2998`).
- The native screen helper captures the full virtual desktop with per-monitor DPI awareness, validates a local PNG destination, caps capture at 250 million pixels, uses GDI/GDI+ with RAII cleanup, and emits content-free structured diagnostics (`native/windows/cue-capture/main.cpp:15-215`).
- Release Windows capture uses only installed/current-executable helper candidates and verifies the adjacent integrity entry before launch. The PowerShell fallback is debug-only (`crates/cue-daemon/src/app.rs:16095-16202`; `crates/cue-daemon/src/audio/helper_trust.rs:1-89`).
- The WASAPI helper now uses event-driven capture with a 100 ms jitter buffer, validates source and mix format, and converts common PCM/float formats (`native/windows/cue-audio/main.c:40-197,199-471`). Averaging downsampling was replaced with a separately host-testable 64-tap, 256-phase Blackman-windowed sinc resampler; coefficients are precomputed at initialization, so live capture performs no trigonometry or allocation (`native/windows/cue-audio/resampler.h:7-29`; `native/windows/cue-audio/resampler.c:6-113`).
- The Windows build packages terminal, daemon, overlay, audio, capture, policy notice, and aliases into one bin directory, then generates a bounded per-file SHA-256/size manifest (`scripts/build-windows.ps1:11-39`; `scripts/write-windows-integrity.ps1:1-45`).
- The installer validates the artifact hash and inner integrity manifest, requires core files, applies a current-user-only bin ACL, stages the new tree, runs canaries, and restores the previous tree on failure (`ops/install/install.ps1:66-190,335-471`).
- Release publication scans every outgoing artifact for source maps, source/debug files, ASAR, package manifests, unsafe archive paths, links/special files, case collisions, excessive expansion, configured secret bytes, and production-unsafe flags (`scripts/check-release-artifact-contents.py:22-235`; `scripts/publish-bluey-release.sh:43-66`; `scripts/release-hygiene-scan.sh:14-16`).
- The binary and release contain a machine-readable policy/provenance marker and a `bluey legal --json` surface. The marker explicitly identifies itself as notice-only, not DRM (`crates/cue-core/src/legal.rs:3-44`; `crates/cue-cli/src/app.rs:44-51,574-594`; `BLUEY-NOTICE.txt:1-14`).

## Inferences, explicitly labeled

- **Architecture inference:** combining Littlebird-style supervised context, Parakeet-style capture-session hints/AEC bounds, and Final Round-style session/update boundaries with Bluey's existing Jobs and Coach contracts is a stronger product direction than copying any one application wholesale. This is an engineering synthesis, not proof of user preference.
- **Latency inference:** event-driven WASAPI and bounded native helpers should reduce avoidable scheduling and bridge overhead compared with renderer-owned audio, but no comparative p50/p95/p99 measurement exists yet. Static code size and queue constants do not prove that Bluey is faster.
- **Reliability inference:** the owner-only named pipe, peer verification, package manifest, atomic staged install, and rollback materially reduce local IPC and partial-update failure modes. They do not establish correctness on Windows until the ACL, session, install, and crash canaries run on real Windows hosts.
- **Audio inference:** ParakeetAI's 10/80/200 ms AEC contract is a useful benchmark hypothesis. Bluey has not implemented or validated AEC in this round, and those values should not be treated as universal optimums.
- **Product inference:** Littlebird's coherent workspace/context presentation should inform further Coach cohesion, while Bluey's reference-only storage and Jobs receipt provenance should remain authoritative. UI quality still requires usability evidence.
- **Protection inference:** removing source maps and debug/source payloads raises reconstruction cost and prevents accidental disclosure. It cannot prevent static analysis of a shipped executable. Policy notices communicate ownership; server-side authority, minimal client secrets, signed releases, and operational enforcement provide the real boundary.

## Cross-application feature matrix

| Capability | Cluely | Littlebird | LockedIn | ParakeetAI | Final Round | Bluey current position |
| --- | --- | --- | --- | --- | --- | --- |
| Windows architecture | Electron; x64/ARM64 | Electron + signed .NET helper; x64 | Electron + native input helpers; x64 | Electron + Rust N-API; x64/ARM64 | Electron + native audio/keyboard addons; x64 | Terminal + Rust daemon + out-of-process native helpers; x64 build path |
| Mic + system audio | SoX mic + Chromium loopback | Signed helper WASAPI mic/loopback | Renderer mic + Chromium loopback | Renderer/Chromium + native AEC | Native WASAPI + WebRTC APM | Direct event-driven WASAPI mic/loopback; real-device proof remains |
| Resampling/AEC | No stronger Windows AEC found | Helper implementation unavailable | No Windows AEC evidence | Exact bounded AEC source | Native AEC symbols/contracts, source absent | 64-tap resampler implemented; AEC missing |
| Meeting hints | Calendar/session | Helper context/events | Manual/session driven | Active capture-session detection | MMDevice/device activity | Implemented statically for the dashboard: eCapture sessions, PID deduplication, content-free allowlist, native/browser debounce, ambiguous-scan fail-safe; terminal/daemon wiring and real-Windows proof remain |
| Screenshot/OCR/context | Screenshot | Window/screen/OCR + exclusions | Screenshot/documents | Screenshot | Screenshot + coding/system design | Native virtual-desktop PNG and existing consent/integrity path; OCR/window selection missing |
| Helper supervision | Basic lifecycle | Best: schema, PID identity, replay, five restarts | macOS-only audio watchdog | Utility stop/restart boundary | Session state machine + pause/resume | Bounded diagnostics, exact readiness contract, 200 ms audio channel, stop/readiness deadlines, minimal environment, and five-restart budget implemented; critical-state replay remains |
| Local command boundary | Generic preload + origin checks | Typed but large preload | Broad bridge + remote input | Generic invoke/on bridge | Registered gateway + sender checks | Stronger: owner-only named pipe, SID/session, bearer, replay, bounds/deadlines |
| Update trust | Signed artifacts | Signed artifacts | Publisher configured | GitHub feed; local chain unverified | Publisher-pinned post-download check | Signed manifest path and inner hashes exist; native publisher verification/signing remain |
| Workspace/interview UX | Modes/history/calendar | Strongest workspace/context cohesion | Interview presets/documents | Presets/auto-answer | Strongest coding/system-design/session panels | Durable owner-scoped workspaces and seven-mode Coach; structured code panels remain P1 |
| Jobs/ATS automation | Not observed | Not observed | Not observed | Not observed | Not observed | Bluey stronger: adapters, isolated profiles, takeover, leases, submit authority, receipts |
| Recommended decision | Adapt state/origin ideas | Adapt supervision/context concepts | Reject remote-input bridge | Adapt MMDevice/AEC contract | Adapt lifecycle/publisher concepts | Preserve Bluey-owned contracts and implement behavior independently |

Bluey's jobs evidence remains in standard adapters (`jobs/automation/src/standard-adapters.ts:47-98,237-309`), challenge takeover (`jobs/automation/src/challenge-handling.ts:55-84`), exclusive submit authority (`jobs/browser/src/irreversible-submit.ts:63-171`), execution leases (`jobs/runner/src/execution-lease.ts:167-230`), encrypted profiles (`jobs/runner/src/profile-store.ts:14-66`), and receipts (`jobs/automation/src/receipts.ts:101-199`). These are outside the five competitor installers' observed scope.

## Implemented changes in this round

### 1. Owner-verified Windows local IPC

Round 509 deliberately failed closed on Windows because it could not prove owner-only capability storage. This round replaces the Windows TCP path with a per-user named pipe and completes that missing OS boundary:

- a protected DACL grants only the current user SID;
- remote pipe clients are rejected;
- peer PID, owner SID, and logon session are checked by both sides;
- capability publication rejects reparse points and verifies the exact DACL;
- per-boot bearer, request UUID, replay detection, authorization classification, request deadline, request size, response-client size, and connection limits remain in force;
- the CLI does not honor an alternate Windows `--addr` transport.

This is a stronger static design than the five audited products expose for local privileged commands. It is not yet a runtime Windows ACL proof.

### 2. Native Windows screenshot capture

Bluey now has a small C++ capture helper instead of relying on PowerShell in release builds. It captures the entire virtual desktop, including negative monitor coordinates, opts into per-monitor DPI awareness, bounds dimensions/pixels, emits PNG, and reports only structured result categories. The daemon accepts release helpers only from installed/current-executable candidates with matching package-integrity metadata. The old PowerShell path remains available only in debug builds.

This implements screenshot parity for the terminal/daemon product without importing an Electron renderer or competitor helper. OCR, per-window selection, privacy exclusions, and capture-exclusion verification remain separate P1 work.

### 3. Windows audio quality and lifecycle foundation

The existing WASAPI helper now validates all arguments and audio formats, uses event callbacks rather than long polling, emits structured ready/error/stopped diagnostics, and performs anti-aliased 16 kHz mono conversion through a bounded 64-tap resampler. Device invalidation/service/resource HRESULTs are marked recoverable for the supervisor.

This resolves the prior naive averaging-downsampler gap. The continuous path now
has a 200 ms bounded channel, 40 ms backpressure ceiling, bounded structured
stderr, exact readiness source/format checks, a three-second readiness deadline,
100 ms stop polling with abort fallback, a five-restart budget, and an
environment allowlist that excludes Bluey secrets
(`crates/cue-daemon/src/audio/system_capture.rs:23-35,43-77,190-218,220-390`).
Automatic endpoint reselection, suspend/resume recovery, AEC, and critical-state
replay still require runtime evidence and follow-up implementation.

### 4. Conservative Windows meeting hints

Bluey-owned Rust now enumerates active Windows eCapture sessions, deduplicates
PIDs, reads process image identity with query-only access, maps only an exact
Teams/Zoom/Webex/Slack/Discord/browser allowlist to content-free labels, and
uses stricter browser start/stop debounce. Inconclusive enumeration cannot start
or stop detection (`crates/cue-daemon/src/cloud/meeting_detect.rs:41-188,269-446`).

This watcher is currently consumed only by the dashboard auto-disguise path
(`crates/cue-dashboard/src/lib.rs:1978-1997`). The terminal/daemon startup path
does not consume it, so Round 510 does not claim terminal meeting detection.
Real Windows MMDevice/process-layout canaries and an explicit terminal UX are
required before that claim.

### 5. Package integrity and transactional Windows install

The Windows build now creates an inner integrity manifest for every packaged executable/helper and notice. Release helper discovery hashes the selected helper against that manifest. The installer verifies both the downloaded artifact and the inner file set, locks the installed bin tree to the current SID, stages the new version, runs a CLI policy and daemon startup canary, and restores the previous bin directory on failure.

The adjacent inner manifest prevents unnoticed local replacement when the attacker cannot also replace protected package state. It is not yet a signed root of trust on its own. The outer signed release manifest and Authenticode publisher verification must remain authoritative.

### 6. Distribution hygiene and ownership signaling

The publisher now fails closed if a release archive contains source maps, debug symbols, source/package trees, ASAR, configured secret bytes, unsafe archive structure, or production development flags. The terminal exposes a machine-readable policy and the binary carries the same notice in a retained read-only data section. The notice truthfully states that it is signaling, not anti-analysis DRM.

## Reuse and provenance decision

The recovery tree separates exact bytes, mechanical extraction, exact source-map strings, mechanical formatting, and clean-room inference. `PROVENANCE-SNAPSHOT.sha256` binds 103 metadata/evidence/reform files; per-product `exact-files.sha256` and extracted-product ledgers bind the recovered trees.

No recovered competitor implementation, renderer asset, endpoint credential, telemetry identifier, signing material, native binary, or source-map file was copied into Bluey. Bluey's changes were independently implemented against Bluey's existing Rust/C/C++/PowerShell contracts. The reusable output of the comparison is behavioral:

- bounded queues and deadlines;
- narrow typed capability surfaces;
- owner/session peer verification;
- structured content-free helper diagnostics;
- crash budgets and critical-state replay;
- active-session hints with debounce;
- staged update deferral, publisher verification, and rollback;
- explicit user consent and capture exclusions.

Direct reuse remains rejected for Cluely's bundles/SoX bytes, Littlebird's maps/UI/helper, LockedIn's remote-input and extension/native bytes, ParakeetAI's source/binaries/endpoints, and Final Round's renderer/addons/model/updater code. A separate ownership, dependency, license, and provenance review would be required before changing that decision.

## Security and privacy findings

### Improved

- Windows privileged daemon traffic no longer uses a generally discoverable TCP endpoint; it uses a remote-rejecting current-user pipe with two-sided peer owner/session validation.
- Capability files are current-user-only, reparse-aware, bounded, and boot-specific.
- Helper binaries are out of process and hash-checked against the packaged manifest before release execution.
- Release screenshot capture no longer interpolates a destination into PowerShell source.
- Release capture runs with a minimal environment, null stdio, a 15-second
  kill deadline, and regular/non-reparse PNG signature and 256 MiB output bounds;
  diagnostics do not include screenshot content
  (`crates/cue-daemon/src/app.rs:16187-16314`).
- Audio input accepts only explicit sources/formats and reports structured recoverable categories.
- Continuous audio uses a minimal environment, bounded queue/stderr, readiness
  and stop deadlines, and forced abort fallback; Bluey secrets are not inherited.
- Windows install is staged, integrity-checked, owner-ACL-protected, canaried, and rollback-capable.
- Successful install removes the prior-bin rollback tree; failed swaps restore it.
- IPC response serialization is memory- and wire-bounded before the response is
  written (`crates/cue-daemon/src/app.rs:2209-2280`).
- Release scanning rejects common accidental reconstruction aids, source trees, unsafe archive members, and configured secret values without printing the secrets.
- The policy marker describes ownership without pretending that client-side metadata blocks inspection.
- No general remote mouse/keyboard injection surface was introduced.

### Residual risks and limitations

- The package integrity JSON is adjacent to the helpers. Protected install ACLs help, but only a signed outer manifest and Authenticode publisher pin establish a release root of trust.
- The current helper verifier checks digest/size, not the executable's Authenticode publisher at every launch.
- SmartScreen reputation, certificate revocation, timestamp policy, and signature-chain behavior are not testable on this macOS host.
- Daemon response serialization now uses a capped `Write` sink that retains at
  most `IPC_MAX_RESPONSE_BYTES - 1` bytes and replaces overflow with a small
  bounded error before writing (`crates/cue-daemon/src/app.rs:2209-2280`).
- Windows runtime-directory validation rejects reparse points but does not itself inspect a directory DACL; capability-file and named-pipe objects carry the owner-only ACL.
- Same-user malware remains inside the local account threat boundary. A same-user peer passes the SID/session test and still needs the per-boot bearer for protected commands.
- The capture helper captures the virtual desktop; per-window selection, sensitive-app/domain exclusions, and Windows capture-exclusion proof are not implemented.
- Native audio lacks automatic endpoint-change recovery, silence continuity, AEC, and measured Bluetooth/suspend behavior.
- Active microphone-session hints are implemented with native/browser debounce
  and ambiguous-scan safeguards (`meeting_detect.rs:41-188,269-446`), but only
  the dashboard consumes the watcher today. Terminal/daemon integration and real
  Windows validation remain before this can be claimed as a terminal feature.
- Removing maps/symbols raises analysis cost but cannot make shipped binaries unrecoverable.
- Static inspection cannot establish the five products' backend authorization, deletion, retention, billing enforcement, or telemetry scrubbing.

## Verification evidence

Only commands actually observed in this worktree are recorded here. Static cross-compilation proves source/ABI compatibility; it is not a substitute for Windows execution.

### Installer recovery and provenance

| Check | Observed result |
| --- | --- |
| `shasum -a 256 -c recovered/PROVENANCE-SNAPSHOT.sha256` | Passed for 103 metadata/evidence/reform files; snapshot SHA-256 `dcead2c50320ce31bb203fb7016f5ec0825ac08b03f17de57e9e302d7ab7b011` |
| Per-product `shasum -a 256 -c exact-files.sha256` | Passed for all five products |
| Extracted-product hash-ledger checks | Passed for all five products |
| Five original installer hash checks | Passed |
| Symlink scan of exact recovery trees | No symlinks found |
| `node --check` over formatted entrypoints | Passed 10/10 |
| Littlebird source-map ledger | Passed 4,350/4,350 relationships |
| Littlebird esbuild syntax transformation | Passed 932/932 TS/TSX files |
| ParakeetAI Rust parser/rustfmt pass | Parsed 18/18; one file noncanonical formatting only |
| Clean-room `tsc --noEmit` | Passed |

### Bluey Windows and release slice

| Check | Observed result |
| --- | --- |
| `x86_64-w64-mingw32-g++ ... native/windows/cue-capture/main.cpp ...` | Passed with `-Wall -Wextra -Werror` |
| `x86_64-w64-mingw32-objdump -p /tmp/bluey-capture.exe` | PE32+ CUI; Windows subsystem version 10.0 |
| `python3 scripts/check-release-artifact-contents.py --self-test` | Passed: two clean archives and five rejection paths |
| `bash scripts/release-hygiene-scan.sh scripts/check-release-artifact-contents.py scripts/publish-bluey-release.sh` | Passed for both scoped files; scanner self-test passed first |
| `bash -n scripts/publish-bluey-release.sh scripts/release-hygiene-scan.sh` | Passed |
| Scoped `git diff --check` for native capture and release-scanner changes | Passed |
| Host build/run of `native/windows/cue-audio/resampler_test.c` | Passed in 0.01 seconds user CPU for three one-second streams: 48 kHz produced 15,979 samples, 44.1 kHz produced 15,977, 1 kHz passband RMS `0.353556`, 12 kHz stopband RMS `0.000022` |
| MinGW warnings-as-errors build of Windows WASAPI helper + resampler | Passed; PE32+ CUI with Windows subsystem version 10.0 |
| `cargo check --target x86_64-pc-windows-gnu` for cue-core, cue-cli, and cue-daemon libraries/binaries | Passed |
| `cargo test -p cue-core -p cue-daemon -p cue-cli --lib` on the frozen tree | Passed 594; five hardware/interactive tests ignored; no ignored Keychain test executed |
| `cargo test -p cue-core ipc_auth --lib` | Passed 4/4 on the macOS host |
| `cargo test -p cue-cli daemon_ipc --lib` | Passed 2/2: oversized response and half-open timeout |
| `cargo test -p cue-daemon ipc_ --lib` | Passed 4/4: authorization, replay bound, raw shutdown, oversized request, slowloris, bind/capacity contracts |
| Capped IPC serializer boundary test | Passed 1/1 at the exact 256 KiB wire limit and overflow fallback |
| Continuous audio integration suite | Passed 6/6; final stop/readiness/environment-focused set passed 7/7 |
| Windows helper-manifest tests | BOM/product round-trip and rejection tests passed |
| Windows capture PNG postcondition test | Passed 1/1 on the host-testable validator |
| Windows meeting classification/debounce tests | Passed 11/11; macOS and Windows GNU strict Clippy passed |
| `cargo clippy -p cue-core -p cue-daemon -p cue-cli --all-targets --target x86_64-pc-windows-gnu -- -D warnings` | Passed after the capped serializer and meeting detector changes |
| Frozen-tree macOS all-target strict Clippy for cue-core/cue-daemon/cue-cli | Passed with warnings denied |
| Frozen-tree Windows GNU CLI/daemon binary link | Passed for all declared bins; `BLUEY_POLICY_JSON_V1` is retained in `bluey.exe` and `bluey-daemon.exe` |
| `cargo fmt --all -- --check` | Passed after the final audio resampler split |
| Scoped `git diff --check` for this Round 510 document | Passed |

The focused Rust tests execute the platform-neutral authorization and framing
contracts on this macOS host. They do not execute the `cfg(windows)` named-pipe,
ACL, peer-process, installer, or helper-launch branches. PowerShell Core was not
available on the analysis host, so `install.ps1`, `build-windows.ps1`, and the
integrity writer still require parse and runtime validation on Windows.

## Unknowns requiring real Windows validation

The following are release gates, not documentation polish:

1. **Named pipe:** create/connect under standard users on Windows 10 and 11; reject another user, another session, remote clients, stale boot, forged bearer, replay, oversized input/output, half-open clients, capacity saturation, and alternate-address attempts.
2. **Capability ACL:** inspect owner and DACL through Windows APIs and `icacls`; exercise reparse, replacement, overlapping daemon boots, crash cleanup, and upgrade paths.
3. **Audio:** real microphone/render-loopback latency, anti-alias response, silence behavior, device switching, endpoint loss, exclusive-mode conflicts, Bluetooth, Remote Desktop, suspend/resume, and clean teardown.
4. **Meeting hints:** eCapture enumeration on Windows 10/11, modern Teams/Webex process layouts, browser process churn, permission denial, device hot-plug, debounce timing, and terminal/daemon consumer UX.
5. **Capture:** negative-coordinate multi-monitor layouts, mixed DPI, HDR/scaling, locked/UAC/secure desktop behavior, content-protected windows, output bounds, and capture consent/integrity flow.
6. **Install/update:** clean install, in-place update, running-daemon replacement, rollback after each staged failure point, PATH behavior, uninstall, non-admin accounts, ARM64 fallback, and disk-full/antivirus file locks.
7. **Trust:** Authenticode publisher pin, trusted timestamp, revocation/offline policy, signed outer release manifest, helper replacement, SmartScreen reputation, and Defender scan.
8. **Reliability/performance:** 8/24/72-hour soak, helper crash budget, daemon restart/state replay, p50/p95/p99 audio-to-answer latency, CPU/RSS, and no-content diagnostics.
9. **Privacy:** no screenshot/audio payload in logs, crash reports, telemetry, temp directories, or failed install backups; verify deletion and stale-capture cleanup.
10. **Compatibility:** x64 is the current concrete build target. Native ARM64 must wait for Rust and every native helper/dependency/signing/SBOM path to reach parity.

Static recovery and macOS-host cross-compilation cannot close any of these gates.

## Priorities and release gates

### P0 — before a Windows production release

1. Sign `bluey.exe`, `bluey-daemon.exe`, and every native helper with the same approved publisher and trusted timestamp. Verify Authenticode publisher plus the signed release-manifest digest before install and before high-privilege helper launch.
2. Run the real Windows named-pipe/ACL adversarial matrix above. Keep the release blocked if any peer, capability, reparse, stale-boot, replay, or capacity invariant fails.
3. Exercise the new bounded response serializer with the largest real
   workspace/session responses on Windows and retain the exact 256 KiB wire cap.
4. Run real-device WASAPI and screenshot canaries across Windows 10/11, common audio devices, mixed-DPI multi-monitor arrangements, sleep/resume, and endpoint changes.
5. Complete automatic audio endpoint-loss/device-change recovery and
   critical-state replay while preserving the implemented bounded restart budget,
   stop/readiness deadlines, and minimal helper environment.
6. Run clean install/update/rollback/uninstall tests under non-admin accounts and antivirus file contention. Verify signed-manifest handoff and fail closed without it for update paths.
7. Freeze the worktree and rerun full Rust tests, warnings-denied Clippy, formatting, Windows cross-target compilation, UI tests/build, PowerShell parse checks, release hygiene, and `git diff --check`.

### P1 — product quality and Windows advantage

1. Wire the implemented Bluey-owned active capture-session hints into the
   terminal/daemon runtime with user-visible confirmation, then validate the
   allowlist, process identity, debounce, ambiguous-scan behavior, and browser
   safeguards on real Windows.
2. Benchmark an optional WebRTC AEC stage with bounded 10 ms frames and compare multiple delay/queue settings. Preserve a headset/microphone-only bypass and raw-helper fallback.
3. Add signed native window selection/OCR context with app/domain exclusions, visible capture controls, strict byte/frame/deadline bounds, and derived-reference-only retention by default.
4. Add structured Approach, Code, Complexity, Tests, and System Design panels over Bluey's existing Coach/artifact contracts. Do not import competitor renderer assets.
5. Add content-free helper supervision metrics and a five-attempt-equivalent crash budget with exponential delay and explicit critical-state replay.
6. Add one read-only calendar or email-outcome connector only after cursor provenance, deletion, retention, and content-free telemetry contracts are complete.

### P2 — optional expansion

1. Publish native Windows ARM64 only after helper/dependency/SBOM/signature/canary parity.
2. Add audio-first mock interview/video collaboration only with explicit pairing, expiry, consent, authenticated commands, and no arbitrary remote input.
3. Measure Coach/workspace usability and Windows latency against task-level targets before making comparative superiority claims.
4. Evaluate stronger binary hardening and symbol management as defense in depth, while keeping proprietary decisions and secrets server-authoritative.

## Concrete next-agent deployment handoff

The next agent should treat this as a release-engineering and Windows-runtime task, not another static-recovery task:

1. Record the exact commit/tree and produce a Windows x64 release candidate through `scripts/build-windows.ps1`. Confirm that no `.map`, PDB, source, ASAR, package manifest, or secret value is present by running `scripts/check-release-artifact-contents.py` on the final archive.
2. Sign every `.exe` helper and primary binary with the approved publisher and timestamp service. Generate the signed outer release manifest, then verify artifact digest, publisher, and inner `bluey-integrity.json` before installation.
3. On clean Windows 10 and 11 standard-user VMs, run the named-pipe and capability adversarial matrix. Capture only content-free results: OS build, architecture, pass/fail code, durations, and binary hashes.
4. Exercise install, start, status, screenshot, audio readiness, listening, answer, stop, update, forced-canary failure, rollback, restart, and uninstall. Verify the previous version is restored byte-for-byte after every injected update failure.
5. Run mic/loopback and multi-monitor capture matrices, including device change, Bluetooth, mixed DPI, negative monitor coordinates, sleep/resume, secure desktop, and Defender/SmartScreen. Record p50/p95/p99 and failure categories.
6. Fix only evidence-backed failures. Do not weaken ACLs, signature checks, integrity checks, capture consent, request bounds, or rollback to make a canary pass.
7. After the P0 matrix passes, validate and wire the implemented active-mic
   meeting hints into the terminal runtime; treat AEC as a separate benchmarked
   change. Keep OCR/window context and structured interview presentation behind
   their own privacy and usability gates.
8. Update this document with signed artifact hashes, certificate subject/thumbprint, Windows build matrix, exact command results, latency distributions, and the final release decision.

Deployment decision at the end of Round 510: **not yet production-ready for Windows**. The architecture and static implementation are substantially stronger, but signed publisher trust and real Windows runtime evidence are mandatory remaining gates.
