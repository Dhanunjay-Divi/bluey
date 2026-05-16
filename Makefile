# Artifact naming: bluey-{version}-{os}-{arch}.{ext}
# Keep in sync with: .github/workflows/release.yml, infra/homebrew/bluey.rb,
#                     infra/scoop/bluey.json, INSTALL.md

.PHONY: build-daemon-release build-dashboard-release build-helpers-release \
        build-darwin-arm64 build-darwin-x86_64 build-windows-x86_64 build-all \
        package-darwin-arm64 package-darwin-x86_64 package-windows-x86_64

VERSION ?= $(shell grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')

build-daemon-release:
	cargo build --release -p cue-daemon -p cue-cli

build-dashboard-release:
	cd crates/cue-dashboard && cargo tauri build --release

build-helpers-release:
	@for s in native/macos/*/build.sh; do [ -f "$$s" ] && bash "$$s" || true; done

build-darwin-arm64:
	cargo build --release --target aarch64-apple-darwin -p cue-daemon -p cue-cli
	cd crates/cue-dashboard && cargo tauri build --target aarch64-apple-darwin

build-darwin-x86_64:
	cargo build --release --target x86_64-apple-darwin -p cue-daemon -p cue-cli
	cd crates/cue-dashboard && cargo tauri build --target x86_64-apple-darwin

build-windows-x86_64:
	cargo build --release --target x86_64-pc-windows-msvc -p cue-daemon -p cue-cli
	cd crates/cue-dashboard && cargo tauri build --target x86_64-pc-windows-msvc

build-all: build-darwin-arm64 build-darwin-x86_64

package-darwin-arm64: build-darwin-arm64
	mkdir -p dist staging-arm64
	cp target/aarch64-apple-darwin/release/bluey-daemon staging-arm64/ || \
		cp target/aarch64-apple-darwin/release/cue-daemon staging-arm64/bluey-daemon
	cp target/aarch64-apple-darwin/release/bluey staging-arm64/ || \
		cp target/aarch64-apple-darwin/release/cue-cli staging-arm64/bluey
	tar -czf dist/bluey-$(VERSION)-darwin-arm64.tar.gz -C staging-arm64 .
	rm -rf staging-arm64

package-darwin-x86_64: build-darwin-x86_64
	mkdir -p dist staging-x86
	cp target/x86_64-apple-darwin/release/bluey-daemon staging-x86/ || \
		cp target/x86_64-apple-darwin/release/cue-daemon staging-x86/bluey-daemon
	cp target/x86_64-apple-darwin/release/bluey staging-x86/ || \
		cp target/x86_64-apple-darwin/release/cue-cli staging-x86/bluey
	tar -czf dist/bluey-$(VERSION)-darwin-x86_64.tar.gz -C staging-x86 .
	rm -rf staging-x86

package-windows-x86_64: build-windows-x86_64
	mkdir -p dist staging-win
	cp target/x86_64-pc-windows-msvc/release/bluey-daemon.exe staging-win/ || \
		cp target/x86_64-pc-windows-msvc/release/cue-daemon.exe staging-win/bluey-daemon.exe
	cp target/x86_64-pc-windows-msvc/release/bluey.exe staging-win/ || \
		cp target/x86_64-pc-windows-msvc/release/cue-cli.exe staging-win/bluey.exe
	cd staging-win && zip ../dist/bluey-$(VERSION)-windows-x86_64.zip *
	rm -rf staging-win
