# ROUND-475 Local Document Tools Bootstrap

Date: 2026-07-11

## Why

A fresh Windows install completed but showed:

```text
Installing Bluey document tools...
Python was not found; run without arguments to install from the Microsoft Store...
WARN Could not prepare Bluey document tools; document conversion will use built-in fallbacks only
```

That happened because the installer relied on global `python` or `python3`.
On Windows, the Microsoft Store Python alias can exist without a real usable Python
runtime, so Bluey attempted setup and fell back.

The product expectation is that Bluey prepares document conversion inside the
Bluey install, without asking users to install Python globally.

## Changed

- Windows `ops/install/install.ps1` now validates that `python`, `python3`, or
  `py -3` is a real Python 3 runtime before using it.
- If no real Python exists, Windows downloads a Bluey-local `uv` helper and uses
  it to install Python and MarkItDown under `%LOCALAPPDATA%\Bluey\tools`.
- Windows chooses the uv package by architecture: x64 or ARM64.
- macOS release installer `ops/install/install.sh` now uses the same local
  bootstrap under `~/.bluey/tools`.
- Legacy/local release installer `scripts/install.sh` now has the same behavior
  for `~/.local/bluey/<version>/tools`.
- All paths still support `BLUEY_SKIP_LOCAL_TOOLS=1` for intentionally skipping
  local document tools.
- If bootstrap or package install fails, Bluey still installs and falls back to
  built-in document conversion paths.

## Local Tool Locations

Windows:

```text
%LOCALAPPDATA%\Bluey\tools\uv\uv.exe
%LOCALAPPDATA%\Bluey\tools\python
%LOCALAPPDATA%\Bluey\tools\doc-converter\.venv
%LOCALAPPDATA%\Bluey\bin\bluey-doc-converter.cmd
```

macOS direct installer:

```text
~/.bluey/tools/uv/uv
~/.bluey/tools/python
~/.bluey/tools/doc-converter/.venv
~/.bluey/bin/bluey-doc-converter
```

macOS versioned installer:

```text
~/.local/bluey/<version>/tools/uv/uv
~/.local/bluey/<version>/tools/python
~/.local/bluey/<version>/tools/doc-converter/.venv
~/.local/bluey/<version>/bin/bluey-doc-converter
```

## Verification

- `bash -n ops/install/install.sh`
- `bash -n scripts/install.sh`
- `git diff --check -- ops/install/install.ps1 ops/install/install.sh scripts/install.sh docs/rounds/ROUND-475-LOCAL-DOC-TOOLS-BOOTSTRAP.md`
- Verified all four uv release URLs return HTTP 200:
  - Windows x64
  - Windows ARM64
  - macOS Apple Silicon
  - macOS Intel

## Follow-Up Smoke

Run on a clean Windows VM without Python:

```powershell
irm https://bluey.sh/install.ps1 | iex
bluey on
```

Expected: installer creates the Bluey-local document tools runtime instead of
showing the Microsoft Store Python alias warning.

Run on a clean macOS user without Homebrew Python:

```bash
curl -fsSL https://bluey.sh/install.sh | bash
bluey on
```

Expected: installer creates the Bluey-local document tools runtime and document
attachments use `bluey-doc-converter`.
