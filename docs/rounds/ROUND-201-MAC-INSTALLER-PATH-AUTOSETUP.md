# Round 201 - Mac Installer Path Autosetup

## Trigger

Owner showed a successful macOS install followed by:

`zsh: command not found: bluey`

The installer had created `~/.local/bin/bluey`, but the user's shell did not
have `~/.local/bin` in `PATH`. Owner asked that normal users should not need to
run:

```sh
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc
bluey on
```

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`
Workspace: `/Users/uno/Downloads/cue`
Branch: `codex/bluey-overlay-routing-hardening`
Round completed: 2026-06-26 16:12 EDT

## Root Cause

`ops/install/install.sh` already preferred `/usr/local/bin`, but on Macs where
that directory is not writable it fell back to `~/.local/bin` and only printed a
warning:

`Add ~/.local/bin to PATH if the bluey command is not found.`

That makes the install look successful while the next command fails for a normal
non-technical user.

## Fix

Updated `ops/install/install.sh`:

- If `/usr/local/bin` is not writable, the installer now asks for sudo and links:
  - `/usr/local/bin/bluey`
  - `/usr/local/bin/bluey-daemon`
- Sudo is used only for the command symlinks, not for the user install root.
- Users can opt out with `BLUEY_INSTALL_NO_SUDO=1`.
- If sudo is unavailable or declined, the installer falls back to
  `~/.local/bin`.
- The fallback now auto-adds `~/.local/bin` to shell startup files for new
  terminals:
  - zsh: `~/.zprofile` and `~/.zshrc`
  - bash: `~/.bash_profile` and `~/.bashrc`
  - other shells: `~/.profile`
- The final instruction now prints `bluey on` only when the command should be
  available in the current shell; otherwise it prints the full path for the
  current shell and notes that new terminals can use `bluey on`.

## Windows Parity

No Windows code change was needed. `ops/install/install.ps1` already calls
`Ensure-UserPathEntry -Dir $BinDir` and prints `Added Bluey to your user PATH`.

## Verification

Passed:

- `bash -n ops/install/install.sh`
- Temp fake-release install with:
  - `BLUEY_INSTALL_NO_SUDO=1`
  - temp `HOME`
  - temp `BLUEY_INSTALL_ROOT`
  - temp local artifact URL
  - `BLUEY_SKIP_CHECKSUM=1`
  - `BLUEY_SKIP_LOCAL_TOOLS=1`
- Verified the fallback path:
  - created `~/.local/bin/bluey`
  - wrote the PATH line to temp `~/.zprofile`
  - wrote the PATH line to temp `~/.zshrc`
  - installed command ran via the full path

## Release Gate

Do not manually publish only `install.sh` to production.

`latest.json` pins the installer SHA, so the live installer should be updated
through the signed release publish flow that regenerates `latest.json` and
`latest.json.sig`.
