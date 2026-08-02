# ParakeetAI 3.7.0 Windows manifest

| Field | Observed value | Evidence |
|---|---|---|
| Installer | `ParakeetAI-Setup-3.7.0.exe`, 224,467,424 bytes | filesystem stat |
| SHA-256 | `63b29b103609417d5b93370b415a1abcfe2c611439b60e659b37bab44a273977` | [hashes](hashes.txt) |
| Format | PE32 NSIS/electron-builder | `file`, `7zz l -slt` |
| Product/version | ParakeetAI 3.7.0; PE product name is a Unicode blank character | PE/package metadata |
| Architectures | x64 and ARM64; OS/subsystem 10.0 | architecture-specific payloads |
| Authenticode | embedded signer `PARAKEETAI d.o.o.`; exact digest matches; chain verification failed only because Microsoft ID Verified roots are absent from the local macOS CA bundle | `osslsigncode verify` |
| Technology | Electron main/preload/renderer + Rust/N-API native module | package and exact Rust source |
| ASAR | 11,726 files; SHA-256 `c46e9111…26c`, identical across architectures | exact payloads |
| Native source | complete packaged Rust for active-audio-session detection and AEC wrapper | `node_modules/@parakeetai-desktop/native-modules/src/` |
| Updater | GitHub `parakeetai/parakeetai-desktop-releases` | `resources/app-update.yml`, main updater module |

Exact tree: `/Users/uno/Downloads/exe_backtrack_code/recovered/parakeetai-3.7.0`.
There are four packaged N-API binaries (macOS x64/ARM64, Windows x64/ARM64)
plus the corresponding Rust project. The Windows `.node` files do not contain
an Authenticode security directory.

## Entrypoints/resources

- `dist/main/main.js` (490,579 bytes), `dist/main/preload.js` (1,330 bytes),
  `dist/main/mic-monitor-worker.js` (25,333 bytes).
- Single bundled renderer `dist/renderer/renderer.js`.
- Windows microphone permission image, tray resources, updater config.
- Exact Rust files for `audio_input`, `audio_processor`, and platform FFI.

No service, driver, or separate Windows helper executable was found; privileged
native behavior is loaded in-process as N-API.
