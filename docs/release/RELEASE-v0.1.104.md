# Bluey Release v0.1.104

Release candidate: 2026-07-19

Audience: controlled production beta

Status: deployed to the controlled production beta; publication-ready but not
yet publicly released

## Sealed Source

The code and artifact seal is commit
`53c258cf843599f595a1e250d1872d191638d57a`, tree
`65dabc483bea3ea3694a28fc168d5b1e9dd247d0`, with build epoch
`1784475153`. The source archive is 35,634,329 bytes with SHA-256
`63e6846bdda0191cb105b61a856e28f424e8d1950a0fd46351073f37aabeaeeb`.
Exact macOS, physical Windows, Linux-server, Jobs, PostgreSQL, and independent
diff/security/release gates all used this seal.

This release record and the two round documents are updated after deployment
as evidence-only documentation. Their later documentation commit is not the
code/artifact seal and must not be substituted for the commit embedded in the
archives or deployed binaries.

## Summary

`0.1.104` integrates the Jobs truth, discovery, browser-controller, submit
authority, provider-spend, PostgreSQL migration, and maintainability work that
was intentionally held out of `0.1.103`. It also avoids an unnecessary macOS
administrator prompt when an update already has correct public command links.

No item in this release note enables employer-facing execution by itself.
Managed Jobs generation, local Browser distribution, and cloud Browser
distribution remain separate production flags and must stay disabled until
their own canaries and operator approvals pass.

## Desktop

- The updater skips `sudo` only when every public command symlink already
  resolves to the fixed Bluey installation directory.
- Public packages target macOS Apple silicon, macOS Intel, macOS universal, and
  Windows x86-64. There is no public Linux desktop artifact in this release.
- Release verification runs with all four secure-store switches set to `0`:
  `BLUEY_USE_OS_KEYCHAIN`, `BLUEY_USE_SECURE_STORE`,
  `BLUEY_LEGACY_KEYRING_FALLBACK`, and `BLUEY_ALLOW_PLAINTEXT_TOKENS`.

## Bluey Jobs

- Verified public ATS discovery supports Greenhouse, Lever, Ashby,
  SmartRecruiters, and Workday through host-pinned, redirect-free, bounded
  readers in `jobs/automation/src/public-ats.ts`.
- Scheduled sources use canonical board ownership and publish only complete
  snapshots. A failed or partial read cannot erase the previous authoritative
  snapshot.
- Managed resume generation stays factual, lease-fenced, spend-bounded, and
  default-off. Deterministic evidence composition remains the fail-closed
  fallback.
- The owner-approved Career Track policy is not complete in this release:
  relevant experience is not yet role-family scoped, required and preferred
  experience are not separated, and profile fit is not persisted separately
  from tailored packet coverage. This is a P0 blocker before any unattended
  Jobs submission or Browser distribution, but not before the terminal/server
  release while all three Jobs flags remain `0`.
- Local Browser deliveries carry separate result, resume, and final-submit
  capabilities. Immediately before every irreversible click, the Browser asks
  the server to recheck current entitlement, verified identity, exact binding,
  and provider-final-review approval.
- A crash after the durable final-submit marker or click remains
  `side_effect_unknown` and is never automatically retried.

## Bluey Browser Controller

- The local renderer is sandboxed, Node-disabled, and protected by a strict
  Content Security Policy. Sensitive application packets and capabilities do
  not enter renderer or notification payloads.
- Background operation is explicit and reversible, with macOS menu-bar and
  Windows tray controls, pause/resume, intervention notifications, safe close
  to tray, and explicit Quit.
- Local copy says work continues only while the computer is online, awake, and
  unlocked. Only an entitled cloud runner may claim computer-off operation.
- Dedicated multi-resolution Browser icons are packaged for macOS, Windows,
  Linux build tooling, and monochrome tray use. Bluey Browser itself remains a
  separately gated, undistributed beta surface.
- Windows secure-store opt-out uses a random local file key protected by a
  current-user-only ACL. Electron `safeStorage`/DPAPI is not loaded unless an
  operator explicitly opts in.

## Provider Spend And PostgreSQL

- Every managed paid route reserves a durable projected provider hold before
  upstream I/O and settles with explicit `exact`, `estimated`, or `missing`
  usage provenance.
- Only exact, route-matching usage may reduce a projection. Missing,
  ambiguous, estimated, or route-mismatched usage preserves the conservative
  hold; an exact over-projection is persisted before the request fails closed.
- Migration 008 preserves an anonymous, bounded pre-cutover spend baseline so
  a provenance migration cannot make the global cap appear to reset to zero.
- Operator and embedded PostgreSQL migrations now share discoverable target
  markers. Physical migration 009 owns the discovery-board uniqueness
  boundary; migration 010 owns provider-usage provenance.
- Main API and Jobs API binaries must move together across this accounting
  boundary. Paid routes are disabled and drained before rollout or rollback.

## Maintainability

- The previous oversized Jobs persistence module is split into profile,
  discovery, eligibility, application, customer-data, local-runner,
  execution-lease, and workspace domains.
- The previous oversized answer router is split into provider runtime, answer
  planning, behavioral grounding, domain intents, prompt contracts, Web
  search, streaming completion, and completion modules.
- The pre-split source was reconstructed byte-for-byte during the refactor
  audit before the new module layout was accepted.

## Public Platform Artifacts

| Platform | Status | Bytes | SHA-256 |
| --- | --- | ---: | --- |
| macOS arm64 | Exact sealed artifact; installed-runtime gate passed | 22,022,647 | `dfddcc30b78cd2819777ca8c8551a43443fc513bafdd630b14cbecb476892b95` |
| macOS x86_64 | Exact sealed artifact; Rosetta gate passed | 23,391,529 | `bd28eb0fe683b90c070776f50e70c563cd5fa9c5cca241004d81b80de390caa4` |
| macOS universal | Exact sealed artifact; native and Rosetta gates passed | 45,418,523 | `b842b6e31a52a749ce3011bf1717a4a0e5f321a0dd7c531bc94167ca8e6cc12c` |
| Windows x86_64 | Exact sealed artifact; physical Windows 11 gate passed | 22,617,970 | `3887013440ed1cd5c55a6656fae6ef36b6bb1dc42a0625c752e21268b4670a23` |
| Linux desktop | Not published | — | — |

Bluey Browser package hashes are test evidence only and do not enter the public
terminal manifest. The immutable terminal artifacts and signed manifest have
not yet been uploaded, so these rows are sealed inputs rather than a claim that
`0.1.104` is publicly available.

The local publication packet is also sealed and its Ed25519 signature verifies:

| Publication input | Bytes | SHA-256 |
| --- | ---: | --- |
| `latest.json` | 1,376 | `b97a6090d93d69c14455d63c9ff354de997f7a2abb99c8417cc81a3cf4f4f716` |
| `latest.json.sig` | 88 | `9b70ccbf7894979d818e53fb20087682d234ef83b2a895929d8dc7dcc9f1c3b9` |
| `install.sh` | 24,466 | `ced25c22f7d8cf58439bcd08e0563cb6f81d83a6ec93a3b443fdecaff1b74e57` |
| `install.ps1` | 20,778 | `74de690c5eebf6a03cac6aafb2e00ab96b410fab9db919ecfc55ef1e111481b3` |

These are pre-publication identities, not a claim that the files are live.

Reproducibility is stated narrowly. Repackaging the same final built outputs
reproduced each immutable archive byte-for-byte, and the Rust CLI/daemon plus
picker `Info.plist` were identical across clean builds. Independent clean Swift
helper relinks changed Apple `LC_UUID`/ad-hoc `CDHash` metadata, so this release
does not claim byte-identical archives across independent clean Swift relinks.
The hash-pinned artifacts above are authoritative; deterministic Swift linker
metadata (including evaluation of `-no_uuid`) remains a P1 packaging follow-up.

## Installed-Runtime And Linux-Server Verification

Exact sealed installed-runtime gates passed for macOS arm64, macOS
x86_64/Rosetta, macOS universal in native and Rosetta modes, and an interactive
physical Windows 11 desktop. Each lane proved version and executable identity,
secure-store opt-out, overlay/controller visibility, capture/privacy state,
clean lifecycle, and zero residual processes. Windows also passed automation
139/139, Browser 95/95, and runner 50/50; the exact macOS lane passed runner
50/50.

Linux verification passed the main API, Jobs API, worker packages, and Jobs
portal against the sealed source, including the server's 788-test full gate and
the runner's 50/50 suite. It does not authorize or advertise a Linux desktop
package. The sealed Linux deployment inputs were:

| Input | Bytes | SHA-256 |
| --- | ---: | --- |
| Main API | 27,327,944 | `7704fb8d279d1d64af15646f19d14a657ed36d5144fcf89d98ac0e8bad593888` |
| Jobs API | 21,364,968 | `687fc4834a08a53465bde64cb55336f931ae26acf3819139e221edb30a07bb2e` |
| Jobs portal archive | 1,279,010 | `488b0f08f15f479b291f517901f99cd3a25562634051475ad8b67274263fbf3f` |

## PostgreSQL Recovery Proof

PostgreSQL 18.4 with pgvector 0.8.3 passed fresh and restored databases,
operator migration discovery, embedded replay, migration 009 uniqueness, local
submit authority, and the 12 targeted runtime tests on the exact seal.
Immediately before production mutation, backup
`bluey-postgres-20260719T163508Z.pgdump` was verified remotely and again after
transfer to macOS: 24,845,155 bytes, SHA-256
`6cd7272a947a88e377cdf477ba182b29ddadde277b92f77c7a92840106b7022a`.
No customer rows are copied into this evidence.

## Release Boundary

The exact local and physical platform gates, artifact checksum/source checks,
PostgreSQL recovery drills, independent audit, atomic production rollout, and
live health verification are green. GitHub Actions did not execute a job step:
every zero-step job reported, "The job was not started because an Actions
budget is preventing further use." This is an explicit infrastructure waiver,
not a green check and not a product-test failure; equivalent or stronger exact
macOS, physical Windows, Linux, Jobs, server, and PostgreSQL gates supplied the
required evidence.

Public release remains pending only on publishing the immutable desktop
artifacts and signed manifest, verifying the public installers/updater against
that manifest, and then updating the website terminal fallback. Until those
steps complete, `0.1.103` remains the public release and `0.1.104` must not be
advertised as downloadable.

## Security And Privacy

- No credentials, prompts, resumes, job descriptions, customer data, restored
  database rows, or Browser application packets belong in release artifacts or
  evidence.
- Public ATS imports are candidate evidence, not permission to fabricate facts
  or submit. Canonical source verification and current policy still control
  readiness.
- CAPTCHA, 2FA, assessment, missing-fact, authorization, legal, and uncertain
  submit states pause for visible user intervention.
- No source analysis or recovered third-party code changes Bluey's ownership,
  consent, security, or provenance requirements.

## Disabled Flags And Non-Claims

- `BLUEY_JOBS_MODEL_GENERATION_ENABLED=0`
- `BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0`
- `BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED=0`
- Bluey Browser is tested but not publicly distributed.
- There is no public Linux desktop artifact.
- Mailbox/calendar OAuth and broad unattended ATS execution are not claimed.

## Deployment, Publication, And Rollback

Production deployed the exact code/artifact seal during a full ingress outage.
Paid dispatch was held closed and all measured work aggregates were `0` before
the matching main API, Jobs API, and portal moved together across migrations
008, 009, and 010. The four replay-safe data markers were present afterward.
Current verified identity is:

- main API PID `2217136`, SHA-256
  `7704fb8d279d1d64af15646f19d14a657ed36d5144fcf89d98ac0e8bad593888`;
- Jobs API PID `2217137`, SHA-256
  `687fc4834a08a53465bde64cb55336f931ae26acf3819139e221edb30a07bb2e`;
- Caddy PID `2217438` and Jobs portal `index.html` SHA-256
  `309c555e40ed568a84c79e681e93756758392d3cf0762e9fc10e32fc1cb08ca3`;
- both services at `NRestarts=0`, public health reporting the exact seal,
  upstream cap 1,000 cents per 24 hours, and all three Jobs flags at `0`.

The locally staged signed-manifest, detached-signature, and installer hashes are
recorded above and verify. Public upload, live installer/updater smokes, and
website publication remain pending. The website terminal fallback has not been
updated.

Rollback is symmetric and outage-bound: stop ingress and both APIs, then never
run an older paid dispatcher against the migrated authority schema merely
because it boots. A true old-version rollback requires restoring the verified
pre-cutover PostgreSQL dump and both saved binaries/configurations together
before restoring the original positive cap. Now that ingress has reopened, a
database restore could discard post-cutover writes, so normal recovery is
fix-forward. The previous `0.1.103` terminal artifacts remain immutable desktop
rollback inputs.

The R2/offsite credential still returns `AccessDenied`. The verified remote and
macOS copies of the pre-cutover PostgreSQL backup are the current recovery
evidence; R2 repair remains an operational follow-up.

## Related Evidence

- `docs/rounds/ROUND-549-JOBS-TRUTH-SPEND-ATOMIC-ROLLOUT.md`
- `docs/rounds/ROUND-550-INTEGRATED-JOBS-BROWSER-DISCOVERY-SPEND-AND-CROSS-PLATFORM-RELEASE.md`
- `jobs/OPERATIONS.md`
- `jobs/browser/README.md`
- `infra/postgres/README.md`
