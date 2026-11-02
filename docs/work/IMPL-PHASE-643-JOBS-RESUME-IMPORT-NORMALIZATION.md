# IMPL643 — Resume import and portal bundle freshness

Preflight: `bluey-ops`; base00617b2c, branch
`feat/phase-643-jobs-resume-import-normalization`. Jobs only.

## Scope

Correct observed import section/range normalization using synthetic regressions
(FIX873), then refresh generated portal output through the existing strict CI
freshness gate (FIX874). No new data model, fabricated experience, candidate
fact inference, native packaging, overlay/Pinky change or production action.

## Files

Parser and its direct tests; reviewed generated `web/jobs` assets only after
qualification; Round643, FIX873/FIX874, this IMPL, REVIEW and CHANGELOG.
Private test controllers/evidence are outside Git. Personal resume stays private.

## Build and test

Actual gate38567 terminated0 on832 files, manifest
`354d0b9968e9b325c7885f034a466ecfe84ac450e622a5e1ac4d177220c17bf6`.
Official private Node22.23.4/npm10.9.9, locked install, strict web audit0,
provenance, all five typechecks/builds and asar roundtrip passed. Native SDK
remained uninstalled/quarantined. The full regression had2519 unique cases:
Node355, Vitest2085, Python31, Browser release11 and managed-cloud release37.
Both previously skipped Chromium fixtures actually ran; all reported skips0.
Focused document33 and focused SDK16 repeat full-suite cases and are not added.

Two builds and a fresh final pre-export inventory matched exactly. Generated
35-file/4,845,198-byte bundle was retained and synchronized with exact readback;
the prior34 tracked files were backed up before23 obsolete generated names were
removed. Source/index stayed exact during qualification; owned root/parent/home
were removed. Private evidence folder `phase643-node-C7t3pV`:
controller journal `e32cedb5712b3c776cfe8b3c0d40ff43d52a1116d56ca9bd3530f838455bb4e4`;
inner journal `dd9d0942e507597b810c9cf581e7cf1d12b8ba3c218137b8d28cd8b18fcd6952`;
controller `a89242a791f57aca2eb37debef7b80f1e574e9ee97b651069dbd2185ddf93338`;
generated manifest `5f9631c543e6d8c85c1370f13213914dd8e518408bfcd6b98d8533e5234fc787`.

Historical34351 passed older parser source but prompted education-scope review;
78490 refused a busy shared lock before starting inner resources.18579 passed
all tests on current parser source but its controller mistook `# skipped 0` for
`# SKIP`; adding a word boundary fixed that diagnostic bug, with four admitted
zero-skip and four rejected real-skip examples verified before the full rerun.
All earlier outcomes remain preserved; they are not counted as current success.

## Review / follow-ups

Independent review found and verified correction of labelled awards, shared
post-date employer headers, mixed credential/award lists and education-scope
spillover. It also required final export equality and runner-specific skip
refusal. Parent reviewed source and every generated asset by exact source-build
inventory mapping rather than treating minified text as authored source.
Actual staged privacy passed2995 tracked paths/2711 scanned text files;
checkpoint/push and new exact-tip hosted run follow this local acceptance.
Original hosted00617b2c run remains failed. Owner authorized up to10 additional
necessary Jobs workflows; one used, nine remain. No automatic retry, merge,
publication or deployment authority follows from that allowance.
