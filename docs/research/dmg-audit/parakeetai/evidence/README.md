# ParakeetAI evidence ledger

Scope: static, read-only inspection of `/Users/uno/Downloads/dmg_backtrack_code/ParakeetAI-3.6.21.dmg`, one separately authorized and tightly constrained unauthenticated runtime observation, and comparison with Bluey at the Git commit recorded in [bluey-code-map.txt](bluey-code-map.txt).

The application was not installed. Static work and the authorized runtime pass both used an image attached with:

```text
hdiutil attach -readonly -nobrowse /Users/uno/Downloads/dmg_backtrack_code/ParakeetAI-3.6.21.dmg
```

`diskutil info /dev/disk7s1` reported both `Media Read-Only: Yes` and `Volume Read-Only: Yes (read-only mount flag set)`. The mounted volume was `/Volumes/ParakeetAI`.

Static inspection used only metadata and read-only tools: `stat`, `shasum`, `hdiutil imageinfo`, `diskutil info`, `find`, `file`, `otool`, `codesign`, `spctl`, `xcrun stapler validate`, `plutil`, `PlistBuddy`, `strings`, and `rg`. No bundled script, package manager, native module, or package lifecycle hook was run during static analysis. The later runtime pass executed only the signed app entrypoint with an isolated temporary HOME/profile and external networking denied. Its exact command, failed harness setup attempts, CDP observations, profile inventory, errors, termination, and cleanup are in [runtime-unauthenticated.txt](runtime-unauthenticated.txt).

The audit helper [asar_static_extract.py](asar_static_extract.py) reads ASAR metadata and regular packed members without importing or executing target content. Extraction was limited to a temporary directory outside the repository. It rejects path traversal, skips links and unpacked members, and verifies each member whose ASAR record supplies a SHA-256 digest. No extracted proprietary bundle or binary is retained in this evidence directory. Native linked-library, build-version, exported-symbol, and helper metadata is normalized separately in [native-metadata.txt](native-metadata.txt).

Evidence convention:

- `app.asar::path` identifies a virtual member of the mounted Electron archive.
- A byte offset is the zero-based byte position returned by `rg -bo` in the identified, minified member. Each significant offset is paired with that member's SHA-256 in [bundle-static.txt](bundle-static.txt), making the observation reproducible against this exact artifact.
- “Observed” means directly present in static resources or Bluey source.
- “Inference” is a reasoned interpretation that still requires runtime or server validation.
- “Not observed” means the inspected static artifact did not expose the behavior; it is not proof that an uninspected server lacks it.

The evidence directory intentionally contains only UTF-8/ASCII text.
