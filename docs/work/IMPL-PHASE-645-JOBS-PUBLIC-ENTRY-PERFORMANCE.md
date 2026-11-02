# IMPL645 — Jobs public first content

Preflight `bluey-ops`, existing agent toolkit, UI/UX/React and performance
guidance. Base0731c172; branch `feat/phase-645-jobs-public-entry-performance`.
Jobs only; current code/local handoff remains authority. No SSD needed.

## Scope

Owner clarified public bluey.sh/jobs feels slow. FIX880 and Round645 record
actual HTTP samples and the empty-root/eager-import weakness without claiming
a reproduced long delay, deployed commit match or measured LCP improvement.
Existing AuthGate stays the sole source of public content; use existing build
tools and dependencies, not a parallel marketing site or new framework.

## Implementation

The dependency-light auth module preserves existing storage ordering/fallback
and login URLs, and API re-exports stay compatible. Anonymous startup retains
only the marked prerendered public root; dev fallback renders AuthGate. Token or
explicit preview loads App dynamically with visible loading/error recovery.
Theme/token storage denial falls back without granting access. Existing beta,
server authorization, application and token-write/sign-out behavior is unchanged.

Build-time rendering uses the same AuthGate and installed Vite/react-dom. One
bounded in-memory SSR chunk admits exactly node:async_hooks/crypto/stream/util;
unexpected imports, dynamic imports, additional chunks or oversized output fail.
No renderer temporary module/root exists. Root/asset/preload/path checks, exact
generated-index identity, fsync and readback complete before successful build.
Failure paths preserve generated HTML. No new dependency/framework/serving app.

## Current qualification

Final gate91375 terminated0 on838-source manifest
`e14512e429aa293928673f2f2d2bff7fbd38689d4bfed9cd5fe33e3d2f354d50`.
It passed2548 unique cases: Node355, Vitest2114 (812/235/315/321/431), Python31,
release11 and managed-cloud37. Focused68 and SDK16 repeat full-suite cases and
are not added. Both actual Chromium fixtures ran; all runners reported zero
skips. Five workspace typechecks/builds, strict web audit0/provenance, archive
roundtrip, actual SSR/data-module execution and both final public-HTML contracts
passed. Repeated builds and fresh pre-export output matched. Source/index,
run parent and control-home cleanup were verified; no new Rust qualification.

Private final evidence `phase645-node-vrN2Su`: controller
`fc2f6dbdf0f00a61cad2984b825984c7f018f7a0d3a5b7c96c822bad8c8991dc`;
controller journal `ac10f8f1ad0bd7f0a084bbdad7f080f6847c3ccba596180e5c881afa13270694`;
inner journal `0c828d0fdece599b357c2fb7dfa21f7d74d1c3a8875929722268673d36697c4a`;
generated manifest `7aa32ac16ae2c230d8d29766af866051371adc409a50850d8aa78f11dc645084`.
All36 generated files/4,863,397 bytes were synchronized after fresh source838
rehash and exact journal/manifest binding. Prior35 files are backed up and
Git-recoverable;21 obsolete hashed filenames were removed. No deploy occurred.
Earlier13662 passed on c4dbdc65 before one final EOF blank-line cleanup. The
full gate was repeated after that whitespace-only correction, with a required
exact7aa32ac1 manifest match to the browser-tested artifact. All36 current
staged files were independently read back against this final export; browser
proof transfers by identical artifact bytes, not by calling old source current.
The old70849c4d controller is retained as the before-whitespace audit copy.

## Actual browser checks and measured output

CUA Chrome observed anonymous hero/sign-in/signup; the local server counted zero
account/API and zero App-chunk requests before preview testing. With entry
scripts deliberately returning503, hero, plans, cloud caveats and anchors stayed
usable. Actual375/768/1024/1440 widths had no overflow. Mobile hero/plans
screenshots were visually inspected. One concrete App chunk was then blocked:
visible recovery and Retry/sign-in appeared, and Retry returned recovery under
continued denial instead of a blank page. All six synthetic preview routes at
375/1440 (12 cases) loaded headings/Preview without overflow; the legacy Browser
URL normalized to Automation with preview/scenario preserved. Browser API count
remained zero. Separate subsequent local GET-workspace/POST-prepare denial
canaries returned503 and are not page requests.

`phase645-browser-qa.json` records observations, not a replayable browser trace.
The QA server stopped through the canonical launcher (exit0), exact root/empty
parent were removed, ports5190–5192 had no listener and viewport overrides were
reset. Preview can store synthetic preferences/theme in its browser origin;
there is no backend persistence/account/provider in this QA service.

Same Node22 gzip9/mtime0 output comparison: HTML715→15,620 bytes (398→5,473 gzip),
entry JS487,828→255,206 (145,772→81,332 gzip), CSS134,041→134,065
(21,576→21,585 gzip). Entry-script gzip is about44% smaller; this is not an
entire import graph, browser LCP, slow-device/network or production latency
measurement. Actual real-token/account transition, reduced-motion/assistive-tech
and multi-browser qualification remain unproven. Staged privacy passed3013
tracked paths/2729 text files; final independent evidence review accepted the
bounded checkpoint. Only FIX880/IMPL645/REVIEW645 docs were restaged afterward;
the full current index is not labelled gate-identical, while product-source
scope remains exactly e14512e4.
Release/whole-product acceptance is excluded.

First actual isolated gate20950 terminated1 in focused tests before the full
suite or portal build. Auth8, App26 and Overview14 passed, but the prerender
suite could not transform a multiline TypeScript cast at test line209, so its
20 cases did not execute. This is not a product/build success. Evidence
`phase645-node-I5gcEQ` preserves the failure on838-source manifest9ad7a49e;
source/index remained unchanged and the owned run parent/control home were
removed. An earlier resource-free preflight refused a stale branch label in
the private controller; correcting that label then passed admission. Neither
failure is relabelled as the later correction's proof.

## Other current evidence

Prior exact ba24cc3d Jobs CI41382722953 completed successfully on Linux and
Darwin, updated2026-11-02T04:13:58Z:
https://github.com/Dhanunjay-Divi/bluey/actions/runs/41382722953 . It does not
qualify newer644/645 bytes. Two of10 additional approved workflows used;8 remain.
Phase644 checkpoint0731c172 was pushed/read back and remains undeployed.

## Boundaries

No merge, deployment, publication, real application/email, new paid resource,
OAuth/provider change, production flag or overlay/Pinky/root-marketing edit.
Cloud-runner startup, ATS/candidate/account/OAuth evidence, cost and release gates
remain separate. The website revamp and whole Jobs product goal are not complete.
