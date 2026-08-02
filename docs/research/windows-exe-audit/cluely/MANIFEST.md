# Cluely 2.0.193 Windows manifest

## Identity and integrity

| Field | Observed value | Evidence |
|---|---|---|
| Installer | `Cluely Setup 2.0.193.exe`, 222,071,256 bytes | filesystem stat |
| SHA-256 | `827b46feccd68e18fa41ef840da03102c8c38f381626b1e6bb899d75a5582e62` | `shasum -a 256`; [hashes](hashes.txt) |
| Format | PE32 NSIS self-extracting installer | `file`, `7zz l -slt` |
| Product/version | Cluely 2.0.193 | PE version resource and `app.asar/package.json` |
| Architectures | x64 and ARM64; PE subsystem 10.0 | exact `app-64.7z` and `app-arm64.7z`; `7zz l -slt` |
| Installer signature | Digest and chain verified; `Cluely Inc`; timestamp 2026-06-04T02:40:09Z | `osslsigncode verify` |
| Main app signatures | Both architecture digests/chains verified to `Cluely Inc` | `osslsigncode verify` |
| Technology | Electron main/preload + React/Vite-style renderer | `package.json`, `dist-electron/`, Chromium resources |
| ASAR | 3,300 files; SHA-256 `607c9294…3e7`, identical on x64/ARM64 | exact payloads and mechanical extraction |
| Updater | `electron-updater` 6.8.3; Cloudflare R2/S3 bucket | `resources/app-update.yml`, `package.json` |
| Windows audio | re-signed 32-bit SoX for microphone; Chromium display-media loopback | helper inventory; `dist-electron/main.js` offsets 501656-503000 |

Windows has no bundle ID equivalent in the payload metadata. Protocol and app
identity are application constants in the main bundle; a live registry install
was not performed. The exact retained tree and full hash ledger are at
`/Users/uno/Downloads/exe_backtrack_code/recovered/cluely-2.0.193`.

## Composition

- Main: `asar-extracted-mechanical/dist-electron/main.js` (543,909 bytes).
- Preload: `dist-electron/preload.mjs` (314 bytes), exposing generic `on`, `send`,
  and `invoke` wrappers.
- Renderer: `dist/assets/` hashed JavaScript, CSS, fonts, images, VAD assets.
- Native/extra: `sox.exe`, its DLLs, cross-platform AudioTee/SoX files, Silero
  VAD/ONNX Runtime Web.
- Installer: architecture-specific 7z payloads, standard electron-builder NSIS
  plugins, signed uninstaller.

No Windows service, driver, scheduled task payload, browser extension, or
separate launch agent was found. Installed registry effects remain unknown
because the installer was not executed.
