# Cluely Windows static evidence

Retained source: `/Users/uno/Downloads/exe_backtrack_code/recovered/cluely-2.0.193`.

Commands used, all non-executing with respect to recovered software:

```text
shasum -a 256 'Cluely Setup 2.0.193.exe'
file 'Cluely Setup 2.0.193.exe'
7zz l -slt 'Cluely Setup 2.0.193.exe'
7zz x -snl -o<recovery> 'Cluely Setup 2.0.193.exe'
node jobs/node_modules/@electron/asar/bin/asar.js extract app.asar <mechanical-tree>
7zz l -slt payload-x64-exact/Cluely.exe
objdump -p payload-x64-exact/Cluely.exe
osslsigncode verify -in payload-x64-exact/Cluely.exe
rg / static reads over the mechanical ASAR tree
```

Observed results:

- NSIS contains `$PLUGINSDIR/app-64.7z` and `app-arm64.7z`.
- x64 and ARM64 ASAR SHA-256 are exactly equal.
- PE version is 2.0.193; OS/subsystem version is 10.0.
- Installer and both application signatures verify to Cluely Inc.
- App entrypoints: `dist-electron/main.js` and `dist-electron/preload.mjs`.
- Update manifest: `resources/app-update.yml`.
- Windows helper: `resources/extra-resources/sox-14.4.1-win32/sox.exe`.

See the recovery tree's `MANIFEST.md`, `RECOVERY-LEDGER.md`, `hashes.txt`,
`exact-files.sha256`, `evidence/signing.txt`, and `evidence/entrypoints.txt` for
the complete exact-file provenance. No installer or payload executable ran.
