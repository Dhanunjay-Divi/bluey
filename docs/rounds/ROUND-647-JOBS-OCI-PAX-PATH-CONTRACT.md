# Round647 — Exact OCI PAX path semantics without ignored security metadata

Base14d56e770670291ab12798888c45c851d34efe42; branch
`feat/phase-647-jobs-oci-pax-path-contract`. Jobs only. Current source/local
handoff/AGENTS remain authoritative; no SSD, overlay, Pinky or meeting changes.

## Observed failure and custody

Actual Phase646 D1 session53434 exited1 after building the exact source image.
Inspection rejected layer type`x`; no positive D1/D2/release is claimed.
Journal4efded7016702ed0eb2b726d0dc7e4d33b20afa6a4a12e0018e4d75b32794ce7.
The exact1,600,514,560-byte failed archive was retained with
SHA341a9f9f67721137545c7c10c008a557e056201a145b72641a6be3ec530afb64;
failure manifestc7912fc3fd5006b484a6e2e3678849f9b83d00f5196c70d57ea2fea7a3297c99.
The external terminal remained1. VM cleanup was reported proven; the canonical
run parent was absent. The diagnostic-only root was copied/hash-verified and
atomically moved into its evidence folder, not deleted. Relocation proof
60826e76258bedfc83a5a9d9f0f7ef1e0a7af8cf30daeac2c56e56a56996aeb7
records13 files/15,472 bytes and old-root absence.

A separate read-only diagnostic walked34,843 headers and found four local PAX
records: UTF-8 certificate path/linkpath metadata and one binary Linux capability
attribute on `usr/lib/x86_64-linux-gnu/gstreamer1.0/gstreamer-1.0/gst-ptp-helper`.
Its first full-UTF8 diagnostic attempt refused the binary value; the byte-aware
diagnostic then completed. Neither diagnostic is qualification or executable
admission. Never ignore those attributes or promote the failed archive.

The first negative replay then exposed a later canonical-path refusal. A
hash-bound bounded metadata audit completed all34,843 layer headers with
17,845,760 control bytes read, four PAX records/376 body bytes and one unsupported
attribute header. Exactly two path/link failures were observed, both literal
backslashes in inherited inactive systemd units:
`usr/lib/systemd/system/system-systemd\x2dcryptsetup.slice` (header8936,
57 bytes/SHA8b5bbd065df580d90e27aead4de0e496bc6c8ffe18e5b77a820d76b89a19275c)
and `usr/lib/systemd/system/system-systemd\x2dveritysetup.slice` (header8937,
58 bytes/SHA5f542635c12db70c97b454e49173df1ecbb652cd4185474e6adb988fe888f91d).
The diagnostic620fc9ac3c91a26e927aacecd3e2c993a156fc2091b2c5dd257023ae587ec951
terminated0, preserved custody and explicitly did not qualify attributes,
regular payload/padding or the symlink graph. It is not D1 evidence.
The final scratch image starts Node as pwuser, not systemd; runner/automation
source has no systemd/cryptsetup references. Independent challenge accepts
removing only those two exact inactive units, rather than changing portable
path identity across JS/Rust/inventory consumers. All path denials remain.

## Bounded decision and requirements

Compare A: narrowly support local layer PAX path/linkpath and remove the exact
unused capability-bearing PTP helper before the final flattened rootfs; B:
model/measure capability and xattr semantics across layers. Independent review
prefers A unless runtime evidence shows the helper is needed. Global stripping,
lossy certificate renaming or a custom repacking pipeline are not substitutes.
The [OCI layer specification](https://github.com/opencontainers/image-spec/blob/main/layer.md)
includes extended attributes among filesystem semantics; silently dropping them
would not be faithful inspection.

1. Only `x` in layer scope may carry one bounded pending local extension. Keep
   global PAX, outer layout and runtime-bundle archives strict. Parse admission
   separately from diagnostic key summaries, using byte record lengths, strict
   UTF-8, unique allowed keys and bounded canonical values.
2. Apply `path`/`linkpath` exactly once to the immediately following ordinary
   header, before effective duplicate/path/link/inventory checks. Reject
   consecutive/dangling records, unknown/duplicate keys, all size/uid/gid/time/
   sparse/xattr overrides, invalid bytes/values, padding/bounds/checksum defects
   and unsupported target types. Count every raw header under a fixed cap.
3. Entry paths remain canonical relative NFC paths. Preserve existing symbolic
   link semantics: an absolute link target is rooted inside the image, and
   relative parent segments are allowed only when resolution stays inside that
   root. Hardlink targets remain canonical relative. Existing target/cycle/
   ancestor/whiteout checks still apply after override; never bypass them.
4. Remove only the exact observed unused helper and the two exact inactive
   systemd unit paths above, with regular/non-symlink presence, successful exact
   removal and file/symlink post-absence assertions in runtime-rootfs before
   runtime measurement/final scratch COPY. Quote literal backslashes; never
   normalize them, glob/remove systemd broadly or rename certificates. Future
   base-image absence must stop for review. Unknown capability metadata must
   still fail. Final inventory must reject the helper's reintroduction and
   canonical path checks must reject either escaped unit's reintroduction.
5. No PAX values enter diagnostics/receipts. Preserve existing bounded rejection
   diagnostics, and ensure newly valid path-only cases do not weaken failures
   for xattrs or metadata overrides.

## Acceptance and execution order

Parent owns docs, branch/resources/controllers, retained archive and actual
runtime/evidence. A bounded builder will own parser/tests/Docker contract after
the design challenge finishes; workers are not alone and preserve other edits.
No worker build/VM/install/service or external write without a specific parent
assignment. The separate B Rust-gate controller may be prepared in parallel,
but all heavy execution shares the existing mac-build lock.

Focused adversarial cases must cover Unicode regular/symlink path overrides,
safe absolute image-root links, duplicates/escape/cycle/missing target,
consecutive/dangling/global headers, unknown/duplicate/metadata/xattr keys,
malformed lengths/UTF8/NFC/control/oversize/padding/header caps and wrong scope.
Run the full relevant Jobs Node/Python/Vitest/type/build/audit/privacy gates.
The retained old archive must still fail at its first incompatible raw unit
path, header8936. Do not skip/repack it or weaken a guard merely to reach the
later capability record. PAX/unknown-attribute/capability denial remains covered
by the actual parser tests; this old-archive replay does not prove that later
record was reached and can never supply a positive D1.
New producer bytes require a new exact commit, reader/source/tool admission,
full actual image/runner/browser smoke and cleanup before normal D2 startup.

No deploy/merge/rebase/retarget/publication, new paid resources, provider scopes,
production flags, real applications or emails. Production disk100% is a
separate release blocker; backup recovery approval remains pending. Four of10
extra approved CI workflows used/six remain; no automatic dispatch/retry.
