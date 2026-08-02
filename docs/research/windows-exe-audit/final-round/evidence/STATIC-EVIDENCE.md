# Final Round Windows static evidence

Retained source: `/Users/uno/Downloads/exe_backtrack_code/recovered/final-round-2.4.0`.

Static methods: SHA-256, `file`, `7zz l/x -slt -snl`, trusted ASAR extraction,
`osslsigncode verify`, `objdump -p`, `strings`, and `rg`/source reads.

Observed: x64 NSIS; subsystem 10.0; Electron main plus nine renderer entrypoints;
6,727 ASAR files; no first-party source maps; signed installer/main; unsigned
audio capture/detect/keyboard N-API modules; WASAPI/WebRTC APM symbols; Silero
ONNX; VC++ redistributable; generic updater with post-download publisher pin.

Full exact provenance is in the retained `MANIFEST.md`, `RECOVERY-LEDGER.md`,
`hashes.txt`, `exact-files.sha256`, and `evidence/`. No installer, native module,
or application process was executed.
