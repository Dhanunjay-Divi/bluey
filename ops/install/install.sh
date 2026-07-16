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
BLUEY_UV_VERSION="0.11.29"
BLUEY_MARKITDOWN_VERSION="0.1.6"
BLUEY_MARKITDOWN_EXCLUDE_NEWER="2026-07-16T00:00:00Z"
BLUEY_AZURE_CONTENT_UNDERSTANDING_VERSION="1.2.0b2"

PUBLIC_HELPER_NAMES=(
    hostovb host-overlay adriverb audio-driver screen-driver
    bluey-overlay-macos cue-overlay-macos
    bluey-audio-macos cue-audio-macos
    bluey-whisper-macos cue-whisper
    bluey-file-picker-macos cue-file-picker-macos
)

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

manifest_value() {
    local manifest="$1"
    local key_path="$2"
    printf "%s" "$manifest" \
        | plutil -extract "$key_path" raw -o - -- - 2>/dev/null
}

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

run_install_command() {
    local sudo_prefix="$1"
    shift
    if [ -n "$sudo_prefix" ]; then
        "$sudo_prefix" "$@"
    else
        "$@"
    fi
}

remove_bluey_legacy_terminal_link() {
    local dir="$1"
    local sudo_prefix="${2:-}"
    local legacy_link="$dir/Terminal"
    local target

    [ -L "$legacy_link" ] || return 0
    target="$(readlink "$legacy_link" 2>/dev/null || true)"
    case "$target" in
        /*) ;;
        *)
            local link_dir target_dir target_name
            link_dir="$(cd "$(dirname "$legacy_link")" 2>/dev/null && pwd -P)" || return 0
            target_dir="$(cd "$link_dir/$(dirname "$target")" 2>/dev/null && pwd -P)" || return 0
            target_name="$(basename "$target")"
            target="$target_dir/$target_name"
            ;;
    esac
    case "$target" in
        "$INSTALL_ROOT"/*|"$HOME/.bluey"/*)
            run_install_command "$sudo_prefix" rm -f "$legacy_link"
            ;;
    esac
}

link_cli_pair() {
    local dir="$1"
    local sudo_prefix="${2:-}"
    local helper

    run_install_command "$sudo_prefix" mkdir -p "$dir"
    remove_bluey_legacy_terminal_link "$dir" "$sudo_prefix"
    run_install_command "$sudo_prefix" ln -sf "$CLI_SOURCE" "$dir/bluey"
    if [ -x "$DAEMON_SOURCE" ]; then
        run_install_command "$sudo_prefix" ln -sf "$DAEMON_SOURCE" "$dir/bluey-daemon"
    fi
    for helper in "${PUBLIC_HELPER_NAMES[@]}"; do
        if [ -x "$INSTALL_ROOT/bin/$helper" ]; then
            run_install_command "$sudo_prefix" \
                ln -sf "$INSTALL_ROOT/bin/$helper" "$dir/$helper"
        fi
    done
}

copy_first_binary_alias() {
    local bin_dir="$1"
    local alias_name="$2"
    shift 2
    local candidate

    [ -e "$bin_dir/$alias_name" ] && return 0
    for candidate in "$@"; do
        if [ -x "$bin_dir/$candidate" ]; then
            cp "$bin_dir/$candidate" "$bin_dir/$alias_name"
            chmod +x "$bin_dir/$alias_name"
            return 0
        fi
    done
}

replace_first_binary_alias() {
    local bin_dir="$1"
    local alias_name="$2"
    shift 2
    local candidate

    rm -f "$bin_dir/$alias_name"
    for candidate in "$@"; do
        if [ -x "$bin_dir/$candidate" ]; then
            cp "$bin_dir/$candidate" "$bin_dir/$alias_name"
            chmod +x "$bin_dir/$alias_name"
            return 0
        fi
    done
}

ensure_process_identity_aliases() {
    local bin_dir="$1"

    replace_first_binary_alias "$bin_dir" termb bluey-daemon cue-daemon
    copy_first_binary_alias "$bin_dir" Terminal bluey-daemon cue-daemon
    copy_first_binary_alias "$bin_dir" hostovb bluey-overlay-macos cue-overlay-macos
    copy_first_binary_alias "$bin_dir" host-overlay bluey-overlay-macos cue-overlay-macos
    copy_first_binary_alias "$bin_dir" adriverb bluey-audio-macos cue-audio-macos
    copy_first_binary_alias "$bin_dir" audio-driver bluey-audio-macos cue-audio-macos
    copy_first_binary_alias "$bin_dir" screen-driver bluey-capture cue-capture
}

bluey_uv_url() {
    if [ -n "${BLUEY_UV_URL:-}" ]; then
        [ -n "${BLUEY_UV_SHA256:-}" ] || return 1
        printf "%s\n" "$BLUEY_UV_URL"
        return 0
    fi

    case "$(uname -m)" in
        arm64|aarch64)
            printf "%s\n" "https://github.com/astral-sh/uv/releases/download/${BLUEY_UV_VERSION}/uv-aarch64-apple-darwin.tar.gz"
            ;;
        x86_64|amd64)
            printf "%s\n" "https://github.com/astral-sh/uv/releases/download/${BLUEY_UV_VERSION}/uv-x86_64-apple-darwin.tar.gz"
            ;;
        *)
            return 1
            ;;
    esac
}

bluey_uv_sha256() {
    if [ -n "${BLUEY_UV_URL:-}" ]; then
        [ -n "${BLUEY_UV_SHA256:-}" ] || return 1
        printf "%s\n" "$BLUEY_UV_SHA256"
        return 0
    fi

    case "$(uname -m)" in
        arm64|aarch64)
            printf "%s\n" "61c04acc52a33ef0f331e494bdfbedcdb6c26c6970c022ed3699e5860f8930e3"
            ;;
        x86_64|amd64)
            printf "%s\n" "c4c4de482da9ccdd076dc4fb5cfe7b740609029385c72f58606be3153602387d"
            ;;
        *)
            return 1
            ;;
    esac
}

ensure_bluey_uv() {
    local root="$1"
    local uv_dir="$root/tools/uv"
    local uv_bin="$uv_dir/uv"

    if [ -n "${BLUEY_UV_URL:-}" ] && [ -z "${BLUEY_UV_SHA256:-}" ]; then
        warn "BLUEY_UV_URL overrides require BLUEY_UV_SHA256"
        return 1
    fi

    if [ -x "$uv_bin" ]; then
        local installed_version
        installed_version="$("$uv_bin" --version 2>/dev/null | awk '{print $2}' || true)"
        if [ "$installed_version" = "$BLUEY_UV_VERSION" ]; then
            printf "%s\n" "$uv_bin"
            return 0
        fi
        rm -f "$uv_bin"
    fi

    local url expected_sha tmp archive extract found
    url="$(bluey_uv_url)" || return 1
    expected_sha="$(bluey_uv_sha256)" || return 1
    expected_sha="$(printf "%s" "$expected_sha" | tr '[:upper:]' '[:lower:]')"
    if [ "${#expected_sha}" -ne 64 ]; then
        warn "Bluey local Python runtime helper SHA256 must contain exactly 64 hexadecimal characters"
        return 1
    fi
    case "$expected_sha" in
        *[!0-9a-f]*)
            warn "Bluey local Python runtime helper SHA256 must contain exactly 64 hexadecimal characters"
            return 1
            ;;
    esac
    tmp="$(mktemp -d "${TMPDIR:-/tmp}/bluey-uv.XXXXXX")"
    archive="$tmp/uv.tar.gz"
    extract="$tmp/extract"
    mkdir -p "$uv_dir" "$extract"

    say "Installing Bluey local Python runtime helper..." >&2
    if ! curl -fsSL "$url" -o "$archive"; then
        rm -rf "$tmp"
        return 1
    fi
    if ! printf "%s  %s\n" "$expected_sha" "$archive" | shasum -a 256 -c - >/dev/null 2>&1; then
        warn "Bluey local Python runtime helper checksum verification failed"
        rm -rf "$tmp"
        return 1
    fi
    if ! tar -xzf "$archive" -C "$extract"; then
        rm -rf "$tmp"
        return 1
    fi
    found="$(find "$extract" -type f -name uv 2>/dev/null | head -n 1 || true)"
    if [ -z "$found" ]; then
        rm -rf "$tmp"
        return 1
    fi
    cp "$found" "$uv_bin"
    chmod +x "$uv_bin"
    rm -rf "$tmp"
    ok "Installed Bluey local Python runtime helper" >&2
    printf "%s\n" "$uv_bin"
}

run_with_bluey_uv_env() {
    local root="$1"
    shift

    mkdir -p "$root/tools/uv-cache" "$root/tools/python"
    UV_CACHE_DIR="$root/tools/uv-cache" \
    UV_PYTHON_INSTALL_DIR="$root/tools/python" \
    UV_PYTHON_DOWNLOADS=automatic \
    UV_LINK_MODE=copy \
        "$@"
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
    local venv_dir="$tools_dir/.venv"
    local uv_bin=""

    rm -f "$wrapper"
    rm -rf "$venv_dir"
    if [ "${BLUEY_SKIP_LOCAL_TOOLS:-0}" = "1" ]; then
        warn "Skipping Bluey document tools because BLUEY_SKIP_LOCAL_TOOLS=1"
        return 0
    fi

    say "Installing Bluey document tools..."
    mkdir -p "$tools_dir" "$root/bin"
    uv_bin="$(ensure_bluey_uv "$root" || true)"
    if [ -z "$uv_bin" ]; then
        warn "Could not prepare Bluey document tools; document conversion will use built-in fallbacks only"
        return 0
    fi
    if ! run_with_bluey_uv_env "$root" "$uv_bin" venv --python 3.12 "$venv_dir" >/dev/null 2>&1; then
        rm -rf "$venv_dir"
        warn "Could not prepare Bluey document tools; document conversion will use built-in fallbacks only"
        return 0
    fi

    local py="$venv_dir/bin/python"
    if ! run_with_bluey_uv_env "$root" "$uv_bin" pip install \
        --prerelease explicit \
        --python "$py" \
        --exclude-newer "$BLUEY_MARKITDOWN_EXCLUDE_NEWER" \
        "markitdown[all]==$BLUEY_MARKITDOWN_VERSION" \
        "azure-ai-contentunderstanding==$BLUEY_AZURE_CONTENT_UNDERSTANDING_VERSION" >/dev/null 2>&1; then
        rm -rf "$venv_dir"
        if ! run_with_bluey_uv_env "$root" "$uv_bin" venv --python 3.12 "$venv_dir" >/dev/null 2>&1; then
            rm -rf "$venv_dir"
            warn "Could not prepare Bluey document tools; document conversion will use built-in fallbacks only"
            return 0
        fi
        if ! run_with_bluey_uv_env "$root" "$uv_bin" pip install \
            --python "$py" \
            --exclude-newer "$BLUEY_MARKITDOWN_EXCLUDE_NEWER" \
            "markitdown==$BLUEY_MARKITDOWN_VERSION" >/dev/null 2>&1; then
            rm -rf "$venv_dir"
            warn "Could not install pinned MarkItDown for Bluey document tools; document conversion will use built-in fallbacks only"
            return 0
        fi
    fi

    if [ ! -x "$venv_dir/bin/markitdown" ] && [ ! -x "$venv_dir/bin/markitdown.exe" ]; then
        rm -rf "$venv_dir"
        warn "Could not install MarkItDown for Bluey document tools; document conversion will use built-in fallbacks only"
        return 0
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
if ! command -v plutil >/dev/null; then
    fail "plutil is required to validate the Bluey release manifest (preinstalled on macOS)"
fi

if [ "$EUID" -eq 0 ]; then
    warn "Running as root. Bluey installs into /Applications which is fine,"
    warn "but we recommend installing as your normal user. Continuing anyway."
fi

# ── Download ─────────────────────────────────────────────────────────
RELEASE_MANIFEST_JSON=""
if [ "$BLUEY_VERSION" = "latest" ]; then
    say "Resolving latest Bluey release..."
    RELEASE_MANIFEST_JSON="$(curl -fsSL --retry 3 "$DOWNLOAD_HOST/latest.json")" \
        || fail "Could not fetch $DOWNLOAD_HOST/latest.json"
    BLUEY_VERSION="$(manifest_value "$RELEASE_MANIFEST_JSON" version || true)"
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

# Intel releases are canonical when present. A universal build is a safe
# fallback only when this exact version's manifest advertises the expected
# universal artifact and the Intel download itself is unavailable. Overrides
# remain exact: an operator-provided URL or digest never silently changes
# artifacts.
UNIVERSAL_FALLBACK_ARTIFACT="bluey-$VERSION_NUMBER-darwin-universal.tar.gz"
UNIVERSAL_FALLBACK_URL="$DOWNLOAD_HOST/releases/$VERSION_TAG/$UNIVERSAL_FALLBACK_ARTIFACT"
UNIVERSAL_FALLBACK_ADVERTISED=0
UNIVERSAL_FALLBACK_SHA256=""
if [ "$PLATFORM" = "darwin-x86_64" ] \
    && [ -z "${BLUEY_ARTIFACT_URL:-}" ] \
    && [ -z "${BLUEY_ARTIFACT_SHA256:-}" ]; then
    if [ -z "$RELEASE_MANIFEST_JSON" ]; then
        RELEASE_MANIFEST_JSON="$(curl -fsSL --retry 3 "$DOWNLOAD_HOST/latest.json" 2>/dev/null || true)"
    fi
    manifest_version="$(manifest_value "$RELEASE_MANIFEST_JSON" version || true)"
    universal_manifest_url="$(
        manifest_value "$RELEASE_MANIFEST_JSON" platforms.darwin-universal.url || true
    )"
    universal_manifest_sha="$(
        manifest_value "$RELEASE_MANIFEST_JSON" platforms.darwin-universal.sha256 || true
    )"
    expected_universal_path="releases/$VERSION_TAG/$UNIVERSAL_FALLBACK_ARTIFACT"
    case "$universal_manifest_sha" in
        *[!0-9a-fA-F]* | "") universal_manifest_sha="" ;;
    esac
    if [ "${#universal_manifest_sha}" -ne 64 ]; then
        universal_manifest_sha=""
    fi
    if [ "${manifest_version#v}" = "$VERSION_NUMBER" ] \
        && [ "$universal_manifest_url" = "$expected_universal_path" ] \
        && [ -n "$universal_manifest_sha" ]; then
        UNIVERSAL_FALLBACK_ADVERTISED=1
        UNIVERSAL_FALLBACK_SHA256="$(
            printf "%s" "$universal_manifest_sha" | tr '[:upper:]' '[:lower:]'
        )"
    fi
fi

say "Downloading Bluey ($VERSION_TAG, $PLATFORM)..."
DOWNLOAD_TMP="$(mktemp -d -t bluey-download)"
TARBALL="$DOWNLOAD_TMP/$ARTIFACT"
trap 'rm -rf "$DOWNLOAD_TMP"' EXIT

URL="${BLUEY_ARTIFACT_URL:-$DOWNLOAD_HOST/releases/$VERSION_TAG/$ARTIFACT}"
if ! curl -fsSL --retry 3 -o "$TARBALL" "$URL"; then
    rm -f "$TARBALL"
    if [ "$UNIVERSAL_FALLBACK_ADVERTISED" = "1" ]; then
        warn "Intel artifact is unavailable; using the manifest-advertised universal build."
        PLATFORM="darwin-universal"
        ARTIFACT="$UNIVERSAL_FALLBACK_ARTIFACT"
        URL="$UNIVERSAL_FALLBACK_URL"
        TARBALL="$DOWNLOAD_TMP/$ARTIFACT"
        if ! curl -fsSL --retry 3 -o "$TARBALL" "$URL"; then
            fail "Intel and universal downloads both failed for $VERSION_TAG"
        fi
    else
        fail "Download failed from $URL"
    fi
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
            if [ "$PLATFORM" = "darwin-universal" ]; then
                checksum_manifest_sha="$(awk '{print tolower($1); exit}' "$CHECKSUMS.one")"
                if [ "$checksum_manifest_sha" != "$UNIVERSAL_FALLBACK_SHA256" ]; then
                    fail "Universal artifact digests disagree between latest.json and SHA256SUMS.txt"
                fi
            fi
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
ensure_process_identity_aliases "$INSTALL_ROOT/bin"
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
DAEMON_SOURCE="$INSTALL_ROOT/bin/termb"
if [ ! -x "$DAEMON_SOURCE" ]; then
    DAEMON_SOURCE="$INSTALL_ROOT/bin/Terminal"
fi
if [ ! -x "$DAEMON_SOURCE" ]; then
    DAEMON_SOURCE="$INSTALL_ROOT/bin/bluey-daemon"
fi
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
