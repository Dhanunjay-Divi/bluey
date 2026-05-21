#!/usr/bin/env bash
# Bluey one-line installer.
#
# Usage: curl -fsSL https://bluey.dev/install.sh | bash
#
# This installer ships Bluey on macOS without requiring an Apple
# Developer ID. We ad-hoc sign the bundle (recognised by Gatekeeper
# as "self-signed") and remove the quarantine bit so first launch
# does not hit the "can't verify developer" hard-block. Same pattern
# Pinky uses.
#
# Refuses to run on unsupported platforms.

set -euo pipefail

# ── Configuration ────────────────────────────────────────────────────
BLUEY_VERSION="${BLUEY_VERSION:-latest}"
DOWNLOAD_HOST="${BLUEY_DOWNLOAD_HOST:-https://bluey.dev}"
INSTALL_DIR="${BLUEY_INSTALL_DIR:-/Applications}"
CLI_DIR="${BLUEY_CLI_DIR:-/usr/local/bin}"

# ── Colors ──────────────────────────────────────────────────────────
if [ -t 1 ]; then
    BOLD="$(tput bold 2>/dev/null || true)"
    DIM="$(tput dim 2>/dev/null || true)"
    GREEN="$(tput setaf 2 2>/dev/null || true)"
    RED="$(tput setaf 1 2>/dev/null || true)"
    BLUE="$(tput setaf 4 2>/dev/null || true)"
    RESET="$(tput sgr0 2>/dev/null || true)"
else
    BOLD="" DIM="" GREEN="" RED="" BLUE="" RESET=""
fi

say()  { printf "%s%s%s\n" "$BLUE$BOLD" "$1" "$RESET"; }
ok()   { printf "%s✓%s %s\n" "$GREEN" "$RESET" "$1"; }
warn() { printf "%s⚠%s %s\n" "$RED" "$RESET" "$1" >&2; }
fail() { warn "$1"; exit 1; }

# ── Pre-flight ───────────────────────────────────────────────────────
case "$(uname -s)" in
    Darwin) ;;
    *) fail "Bluey currently supports macOS only. Detected: $(uname -s)" ;;
esac

ARCH="$(uname -m)"
case "$ARCH" in
    arm64|aarch64) ARCH=aarch64 ;;
    x86_64|amd64)  ARCH=x86_64 ;;
    *) fail "Unsupported architecture: $ARCH" ;;
esac

if ! command -v curl >/dev/null; then
    fail "curl is required (preinstalled on macOS — your environment is unusual)"
fi

if [ "$EUID" -eq 0 ]; then
    warn "Running as root. Bluey installs into /Applications which is fine,"
    warn "but we recommend installing as your normal user. Continuing anyway."
fi

# ── Download ─────────────────────────────────────────────────────────
say "Downloading Bluey ($BLUEY_VERSION, $ARCH)..."
TARBALL="$(mktemp -t bluey).tar.gz"
trap 'rm -f "$TARBALL"' EXIT

URL="$DOWNLOAD_HOST/releases/$BLUEY_VERSION/Bluey-$ARCH.tar.gz"
if ! curl -fsSL --retry 3 -o "$TARBALL" "$URL"; then
    fail "Download failed from $URL"
fi
ok "Downloaded $(stat -f%z "$TARBALL" 2>/dev/null || stat -c%s "$TARBALL") bytes"

# ── Extract ──────────────────────────────────────────────────────────
say "Installing to $INSTALL_DIR/Bluey.app..."
WORKDIR="$(mktemp -d -t bluey-install)"
trap 'rm -rf "$WORKDIR" "$TARBALL"' EXIT

tar -xzf "$TARBALL" -C "$WORKDIR"

if [ ! -d "$WORKDIR/Bluey.app" ]; then
    fail "Tarball did not contain Bluey.app at the top level"
fi

# Replace any prior install.
if [ -d "$INSTALL_DIR/Bluey.app" ]; then
    say "Replacing existing $INSTALL_DIR/Bluey.app..."
    rm -rf "$INSTALL_DIR/Bluey.app"
fi
mv "$WORKDIR/Bluey.app" "$INSTALL_DIR/Bluey.app"
ok "Installed $INSTALL_DIR/Bluey.app"

# ── Ad-hoc sign ──────────────────────────────────────────────────────
# Without this, Gatekeeper blocks first-launch entirely. With ad-hoc
# signing the customer sees right-click→Open at most.
say "Ad-hoc signing the bundle..."
if codesign --force --deep --sign - "$INSTALL_DIR/Bluey.app" 2>/dev/null; then
    ok "Ad-hoc signed"
else
    warn "codesign failed — you may see a Gatekeeper prompt on first launch."
    warn "Right-click Bluey.app → Open to bypass it once."
fi

# ── Remove quarantine ────────────────────────────────────────────────
# Strip the com.apple.quarantine extended attribute so Gatekeeper
# doesn't gate the first-launch warning at all.
xattr -dr com.apple.quarantine "$INSTALL_DIR/Bluey.app" 2>/dev/null || true
ok "Quarantine attribute cleared"

# ── Register URL scheme ──────────────────────────────────────────────
# Forces Launch Services to pick up our bluey:// scheme registration
# from Info.plist.
LSREGISTER="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"
if [ -x "$LSREGISTER" ]; then
    "$LSREGISTER" -f "$INSTALL_DIR/Bluey.app" 2>/dev/null || true
    ok "bluey:// URL scheme registered"
fi

# ── CLI symlink ──────────────────────────────────────────────────────
if [ -d "$CLI_DIR" ] && [ -w "$CLI_DIR" ]; then
    if [ -f "$INSTALL_DIR/Bluey.app/Contents/Resources/bluey-cli" ]; then
        ln -sf "$INSTALL_DIR/Bluey.app/Contents/Resources/bluey-cli" \
            "$CLI_DIR/bluey"
        ok "CLI symlink: $CLI_DIR/bluey"
    fi
fi

# ── Done ─────────────────────────────────────────────────────────────
echo
say "Bluey installed."
echo
printf "  ${BOLD}Open Bluey from Applications${RESET}, or run from Spotlight.\n"
printf "  Sign in: a browser tab will open. After signing in, the\n"
printf "  Bluey app picks up your tokens automatically.\n"
echo
printf "  ${DIM}First launch will ask for Accessibility permission so\n"
printf "  the F19 hotkey works. Grant it and you're ready.${RESET}\n"
echo
