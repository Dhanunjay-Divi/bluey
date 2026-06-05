#!/usr/bin/env bash
# Bluey one-line installer.
#
# Usage: curl -fsSL https://bluey.sh/install.sh | bash
#
# This installer ships the current terminal-first Bluey bundle on macOS
# without requiring an Apple Developer ID. It installs the CLI plus native
# helper binaries, ad-hoc signs them, clears quarantine, and creates a
# `bluey` command symlink when possible.
#
# Refuses to run on unsupported platforms.

set -euo pipefail

# ── Configuration ────────────────────────────────────────────────────
BLUEY_VERSION="${BLUEY_VERSION:-latest}"
DOWNLOAD_HOST="${BLUEY_DOWNLOAD_HOST:-https://bluey.sh}"
INSTALL_ROOT="${BLUEY_INSTALL_ROOT:-$HOME/.bluey}"
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
    arm64|aarch64) PLATFORM=darwin-arm64 ;;
    x86_64|amd64)  fail "Bluey v0.2 alpha installer currently supports Apple Silicon Macs only." ;;
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
if [ "$BLUEY_VERSION" = "latest" ]; then
    say "Resolving latest Bluey release..."
    LATEST_JSON="$(curl -fsSL --retry 3 "$DOWNLOAD_HOST/latest.json")" \
        || fail "Could not fetch $DOWNLOAD_HOST/latest.json"
    BLUEY_VERSION="$(printf "%s" "$LATEST_JSON" \
        | sed -nE 's/.*"version"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/p' \
        | head -1)"
    [ -n "$BLUEY_VERSION" ] || fail "latest.json did not include a version"
fi

VERSION_TAG="$BLUEY_VERSION"
VERSION_NUMBER="${BLUEY_VERSION#v}"
case "$VERSION_TAG" in
    latest) ARTIFACT="bluey-latest-$PLATFORM.tar.gz" ;;
    v*)     ARTIFACT="bluey-$VERSION_NUMBER-$PLATFORM.tar.gz" ;;
    *)      VERSION_TAG="v$VERSION_NUMBER"; ARTIFACT="bluey-$VERSION_NUMBER-$PLATFORM.tar.gz" ;;
esac

if [ -n "${BLUEY_ARTIFACT_URL:-}" ]; then
    ARTIFACT="$(basename "${BLUEY_ARTIFACT_URL%%\?*}")"
fi

say "Downloading Bluey ($VERSION_TAG, $PLATFORM)..."
DOWNLOAD_TMP="$(mktemp -d -t bluey-download)"
TARBALL="$DOWNLOAD_TMP/$ARTIFACT"
trap 'rm -rf "$DOWNLOAD_TMP"' EXIT

URL="${BLUEY_ARTIFACT_URL:-$DOWNLOAD_HOST/releases/$VERSION_TAG/$ARTIFACT}"
if ! curl -fsSL --retry 3 -o "$TARBALL" "$URL"; then
    fail "Download failed from $URL"
fi
ok "Downloaded $(stat -f%z "$TARBALL" 2>/dev/null || stat -c%s "$TARBALL") bytes"

if [ "${BLUEY_SKIP_CHECKSUM:-0}" != "1" ] && command -v shasum >/dev/null; then
    CHECKSUMS="$DOWNLOAD_TMP/SHA256SUMS.txt"
    if [ -n "${BLUEY_ARTIFACT_SHA256:-}" ]; then
        printf "%s  %s\n" "$BLUEY_ARTIFACT_SHA256" "$ARTIFACT" > "$CHECKSUMS.one"
        (cd "$(dirname "$CHECKSUMS")" && shasum -a 256 -c "$CHECKSUMS.one")
        ok "Checksum verified"
    elif curl -fsSL --retry 3 -o "$CHECKSUMS" "$DOWNLOAD_HOST/releases/$VERSION_TAG/SHA256SUMS.txt"; then
        if grep " $ARTIFACT\$" "$CHECKSUMS" > "$CHECKSUMS.one"; then
            (cd "$(dirname "$CHECKSUMS")" && shasum -a 256 -c "$CHECKSUMS.one")
            ok "Checksum verified"
        else
            fail "Checksum for $ARTIFACT not found in SHA256SUMS.txt"
        fi
    else
        warn "Checksum manifest unavailable; relying on HTTPS download."
    fi
fi

# ── Extract ──────────────────────────────────────────────────────────
say "Installing Bluey to $INSTALL_ROOT..."
WORKDIR="$(mktemp -d -t bluey-install)"
trap 'rm -rf "$WORKDIR" "$DOWNLOAD_TMP"' EXIT

tar -xzf "$TARBALL" -C "$WORKDIR"

if [ ! -x "$WORKDIR/bin/bluey" ]; then
    fail "Tarball did not contain bin/bluey"
fi

# Replace any prior terminal bundle.
mkdir -p "$INSTALL_ROOT"
rm -rf "$INSTALL_ROOT/bin"
cp -R "$WORKDIR/bin" "$INSTALL_ROOT/bin"
chmod +x "$INSTALL_ROOT/bin/"*
ok "Installed helper bundle"

# ── Ad-hoc sign ──────────────────────────────────────────────────────
say "Ad-hoc signing native binaries..."
signed_any=0
for bin in "$INSTALL_ROOT"/bin/*; do
    [ -f "$bin" ] || continue
    [ -x "$bin" ] || continue
    if codesign --force --sign - "$bin" 2>/dev/null; then
        signed_any=1
    fi
done
if [ "$signed_any" = "1" ]; then
    ok "Ad-hoc signed"
else
    warn "codesign did not sign any helper binaries; continuing."
fi

# ── Remove quarantine ────────────────────────────────────────────────
# Strip the com.apple.quarantine extended attribute if the tarball arrived
# through a quarantined path.
xattr -dr com.apple.quarantine "$INSTALL_ROOT/bin" 2>/dev/null || true
ok "Quarantine attribute cleared"

# ── CLI symlink ──────────────────────────────────────────────────────
CLI_SOURCE="$INSTALL_ROOT/bin/bluey"
if [ -d "$CLI_DIR" ] && [ -w "$CLI_DIR" ]; then
    ln -sf "$CLI_SOURCE" "$CLI_DIR/bluey"
    ok "CLI symlink: $CLI_DIR/bluey"
elif [ -d "$HOME/.local/bin" ] || mkdir -p "$HOME/.local/bin" 2>/dev/null; then
    ln -sf "$CLI_SOURCE" "$HOME/.local/bin/bluey"
    ok "CLI symlink: $HOME/.local/bin/bluey"
    case ":$PATH:" in
        *":$HOME/.local/bin:"*) ;;
        *) warn "Add $HOME/.local/bin to PATH if the bluey command is not found." ;;
    esac
else
    warn "Could not create a CLI symlink. Run $CLI_SOURCE directly."
fi

# ── Done ─────────────────────────────────────────────────────────────
echo
say "Bluey installed."
echo
printf "  ${BOLD}Run:${RESET} bluey on\n"
printf "  Bluey opens the browser sign-in flow only when your account\n"
printf "  is not already linked. Use ${BOLD}bluey off${RESET} to stop it.\n"
echo
printf "  ${DIM}First launch will ask for Accessibility permission so\n"
printf "  the F19 hotkey works. Grant it and you're ready.${RESET}\n"
echo
