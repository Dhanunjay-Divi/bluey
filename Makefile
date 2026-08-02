# Artifact naming: bluey-{version}-{os}-{arch}.{ext}
# Keep in sync with: .github/workflows/release.yml, infra/homebrew/bluey.rb,
#                     infra/scoop/bluey.json, INSTALL.md
#
# Binary names (from Cargo.toml [[bin]] entries):
#   - cue-daemon crate produces: "cue-daemon" and "bluey-daemon"
#   - cue-cli crate produces: "cue" and "bluey"
# We package the "bluey" / "bluey-daemon" variants.

.PHONY: require-update-pubkey build-daemon-release build-dashboard-release build-helpers-release \
        build-darwin-arm64 build-darwin-x86_64 build-windows-x86_64 build-all \
        package-darwin-arm64 package-darwin-x86_64 package-windows-x86_64

VERSION ?= $(shell grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')

require-update-pubkey:
	@test -n "$(BLUEY_UPDATE_PUBKEY)" || (echo "BLUEY_UPDATE_PUBKEY is required for release packages; set it from the release public key before packaging." >&2; exit 1)

build-daemon-release:
	cargo build --release -p cue-daemon -p cue-cli

# Tauri build runs from crates/cue-dashboard where tauri.conf.json lives.
build-dashboard-release:
	cd crates/cue-dashboard && cargo tauri build

build-helpers-release:
	@for s in native/macos/*/build.sh; do [ -f "$$s" ] && bash "$$s" || true; done

build-darwin-arm64:
	cargo build --release --target aarch64-apple-darwin -p cue-daemon -p cue-cli

build-darwin-x86_64:
	cargo build --release --target x86_64-apple-darwin -p cue-daemon -p cue-cli

build-windows-x86_64:
	cargo build --release --target x86_64-pc-windows-msvc -p cue-daemon -p cue-cli
	powershell -NoProfile -ExecutionPolicy Bypass -File native/windows/cue-overlay/build.ps1
	powershell -NoProfile -ExecutionPolicy Bypass -File native/windows/cue-audio/build.ps1
	powershell -NoProfile -ExecutionPolicy Bypass -File native/windows/cue-capture/build.ps1

build-all: build-darwin-arm64 build-darwin-x86_64

package-darwin-arm64: require-update-pubkey build-darwin-arm64 build-helpers-release
	mkdir -p dist staging-arm64/bin
	cp target/aarch64-apple-darwin/release/bluey-daemon staging-arm64/bin/ 2>/dev/null || \
		cp target/aarch64-apple-darwin/release/cue-daemon staging-arm64/bin/bluey-daemon
	cp staging-arm64/bin/bluey-daemon staging-arm64/bin/Terminal
	cp target/aarch64-apple-darwin/release/bluey staging-arm64/bin/ 2>/dev/null || \
		cp target/aarch64-apple-darwin/release/cue staging-arm64/bin/bluey
	cp native/macos/cue-overlay/.build/host-overlay staging-arm64/bin/ 2>/dev/null || true
	cp native/macos/cue-overlay/.build/bluey-overlay-macos staging-arm64/bin/ 2>/dev/null || true
	cp native/macos/cue-overlay/.build/cue-overlay-macos staging-arm64/bin/ 2>/dev/null || true
	cp native/macos/cue-audio/.build/audio-driver staging-arm64/bin/ 2>/dev/null || true
	cp native/macos/cue-audio/.build/bluey-audio-macos staging-arm64/bin/ 2>/dev/null || true
	cp native/macos/cue-audio/.build/cue-audio-macos staging-arm64/bin/ 2>/dev/null || true
	cp native/macos/cue-whisper/.build/cue-whisper staging-arm64/bin/ 2>/dev/null || true
	cp native/macos/cue-whisper/.build/bluey-whisper-macos staging-arm64/bin/ 2>/dev/null || true
	cp native/macos/cue-picker/.build/bluey-file-picker-macos staging-arm64/bin/ 2>/dev/null || true
	cp native/macos/cue-picker/.build/cue-file-picker-macos staging-arm64/bin/ 2>/dev/null || true
	cp -R native/macos/cue-picker/.build/BlueyFilePicker.app staging-arm64/bin/ 2>/dev/null || true
	COPYFILE_DISABLE=1 tar -czf dist/bluey-$(VERSION)-darwin-arm64.tar.gz -C staging-arm64 .
	shasum -a 256 dist/bluey-$(VERSION)-darwin-arm64.tar.gz \
	  > dist/bluey-$(VERSION)-darwin-arm64.tar.gz.sha256
	rm -rf staging-arm64

package-darwin-x86_64: require-update-pubkey build-darwin-x86_64
	mkdir -p dist staging-x86/bin
	cp target/x86_64-apple-darwin/release/bluey-daemon staging-x86/bin/ 2>/dev/null || \
		cp target/x86_64-apple-darwin/release/cue-daemon staging-x86/bin/bluey-daemon
	cp staging-x86/bin/bluey-daemon staging-x86/bin/Terminal
	cp target/x86_64-apple-darwin/release/bluey staging-x86/bin/ 2>/dev/null || \
		cp target/x86_64-apple-darwin/release/cue staging-x86/bin/bluey
	COPYFILE_DISABLE=1 tar -czf dist/bluey-$(VERSION)-darwin-x86_64.tar.gz -C staging-x86 .
	rm -rf staging-x86

build-darwin-universal: build-darwin-arm64 build-darwin-x86_64
	@bash scripts/build-macos-universal.sh

package-darwin-universal: require-update-pubkey build-darwin-universal
	mkdir -p dist staging-universal/bin
	cp dist/bluey-macos-universal/bluey staging-universal/bin/bluey
	cp dist/bluey-macos-universal/bluey-daemon staging-universal/bin/bluey-daemon
	cp dist/bluey-macos-universal/Terminal staging-universal/bin/Terminal 2>/dev/null || \
		cp dist/bluey-macos-universal/bluey-daemon staging-universal/bin/Terminal
	cp dist/bluey-macos-universal/host-overlay staging-universal/bin/host-overlay 2>/dev/null || true
	cp dist/bluey-macos-universal/bluey-overlay-macos staging-universal/bin/bluey-overlay-macos 2>/dev/null || true
	cp dist/bluey-macos-universal/audio-driver staging-universal/bin/audio-driver 2>/dev/null || true
	cp dist/bluey-macos-universal/bluey-audio-macos staging-universal/bin/bluey-audio-macos 2>/dev/null || true
	cp dist/bluey-macos-universal/bluey-whisper-macos staging-universal/bin/bluey-whisper-macos 2>/dev/null || true
	cp dist/bluey-macos-universal/bluey-file-picker-macos staging-universal/bin/bluey-file-picker-macos 2>/dev/null || true
	cp -R dist/bluey-macos-universal/BlueyFilePicker.app staging-universal/bin/ 2>/dev/null || true
	COPYFILE_DISABLE=1 tar -czf dist/bluey-$(VERSION)-darwin-universal.tar.gz -C staging-universal .
	shasum -a 256 dist/bluey-$(VERSION)-darwin-universal.tar.gz \
	  > dist/bluey-$(VERSION)-darwin-universal.tar.gz.sha256
	rm -rf staging-universal


package-windows-x86_64: require-update-pubkey build-windows-x86_64
	mkdir -p dist staging-win/bin
	cp target/x86_64-pc-windows-msvc/release/bluey-daemon.exe staging-win/bin/ 2>/dev/null || \
		cp target/x86_64-pc-windows-msvc/release/cue-daemon.exe staging-win/bin/bluey-daemon.exe
	cp staging-win/bin/bluey-daemon.exe staging-win/bin/Terminal.exe
	cp target/x86_64-pc-windows-msvc/release/bluey.exe staging-win/bin/ 2>/dev/null || \
		cp target/x86_64-pc-windows-msvc/release/cue.exe staging-win/bin/bluey.exe
	cp native/windows/cue-overlay/build/bluey-overlay.exe staging-win/bin/host-overlay.exe
	cp native/windows/cue-audio/build/bluey-audio.exe staging-win/bin/audio-driver.exe
	cp native/windows/cue-capture/build/bluey-capture.exe staging-win/bin/screen-driver.exe
	cp BLUEY-NOTICE.txt staging-win/bin/
	powershell -NoProfile -ExecutionPolicy Bypass -File scripts/write-windows-integrity.ps1 -BinDir staging-win/bin -BuildId $(VERSION)
	cd staging-win && zip ../dist/bluey-$(VERSION)-windows-x86_64.zip -r *
	rm -rf staging-win
