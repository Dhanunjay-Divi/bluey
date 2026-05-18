# Installing Bluey

> **v0.1.0 scope:** macOS arm64 (Apple Silicon) only. Intel Macs, Windows, and
> Linux are not supported in this release; their builds and end-to-end testing
> are scheduled for a later round (see `docs/work/PHASE-3-ROUND-13-PLAN.md`).
> If you are on an unsupported platform, please wait for a later release.

## Requirements

- macOS 13 (Ventura) or newer on Apple Silicon (M1, M2, M3, …)
- Microphone, Screen Recording, and Accessibility permissions (granted at first
  launch via the standard macOS prompts)

## Manual install

Download the release archive and extract it:

```bash
curl -LO https://github.com/Dhanunjay-Divi/bluey/releases/latest/download/bluey-0.1.0-darwin-arm64.tar.gz
curl -LO https://github.com/Dhanunjay-Divi/bluey/releases/latest/download/SHA256SUMS.txt
grep ' bluey-0.1.0-darwin-arm64.tar.gz$' SHA256SUMS.txt | shasum -a 256 -c -

mkdir -p /usr/local/lib/bluey/0.1.0
tar -xzf bluey-0.1.0-darwin-arm64.tar.gz -C /usr/local/lib/bluey/0.1.0

ln -sf /usr/local/lib/bluey/0.1.0/bin/bluey        /usr/local/bin/bluey
ln -sf /usr/local/lib/bluey/0.1.0/bin/bluey-daemon /usr/local/bin/bluey-daemon
```

For local/dev validation, the same install layout can be produced with:

```bash
make package-darwin-arm64
BLUEY_ARCHIVE=dist/bluey-0.1.0-darwin-arm64.tar.gz scripts/install.sh
```

## Code signing and Gatekeeper

The v0.1.0 tarball is **not** code-signed or notarized. Code signing
is deferred to a later release; do not assume Gatekeeper or quarantine will
silently allow unsigned binaries.

If you download the archive through a browser, macOS may attach the
`com.apple.quarantine` extended attribute. The standard remediation is:

```bash
xattr -d com.apple.quarantine /usr/local/lib/bluey/0.1.0/bin/*
```

If you `curl` the archive from a terminal, quarantine is typically not
attached, but this is not a guaranteed bypass — it depends on the network
client and macOS version. Validate behaviour on a clean machine before
distributing internally.

We will revisit signing + notarization once we are ready to publish a
public release outside our internal distribution.

## Running

```bash
# Start the daemon
bluey-daemon &

# Use the CLI
bluey on --title "My meeting"
bluey off
```

The dashboard (`bluey-dashboard`) is a developer tool and is **not** part of
the v0.1.0 distribution. It will be reintroduced in a later release once it
has been bundled and signed properly.

## Auto-Update

There is no auto-update mechanism in v0.1.0. Future releases will document
the upgrade path explicitly.

## Uninstall

```bash
rm /usr/local/bin/bluey /usr/local/bin/bluey-daemon
rm -rf /usr/local/lib/bluey
```

User data lives under `~/Library/Application Support/bluey/`; remove it
manually if you want a fully clean uninstall.

## Other platforms

Intel Macs, Linux, and Windows are tracked as future work:

- macOS x86_64 (Intel): cross-compile + smoke-test on a clean Intel Mac.
- Linux x86_64: cross-compile + audio capture validation on Linux.
- Windows x86_64: blocked on the Windows whisper.cpp port (R12.5 / R13.4)
  plus end-to-end testing on a clean Windows machine.

When those land, this document will be updated. Until then, please do not
treat the older multi-platform install instructions as a support promise.
