# Littlebird Windows static evidence

Retained source: `/Users/uno/Downloads/exe_backtrack_code/recovered/littlebird-0.81.10`.

```text
shasum -a 256 Littlebird-Win-x64-0.81.10-Setup.exe
file Littlebird-Win-x64-0.81.10-Setup.exe
7zz l -slt <installer and PE files>
7zz x -snl -o<recovery> <installer>
node jobs/node_modules/@electron/asar/bin/asar.js extract app.asar <tree>
osslsigncode verify -in <installer/app/capture-helper/rg>
objdump -p <capture-helper>
strings/rg static reads; source-map `sourcesContent` ledgering
```

Results: NSIS x64 payload; Electron entrypoints
`dist-electron/main/index.js` and `dist-electron/preload/index.mjs`; 64,071 ASAR
files; 705 non-dependency source maps; signed x64 `littlebird-capture.exe` and
`rg.exe`; Windows 10.0.19041 .NET helper evidence; S3 alpha updater.

Full provenance is in the retained tree's `MANIFEST.md`, `RECOVERY-LEDGER.md`,
`hashes.txt`, `exact-files.sha256`, `source-map-sources-exact/SUMMARY.json`, and
`evidence/`. No recovered executable was launched.
