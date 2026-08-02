# LockedIn 1.8.8 Windows manifest

| Field | Observed value | Evidence |
|---|---|---|
| Installer | `LockedIn Setup 1.8.8.exe`, 172,231,944 bytes | filesystem stat |
| SHA-256 | `0240d1872560bf5c49a9372fc2919f8d698b667d80bd2e203b316669c3c9acb1` | [hashes](hashes.txt) |
| Format | PE32 NSIS/electron-builder | `file`, `7zz l -slt` |
| Product/version | LockedIn 1.8.8 | PE/package metadata |
| Architecture | x64; OS/subsystem version 10.0 | `app-64.7z`, PE fields |
| Authenticode | installer/app/WinKeyServer digest/chains verified to Cyber Gravity LLC | `osslsigncode verify` |
| Technology | Electron main/preload + React; Firebase/Clerk; Socket.IO/WebRTC; robotjs | package and first-party source |
| ASAR | 49,603 files; SHA-256 `e9d560c8…d5b` | mechanical extraction |
| Source recoverability | readable main/preload, complete minified renderer, and substantial packaged `src/`; no first-party maps | retained ASAR |
| Updater | S3 `desktop-app-updates-lockedin-ai`, latest channel, publisher Cyber Gravity LLC | `resources/app-update.yml` |

Exact tree: `/Users/uno/Downloads/exe_backtrack_code/recovered/lockedin-1.8.8`.
The ASAR unusually includes `.env.local`, packaging scripts, a browser extension
ZIP, lockfiles, and source directories. Sensitive values were not copied into
the audit documentation.

## Native/helper inventory

- Signed x64 `WinKeyServer.exe` using a global keyboard hook.
- Packaged `@jitsi/robotjs` prebuilds for Windows x64/ARM64/ia32 and other OSes;
  the x64 `.node` itself is not Authenticode-signed.
- macOS-only custom audio addons are packaged but not loaded on Windows.
- Windows system audio uses Chromium display-media loopback, not the macOS addon.
- Large canvas/PDF native dependency tree plus `extension.zip`.

No Windows driver or installed service payload was found.
