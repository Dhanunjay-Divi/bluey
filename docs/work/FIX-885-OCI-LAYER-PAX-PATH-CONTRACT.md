# FIX-885: Preserve layer PAX paths without discarding security attributes

> **Codex preflight:** Load `$bluey-ops`; current code/local handoff govern.

## Issue

Actual Phase646 D1 built the runner image but rejected a valid local PAX path
record during OCI inspection. The exact failed archive is retained; no runtime
or release acceptance exists. Round647 records its terminal/hash/custody.

## Root Cause

The strict tar reader accepts only ordinary ustar entry types. The pinned
Playwright base contains UTF-8 CA filenames, which the producer expresses using
local PAX path/linkpath records. The retained bytes additionally contain one
binary Linux-capability attribute on the GStreamer PTP helper. Therefore
skipping PAX or treating its body as inert would lose filesystem or security
semantics; deleting the certificate would change trust behavior unnecessarily.

After those PAX names became readable, the old archive reached a separate
portable-path incompatibility: two inherited systemd slice-unit names contain
literal backslashes. The bounded complete metadata audit found exactly those
two path/link defects; Round647 records names/hashes/counts. The final scratch
container starts Node, not systemd. Excluding those exact inactive units is
smaller and more faithful than changing the shared portable path identity.

## Fix Summary

Source implementation is frozen and independently reviewed under Approach A: layer-only
single-next-entry path/linkpath support, all unsafe/unknown/metadata/xattr keys
still denied, exact unused PTP-helper removal before final flattened image,
and independent final-inventory absence. It also removes only the two exact
observed inactive escaped-name systemd units before measurement, with the same
regular/non-symlink/pre/post assertions. Entry and link semantics remain
distinct; existing target/cycle/escape/ancestor/whiteout checks still apply.
No broad xattr support, global stripping or archive-repacking framework.

## Files Modified

| File | Change |
|------|--------|
| `jobs/scripts/managed-cloud-release-gate.mjs` | Bounded layer-local path semantics and runner absence contract |
| `jobs/scripts/managed-cloud-release-gate.test.mjs` | Positive Unicode/link and adversarial metadata/scope cases |
| `jobs/runner/Dockerfile` | Exact helper removal/assertions before final flatten |

## Edge Cases Handled

Source-authored cases cover strict byte lengths/UTF8/NFC/value bounds; duplicate/unknown keys;
consecutive/dangling/global extensions; entry/link distinction; duplicates after
override; invalid scope/type/targets; zero padding; physical header/cumulative
metadata limits; security-capability and other metadata rejection. Existing CA
paths and safe image-root symlink resolution must be preserved.

## How to Test

Current corrected source passed actual run91533:2770 unique/0failed/0skipped,
five workspace types/builds, both real Chromium fixtures, audit0, deterministic
export, source/index equality and cleanup. Focused248 and repeats excluded from
the full count. Negative replay8006 passed only as negative evidence; old bytes
remain refused at8936 and capability9889 was not reached. IMPL/REVIEW record
exact source/controller/journal hashes. The earlier failure below is historical.

The first retained-archive replay terminated1 on raw path header8936 before
the expected capability record9889. A bounded copy-only predicate diagnostic
identified a57-byte regular-file name containing a literal backslash, with all
other path predicates valid; REVIEW records the exact hashes/bounds. The
complete metadata scan identified two inactive systemd slice units; their
exact producer-removal correction passed independent source review with no
P0-P3. Parser0aeb stayed unchanged through that correction. The full source
gate subsequently passed; new actual image/VM smoke remains unrun. Source
acceptance is not D1/release acceptance.
There are87 new discovered cases and two obsolete valid-path denial cases
removed, net85 additions, confirmed by managed150/focused248/unique2770. The
51MiB header-limit case passed within the bounded focused/full command gates;
no standalone performance benchmark is claimed.
An offline replay of the retained failed bytes must preserve first-path refusal
at8936. It does not reach/prove the later capability record and cannot qualify
the old artifact. No repacking or skip is allowed to manufacture that outcome.
Actual browser/native/runner smoke must prove the helper removal is compatible.

## Known Limitations

Source-only acceptance waives no finding/test. A new exact commit, source and
tool/reader admission, fresh image build/D1, normal D2 startup and downstream
product/release evidence remain required. Final actual image qualification is
consolidated after648 source freeze so its shared runtime/protocol changes do
not invalidate a knowingly intermediate image. Production disk100% and pending
verified-backup recovery are separate blockers. No production mutation,
deployment, application/email, CI dispatch or paid infrastructure in this fix.
