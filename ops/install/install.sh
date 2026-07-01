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

path_has_dir() {
    case ":$PATH:" in
        *":$1:"*) return 0 ;;
        *) return 1 ;;
    esac
}

append_path_profile_line() {
    local profile="$1"
    local dir="$2"
    local export_line='export PATH="$HOME/.local/bin:$PATH"'

    [ "$dir" = "$HOME/.local/bin" ] || return 0
    if [ -f "$profile" ] && grep -Fq "$export_line" "$profile"; then
        return 0
    fi
    mkdir -p "$(dirname "$profile")"
    {
        printf "\n# Added by Bluey installer.\n"
        printf "%s\n" "$export_line"
    } >> "$profile"
}

ensure_user_path_entry() {
    local dir="$1"
    path_has_dir "$dir" && return 0

    local shell_name
    shell_name="$(basename "${SHELL:-}")"
    case "$shell_name" in
        zsh)
            append_path_profile_line "$HOME/.zprofile" "$dir"
            append_path_profile_line "$HOME/.zshrc" "$dir"
            ;;
        bash)
            append_path_profile_line "$HOME/.bash_profile" "$dir"
            append_path_profile_line "$HOME/.bashrc" "$dir"
            ;;
        *)
            append_path_profile_line "$HOME/.profile" "$dir"
            ;;
    esac
}

link_cli_pair() {
    local dir="$1"
    local sudo_prefix="${2:-}"

    if [ -n "$sudo_prefix" ]; then
        $sudo_prefix mkdir -p "$dir"
        $sudo_prefix ln -sf "$CLI_SOURCE" "$dir/bluey"
        if [ -x "$DAEMON_SOURCE" ]; then
            $sudo_prefix ln -sf "$DAEMON_SOURCE" "$dir/bluey-daemon"
        fi
    else
        mkdir -p "$dir"
        ln -sf "$CLI_SOURCE" "$dir/bluey"
        if [ -x "$DAEMON_SOURCE" ]; then
            ln -sf "$DAEMON_SOURCE" "$dir/bluey-daemon"
        fi
    fi
}

try_sudo_cli_link() {
    local dir="$1"

    [ "${BLUEY_INSTALL_NO_SUDO:-0}" != "1" ] || return 1
    [ "$dir" = "/usr/local/bin" ] || return 1
    command -v sudo >/dev/null 2>&1 || return 1
    [ -r /dev/tty ] || return 1

    say "Installing Bluey command to $dir..."
    printf "  macOS may ask for your password to make ${BOLD}bluey${RESET} available in every terminal.\n"
    if sudo -v </dev/tty; then
        link_cli_pair "$dir" sudo
        return 0
    fi
    warn "Could not get sudo permission; falling back to a user-local command path."
    return 1
}

install_local_doc_tools() {
    local root="$1"
    local tools_dir="$root/tools/doc-converter"
    local wrapper="$root/bin/bluey-doc-converter"

    if [ "${BLUEY_SKIP_LOCAL_TOOLS:-0}" = "1" ]; then
        warn "Skipping Bluey document tools because BLUEY_SKIP_LOCAL_TOOLS=1"
        return 0
    fi
    if ! command -v python3 >/dev/null 2>&1; then
        warn "python3 was not found; document conversion will use built-in fallbacks only"
        return 0
    fi

    say "Installing Bluey document tools..."
    mkdir -p "$tools_dir" "$root/bin"
    if ! python3 -m venv "$tools_dir/.venv" >/dev/null 2>&1; then
        warn "Could not prepare Bluey document tools; document conversion will use built-in fallbacks only"
        return 0
    fi

    local py="$tools_dir/.venv/bin/python"
    "$py" -m pip install --disable-pip-version-check --upgrade pip >/dev/null 2>&1 || true
    if ! "$py" -m pip install --disable-pip-version-check "markitdown[all]" >/dev/null 2>&1; then
        if ! "$py" -m pip install --disable-pip-version-check markitdown >/dev/null 2>&1; then
            warn "Could not install MarkItDown for Bluey document tools; document conversion will use built-in fallbacks only"
            return 0
        fi
    fi

    cat > "$wrapper" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
exec "$ROOT/tools/doc-converter/.venv/bin/markitdown" "$@"
SH
    chmod +x "$wrapper"
    ok "Bluey document tools installed"
}

# ── Pre-flight ───────────────────────────────────────────────────────
case "$(uname -s)" in
    Darwin) ;;
    *) fail "Bluey currently supports macOS only. Detected: $(uname -s)" ;;
esac

ARCH="$(uname -m)"
case "$ARCH" in
    arm64|aarch64) PLATFORM=darwin-arm64 ;;
    x86_64|amd64)  PLATFORM=darwin-x86_64 ;;
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

if [ "${BLUEY_SKIP_CHECKSUM:-0}" != "1" ]; then
    command -v shasum >/dev/null || fail "shasum is required to verify the Bluey download. Install shasum and rerun the Bluey installer."
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
        fail "Checksum manifest unavailable for $ARTIFACT; refusing unverified download"
    fi
else
    warn "Skipping checksum because BLUEY_SKIP_CHECKSUM=1"
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

install_local_doc_tools "$INSTALL_ROOT"

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
DAEMON_SOURCE="$INSTALL_ROOT/bin/bluey-daemon"
CLI_LINK_PATH=""
if [ -n "${BLUEY_CLI_DIR:-}" ] || { [ -d "$CLI_DIR" ] && [ -w "$CLI_DIR" ]; }; then
    link_cli_pair "$CLI_DIR"
    ok "CLI symlink: $CLI_DIR/bluey"
    CLI_LINK_PATH="$CLI_DIR/bluey"
elif try_sudo_cli_link "$CLI_DIR"; then
    ok "CLI symlink: $CLI_DIR/bluey"
    CLI_LINK_PATH="$CLI_DIR/bluey"
elif [ -d "$HOME/.local/bin" ] || mkdir -p "$HOME/.local/bin" 2>/dev/null; then
    link_cli_pair "$HOME/.local/bin"
    ok "CLI symlink: $HOME/.local/bin/bluey"
    CLI_LINK_PATH="$HOME/.local/bin/bluey"
    if ! path_has_dir "$HOME/.local/bin"; then
        ensure_user_path_entry "$HOME/.local/bin"
        ok "Added $HOME/.local/bin to your shell PATH for new terminals"
    fi
else
    warn "Could not create a CLI symlink. Run $CLI_SOURCE directly."
    CLI_LINK_PATH="$CLI_SOURCE"
fi

# ── Done ─────────────────────────────────────────────────────────────
echo
say "Bluey installed."
echo
if command -v bluey >/dev/null 2>&1 || path_has_dir "$(dirname "$CLI_LINK_PATH")"; then
    printf "  ${BOLD}Run:${RESET} bluey on\n"
else
    printf "  ${BOLD}Run:${RESET} %s on\n" "$CLI_LINK_PATH"
    printf "  Open a new terminal after install and ${BOLD}bluey on${RESET} will work normally.\n"
fi
printf "  Bluey opens the browser sign-in flow only when your account\n"
printf "  is not already linked. Use ${BOLD}bluey off${RESET} to stop it.\n"
printf "  To remove the app later, run ${BOLD}bluey uninstall${RESET}.\n"
echo
printf "  ${DIM}First launch may ask for Accessibility permission so\n"
printf "  Bluey shortcuts work. Press Ctrl+Option+B to minimize/restore.${RESET}\n"
echo
