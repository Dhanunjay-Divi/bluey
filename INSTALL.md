# Installing Bluey

## macOS (Homebrew)

```bash
brew tap <org>/bluey
brew install bluey
```

On first launch, grant permissions when prompted:
- **Microphone** — meeting audio capture
- **Screen Recording** — screen-aware context
- **Accessibility** — overlay positioning

These can be managed in **System Settings → Privacy & Security**.

## Windows (Scoop)

```powershell
scoop bucket add bluey https://github.com/<org>/scoop-bluey
scoop install bluey
```

## Manual Install

### macOS / Linux

```bash
# Download the latest release for your platform
curl -LO https://github.com/<org>/bluey/releases/latest/download/bluey-darwin-arm64.tar.gz
tar -xzf bluey-darwin-arm64.tar.gz -C /usr/local/bin
```

### Windows

Download `bluey-windows-x86_64.zip` from the
[latest release](https://github.com/<org>/bluey/releases/latest),
extract, and add the folder to your PATH.

## Running

```bash
# Start the daemon
bluey-daemon &

# Use the CLI
bluey on --title "My meeting"
bluey off

# Open the dashboard (macOS Homebrew install)
bluey-dashboard
```

## Auto-Update

The dashboard includes a built-in updater (Tauri updater plugin) that checks
GitHub Releases for new versions. No action needed — you'll be prompted when
an update is available.

## Uninstall

### Homebrew
```bash
brew uninstall bluey
brew untap <org>/bluey
```

### Scoop
```powershell
scoop uninstall bluey
scoop bucket rm bluey
```

### Manual
Remove the binaries from wherever you placed them and delete `~/.bluey/`
(macOS/Linux) or `%USERPROFILE%\.bluey\` (Windows).
