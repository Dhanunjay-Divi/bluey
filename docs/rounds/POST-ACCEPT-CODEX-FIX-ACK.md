# Post-Acceptance Codex Fix Acknowledgment

**Branch:** `feat/phase-3-round-12`
**Sealed at tip:** `0c13520 fix(ops): align unsigned macOS distribution artifacts`
**Date:** 2026-05-21

This document closes the loop on three codex `fix(...)` commits that
landed inside their review process across two acceptance gates. All
fixes are accepted and the v0.2 alpha codebase is sealed for the
remaining operator-side launch gates.

---

## Round 1: S12-17 fix wave (pre-Stages 18/23/24 acceptance)

### `e064e46 fix(p3r12): close codex review blockers`

Closed during codex's review of Stages 12-17. All blockers + nits:

| Issue | Fix |
|---|---|
| Duplicate `.setup(...)` Tauri callbacks meant deep-links + tray + disguise menu + meeting-watch were dead | Merged into a single setup hook |
| Settings/Onboarding Tauri commands missing | Added `account_me`, `billing_portal_url`, `sign_out`, `delete_account_now`, `get_signin_url`, `complete_onboarding` |
| Broken `@tauri-apps/plugin-opener` import | Replaced with `window.open` |
| Sign-out / delete-account didn't reset onboarding state | Now mark onboarding incomplete + reload UI through first-run gate |
| Auto-disguise prompt accept/decline was in-memory only | Persisted into `CueSettings` |
| `BLUEY_TEST_DEEPGRAM_URL` / `BLUEY_TEST_STRIPE_URL` env overrides not threaded through | Wired into dispatcher + billing |
| GDPR webhook cleanup SQL silently ignored errors with unquoted JSON paths | Quoted JSON paths + hard 500 on cleanup failure |

**Verdict received:** 🟢 ACCEPT on Stages 12-24 with this fix wave.

---

## Round 2: Post-acceptance invisibility/UI/ops review

**Codex review doc:** `docs/reviews/REVIEW-POST-ACCEPT-INVISIBILITY-OPS.md`
**Verdict:** 🟢 ACCEPT

### `43b5728 fix(dashboard): apply startup disguise and focus agent window`

Two fixes that land squarely on the items flagged in the handoff:

**Fix A: Startup disguise reapplication**

Without this, the customer's persisted disguise mode (e.g., "activity")
was applied to the process name and CFBundleName at boot via cue_stealth,
but the **tray icon** and **window title** still showed the default Bluey
state until the customer re-selected the disguise from the menu. Boot was
visually inconsistent with the disguise the customer had set.

Codex added a `set_disguise(persisted_mode, app.handle())` call right
after tray setup. Tray icon, title, and process state now align at boot.

**Fix B: LSUIElement activation before focus**

`LSUIElement=YES` removes Bluey from the Dock and Cmd-Tab. A side effect
is that `window.show()` + `window.set_focus()` from a tray menu item or
global shortcut would not actually bring the window to the foreground —
the OS treats LSUIElement apps as background agents.

Codex added a `crate::macos::activate_ignoring_other_apps()` helper that
calls `[NSApplication.sharedApplication activateIgnoringOtherApps:YES]`
via objc2 msg_send before any `show()`/`set_focus()`. This is the
standard AppKit pattern for menu-bar agents that need to surface a
window. Wired into all dashboard show paths (tray, settings, sign-in,
global shortcut).

---

## Round 3: Post-acceptance distribution-path review

**Codex review doc:** `docs/reviews/REVIEW-POST-ACCEPT-DISTRIBUTION-PATH.md`
**Verdict:** 🟢 ACCEPT

### `0c13520 fix(ops): align unsigned macOS distribution artifacts`

This fix caught a real bug that would have failed the very first install
smoke test on a clean Mac.

**Critical fix: Release workflow produced a `bin/` directory, not `Bluey.app`**

Our `install.sh` and `bluey.rb` cask both expected `Bluey.app` at the
tarball root. The existing release pipeline only emitted CLI binaries
under `bin/`. The first real `curl ... | bash` would have failed with
`Tarball did not contain Bluey.app at the top level`.

Codex extended `.github/workflows/release.yml` to:
- Stage the actual `Bluey.app` bundle from the Tauri build output
- Copy `bin/bluey` → `Bluey.app/Contents/Resources/bluey-cli`
- Copy `bin/bluey-daemon` + helper binaries into `Resources/`
- Optionally include `BlueyOverlay.app` from the Swift overlay build
- Tarball the whole thing

**Other alignments in the same commit:**

| Issue | Fix |
|---|---|
| Artifact naming inconsistent | `bluey-0.2.0-darwin-arm64.tar.gz` |
| `install.sh` had `sha256 :no_check` placeholder | Now downloads `SHA256SUMS.txt` + verifies via `shasum -a 256 -c` (skip via `BLUEY_SKIP_CHECKSUM=1`) |
| Cask `uninstall quit:` used `com.bluey.app` | Corrected to `com.bluey.dashboard` (matches actual CFBundleIdentifier) |
| Cask claimed both arm64 + x86_64 | Scoped to arm64 only — matches current release scope |

---

## Pipeline state at sealed tip `0c13520`

```
✅ cargo fmt --all --check
✅ cargo clippy --all-targets -- -D warnings (cue + server)
✅ 422 cue workspace cargo tests
✅ 75 server cargo tests
✅ 15 vitest
✅ npm run build (dashboard UI)
✅ swift build (overlay)
✅ bash -n ops/install/install.sh
✅ ruby -c ops/Casks/bluey.rb
✅ release.yml structure (247 lines)
```

Sealed: code work for v0.2 alpha is complete. No further code commits
needed before the operator-side launch gates.

---

## Remaining gates (operator-side, NOT code)

Per `docs/PRELAUNCH-CHECKLIST.md`:

- [ ] Tag `v0.2.0` and trigger the release workflow
- [ ] Publish `bluey-0.2.0-darwin-arm64.tar.gz` + `SHA256SUMS.txt`
- [ ] Host `https://bluey.dev/install.sh`
- [ ] Create `bluey-dev/homebrew-bluey` tap and publish `bluey.rb`
- [ ] Clean-Mac smoke: `curl -fsSL https://bluey.dev/install.sh | bash`
- [ ] Clean-Mac smoke: `brew tap bluey-dev/bluey && brew install --cask bluey`
- [ ] Real-Mac validation: app launch, tray icon, tray-menu focus,
      `bluey://` deep-link, F19 hotkey, overlay visibility, capture
      exclusion in Zoom/Teams/Meet
- [ ] DigitalOcean droplet provisioned per `docs/PRODUCTION-DEPLOY-RUNBOOK.md`
      (Caddy, systemd, backups, SQLite migrations)
- [ ] DNS for `bluey.dev` + `api.bluey.dev`
- [ ] Stripe live mode keys + webhook endpoint
- [ ] SMTP (transactional + magic-link)
- [ ] Marketing/legal web pages on `bluey.dev`
- [ ] Monitoring (Carnaval / equivalent)

---

## Notes for v0.2.x and v1.0

- **Auto-update**: Tauri's built-in updater is signed-only and currently
  out of scope. Replacement path is "next release's `install.sh`
  overwrites prior install". Could add a `bluey check-update` CLI
  command later that hits a release manifest endpoint.
- **Apple Developer ID + notarized DMG**: Optional v1.0 polish for
  non-technical buyers. Not a v0.2 alpha gate. The ad-hoc-signed
  install path is sufficient for the developer/early-adopter audience.
- **Mission Control / Stage Manager / Cmd-Tab**: A coworker physically
  next to the laptop can still see the overlay window if they trigger
  Mission Control on the customer's machine. This is intentional — we
  defend against screen-share + recording, not physical observation.
