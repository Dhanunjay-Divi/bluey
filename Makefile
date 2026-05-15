.PHONY: build-daemon-release build-dashboard-release build-helpers-release \
        build-darwin-arm64 build-darwin-x86_64 build-windows-x86_64 build-all \
        package-darwin-arm64 package-darwin-x86_64 package-windows-x86_64

VERSION ?= $(shell grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')

build-daemon-release:
	cargo build --release -p cue-daemon -p cue-cli

build-dashboard-release:
	cargo tauri build --release

build-helpers-release:
	@for s in native/macos/*/build.sh; do [ -f "$$s" ] && bash "$$s" || true; done

build-darwin-arm64:
	cargo build --release --target aarch64-apple-darwin -p cue-daemon -p cue-cli
	cargo tauri build --target aarch64-apple-darwin

build-darwin-x86_64:
	cargo build --release --target x86_64-apple-darwin -p cue-daemon -p cue-cli
	cargo tauri build --target x86_64-apple-darwin

build-windows-x86_64:
	cargo build --release --target x86_64-pc-windows-msvc -p cue-daemon -p cue-cli
	cargo tauri build --target x86_64-pc-windows-msvc

build-all: build-darwin-arm64 build-darwin-x86_64

package-darwin-arm64: build-darwin-arm64
	mkdir -p dist
	tar -czf dist/bluey-$(VERSION)-darwin-arm64.tar.gz \
		-C target/aarch64-apple-darwin/release cue-daemon cue-cli

package-darwin-x86_64: build-darwin-x86_64
	mkdir -p dist
	tar -czf dist/bluey-$(VERSION)-darwin-x86_64.tar.gz \
		-C target/x86_64-apple-darwin/release cue-daemon cue-cli

package-windows-x86_64: build-windows-x86_64
	mkdir -p dist
	cd target/x86_64-pc-windows-msvc/release && \
		zip ../../../dist/bluey-$(VERSION)-windows-x86_64.zip cue-daemon.exe cue-cli.exe
