# IMPL — Phase 3 Process Masquerading (Round 8 — Stealth + Settings Fix)

**Branch**: `feat/phase-3-round-8`
**Base**: `feat/phase-3-round-7` tip (`97aa759`, 213 tests)
**Tip**: `d00880a` (7 commits ahead of R7, 224 tests)

## Scope

**Process masquerading (Activity Monitor / Task Manager spoofing) per the natively-cluely playbook + Settings route regression fix.**

**Does:**

1. New `cue-stealth` crate with per-platform process-name masquerading: macOS (argv[0] overwrite + CFBundleName env var), Linux (prctl PR_SET_NAME), Windows (SetCurrentProcessExplicitAppUserModelID).
2. Single `apply_disguise()` API that dispatches to the correct platform implementation.
3. `DisguiseMode` enum (Terminal, Settings, Activity, None) with platform-aware metadata (app name, icon filename, AUMID).
4. Startup disguise application from persisted settings + re-assertion timers at 200ms, 1s, 5s to fight OS drift.
5. Window title updates across all open Tauri windows when disguise changes.
6. `set_disguise` / `get_disguise` Tauri commands with persistence via existing `save_settings` DB layer.
7. Settings page "Stealth & Disguise" section with platform-aware dropdown labels.
8. Fix Settings route regression: was mounting `<Placeholder name="Settings" />` despite `Settings.tsx` existing.
9. 6 placeholder disguise icons (256×256 PNGs, 3 per platform: mac + win) with generation docs.

**Does NOT:**

- Change macOS Activity Monitor binary name (read from Mach-O at launch — Apple sealed).
- Hide from macOS dock (requires NSApplication.setActivationPolicy via objc2 — deferred).
- Change Windows Task Manager process name column (shows .exe filename, not AUMID).
- Ship production-quality icons (placeholders only; swap when designer-supplied PNGs land).
- Add E2E route registration assertions (follow-up).

## Commits (7, chronological bottom → top)

| # | Hash | Title | Role |
|---|------|-------|------|
| 1 | `268a138` | `feat(stealth): cue-stealth crate with macOS/Windows/Linux process masquerade [P3.R8]` | Core crate: DisguiseMode, build_request, apply_disguise, per-platform FFI |
| 2 | `e068044` | `feat(dashboard): apply disguise on startup + re-assertion timers [P3.R8]` | Startup integration: read persisted mode, apply, spawn re-assertion thread |
| 3 | `862e480` | `feat(dashboard): set_disguise / get_disguise Tauri commands [P3.R8]` | Tauri command layer: invoke from UI, persist, update windows |
| 4 | `237123d` | `feat(dashboard): generate disguise icon placeholders + asset README [P3.R8]` | 6 PNGs + README documenting generation + replacement path |
| 5 | `972a92b` | `fix(dashboard): mount real Settings page (was placeholder regression) [P3.R8]` | Route fix: `<Settings />` replaces `<Placeholder name="Settings" />` |
| 6 | `e19c1b4` | `feat(dashboard): disguise mode dropdown in Settings + invoke wrappers [P3.R8]` | UI: dropdown, platform labels, disguise.ts invoke helpers |
| 7 | `d00880a` | `chore(p3r8): cargo fmt across cherry-picks` | Formatting normalization |

## Files Created / Modified

### cue-stealth crate (new)

| File | Action | Purpose |
|------|--------|---------|
| `crates/cue-stealth/Cargo.toml` | Created | Crate manifest: thiserror, tracing, serde, platform-conditional libc/windows |
| `crates/cue-stealth/src/lib.rs` | Created | DisguiseMode enum, build_request(), mode_metadata(), apply_disguise(), 8 unit tests |
| `crates/cue-stealth/src/macos.rs` | Created | argv[0] overwrite via _NSGetArgv() + null-padding; 2 tests |
| `crates/cue-stealth/src/linux.rs` | Created | prctl(PR_SET_NAME) with 15-char truncation; 2 tests |
| `crates/cue-stealth/src/windows.rs` | Created | SetCurrentProcessExplicitAppUserModelID via windows crate; 1 test |

### cue-dashboard Rust

| File | Action | Purpose |
|------|--------|---------|
| `crates/cue-dashboard/src/lib.rs` | Modified | Startup disguise block: load mode → apply → re-assertion thread (200ms/1s/5s) → window title |
| `crates/cue-dashboard/src/commands.rs` | Modified | `set_disguise` + `get_disguise` commands; invoke handler registration |
| `crates/cue-dashboard/Cargo.toml` | Modified | `cue-stealth` dependency added |

### cue-dashboard UI

| File | Action | Purpose |
|------|--------|---------|
| `crates/cue-dashboard/ui/src/pages/Settings.tsx` | Created | Full settings page with STT config + Stealth & Disguise dropdown |
| `crates/cue-dashboard/ui/src/lib/disguise.ts` | Created | `setDisguise()` / `getDisguise()` invoke wrappers |
| `crates/cue-dashboard/ui/src/App.tsx` | Modified | Route fix: `<Settings />` replaces `<Placeholder name="Settings" />` |

### Assets

| File | Action | Purpose |
|------|--------|---------|
| `crates/cue-dashboard/icons/disguise/mac/terminal.png` | Created | 256×256 placeholder |
| `crates/cue-dashboard/icons/disguise/mac/settings.png` | Created | 256×256 placeholder |
| `crates/cue-dashboard/icons/disguise/mac/activity.png` | Created | 256×256 placeholder |
| `crates/cue-dashboard/icons/disguise/win/terminal.png` | Created | 256×256 placeholder |
| `crates/cue-dashboard/icons/disguise/win/settings.png` | Created | 256×256 placeholder |
| `crates/cue-dashboard/icons/disguise/win/activity.png` | Created | 256×256 placeholder |
| `crates/cue-dashboard/icons/disguise/README.md` | Created | Generation docs + replacement instructions + smoke test |

## Design Decisions

### 1. Per-platform implementation behind single API

`apply_disguise(&DisguiseRequest)` dispatches to platform-specific modules via `#[cfg(target_os)]`. Each platform does the maximum possible at runtime:
- **macOS**: `_NSGetArgv()` argv[0] overwrite (affects `ps`) + `CFBundleName` env var (affects menu bar title).
- **Linux**: `prctl(PR_SET_NAME)` (affects `top`, `htop`, `/proc/self/comm`).
- **Windows**: `SetCurrentProcessExplicitAppUserModelID` (affects taskbar grouping + notifications).

### 2. Re-assertion timers (200ms, 1s, 5s)

Mirrors the natively-cluely Electron pattern. macOS and some Linux desktop environments can drift the process title back to the original after window manager events. The re-assertion thread fires at 200ms, 1s, and 5s post-startup to re-apply. Cheap (3 syscalls total) and effective.

### 3. Window-level changes via Tauri

`window.set_title()` is fully effective and orthogonal to process-level changes. The `set_disguise` command iterates all open webview windows and updates their titles. Icon changes via `window.set_icon()` are supported but deferred until production icons land.

### 4. Honest deferrals

- **macOS Activity Monitor** reads the Mach-O binary name at launch time — cannot be changed at runtime (Apple sealed since macOS 11). The argv[0] trick affects `ps` but not Activity Monitor's process column.
- **macOS dock icon hiding** requires `NSApplication.setActivationPolicy(.prohibited)` via objc2; deferred because it interacts with Tauri's own dock management.
- **Windows Task Manager** shows the .exe filename, not the AUMID. Changing it requires PE resource modification at build time.
- **Linux prctl** truncates at 15 chars (kernel limit for `comm`).

### 5. Settings regression fix

R5/R6 had `<Route path="settings" element={<Placeholder name="Settings" />} />` despite `Settings.tsx` existing and being imported. The unit-test-only verification missed it because there was no E2E route registration test asserting that real components are mounted. Fixed in commit `972a92b` by replacing the Placeholder with `<Settings />`.

### 6. Icon assets

6 PNGs (256×256) generated via Python Pillow with simple text glyphs (">_", "⚙", "i") on solid backgrounds. The README documents the generation method and the swap-when-designer-supplied path. Filenames are referenced by the stealth crate's `mode_metadata()`.

## Test Count Progression

| Stage | Running tests | Δ |
|-------|---------------|---|
| R7 final | 213 | — |
| R8 final | 224 | +11 |

### Tests added in R8 (+11, all in cue-stealth)

| Test | Type |
|------|------|
| `mode_from_str_round_trip` | Unit |
| `mode_metadata_terminal` | Unit |
| `mode_metadata_settings` | Unit |
| `mode_metadata_activity` | Unit |
| `mode_metadata_none` | Unit |
| `build_request_without_icon_dir` | Unit |
| `build_request_with_icon_dir` | Unit |
| `apply_disguise_none_does_not_panic` | Unit |
| `set_process_name_does_not_crash` (macOS) | Unit |
| `set_process_name_long_name_truncates` (macOS) | Unit |
| `apply_disguise_terminal_macos` (macOS, cfg-gated) | Unit |

(Linux and Windows platform tests run only on their respective targets.)

## Build & Test

```
cargo fmt --all --check                              ✅ pass
cargo clippy --all-targets -- -D warnings            ✅ pass
cargo build --all-targets                            ✅ pass
cargo test --all-targets                             ✅ 224 pass, 2 ignored
cd crates/cue-dashboard/ui && npm run build          ✅ pass
git -P diff --check feat/phase-3-round-7..HEAD       ✅ clean
```

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| No window icon changes at runtime | Production icons not yet available; `set_icon()` call deferred until designer PNGs land |
| No dock icon hiding on macOS | Requires objc2 FFI that conflicts with Tauri's dock management; deferred |
| No E2E route registration test | Would require a test framework that renders the full React router; follow-up |

## Known Follow-ups

1. **macOS dock icon hiding** — `NSApplication.setActivationPolicy(.prohibited)` via objc2 crate.
2. **Production-quality disguise icons** — replace Pillow placeholders with designer-supplied PNGs.
3. **E2E route registration assertions** — test that all routes mount real components, not Placeholders.
4. **Windows Task Manager process name** — requires build-time PE resource modification (out of runtime scope).
5. **Linux /proc/self/cmdline** — argv[0] overwrite (similar to macOS approach) for full `ps` spoofing.

## Review Checklist (for reviewer)

- [ ] cue-stealth: `DisguiseMode` round-trips through `from_str_loose` / `as_str`
- [ ] cue-stealth: `mode_metadata` returns correct platform-conditional names
- [ ] cue-stealth: `build_request` constructs icon paths with correct platform subdirectory
- [ ] cue-stealth: macOS argv[0] overwrite truncates + null-pads (no buffer overflow)
- [ ] cue-stealth: Linux prctl truncates to 15 chars (kernel limit)
- [ ] cue-stealth: Windows AUMID uses HSTRING correctly
- [ ] Dashboard: startup disguise reads from DB, applies, spawns re-assertion thread
- [ ] Dashboard: re-assertion fires at 200ms, 1s, 5s (not blocking setup)
- [ ] Dashboard: `set_disguise` updates all windows + persists to DB
- [ ] Dashboard: `get_disguise` returns "none" when no setting exists
- [ ] Settings route: `<Settings />` mounted (not Placeholder)
- [ ] Settings UI: dropdown shows platform-aware labels (mac vs win)
- [ ] Settings UI: disguise change invokes `set_disguise` command
- [ ] Icons: 6 PNGs present at correct paths, README documents replacement
- [ ] No secrets logged, no PII in stdout
- [ ] Code style matches CLAUDE.md rules
