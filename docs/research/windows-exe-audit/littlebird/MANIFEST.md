# Littlebird 0.81.10 Windows manifest

| Field | Observed value | Evidence |
|---|---|---|
| Installer | `Littlebird-Win-x64-0.81.10-Setup.exe`, 334,080,984 bytes | filesystem stat |
| SHA-256 | `2e202cde30b2d8cedf4295bf4d6160a044797086f2083f7afb07d01a9fcf525f` | [hashes](hashes.txt) |
| Format | PE32 NSIS/electron-builder | `file`, `7zz l -slt` |
| Product/version | Littlebird 0.81.10 | PE version resource, package metadata |
| Architecture | x64; PE OS/subsystem version 10.0 | `app-64.7z`, `7zz l -slt` |
| Authenticode | installer, app, `littlebird-capture.exe`, and `rg.exe` digest/chains verified to LITTLE BIRD SOFTWARE LLC | `osslsigncode verify` |
| Technology | Electron main/preload; React/TanStack renderer; .NET 8 Windows observer | package/dependency tree, helper strings/PDB path |
| ASAR | 64,071 files; SHA-256 `dc51fb4f…1a0` | mechanical extraction |
| First-party maps | 705 non-dependency source maps; extensive original TS/TSX `sourcesContent` | `dist/assets/*.map`, recovery source-map ledger |
| Updater | electron-updater S3 `little-bird-releases`, x64 alpha channel | `resources/app-update.yml` |

Exact tree: `/Users/uno/Downloads/exe_backtrack_code/recovered/littlebird-0.81.10`.
The package declares production identity `com.genos.littlebird` and protocol
`little-bird` in `dist-electron/main/index.js:2980-3026`; Windows registry state
was not created because the installer was not run.

## Composition

- Main: `dist-electron/main/index.js` (509,050 bytes, readable bundled output).
- Preload: `dist-electron/preload/index.mjs` (63,243 bytes).
- Renderer: routes/chunks plus 705 first-party maps in `dist/assets/`.
- Windows helper: signed x64 .NET 8 `resources/bin/littlebird-capture.exe`.
- Search helper: signed x64 ripgrep `resources/bin/rg.exe`.
- Seed storage: `resources/category-seed.sqlite`.
- Native image stack: Sharp/libvips x64.

No driver, service executable, or browser extension was found. The observer is a
user child process, not an installed service.
