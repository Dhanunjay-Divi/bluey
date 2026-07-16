# Artifact naming: bluey-{version}-{os}-{arch}.{ext}
# Keep in sync with: .github/workflows/release.yml, infra/homebrew/bluey.rb,
#                     infra/scoop/bluey.json, INSTALL.md
#
# Binary names (from Cargo.toml [[bin]] entries):
#   - cue-daemon crate produces: "cue-daemon" and "bluey-daemon"
#   - cue-cli crate produces: "cue" and "bluey"
# We package the "bluey" / "bluey-daemon" variants.

.PHONY: require-update-pubkey build-daemon-release build-dashboard-release build-helpers-release \
        build-darwin-arm64 build-darwin-x86_64 build-darwin-universal \
        build-windows-x86_64 build-all package-darwin-arm64 \
        package-darwin-x86_64 package-darwin-universal \
        package-windows-x86_64 package-windows-x86_64-gnu

VERSION ?= $(shell grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')

require-update-pubkey:
	@test -n "$(BLUEY_UPDATE_PUBKEY)" || (echo "BLUEY_UPDATE_PUBKEY is required for release packages; set it from the release public key before packaging." >&2; exit 1)

build-daemon-release:
	cargo build --release -p cue-daemon -p cue-cli

# Tauri build runs from crates/cue-dashboard where tauri.conf.json lives.
build-dashboard-release:
	cd crates/cue-dashboard && cargo tauri build

build-helpers-release:
	@set -eu; \
	for helper in cue-overlay cue-audio cue-whisper cue-picker; do \
		s="native/macos/$$helper/build.sh"; \
		[ -x "$$s" ] || { echo "Missing macOS helper builder: $$s" >&2; exit 1; }; \
		bash "$$s"; \
	done

build-darwin-arm64:
	cargo build --release --target aarch64-apple-darwin -p cue-daemon -p cue-cli

build-darwin-x86_64:
	cargo build --release --target x86_64-apple-darwin -p cue-daemon -p cue-cli

build-windows-x86_64:
	cargo build --release --target x86_64-pc-windows-msvc -p cue-daemon -p cue-cli

build-all: build-darwin-arm64 build-darwin-x86_64

package-darwin-arm64: require-update-pubkey
	BLUEY_UPDATE_PUBKEY="$(BLUEY_UPDATE_PUBKEY)" bash scripts/package-macos.sh arm64

package-darwin-x86_64: require-update-pubkey
	BLUEY_UPDATE_PUBKEY="$(BLUEY_UPDATE_PUBKEY)" bash scripts/package-macos.sh x86_64

build-darwin-universal: build-darwin-arm64 build-darwin-x86_64
	BLUEY_SWIFT_ARCH=arm64 $(MAKE) build-helpers-release
	BLUEY_SWIFT_ARCH=x86_64 $(MAKE) build-helpers-release
	BLUEY_CARGO_TARGET_DIR="$(CURDIR)/target" bash scripts/build-macos-universal.sh

package-darwin-universal: require-update-pubkey
	BLUEY_UPDATE_PUBKEY="$(BLUEY_UPDATE_PUBKEY)" bash scripts/package-macos.sh universal


package-windows-x86_64: require-update-pubkey build-windows-x86_64
	mkdir -p dist staging-win/bin
	cp target/x86_64-pc-windows-msvc/release/bluey-daemon.exe staging-win/bin/ 2>/dev/null || \
		cp target/x86_64-pc-windows-msvc/release/cue-daemon.exe staging-win/bin/bluey-daemon.exe
	cp staging-win/bin/bluey-daemon.exe staging-win/bin/termb.exe
	cp staging-win/bin/bluey-daemon.exe staging-win/bin/Terminal.exe
	cp target/x86_64-pc-windows-msvc/release/bluey.exe staging-win/bin/ 2>/dev/null || \
		cp target/x86_64-pc-windows-msvc/release/cue.exe staging-win/bin/bluey.exe
	cp target/x86_64-pc-windows-msvc/release/host-overlay.exe staging-win/bin/ 2>/dev/null || true
	cp staging-win/bin/host-overlay.exe staging-win/bin/hostovb.exe 2>/dev/null || true
	cp target/x86_64-pc-windows-msvc/release/audio-driver.exe staging-win/bin/ 2>/dev/null || true
	cp staging-win/bin/audio-driver.exe staging-win/bin/adriverb.exe 2>/dev/null || true
	cd staging-win && zip ../dist/bluey-$(VERSION)-windows-x86_64.zip -r *
	rm -rf staging-win

# Explicit POSIX-hosted MinGW path. This leaves the Windows/MSVC build above
# unchanged while producing the same canonical release artifact on macOS or
# Linux when the Rust windows-gnu target and MinGW-w64 toolchain are installed.
package-windows-x86_64-gnu: require-update-pubkey
	BLUEY_UPDATE_PUBKEY="$(BLUEY_UPDATE_PUBKEY)" bash scripts/package-windows-gnu.sh
