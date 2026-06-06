# v0.1.1 — hotfix release

## What Changed

- Fixed `bluey on` for public installs where `bluey` is launched through the
  `~/.local/bin/bluey` symlink.
- Bluey now resolves `bluey-daemon` from the canonical install root
  (`~/.bluey/bin`) as well as executable siblings.
- The installer now symlinks `bluey-daemon` next to `bluey` when the CLI
  directory is writable.

## Why

The v0.1.0 tarball included `bin/bluey-daemon`, but the installed CLI could
miss it after launch through the public symlink path. Existing v0.1.0 installs
can update to this release automatically on the next `bluey on`.
