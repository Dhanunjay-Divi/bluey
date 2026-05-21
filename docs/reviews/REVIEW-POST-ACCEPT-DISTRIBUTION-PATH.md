# REVIEW: Post-Acceptance Distribution Path

**Commit range:** `9aeec02..d531a2c`
**Reviewer:** Codex
**Date:** 2026-05-21

## Per-Task Review

### Unsigned macOS Install Path

| Field | Value |
|-------|-------|
| Files | `ops/install/install.sh`, `.github/workflows/release.yml`, `docs/PRELAUNCH-CHECKLIST.md` |
| Verdict | 🟢 accept after Codex fix |

**Findings:**
- 🔴 Found and fixed during review: `install.sh` required a top-level `Bluey.app`, but the release workflow only produced a `bin/` terminal archive. The release workflow now builds the Tauri dashboard app on macOS and packages `Bluey.app` alongside `bin/bluey`.
- 🔴 Found and fixed during review: artifact names did not match the existing release convention. `install.sh`, the cask, and checklist now use `bluey-0.2.0-darwin-arm64.tar.gz`.
- 🟢 The installer ad-hoc signs the installed app and clears quarantine in the user-invoked install path.
- 🟢 `install.sh` now verifies `SHA256SUMS.txt` when present, with `BLUEY_SKIP_CHECKSUM=1` as the explicit escape hatch.
- 🟡 Clean-Mac QA is still required. Ad-hoc signing plus `xattr -dr` is acceptable for the alpha path, but macOS behavior must be validated on the exact supported OS versions before public announcement.

---

### Homebrew Cask

| Field | Value |
|-------|-------|
| Files | `ops/Casks/bluey.rb` |
| Verdict | 🟢 accept after Codex fix |

**Findings:**
- 🔴 Found and fixed during review: `uninstall quit:` used `com.bluey.app`, but the actual Tauri bundle identifier is `com.bluey.dashboard`.
- 🔴 Found and fixed during review: the cask `binary` stanza pointed at `Bluey.app/Contents/Resources/bluey-cli`, which was not produced by the existing artifact. The release artifact now includes top-level `bin/bluey`, and the cask links that path.
- 🟡 `sha256 :no_check` is acceptable for alpha only. The file now carries an explicit TODO to pin checksums once release artifacts are stable.
- 🟢 The cask is scoped to arm64, matching the current release workflow.

---

### Auto-Update Story

| Field | Value |
|-------|-------|
| Files | `crates/cue-dashboard/tauri.conf.json`, `ops/install/install.sh`, `docs/PRELAUNCH-CHECKLIST.md` |
| Verdict | 🟢 accept with follow-up |

**Findings:**
- 🟢 Dropping signed/notarized auto-update as a v0.2 alpha gate is reasonable for the terminal/cask install path.
- 🟡 Follow-up: add a small `bluey check-update` or `bluey update` command that checks a release manifest and reruns the installer or prints the exact command. This is not a blocker for v0.2 alpha.

## Cross-Task Findings

- The user was right that Apple Developer ID should not block v0.2 alpha. The product can ship a Pinky-style unsigned/ad-hoc-signed alpha as long as the installer is transparent, user-invoked, and clean-Mac tested.
- The original three commits were not shippable as-is because the new installer/cask described an app-bundle artifact that CI did not produce. The Codex fix closes that packaging mismatch.
- The current distribution scope is macOS arm64. Intel macOS should not be promised until the helper/app bundle is built and smoked for that target.

## Build & Test Verification

```bash
bash -n ops/install/install.sh                                      # ✅
ruby -c ops/Casks/bluey.rb                                          # ✅
ruby -e 'require "yaml"; YAML.load_file(".github/workflows/release.yml")' # ✅
ops/install/install.sh local file:// fake-release smoke             # ✅
git diff --check                                                    # ✅
```

**Not run locally:**
- `brew audit/style` against a real tap. `ruby -c` passed; Homebrew's local `brew style --cask ops/Casks/bluey.rb` treats the path like a tap formula name on this machine.
- Clean-Mac install smoke from `https://bluey.dev`. Requires hosted artifacts.

## Overall Verdict

🟢 **ACCEPT** — after the Codex packaging corrections, the unsigned macOS install direction is acceptable for v0.2 alpha.

## Follow-ups for Operator Gate

- Publish `bluey-0.2.0-darwin-arm64.tar.gz` containing both `Bluey.app` and `bin/bluey`.
- Publish matching `SHA256SUMS.txt`.
- Run clean-Mac smoke for `curl -fsSL https://bluey.dev/install.sh | bash`.
- Run clean-Mac smoke for `brew tap bluey-dev/bluey && brew install --cask bluey`.
- Add `bluey check-update` / `bluey update` in a v0.2.x follow-up.
