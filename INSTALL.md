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

Download the release archive matching your platform:
- `bluey-{version}-darwin-arm64.tar.gz` (Apple Silicon)
- `bluey-{version}-darwin-x86_64.tar.gz` (Intel Mac)
- `bluey-{version}-linux-x86_64.tar.gz` (Linux x86_64)

Archives contain binaries under a `bin/` subdirectory:

```bash
# Example for Apple Silicon:
curl -LO https://github.com/<org>/bluey/releases/latest/download/bluey-0.1.0-darwin-arm64.tar.gz
mkdir -p /usr/local/lib/bluey && tar -xzf bluey-0.1.0-darwin-arm64.tar.gz -C /usr/local/lib/bluey
ln -sf /usr/local/lib/bluey/bin/bluey /usr/local/bin/bluey
ln -sf /usr/local/lib/bluey/bin/bluey-daemon /usr/local/bin/bluey-daemon
```

### Windows

Download `bluey-{version}-windows-x86_64.zip` from the
[latest release](https://github.com/<org>/bluey/releases/latest),
extract, and add the `bin\` folder to your PATH.

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
