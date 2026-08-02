# Final Round 2.4.0 Windows manifest

| Field | Observed value | Evidence |
|---|---|---|
| Installer | `final-round-desktop-2.4.0-setup.exe`, 209,350,488 bytes | filesystem stat |
| SHA-256 | `74410be0e746553441b9542fd5fc5c8719554139707f167a4fc51ee59de65ffe` | [hashes](hashes.txt) |
| Format | PE32 NSIS/electron-builder | `file`, `7zz l -slt` |
| Product/version | Final Round 2.4.0 | PE/package metadata |
| Architecture | x64; OS/subsystem 10.0 | `app-64.7z`, PE fields |
| Authenticode | installer/main digest and chain verified to Final Round AI, Inc | `osslsigncode verify` |
| Technology | Electron main + nine renderer windows; React/TypeScript output; native C++ N-API | bundle/resource inventory |
| ASAR | 6,727 files; SHA-256 `6ffc3dbd…3f0`; no first-party source maps | mechanical extraction |
| Native modules | unsigned x64 `audio_capture.node`, `audio_detect.node`, `keyboard_monitor.node`; Silero ONNX and ONNX Runtime | resources/imports/strings |
| Updater | generic `https://releases.finalroundai.com/latest`; post-download publisher verification | `app-update.yml`, `out/main/index.mjs:6767-7089` |

Exact tree: `/Users/uno/Downloads/exe_backtrack_code/recovered/final-round-2.4.0`.
The installer also includes `VC_redist.x64.exe`, consistent with the native C++
addons' MSVC runtime imports.

## Window/resource inventory

Nine packaged renderer entrypoints: main, pill, capture halo, launch setup,
intro, coach video, session config, audio indicator, and interview assistant.
Main code is readable bundled ESM at `out/main/index.mjs`; renderer chunks are
complete minified production output. No original TS/TSX, tests, server code, or
native C++ source was packaged.
