# ParakeetAI Windows static evidence

Retained source: `/Users/uno/Downloads/exe_backtrack_code/recovered/parakeetai-3.7.0`.

Static methods: SHA-256, `file`, `7zz l/x -slt -snl`, trusted ASAR extraction,
`osslsigncode verify`, `objdump -p`, `strings`, and source reads. No lifecycle
hook or executable was run.

Observed: NSIS x64+ARM64 payloads; identical ASAR; Electron main/preload/renderer;
exact packaged Rust; Windows x64/ARM64 N-API modules; subsystem 10.0; matching
Authenticode digest but local chain failure caused by absent Microsoft Identity
Verification root; GitHub updater.

Full provenance: retained `MANIFEST.md`, `RECOVERY-LEDGER.md`, `hashes.txt`,
`exact-files.sha256`, and `evidence/`. The Rust references cited in this audit are
inside `asar-extracted-mechanical/node_modules/@parakeetai-desktop/native-modules/`.
