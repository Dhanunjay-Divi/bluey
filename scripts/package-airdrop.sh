#!/usr/bin/env bash
# Assemble a SELF-CONTAINED AirDrop tarball from a completed build-macos.sh run.
#
# Why this exists: build-macos.sh stages the built binaries FLAT into
# dist/bluey-macos-<arch>/, but scripts/install.sh expects an archive that
# extracts to a bin/ subdir (it checks extract_dir/bin/bluey) AND it expects the
# installer itself to be run from the unpacked tarball. This script bridges the
# two: it lays the staged binaries out under bin/, drops the signing-enabled
# install.sh + a short README beside them, and tars the whole thing into ONE
# file. AirDrop that file; the receiver extracts and runs ./install.sh.
#
# The receiver flow (no Apple account, no Homebrew, no build tools needed):
#   tar -xzf bluey-<ver>-darwin-<arch>.tar.gz
#   cd bluey-<ver>-darwin-<arch>
#   ./install.sh                         # ad-hoc signs + strips quarantine
#   bluey on                             # first run downloads the STT model
#
# Prereq: run scripts/build-macos.sh first (with BLUEY_DIARIZE_BUILD=1 for
# speaker labels). This script does NOT build — it only packages what's staged.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

ARCH="$(uname -m)"
# install.sh names archives darwin-arm64 / darwin-x86_64; map uname -m to that.
case "$ARCH" in
  arm64) PLATFORM="darwin-arm64" ;;
  x86_64) PLATFORM="darwin-x86_64" ;;
  *) echo "package-airdrop: unsupported arch $ARCH" >&2; exit 2 ;;
esac
VERSION="${BLUEY_VERSION:-$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')}"

STAGED="dist/bluey-macos-${ARCH}"
[[ -d "$STAGED" ]] || { echo "package-airdrop: $STAGED missing — run scripts/build-macos.sh first." >&2; exit 1; }
[[ -x "$STAGED/bluey" ]] || { echo "package-airdrop: $STAGED/bluey missing — build incomplete." >&2; exit 1; }

NAME="bluey-${VERSION}-${PLATFORM}"
OUT="dist/$NAME"
TARBALL="dist/${NAME}.tar.gz"

rm -rf "$OUT" "$TARBALL"
mkdir -p "$OUT/bin"

# Lay the flat staged binaries + .app bundles + OpenBLAS dylib under bin/ (the
# layout install.sh expects). cp -R carries the .app bundles and any dylib.
cp -R "$STAGED"/. "$OUT/bin"/
if [[ "$(uname -s)" == "Darwin" && -d "$OUT/bin/BlueyAudio.app" ]]; then
  BLUEY_VERIFY_LAUNCH=1 \
    bash native/macos/cue-audio/verify-app.sh "$OUT/bin/BlueyAudio.app"
fi

# Ship the question-detection classifier next to the daemon binary
# (bin/models/qdetect-en — the daemon's exe-adjacent resolution path). It is
# exported once by scripts/export-qdetect-onnx.sh; absent = the daemon falls
# back to regex-only question detection, so packaging proceeds with a warning.
if [[ -f "dist/models/qdetect-en/model_int8.onnx" ]]; then
  mkdir -p "$OUT/bin/models"
  cp -R "dist/models/qdetect-en" "$OUT/bin/models/"
else
  echo "package-airdrop: WARNING — dist/models/qdetect-en missing (run scripts/export-qdetect-onnx.sh); shipping regex-only question detection" >&2
fi

# Ship the AirDrop-capable installer (BLUEY_ARCHIVE local mode + ad-hoc re-sign)
# at the tarball root so the receiver runs ./install.sh with no download.
cp scripts/install.sh "$OUT/install.sh"
chmod +x "$OUT/install.sh"

cat > "$OUT/README.txt" <<EOF
Bluey ${VERSION} — on-device meeting copilot (macOS ${PLATFORM#darwin-})

INSTALL (from this folder):
  ./install.sh

Then start it:
  bluey on

Notes:
  • First run downloads the speech model (~600MB, one time, needs internet).
    After that, transcription runs fully on-device / offline.
  • Speaker labels (diarization) download a small model on first use.
  • Asking questions / AI summaries need an API key or the managed backend;
    transcription itself is keyless and local.
  • install.sh ad-hoc code-signs the binaries so macOS Gatekeeper allows them
    (no Apple Developer account needed). No admin/sudo required.
EOF

# Tar from dist/ so the archive unpacks to a single top-level folder.
tar -czf "$TARBALL" -C dist "$NAME"

( cd dist && shasum -a 256 "$(basename "$TARBALL")" > "${NAME}.sha256" )

SIZE="$(du -h "$TARBALL" | awk '{print $1}')"
echo "package-airdrop: built $TARBALL ($SIZE)"
echo "package-airdrop: sha256 -> dist/${NAME}.sha256"
echo
echo "AirDrop dist/${NAME}.tar.gz — the receiver runs:"
echo "  tar -xzf ${NAME}.tar.gz && cd ${NAME} && ./install.sh && bluey on"
