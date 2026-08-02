# LockedIn Windows static evidence

Retained source: `/Users/uno/Downloads/exe_backtrack_code/recovered/lockedin-1.8.8`.

Static commands: `shasum -a 256`, `file`, `7zz l/x -slt -snl`, trusted ASAR
mechanical extraction, `osslsigncode verify`, `objdump -p`, `strings`, and `rg`.

Observed: NSIS x64 payload; Electron main `public/electron.js`, preload
`public/preload.js`; 49,603 ASAR files; no first-party source maps; signed main
and WinKeyServer; unsigned robotjs N-API binary; macOS-only custom audio addon;
Windows Chromium loopback; broad IPC/remote-input handler; S3 updater.

Full exact evidence: retained `MANIFEST.md`, `RECOVERY-LEDGER.md`, `hashes.txt`,
`exact-files.sha256`, and `evidence/`. No recovered code was executed and no
packaged configuration value was copied into Bluey.
