# Phase 3 Round 8 — Handoff for Codex Review

**Branch**: `feat/phase-3-round-8`
**Base**: `feat/phase-3-round-7` tip (`97aa759`, 213 tests)
**Authors**: kiro (2 parallel subagents: core + UI), uno (user — oversight)

## Scope

Round 8 of Phase 3. Two themes: process masquerading (stealth) and a Settings route regression fix. Implemented by parallel subagents in isolated worktrees, then cherry-picked onto the feature branch. Pipeline green at 224 tests.

### Commits (7 ahead of R7)

```
d00880a chore(p3r8): cargo fmt across cherry-picks
e19c1b4 feat(dashboard): disguise mode dropdown in Settings + invoke wrappers [P3.R8]
972a92b fix(dashboard): mount real Settings page (was placeholder regression) [P3.R8]
237123d feat(dashboard): generate disguise icon placeholders + asset README [P3.R8]
862e480 feat(dashboard): set_disguise / get_disguise Tauri commands [P3.R8]
e068044 feat(dashboard): apply disguise on startup + re-assertion timers [P3.R8]
268a138 feat(stealth): cue-stealth crate with macOS/Windows/Linux process masquerade [P3.R8]
```

## Verification — ALL GREEN

```
cargo fmt --all --check                              ✅ pass
cargo clippy --all-targets -- -D warnings            ✅ pass
cargo build --all-targets                            ✅ pass
cargo test --all-targets                             ✅ 224 pass, 2 ignored
cd crates/cue-dashboard/ui && npm run build          ✅ pass
git -P diff --check feat/phase-3-round-7..HEAD       ✅ clean
```

### Test count delta

| Tier | R7 final | R8 final | Δ |
|------|----------|----------|---|
| cue-stealth (new crate) | 0 | 11 | +11 |
| Previous (carried forward) | 213 | 213 | — |
| **Total running** | **213** | **224** | **+11** |

## Architecture Diagram — Round 8 Additions

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         cue-stealth crate [NEW]                              │
│                                                                             │
│  DisguiseMode ──▶ build_request() ──▶ DisguiseRequest                      │
│                                            │                                │
│                                            ▼                                │
│                                     apply_disguise()                        │
│                                            │                                │
│                    ┌───────────────────────┼───────────────────────┐        │
│                    ▼                       ▼                       ▼        │
│              macOS: argv[0]         Linux: prctl         Windows: AUMID     │
│              + CFBundleName         PR_SET_NAME          SetCurrent...ID    │
│              (affects ps)           (affects top)        (affects taskbar)  │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
         │
         ▼ (called from)
┌─────────────────────────────────────────────────────────────────────────────┐
│                           cue-dashboard                                      │
│                                                                             │
│  Startup (lib.rs):                                                          │
│    1. Load disguise_mode from DB                                            │
│    2. apply_disguise(build_request(mode))                                   │
│    3. Spawn re-assertion thread: 200ms → 1s → 5s                           │
│    4. Set window title if mode != None                                      │
│                                                                             │
│  Tauri Commands (commands.rs):                                              │
│    set_disguise(mode) → apply + update all windows + persist to DB          │
│    get_disguise() → read from DB (default: "none")                          │
│                                                                             │
│  UI (Settings.tsx):                                                         │
│    ┌─────────────────────────────────────────┐                              │
│    │ Stealth & Disguise                      │                              │
│    │ ┌─────────────────────────────────────┐ │                              │
│    │ │ Disguise Mode: [Terminal ▾]         │ │                              │
│    │ │   None / Terminal / Settings /      │ │                              │
│    │ │   Activity Monitor                  │ │                              │
│    │ └─────────────────────────────────────┘ │                              │
│    └─────────────────────────────────────────┘                              │
│    Platform-aware labels (mac: "Activity Monitor", win: "Task Manager")     │
│                                                                             │
│  Assets (icons/disguise/):                                                  │
│    mac/ → terminal.png, settings.png, activity.png (256×256)                │
│    win/ → terminal.png, settings.png, activity.png (256×256)                │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Per-Commit Review Checklist

### `268a138` — cue-stealth crate: per-platform process masquerade

**What changed:** New crate with `DisguiseMode` enum, `build_request()`, `mode_metadata()`, `apply_disguise()`, and platform-specific modules (macos.rs, linux.rs, windows.rs).

- [ ] `DisguiseMode` serde round-trips correctly (rename_all = "lowercase")
- [ ] `from_str_loose` is case-insensitive and defaults to None for unknown input
- [ ] `mode_metadata` returns platform-conditional names (e.g. "Command Prompt" on Windows, "Terminal" on macOS)
- [ ] `build_request` constructs icon paths with correct platform subdirectory (mac/win/linux) — NOTE: paths are built but NOT applied at runtime (deferred feature)
- [ ] macOS `set_process_name`: uses `_NSGetArgv()` safely — null checks on all 3 pointer levels
- [ ] macOS: truncates new name to original argv[0] length (no buffer overflow)
- [ ] macOS: null-pads remainder after copy (no stale bytes)
- [ ] macOS: `CFBundleName` env var set before process name (menu bar title)
- [ ] Linux: `prctl(PR_SET_NAME)` with CString conversion; truncates to 15 chars
- [ ] Linux: returns error on prctl failure (non-zero return)
- [ ] Windows: `HSTRING` from aumid string; `SetCurrentProcessExplicitAppUserModelID` called in unsafe block
- [ ] 8 unit tests in lib.rs + 2 macOS tests + 2 Linux tests + 1 Windows test = 11 total (platform-gated)
- [ ] No UB: argv[0] overwrite bounded by `strlen(arg0)`, null-padded

### `e068044` — Apply disguise on startup + re-assertion timers

**What changed:** `lib.rs` setup block loads `disguise_mode` from DB, calls `apply_disguise`, spawns a thread that re-applies at 200ms, 1s, 5s, and sets window title.

- [ ] Reads `disguise_mode` setting from DB (defaults to "none" on missing/error)
- [ ] `apply_disguise` called before re-assertion thread spawns
- [ ] Re-assertion thread uses `std::thread::spawn` (not blocking setup)
- [ ] Delays are 200ms, 1s, 5s (matches natively-cluely pattern)
- [ ] Window title set only when mode != None
- [ ] Error from `apply_disguise` is logged but does not crash startup

### `862e480` — set_disguise / get_disguise Tauri commands

**What changed:** Two new Tauri commands in `commands.rs`; registered in `invoke_handler`.

- [ ] `set_disguise`: parses mode string → builds request → applies → updates all window titles → persists to DB
- [ ] `set_disguise`: iterates `app.webview_windows()` for title update (handles multiple windows)
- [ ] `get_disguise`: reads from DB, returns "none" as default
- [ ] Both commands registered in `invoke_handler` macro
- [ ] No raw API keys or secrets in disguise flow

### `237123d` — Disguise icon placeholders + asset README

**What changed:** 6 PNG files (256×256) in `icons/disguise/{mac,win}/` + README.md documenting generation and replacement.

- [ ] 6 PNGs present: mac/{terminal,settings,activity}.png + win/{terminal,settings,activity}.png
- [ ] README documents: structure, generation method (Pillow), replacement instructions, smoke test steps
- [ ] Filenames match what `mode_metadata()` returns (terminal.png, settings.png, activity.png)
- [ ] No linux/ directory (Linux desktop icons are a follow-up)

### `972a92b` — Fix Settings route regression

**What changed:** `App.tsx` route for `/settings` changed from `<Placeholder name="Settings" />` to `<Settings />`.

- [ ] `Settings` component imported at top of App.tsx
- [ ] Route element is `<Settings />` not `<Placeholder ...>`
- [ ] No other routes accidentally changed
- [ ] Codex should grep for any remaining `<Placeholder name="..."` mounts that mask real components (currently: Home, Prompts, Shortcuts, Responses, Screenshot, Audio, Dev Tools — all intentionally placeholder)

### `e19c1b4` — Disguise mode dropdown in Settings + invoke wrappers

**What changed:** `Settings.tsx` with "Stealth & Disguise" section; `disguise.ts` with typed invoke wrappers.

- [ ] `disguise.ts` exports `DisguiseMode` type and `setDisguise`/`getDisguise` async functions
- [ ] `Settings.tsx` loads current disguise on mount via `getDisguise()`
- [ ] Dropdown uses platform-aware labels (`MAC_LABELS` vs `WIN_LABELS` based on `navigator.platform`)
- [ ] `handleDisguiseChange` calls `setDisguise(mode)` — no save button needed (immediate apply)
- [ ] STT settings section still present and functional (not broken by disguise addition)

### `d00880a` — cargo fmt normalization

**What changed:** Formatting pass across cherry-picked files.

- [ ] Only whitespace/formatting changes
- [ ] No logic changes
- [ ] All tests still pass

## Explicit Deferrals (NOT in Round 8)

1. **macOS Activity Monitor binary name** — reads Mach-O at launch; cannot change at runtime (Apple sealed).
2. **macOS dock icon hiding** — requires `NSApplication.setActivationPolicy(.prohibited)` via objc2; conflicts with Tauri dock management.
3. **Windows Task Manager process name** — shows .exe filename; requires PE resource modification at build time.
4. **Linux prctl 15-char limit** — kernel constraint; cannot be extended.
5. **Production-quality disguise icons** — placeholders shipped; swap when designer-supplied PNGs land.
6. **E2E route registration assertions** — no test framework currently renders full React router; follow-up.
7. **Window icon changes at runtime** — `set_icon()` deferred until production icons available.

## Verdict Request

Codex: review the 7 commits (2 themes: process masquerading, Settings regression fix). Write `docs/work/REVIEW-PHASE-3-ROUND-8.md` with verdict.

- 🟢 **ACCEPT** → merge R7+R8 to main, start Round 9
- 🟡 **ACCEPT WITH NITS** → fold nits into Round 9
- 🔴 **REQUEST CHANGES** → kiro writes fix doc and re-hands
