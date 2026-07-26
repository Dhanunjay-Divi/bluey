# Artifact naming: bluey-{version}-{os}-{arch}.{ext}
# Keep in sync with: .github/workflows/release.yml, infra/homebrew/bluey.rb,
#                     infra/scoop/bluey.json, INSTALL.md
#
# Binary names (from Cargo.toml [[bin]] entries):
#   - cue-daemon crate produces: "cue-daemon" and "bluey-daemon"
#   - cue-cli crate produces: "cue" and "bluey"
# We package the "bluey" / "bluey-daemon" variants.

.PHONY: build-daemon-release build-dashboard-release build-helpers-release \
        build-meeting-overlay-ui build-meeting-overlay-release \
        build-meeting-overlay-darwin-arm64 build-meeting-overlay-darwin-x86_64 \
        build-darwin-arm64 build-darwin-x86_64 build-windows-x86_64 build-all \
        package-darwin-arm64 package-darwin-x86_64 package-windows-x86_64

VERSION ?= $(shell grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')

# `cue-daemon/parakeet-stt` is package-qualified: it enables the parakeet-stt
# feature on cue-daemon only (cue-cli has no such feature). On-device STT is
# compiled in; model weights (~600MB) are fetched at first run, not bundled.
build-daemon-release:
	cargo build --release -p cue-daemon -p cue-cli --features cue-daemon/parakeet-stt

# Tauri build runs from crates/cue-dashboard where tauri.conf.json lives.
build-dashboard-release:
	cd crates/cue-dashboard && cargo tauri build --release

# The REAL meeting overlay (cue-meeting-overlay) is a Tauri 2 app: a Rust shell
# (crates/cue-meeting-overlay/src) + a React UI (crates/cue-meeting-overlay/ui,
# built to ui/dist via `npm run build`). The daemon launches the overlay by
# spawning the *binary* named cue-meeting-overlay directly (see app.rs
# spawn_stdio_overlay / spawn_macos_socket_overlay) — it does NOT need the .app
# wrapper at runtime. So packaging the plain release binary is sufficient.
#
# UI bundle must be built first because tauri.conf.json's frontendDist is
# "ui/dist"; the Rust build embeds it via tauri-build at compile time.
build-meeting-overlay-ui:
	cd crates/cue-meeting-overlay/ui && \
		if [ -f package-lock.json ]; then npm ci; else npm install; fi && \
		npm run build

# Builds the meeting-overlay UI then the launchable cue-meeting-overlay binary.
# NOTE: we use `cargo build` (not `cargo tauri build`) because the daemon spawns
# the binary directly and `cargo tauri` (the tauri-cli) may not be installed in
# CI. If a full .app bundle is ever required, install tauri-cli and run
# `cd crates/cue-meeting-overlay && cargo tauri build --release` instead.
build-meeting-overlay-release: build-meeting-overlay-ui
	cargo build --release -p cue-meeting-overlay

build-helpers-release:
	@for s in native/macos/*/build.sh; do [ -f "$$s" ] && bash "$$s" || true; done

# arm64 is the MVP target for on-device STT. parakeet-stt uses the native
# prebuilt ONNX Runtime (ort-defaults) on Apple Silicon; nothing extra to ship.
build-darwin-arm64:
	cargo build --release --target aarch64-apple-darwin -p cue-daemon -p cue-cli --features cue-daemon/parakeet-stt,cue-daemon/cloud-calendar

# NOTE (Intel mac): on x86_64 macOS, parakeet-rs links via `load-dynamic`
# (see crates/cue-daemon/Cargo.toml), so the packaged binary expects an ONNX
# Runtime dylib (libonnxruntime) to be present at runtime. The dylib is NOT
# bundled here. arm64 is the MVP target; x86_64 on-device STT needs a separate
# dylib-staging step before it is fully usable. Feature is enabled so the code
# path compiles in for parity.
build-darwin-x86_64:
	cargo build --release --target x86_64-apple-darwin -p cue-daemon -p cue-cli --features cue-daemon/parakeet-stt,cue-daemon/cloud-calendar

build-windows-x86_64:
	cargo build --release --target x86_64-pc-windows-msvc -p cue-daemon -p cue-cli

build-all: build-darwin-arm64 build-darwin-x86_64

# Build the meeting overlay (UI + binary) for arm64 so the package ships it.
# Mirrors build-meeting-overlay-release but pins the arm64 target to match the
# rest of package-darwin-arm64 (default rustup host here is x86_64).
build-meeting-overlay-darwin-arm64: build-meeting-overlay-ui
	cargo build --release --target aarch64-apple-darwin -p cue-meeting-overlay

package-darwin-arm64: build-darwin-arm64 build-helpers-release build-meeting-overlay-darwin-arm64
	mkdir -p dist staging-arm64/bin
	cp target/aarch64-apple-darwin/release/bluey-daemon staging-arm64/bin/ 2>/dev/null || \
		cp target/aarch64-apple-darwin/release/cue-daemon staging-arm64/bin/bluey-daemon
	cp target/aarch64-apple-darwin/release/bluey staging-arm64/bin/ 2>/dev/null || \
		cp target/aarch64-apple-darwin/release/cue staging-arm64/bin/bluey
	# The meeting overlay: stage under its real name AND under cue-overlay-tauri,
	# the name discover_overlay_bin() looks for in bin/ at runtime (both names are
	# socket-routed by should_use_macos_socket_overlay).
	cp target/aarch64-apple-darwin/release/cue-meeting-overlay staging-arm64/bin/cue-meeting-overlay 2>/dev/null || true
	cp target/aarch64-apple-darwin/release/cue-meeting-overlay staging-arm64/bin/cue-overlay-tauri 2>/dev/null || true
	cp native/macos/cue-overlay/.build/bluey-overlay-macos staging-arm64/bin/ 2>/dev/null || true
	cp native/macos/cue-overlay/.build/cue-overlay-macos staging-arm64/bin/ 2>/dev/null || true
	cp -R native/macos/cue-audio/.build/BlueyAudio.app staging-arm64/bin/ 2>/dev/null || true
	cp native/macos/cue-audio/.build/bluey-audio-macos staging-arm64/bin/ 2>/dev/null || true
	cp native/macos/cue-audio/.build/cue-audio-macos staging-arm64/bin/ 2>/dev/null || true
	cp native/macos/cue-whisper/.build/cue-whisper staging-arm64/bin/ 2>/dev/null || true
	cp native/macos/cue-whisper/.build/bluey-whisper-macos staging-arm64/bin/ 2>/dev/null || true
	cp native/macos/cue-picker/.build/bluey-file-picker-macos staging-arm64/bin/ 2>/dev/null || true
	cp native/macos/cue-picker/.build/cue-file-picker-macos staging-arm64/bin/ 2>/dev/null || true
	cp -R native/macos/cue-picker/.build/BlueyFilePicker.app staging-arm64/bin/ 2>/dev/null || true
	cp -R native/macos/cue-shot/.build/BlueyShot.app staging-arm64/bin/ 2>/dev/null || true
	cp scripts/install.sh staging-arm64/install.sh 2>/dev/null || cp dist/bluey-0.1.10-darwin-arm64/install.sh staging-arm64/install.sh 2>/dev/null || true
	cp dist/bluey-0.1.10-darwin-arm64/README.txt staging-arm64/README.txt 2>/dev/null || true
	tar -czf dist/bluey-$(VERSION)-darwin-arm64.tar.gz -C staging-arm64 .
	shasum -a 256 dist/bluey-$(VERSION)-darwin-arm64.tar.gz \
	  > dist/bluey-$(VERSION)-darwin-arm64.tar.gz.sha256
	rm -rf staging-arm64

# x86_64 meeting overlay (UI is arch-independent; only the Rust binary differs).
build-meeting-overlay-darwin-x86_64: build-meeting-overlay-ui
	cargo build --release --target x86_64-apple-darwin -p cue-meeting-overlay

package-darwin-x86_64: build-darwin-x86_64 build-meeting-overlay-darwin-x86_64
	mkdir -p dist staging-x86/bin
	cp target/x86_64-apple-darwin/release/bluey-daemon staging-x86/bin/ 2>/dev/null || \
		cp target/x86_64-apple-darwin/release/cue-daemon staging-x86/bin/bluey-daemon
	cp target/x86_64-apple-darwin/release/bluey staging-x86/bin/ 2>/dev/null || \
		cp target/x86_64-apple-darwin/release/cue staging-x86/bin/bluey
	cp target/x86_64-apple-darwin/release/cue-meeting-overlay staging-x86/bin/cue-meeting-overlay 2>/dev/null || true
	cp target/x86_64-apple-darwin/release/cue-meeting-overlay staging-x86/bin/cue-overlay-tauri 2>/dev/null || true
	tar -czf dist/bluey-$(VERSION)-darwin-x86_64.tar.gz -C staging-x86 .
	rm -rf staging-x86

build-darwin-universal: build-darwin-arm64 build-darwin-x86_64 \
		build-meeting-overlay-darwin-arm64 build-meeting-overlay-darwin-x86_64
	@bash scripts/build-macos-universal.sh

package-darwin-universal: build-darwin-universal
	mkdir -p dist staging-universal/bin
	cp dist/bluey-macos-universal/bluey staging-universal/bin/bluey
	cp dist/bluey-macos-universal/bluey-daemon staging-universal/bin/bluey-daemon
	cp dist/bluey-macos-universal/cue-meeting-overlay staging-universal/bin/cue-meeting-overlay 2>/dev/null || true
	cp dist/bluey-macos-universal/cue-meeting-overlay staging-universal/bin/cue-overlay-tauri 2>/dev/null || true
	cp dist/bluey-macos-universal/bluey-overlay-macos staging-universal/bin/bluey-overlay-macos 2>/dev/null || true
	cp dist/bluey-macos-universal/bluey-audio-macos staging-universal/bin/bluey-audio-macos 2>/dev/null || true
	cp dist/bluey-macos-universal/bluey-whisper-macos staging-universal/bin/bluey-whisper-macos 2>/dev/null || true
	cp dist/bluey-macos-universal/bluey-file-picker-macos staging-universal/bin/bluey-file-picker-macos 2>/dev/null || true
	cp -R dist/bluey-macos-universal/BlueyFilePicker.app staging-universal/bin/ 2>/dev/null || true
	tar -czf dist/bluey-$(VERSION)-darwin-universal.tar.gz -C staging-universal .
	shasum -a 256 dist/bluey-$(VERSION)-darwin-universal.tar.gz \
	  > dist/bluey-$(VERSION)-darwin-universal.tar.gz.sha256
	rm -rf staging-universal


package-windows-x86_64: build-windows-x86_64
	mkdir -p dist staging-win/bin
	cp target/x86_64-pc-windows-msvc/release/bluey-daemon.exe staging-win/bin/ 2>/dev/null || \
		cp target/x86_64-pc-windows-msvc/release/cue-daemon.exe staging-win/bin/bluey-daemon.exe
	cp target/x86_64-pc-windows-msvc/release/bluey.exe staging-win/bin/ 2>/dev/null || \
		cp target/x86_64-pc-windows-msvc/release/cue.exe staging-win/bin/bluey.exe
	cd staging-win && zip ../dist/bluey-$(VERSION)-windows-x86_64.zip -r *
	rm -rf staging-win
