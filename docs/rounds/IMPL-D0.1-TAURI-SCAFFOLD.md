# IMPL-D0.1: Tauri 2 Workspace Scaffold

## Summary

Added crates/cue-dashboard - a Tauri 2 binary crate that will host the bluey
dashboard window. Includes NSPanel integration on macOS for non-activating
overlay behavior, a React 19 + Vite frontend scaffold, and a proof-of-concept
Tauri command.

## Files Created

### Rust crate (crates/cue-dashboard/)
- Cargo.toml - Tauri 2 deps + tauri-nspanel (macOS)
- build.rs - tauri-build with auto-creation of ui/dist placeholder
- src/main.rs - binary entry point
- src/lib.rs - Tauri app builder with plugins + get_app_version command
- src/macos.rs - NSPanel setup (float level, non-activating, all-spaces)
- tauri.conf.json - window config (1200x800, centered, content_protected)
- capabilities/default.json - Tauri 2 permission capabilities
- icons/ - placeholder RGBA PNGs for build (replace with real icons later)

### Frontend scaffold (crates/cue-dashboard/ui/)
- package.json - React 19 + Vite + TypeScript
- vite.config.ts - Vite config with Tauri-aware settings
- tsconfig.json - strict TypeScript config
- index.html - HTML entry
- src/main.tsx - React entry point
- src/App.tsx - placeholder component
- src/index.css - basic reset + dark theme
- .gitignore - excludes node_modules and dist

### Workspace
- Cargo.toml (root) - added crates/cue-dashboard to members
- Cargo.lock - updated with new dependencies

## Build Commands + Results

```
$ cargo build --bin cue-dashboard
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 40.33s

$ cargo build --release
    Finished `release` profile [optimized] target(s) in 1m 24s
```

Warnings only (all from deprecated cocoa APIs in tauri-nspanel macro - expected,
matches pluely reference).

## Deviations from Plan

1. **Placeholder icons**: Created minimal valid RGBA PNGs instead of real app
   icons. Tauri's `generate_context!` macro validates icons at compile time, so
   they must exist and be valid RGBA format. Real icons to be added in a design pass.

2. **build.rs auto-creates ui/dist**: Since `npm install` is not run during
   Rust-only builds, `build.rs` creates a minimal `ui/dist/index.html` placeholder
   if missing. This ensures `cargo build` works without requiring the frontend to
   be built first.

3. **`#![allow(deprecated)]` on macos.rs**: The `tauri-nspanel` crate uses
   deprecated `cocoa` crate APIs internally (it re-exports them). This is expected
   and matches the pluely reference implementation.

## Known Follow-ups

- [ ] React dashboard UI is only a placeholder - real UI comes in Phase 1 (B6.3)
- [ ] `npm install` must be run before `cargo tauri dev` works (intentionally skipped)
- [ ] Icons are placeholder blue squares - need real bluey icons
- [ ] NSPanel delegate only logs events - functional behavior comes in D0.4
- [ ] No IPC wiring to daemon yet - that is D0.2
- [ ] No hotkey registration - that is B6.1
- [ ] Window does not actually float as panel until app is run (build-only verification)

## Review Checklist

- [ ] `cargo build --bin cue-dashboard` compiles without errors
- [ ] `cargo build --release` (full workspace) compiles without errors
- [ ] `tauri.conf.json` has `contentProtected: true` and `macOSPrivateApi: true`
- [ ] NSPanel setup uses correct constants: level=4, style_mask=1<<7
- [ ] Collection behavior includes FullScreenAuxiliary + CanJoinAllSpaces
- [ ] `get_app_version` command returns `CARGO_PKG_VERSION`
- [ ] Frontend scaffold has React 19 (not 18) in package.json
- [ ] No node_modules or dist committed
