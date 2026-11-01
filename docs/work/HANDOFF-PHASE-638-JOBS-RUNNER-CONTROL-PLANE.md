# Phase638 continuation — review-first protocol qualified; broader gates open

Date2026-10-28. The overall Jobs production-readiness goal remains ACTIVE and
unfinished. This is an intermediate local handoff, not source/release acceptance.

## Current checkpoint (supersedes earlier run chronology below)

### November 1 test checkpoint — current

The owner subsequently approved the disposable4CPU/6GiB local VM, with no paid
resources, production data or real job/mail actions. Next is a separate bounded
managed-runtime startup phase; no VM has started under this approval yet.
The reviewed Phase638 local checkpoint must precede its HEAD-bound image build.

Post-fixture regression `56666` is TERMINAL0 on current774/9b1ee: Node
306/7/17 plus Vitest806/219/310/321/397 = 2,383 actual offline cases,
zero extra skips, all five typechecks, four nonportal builds and actual-config
private portal build passed. Both mandatory browser files remain explicit
exclusions, not new passes. Journal `EVIDENCE-PHASE-638-SAVED-REGRESSION-20261101.jsonl`
has eleven validated events; SHA-256
`b443fa8ef4f0d1ea9e1cab857e5a11203eb1fed897ea69637a5112dc586c9b11`.
All source/HEAD/scoped and full status, tracked portal, private browser inventories,
exact root removal, empty parent, launcher PID/process-group and real-index checks
passed. Recipe SHA before execution:
`69cc447ae5444c9672e25b45c1bb9db0095839a65281cb7e7a5c0a941e9593aa`.
Launcher self-test `44139` then exited0, including its own residue assertions;
its expected cleanup-failure diagnostic is a deliberate negative, not a run failure.
Fresh source/HEAD/status/index remain identical. Parent computationally replaced
only the four declared test-file hashes with their recorded bases in the current
774-entry map and recovered EXACT prior manifest3de70. This proves all other770
inputs, including Rust and the two older browser gates, unchanged; it is not a
new Rust/browser execution or broad release acceptance.

The owner targets a user test on November 2, not authorization to deploy or send
real applications/emails. The following full gates qualified the pre-fixture
774-file `3de70...` manifest at HEAD `40949858`, raw scoped status `6754505e...`.
API preservation `59610` then exited 0 with ten durable events and final source,
PG/PID/listener/root/parent/index checks. Journal SHA-256:
`fde65fbce37805a55ed17820658c67f177236b93527396d8bffaf08ac34be441`.
Parent then applied the reviewed four-file test-only human fixture. Current774
manifest is `9b1ee47e1deeb796af1d771513f6c2dd60362f832f1f4c5711bd8d9b2990c764`,
raw scoped status `932ee3fd34f0e460355ecf5ef07063c2c940cd431cacf8e2fc22da5588722fdc`.
Four module syntax checks and nine report tests passed. The four new-source
fixture runs subsequently qualified: default33796 v4 9/9, headed agent93177
v1 8/8, startup SIGTERM5512 and ready SIGTERM10800. All four controllers exited0;
negative wrappers intentionally did not pass. Final source/PG/PID/listener/root/
parent/index checks and fresh retained-report/journal validation passed.
Journal SHA-256 values, in that order:
`35130f465cd2157faaf8d928a45a4d0876a8ebeeb90d1f0699ad8785852bae54`,
`308be50b1d0cbed1c0ced7a22b4ad7b6ac634e21174743a28d30f3716793df08`,
`9430914ea7fec1f5c2da18ab0955b55a77d68c6e9bdf5671f41f0a9d1b002594`,
`f7c017633ee6886e4bed568576d8a4dc163d8d77676c5ae32c4939b461bb31fb`.
Four malformed modes also denied before resource startup. Human guide now gives
the November2 fresh private launch command. This is one disposable saved session,
not password login/cross-restart durability/owner approval/release. Prior full
gates retain their exact pre-fixture scope rather than silently qualifying all
changed source. No Rust/portal product implementation changed in this addition.

- Offline Jobs run `31214` exited 0: 2,380 tests, zero unexpected skips, five
  typechecks, four non-portal builds and the private portal build; tracked portal
  and source unchanged. Parent final identity matched; private parent/index empty.
  Durable report: `EVIDENCE-PHASE-638-OFFLINE-JOBS-20261101.json`, SHA-256
  `5f3b11ebc81f9ca0881cb33299007aa88392a7dd7a7785b26616d4c741464495`.
- Browser run `61556` exited 0: the two previously excluded singleton cases
  actually passed, no skips. Nested document cases: PDF, DOCX symbols, TXT,
  malformed and oversize; real PDF worker, zero unexpected requests/page errors,
  clean browser/engine/server/listener shutdown. No employer request escaped the
  synthetic exact-submit fixture. Source/portal unchanged; final identity and
  empty parent/index verified. Durable report:
  `EVIDENCE-PHASE-638-TWO-BROWSER-GATES-20261101.json`, SHA-256
  `f185717c4c877bd0c45db569374d1d6c0a14b3df192c8c70137e79eb834b2b6f`.
- Combined selected Jobs test evidence is 2,382 actual cases across these two
  runs, not a claim that the offline-only run launched a browser.
- New full Rust gate `52148` is TERMINAL 0, accepted only as local Rust evidence. Recipe:
  `COMMAND-PHASE-638-NOV1-REVERIFY.md`, SHA-256
  `17f1d436775ccca315a6bbb9a7f83d55f54f31844e64473eade6ecf9150bae7a`.
  Root: `/private/tmp/bluey-test-runs-phase638-nov1-rust-20261101/run.1793511542.16881.2974195990`.
  Durable fsynced journal: `EVIDENCE-PHASE-638-NOV1-RUST-20261101.jsonl`.
  Format and strict both-feature all-target Clippy PASSED. Full all-target counts
  are `[1767,0,1,14,1,1,2,113,2,1,5,9]`: 1,916 passed, zero failed/ignored, all four
  exact FIX831 regressions passed. Library duration 3,133.11s; 113-case HTTP
  integration duration 606.44s. It reuses separately recorded exact-byte 36
  focused/43 actual-PG passes, without relabeling lost run76069 green. Parent read
  and validated the exact nine-event fsynced journal, SHA-256
  `61ab2f12353a85110fc96714514ccb9051b1dc2dfd609cd7357865597cebd2d5`.
  Fresh final source/HEAD/raw status matched BEFORE product edits; exact root and
  launcher PID absent, private parent and real index empty. Do not poll/restart
  this terminated session. Ordinary API preservation and the subsequent fixture
  qualifications have now completed as recorded above.
- Safe human preview is running in session `79377` at
  `http://127.0.0.1:5187/jobs/?preview=1`. It uses private HOME/cache, disables dotenv
  and blocks `/api`, `/auth`, `/account` routes with 403. HTML routes force preview.
  Parent verified those guards and visibly walked Overview, Resume, Matches and
  synthetic Prepare -> Applications/Awaiting Review. This is in-memory UX only;
  no auth/persistence/runner/provider/receipt evidence. The preview's own canonical
  root is intentionally live while serving; it is separate from the Rust root.
  Human guide: `TEST-BLUEY-JOBS-2026-11-02.md`. This preview remains separate from
  the subsequently qualified interactive private saved-session fixture above.

The owner subsequently selected **Saved non-production account**, not the resettable
preview. Parent clarified the first target as same-run PostgreSQL-backed profile/
prepared-application persistence across refresh, not password-login or cross-restart
durability. `jobs_client_release_review` owns only the prepared artifact
`PREPARED-PHASE-638-HUMAN-HOLD.patch`; parent subsequently applied it only after
API preservation `59610` and final cleanup passed, as recorded above.
It targets four existing test-only files and a bounded headed synthetic-session
mode. Independent review corrected encrypted-column SQL casts, lost controls after
refresh, egress proof/error gates, cleanup inventory and signal-safe shutdown.
The prepared native-apply artifact SHA-256 is
`7617a146edd08f685c9dfaf008ec2f429f90d1dca77ae3f0d9b7dc9ad11f77f5`.
Only file/hunk headers and empty context padding were mechanically converted
from the reviewed Git diff
`266bb68c52b0c84b48a94ee6ead9b45b5f0a05ff4140b50c10910065e1f80542`;
independent static review has no remaining P0-P3 findings. The last change only
makes the temporary test control compact so it does not stretch across the footer.
Parent also corrected the row locator to match the actual versioned Review First
label in ApplicationsView, retaining the independent API submission-mode assertion.
Preparation history: parent first applied the diff strictly IN MEMORY (not to
files): all four proposed modules parsed and nine pure report tests passed,
zero failures/skips. Only after ordinary API preservation and final cleanup did
parent apply it to files and run the four qualifications summarized above.
The synthetic saved-session test is now qualified within its stated limits;
that does not establish production readiness.

Prospective scratch-index privacy before Rust dispatch passed: 80 owned paths,
2,953 tracked/2,677 text, tree `a32bb257a2af5fd871169d19219d9fe125109552`.
Two earlier preflights stopped on an extra EOF blank in the new command doc;
it was corrected before any successor runtime. Real index was never staged.
No production, hosted CI, paid resource, real provider/application/email, VM,
Temporal, overlay or Pinky action. API/cloud/provider/capacity/OAuth/
C2C/MCP/contracts/cost/latency and release acceptance remain open.

### October 29 continuity correction — supersedes earlier liveness

Run76069 is no longer observable or active. Its process handle and observer cell
are unavailable after the preceding turn failed with model-capacity error; no
terminal exit code or full all-target summary was recovered. Do not describe it
as live, passed or failed. The recorded 36 focused and43 actual private-PG passes
retain their bounded scope; the full gate remains UNVERIFIED.

Fresh read-only reconciliation found HEAD409498589b8d2e62b5ef54ebdee1372aeb8d005f,
774-source manifest3de70edc32c2f4b9b3199747265b5eaa78e75a38ed30f60d3361fef4f14e22c1,
raw scoped-status SHA6754505e7726bb0a1fb2c63b1c8f37767b2afe525e6f16218ed86c5dfa0f85d5
and tests SHAcca8f7aa8fa16ae98a84c5a7e5a23d638375e7802f0ee04a4ac684905cf7bb72
match the admitted source. The exact recorded run root is absent, its private
parent is empty, the real Git index is empty, and process inspection found no
related Cargo/test/PG/launcher process. This proves current quiescence and source
identity, not the missing terminal outcome. No cleanup/deletion was performed.

API preservation and release acceptance remain blocked by the missing full
result; do not execute the obsolete API recipe. A successor verification must
retain its bounded non-secret exit/summary/source/cleanup evidence outside the
auto-cleaned runtime root before declaring acceptance. Review that capture first;
do not silently restart the old run or infer success from an absent root.

Cost review is recorded in COST-PHASE-638-COMPETITIVE-ACCEPTANCE.md. Pending held
provider exposure must not disappear from a settled-only cost numerator. Neither
competitive total COGS nor full product parity/production readiness is proven.
Only evidence docs/Asana were updated in this continuation; source stays frozen.

FIX842 successor76069 is LIVE/RUNNING, not accepted, at frozen774-source
3de70edc32c2f4b9b3199747265b5eaa78e75a38ed30f60d3361fef4f14e22c1,
HEAD40949858/status6754505e; testsSHA
cca8f7aa8fa16ae98a84c5a7e5a23d638375e7802f0ee04a4ac684905cf7bb72.
Independent source/command review and scoped rustfmt/whitespace clear. Test-only
change scopes the same strict final-submit prelock assertion to its existing PG
branch; production order/guards untouched. COMMAND-PHASE-638-FIX842-RUST-GATES.md
runs failedguard FIRST in6 singleton cases, then7preflights/43actualPG/23focus/full.
36focusedtotal; fullfloor1875/testcount unchanged. Canonical sharedmac-build and
private root cleanup remain mandatory; freeze through terminal AND final checks.
Pre-run privacy2947tracked/2671text passed tree
debad273681591eaf91694705d8d22cb0c715946. Sharedmac-build/canonical exact root:
/private/tmp/bluey-test-runs-phase638-fix842-rust-20261028/run.1793238734.57536.1037143200.
Poll76069, not failed56416; do not restart on quiet output. No stage result is
inferred from compilation. Parent alone owns runtime; all source remains frozen.
Now observed: fmt/strictbothfeaturealltargetClippy PASS2m05, testbuild2m43;
all6 exact upfront cases PASS, including formerly failing PG prelock guard FIRST
(1pass/0fail/0ignored,0.01s). All7 preflights+23 postPGfocus PASS,36focusedtotal.
All43 ACTUAL fresh-per-casePG PASS, wrapperstatus0/serverExitedtrue and parentPID
absence confirmed BEFORE fullSQLite. Mandatory no-PG all-target gate has STARTED,
not finished. Midrun exact774/3de70/HEAD/status match. Final identity/rootcleanup
pending. This validates bounded components, not the full gate or release.
The formerly failing postgres_operational_hold_lock_is_first_in_every_protected_admission
has now also emitted exactly one passing line INSIDE this current full segment,
not just its focused preflight. Full run remains live; no terminal acceptance yet.

Previous FIX841 run56416 is TERMINAL1, not live. Library1766pass/1fail/0ignored in
2882.19s; later all-target binaries/integration/examples NOT reached. Exactpanic
tests.rs883: cloud final-submit admission performs work before prelock. FIX842
confirms stale lexical .find started at SQLite's newly shared marker, not PG.
Prior35focused+43ACTUAL freshPG cases and fmt/strictClippy2m00 passed at50f7;
those bounded passes do not qualify new3de70 bytes or release. Final774/50f7/HEAD/
status EXACT beforeafter captured BEFORE edits; PG already stopped/PID absent,
exact run.1793234488.36773.2708630970 root absent, parent/indexempty37GiBfree.
Do NOT poll/restart56416 or relabel it green.

Prepared API retry doc COMMAND-PHASE-638-CURRENT-API-RETRY-PRESERVATION.md,
SHA ab427aa5fd67fb18c2472f481590985bdd41aa0e22b3fdebb48194c28cfe90ac,
is OBSOLETE/BLOCKED on failed50f7. It was independently reviewed with bounded
no-follow report basename/bytes/SHA capture BEFORE cleanup; no runtime ran.
Repin/re-review only after current-source full0/final equality+cleanup and all4
FIX831 exact current-full passes. Future API parent:
 /private/tmp/bluey-test-runs-phase638-current-api-retry-20261028.
Offline Jobs command also has old50f7 pin; repin before execution. Its two
mandatory browser fixtures remain UNVERIFIED/pending owner's explicit synthetic-
browser answer. No browser/provider/VM/Temporal/employer/mail/production/price/
flag/owner DB action. Goal remains ACTIVE and broader capabilities remain open.

FIX840 successor62964 is TERMINAL1, not accepted, at frozen774-source
d5619475934fec80eda3dfc5b5d4dbfcd2649b768f355958916f66b94d9c1a46,
HEAD40949858/status6754505e. Command-only correct DB-module names and exact5
singleton discovery reviewed clear. Pre-run privacy2942tracked/2666text passed,
tree357aadfb683faa22b55f0ed999898366f987349c. Sharedmac-build/canonical root:
/private/tmp/bluey-test-runs-phase638-fix840-rust-20261028/run.1793233496.28456.294284640.
Do NOT poll/restart62964. All5 focused+7preflight passed; actualPG43=41passed/2new
fixture AdminActorRequired failures. Later23focus/full not reached. Final exact
source/HEAD/status matched BEFORE edits; PG stopped/serverExitedtrue, exact root
absent/parent-indexempty37GiB. FIX841 tests-only correction is statically clear:
separate verified synthetic Account API admin authorizes audited cohort setup;
candidate stays explicitly non-admin and alone receives enrollment/identity/Track.
Current774-source50f7a7f78cc2a96d5664580833f5546ced067993e5d4ac146062ea61f2dc0df4,
testsSHA bc24abd478512f85c9c5c3e8040d24a519906e4843c66fd618445db5fc303a82.
COMMAND-PHASE-638-FIX841-RUST-GATES.md is prepared, NOT executed; only pin/private
parent/eventlabel differs, all selection/discovery/strict/count/cleanup unchanged.
No browser/provider/VM/Temporal/employer/mail/production/owner DB action.
Offline Jobs command is prepared with old d561 pin (update to current before run),
not executed; two mandatory browser fixtures remain unverified/excluded pending
owner's explicit synthetic-browser answer. Next API preservation needs a separate
fresh canonical PG root after this Rust run terminates/cleans.

FIX839 successor62014 is TERMINAL1, not accepted, on frozen774-source
d5619475934fec80eda3dfc5b5d4dbfcd2649b768f355958916f66b94d9c1a46,
HEAD40949858/status6754505e. Pre-run privacy2940tracked/2664text passed,
tree6394bb2c2238b2bb43a1d91f51088119c29a33ea. Tests SHA d00abbdb...b6f9f0 and
independent static review clear. Shared mac-build/canonical root:
/private/tmp/bluey-test-runs-phase638-fix839-rust-20261028/run.1793233031.25828.25451104120.
Fmt/strictalltargetClippy passed1m57/testbuild2m35. First3 core tests PASSED1each:
original4.01s/authenticatedunmanaged3.45s/manageddeny+start+replay5.53s. Fourth exact
session filter matched0; mandatory countguard stopped before remainingcase/PG/full.
FIX840 corrects module name to execution_session_authority_db_tests (actual path
declaration in jobs.rs), adds exact singleton --list discovery for ALL5 before any
execute. New command is prepared, not executed; source unchanged. Parent FINAL
rehash BEFORE edits matched exact d561/HEAD409/status6754505e; root absent/parent-
indexempty37GiB/noPGstarted. Do not poll62014. No external effects.

FIX838 successor89202 is TERMINAL1. Format/strictalltargetClippy passed1m59s,
test build2m35s, original unmanaged-start red passed1/0/0 in3.87s. Next SQLite
authenticated claim/start/replay failed the snapshot's nonexistent account_id on
managed workflow bindings (HMAC-subject table); PG/full not reached. FIX839 records
test-only whole16family correction and static re-review, SHA d00abbdb...b6f9f0.
No runtime qualified yet. Next prepared COMMAND-PHASE-638-FIX839-RUST-GATES.md uses
774-source d5619475934fec80eda3dfc5b5d4dbfcd2649b768f355958916f66b94d9c1a46.
Past89202 pre-run privacy2938tracked/2662text passed,
tree16c72b43433c304307feddeafabdc326f4fbd144. Its admitted774-source
83ea8ce0d6c1061f3b6ccefcd10d3924f2f7d24f9c9818a0b1ae660a1fdd647a,
HEAD40949858/status6754505e. Shared mac-build/canonical root:
/private/tmp/bluey-test-runs-phase638-fix838-rust-20261028/run.1793232521.22993.2021017080.
Exact root removed, parent/index empty37GiBfree; no PG started. Do NOT poll89202.
Postterminal worker edit began before parent rehash, so no exact unchanged-after
claim: reconstructing only the admitted tests SHA reproduces83ea8ce0 and confirms
all other source unchanged. Next final rehash must precede dispatching any edits.
The retained recipe orders strict
format/Clippy,5FIX838 focused cases,7preflights,43fresh-DB actualPG cases (32selectors
unchanged cap), PG stop/PID absence,23focused then full SQLite all-target suite.
No stage completion is inferred from compilation. Corrected d561 source must stay
frozen through next terminal and final identity/cleanup. No browser/provider/VM/
Temporal/production or owner DB. Independent review retracted the old schema-filter
clearance and checked all16 tables/current isolated callers after FIX839.

Full Rust15847 is TERMINAL1, not live. Library1763passed/1failed/0ignored in2584.32s;
later all-target stages not reached. Exact panic tests.rs20737:10: unmanaged
authenticated start with None managed tuple returned NotFound. FIX838 records
confirmed outer-Some/inner-None session-selection regression in all4 claim/start
DB selectors. Parent has implemented scoped production dispatch; tests worker owns
tests.rs regressions. No runtime acceptance yet. Root/API-group cleanup succeeded,
parent/index empty,37GiBfree; source774/e959/HEAD409/status6754505e matched beforeafter.
No live test handle remains. Do NOT poll/restart15847; next is focused corrected-source
legacy/managed/actualPG qualification after worker freeze/review/privacy.

Parallel read-only product audits are complete and independently reviewed:
AUDIT-PHASE-638-C2C-MCP-READINESS.md. Inspected shipped Jobs source has REST
capabilities but no MCP server/tool registry/transport/CLI. Audience/Track/tool
delegation and irreversible-action challenges are planned, not implemented MCP
authority. C2C/W2/fulltime taxonomy/persistence/eligibility exist; Phase633 proved
fulltime+w2 to local queue, but C2C preference persistence is not an exclusive C2C
runner/receipt journey. Vendor chat/outreach/contracts remain Phase620D design-only;
application-bound recruiter replies/calendar are not that product. Proposed first
MCP read-only and no-egress C2C slices do not redefine the full goal as complete.
Owner program brief1219993222870748 now records these gaps and remains incomplete.
No source code or runtime action was made by the two audit assignments.

Historical failed Rust15847 used COMMAND-PHASE-638-CURRENT-RUST-GATES.md on sourcee959,
HEAD40949858/status6754505e. Formatting/strict both-feature all-target Clippy passed
(1m59s), all41 fresh-DB actualPG cases and30 focused cases passed before the full
library failure. PG was stopped/PIDfile absent before SQLite. The full all-target
suite started23:12UTC but ended at library1763passed/1failed; later targets did not
run. Final source/HEAD/status matched. Its exact root
/private/tmp/bluey-test-runs-phase638-current-rust-20261028/run.1793228345.7208.3564214310
was removed. No lock/run handle remains live. These earlier passes do not qualify
the corrected FIX838 source. Do not restart or poll15847. Next successor must run
focused unmanaged success AND complete-managed omission denial in both dialects
before full gates. Preserve private fresh-DB PG isolation/explicit shutdown before
SQLite, both features, strict lint and canonical cleanup; no browser/VM/Temporal or
production action. A later API preservation run needs another fresh private root.
Pre-run privacy2933tracked/2657text passed tree660b5946f44c7a0975cfdb8eba3c59497104bbd7.
An earlier whitespace preflight stopped before execution; the command doc's extra
EOF blank was removed before the successful privacy gate and dispatch.

Read-only inherited PR-chain reconciliation is in AUDIT-PHASE-638-PR-CHAIN-26-32.md.
All seven remain open drafts; #27–#32 report historical Jobs CI success, #26 no
rollup. #29's non-ancestor CAS fix is patch-equivalent to ancestor2e2a18a9, not a
missing correction. No cherry-pick/PR mutation is indicated. Original f501 tip is
older than current remote #32 tip66ad0fa8; never push the obsolete handoff tip over
that history. Current local40949858 descends from66ad0fa8; Phase638 remains dirty.

Prepared negative matrix PASSED in both modes: review-first83098 terminal0,
build1m25s; auto50372 terminal0, build1m23s. Both used unchanged774-source
e9591654d8b9b91d7c58976887579f655dd0bd38f043fccb299bec9422cb19a3,
HEAD409498589b8d2e62b5ef54ebdee1372aeb8d005f and scoped status6754505e...
before/after. All24 ordered cases per mode required one actual HTTP response,
exact status/body SHA/client classification, honest nonce/proof freshness,
bound inputs and unchanged20-family state after each denial. Retained valid
heartbeat200/same token+fence+scope/extended expiry, safe release204, fresh terminal
replay204, nine immutable families/stable attempt and zero effects all passed.
Review-first: zeroATS/3clean children; auto: ordinary Track consent, factual packet,
production-decoded initialATS, complete ATS row unchanged/4clean children. One
owned gateway503/no retry>=11s is not Temporal acceptance. API/PIDs/listeners/PG/
exact roots cleaned; both parents empty, real index empty,38GiBfree after auto.
295 Node report tests pass/zero skips. Exact artifact hashes and failed test-contract
runs are in COMMAND-PHASE-638-PREPARED-PROTOCOL-NEGATIVES.md and FIX836/FIX837.
Launcher self-test92494 terminal0 passed, including intentional cleanup-refusal
negative cases and residue checks. No runtime remains live. Existing Asana evidence
and protocol cards remain incomplete; latest project status1219999301138331 is
at_risk: Both prepared safety matrices passed — release and cost proof still open.

The matrix and earlier prepared recovery/retry are bounded local evidence only.
Cross-account ordinary admission, readiness/policy-head rotation, session reuse,
key retention/rotation, concurrency, unsafe/unknown/trusted-submitted recovery,
ordinary rotated-lease PhaseB, full Jobs/server/Clippy/native-cloud/provider receipts,
measured cost/latency and release gates remain open. No production/price/flag,
employer/email/browser/Temporal/owner DB/overlay/Pinky action. Overall goal ACTIVE.
FIX831 preservation33250 below is qualified at d66, not the later e959 bytes;
include prepared-repeat preservation in the eventual exact-source regression gate.

Cost audit: existing model-attempt holds/settlement, usage and spend caps are not
full per-submission COGS. Missing billed browser/runner active-idle evidence, precise
cost/rate/invoice provenance and durable application attribution beyond hold
retention block a proven sub-cent cost or six-month report. Next smallest cost step
is a read-only, verified-receipt sample joining ALL exposed attempts; unknown runner
cost must stay unverified, not zero. Existing Asana board owns delivery tracking.
Public Tsenta pricing was rechecked for the owner's latest cost request: monthly
full-use USD prices remain0.031667/0.026/0.022, not a proven0.05 internal cost.
Benchmark completed, receipt-verified applications at equivalent quality and full
cost, not packets/attempts. Do not change prices or claim an advantage before proof.

Negative review-first40249 TERMINAL1 at wrong_run: exact409/unavailable and real
claim lease_unavailable instead of404/request_failed. Frozen workflow run binding
is checked before target lookup. FIX837 changes only catalog/test expectation,
not production; full remaining-case source review and fresh actual runs required.
774source10e54a5d4efe4649378ab47b083c9e51ea5cfb713c878064e21cb4ef0ba7050c
matched beforeafter; API/PG/root cleanup, parentempty. No runtime remains live.

Negative review-first55887 TERMINAL1 at consumedproofreplay: actual409/unavailable,
expected401/volumeinvalid. FIX836 corrects only the test contract after source
confirmation of transactional consume Unauthorized→leaseConflict→HTTP409 mapping.
No production change. Exact774-source59b955b1434baf9739f97b137306e43a09d5535b947ddc36d92590758eb237de
matched beforeafter; API/PG/root cleanup, parentempty37GiB. No test runtime live.
No matrix pass: after-snapshot/tail not reached. Re-review/privacy and fresh same
private review-first retry, then separate auto required. Current294 Node report
tests pass/zero skips, including explicit rejection of the old401 expectation.

Current-source FIX831 retry preservation33250 TERMINAL0/build1m24s passed on
the same774-source manifest
d66a712f17e2cbced73fc1b2006836e771cc8abe7c5e5c2db2760861d932cb5d.
Ordinary auto firstclaim200/repeat200/newtoken+higherfence, complete frozenATS
unchanged, staleheartbeat+safe-finish409/20familiesunchanged, retainedheartbeat200,
release204/freshterminalreplay204/zeroeffects and4children/API/PG/rootcleanup
passed; parentempty/38GiBfree,HEAD/status/source matched beforeafter. API SHA
3765dfc6bb40590a8977bf3d3b385d20118c3e67de634b709b9a74a03dacb243;
fixture SHA113dbedc181dfbda96818b5e0bbcd5e3389c34b7fc2ee284489889f9a7361314.

Next bounded implementation: explicit dual-mode prepared protocol negative
matrix,24 exact cases,20-family equality for every denial, real client plus
closed private adapter faults, retained heartbeat/release/freshterminalreplay.
Cross-account ordinary admission/readiness/head rotation are explicitly deferred,
not qualified by an earlier middleware/proof rejection. New Node/adapter changes
are UNQUALIFIED pending independent review, prospective privacy, and separate
actual private-PG runs. Prior d66 runtime passes do not qualify changed source.

LatestFIX832/FIX835 prepared recovery PASSED in BOTH modes: review-first86077
TERMINAL0/build1m27s and auto20772 TERMINAL0/build1m24s on exact same774-source
d66a712f17e2cbced73fc1b2006836e771cc8abe7c5e5c2db2760861d932cb5d.
Both real-client mode-bound reports prove ordinaryclaim/observedheartbeat,
typed READONLY before/after/exact-replay invariance, actualrecover200/freshrecover
replay200/20familiesunchanged onreplay,16stablefamilies/sevenimmutables/stablelease
andattempt, intendedfailedapplication/session+releasedlease/attempt, terminalfinish
204/freshHTTPfinishreplay204 andzeroeffects. Reviewfirst6children/zeroATS; auto
7children/ordinaryTrackconsent+wholeATSrowunchanged. Both API/PG/rootclean,
parentempty/38GiBfree, realindexempty, HEAD/status/source matchedbeforeafter.
Final launcher self-test3886 TERMINAL0 passed including deliberate cleanup-refusal
negatives and residue checks. No test runtime remains live; all workers completed.
187 Node report/source tests pass. Prior23focused tests passed source0b032 before
onePGBoolean expression correction; parser17989 and actualdual-modePG qualify
that correction, not full current-source suite. FIX832/FIX835/IMPL/REVIEW have
exactartifacts. Overall goalACTIVE; no Phase638/source/release acceptance.
Next: expandedclaim/later-op
negative/sessionreuse/headrotation/keyretention/concurrency, unsafe/unknown and
trusted-submitted actualPG parity, fullserver/Jobs/Clippy/nativecloud/provider/
receipt/economics/release gates. No production/employer/email/flag/Pinky action.

Owner-facing Asana recovery/protocol/evidence cards were updated with these exact
bounded results and remain incomplete. Latest status1219998633756669 isat_risk,
not source/release acceptance. Existing project remains the only delivery board.

Retry77890 TERMINAL1: all23 focused tests PASSED (13pure/5SQLite/3fixture/2example,
zeroignored); actual PG claim/heartbeat/before-inspector passed, but recovery500/
18ms with42883/operator_type. No panic/blockers; API/PG/root cleaned,parentempty/
38GiBfree. Source7740b03233dbbbecaa986fca2ee23877742a8a64e250ae3380a9aa6bb1477975ba7
and HEAD/status matched. No test process remains live. FIX835 proves exact newPG
marker=1 vs migratedBOOLEAN cause: source-derived private prepare17989 confirmed
original42883/correctedIS TRUE prepare +3Boolean cases. Parent changed ONLYPG
expression, SQLite unchanged;187 Node tests pass. Independent review/prospective
privacy then BUILD-ONLY fresh-PG real-client recovery rerun required in both modes.
Do not redo23focused passes solely to rediscover this parser failure; full current-
source gates still required eventually. FIX835 records artifacts/probe limitations.

FIX834 statically cleared; retry77890 is currently RUNNING on exact774-source
0b03233dbbbecaa986fca2ee23877742a8a64e250ae3380a9aa6bb1477975ba7,
HEAD40949858/status6754505e... unchanged at dispatch. Prospective privacy2928/2652
passed tree4c0bc054e820feff76799b5fea0aae3bc073803c. Use the existing live handle;
do not restart on observation timeout or infer results. Parent owns runtime.
All five backend/inspector Rust files plus three Node files are source-frozen and
reviewed (mode false-green FIX833 closed,186 Node tests pass). Actual corrected
recovery/dual-mode acceptance is NOT yet established. Wait for13pure/5SQLite/
3fixture/2example/API results and cleanup, then compare source/HEAD/status.

Latest corrected-recovery qualification6759 TERMINAL101: build2m55s, first13pure
filter9passed/4failed/zeroignored, all sharedcfgtest historical_fixture request-ID
setupConflict atline894. No DB/fixture/example/API gate reached. PG/root cleanup
passed,parentempty/38GiBfree;774sourcef7226ec3ea1ef4c7d2c5778449e465f26fa5b3e33d508623943daa4193d07027
and HEAD/status matched. No test process live. FIX834 now fixes ONLY fixture START
result_request_id timing; RESUME consistency explicitly asserted. Corrected file
SHAb6852a50a42e728da11752786be221a7aa0b2d4b8d4725bb0233be150829a423.
Independent review/privacy freeze then rerun same bounded command required. Do not
infer a production correction qualification from9passes; all later gates remain.

FIX832 local implementation is now assigned/in progress, not qualified. Existing
fixture specialist owns execution_session_authority.rs, execution_leases.rs and
execution_session_authority_tests.rs; parent owns Node/docs/all runtime. Existing
client specialist proposes a closed READ ONLY before/after/replay inspector in
runner_control_plane_fixture.rs + guarded example, NO edits until contract approval.
Sol reviews independently. No new model/runtime and no collisions are authorized.
Reviewed lock solution uses ONLY plain immutable historical reads after the locked
lease/account fence; no command/binding locks are added. Complete metadata uses
exact retained START/request/canonical lease authority; partial denies; all-null
legacy uses existing unmanaged classifier. Trusted submitted replay precedes any
history/session dependency after exact retained checks. Source freeze/green recovery
and complete negative matrix remain pending; do not rerun before both review and
privacy freeze. Last tests29416/63105 are terminal0, launcher78445 self-test terminal0,
no test process live; current worker work must be awaited/reviewed before handoff.

Latest29416 TERMINAL0 confirmed checkpoint-session causal404 after ordinary
review-first claim200/observedheartbeat/exactv2preparedoriginalcredentials.20families
unchanged; retained original heartbeat200/saferelease204/freshterminalreplay204,
zeroATS/effect/capacity/canary/receipt and3child/listener/PG/root cleanup passed.
Parentempty/38GiBfree.156 Node tests pass; no test process remains live. Source774
79f252d90da5b58ea027f379d4727941ad2284f0b6ea7e7e54778f99fd042463 andHEAD/status
matched; privacy2925/2649 passed. FIX832/COMMANDCHECKPOINTRED hold evidence. This
is an availability defect, NOT fixed recovery. Independent design review compares
exact retained START/request/lease authority with explicit legacy compatibility;
no production recovery patch yet. Never use active current START/head or guessed
session fallback for qualified managed leases.

Prior current-source review-first preservation63105 TERMINAL0 passed at a970source
(same as auto-green55819), build1m34s,3children/listeners/PG/rootclean/38GiBfree,
HEAD/status/source unchanged. API/fixture hashes in IMPL638. Node-only causal-case
changes followed; production Rust remains the qualified55819 source so far.

Latest55819 TERMINAL0 qualifies the bounded same-owner live prepared auto retry.
Four exact Rust regressions passed (2new/2extended), zero ignored; compile2m59s,
API/fixture build2.78s. Ordinary auto packet/Track consent/first managed claim200,
exact production-decoded ATS/lease, observed heartbeat, fresh readiness and fresh-
proof repeat200/new token/higher fence passed. Complete ATS row bytes/deadline
stayed unchanged. Fresh old-token heartbeat+safe finish409 each left20 families
unchanged; current heartbeat200, release204 and fresh terminal replay204 passed.
Exact lease/volume/two-runtime generations, immutable/attempt/subject and zero
effect/capacity/canary/receipt checks passed;4children/listeners/PG/root cleaned,
parentempty/38GiBfree, realindexempty. No process remains live.774-source manifest
a970b9c46405079358550f3363d8568f9e3fc0df62b67a2198efb0840b8801fe
and HEAD/scopedstatus matched before/after.133 Node tests and launcher24516 pass;
prospective privacy2923tracked/2647text passed tree1c90d3ea9a1281243f13bbfc1240d8d880a293b2.
FIX831/COMMANDGREEN/IMPL638 hold artifact identities and exact limits. This is
NOT full source/release, expired/takeover renewal, ordinary API PhaseB/employer,
native/cloud/runtime, cost or overall goal acceptance. Same-source review-first,
remaining negative/policy/session/recovery matrix and full gates remain required.

80446 exited0: ordinary queue and real managed client completed review-first
claim200, observed heartbeat, safe release204 and fresh signed terminal replay204.
Exact lease/volume/process/noalias/ATS-zero counts, immutable/effect readback and
cleanup passed. Source774 manifest6b6fccb221db48534ee54dea6632666f9d4a83732805481f8b21c4e61ea70c6b
and HEAD/status matched. API/fixture build1m32s; sourceAcceptedfalse intentionally.
14 focused Rust tests passed89311, including actualPG finalizer; shared prefixed
proof-ID generator passed2 selected TS tests, runner typecheck and1 native256-vector
test;65 Node tests passed that checkpoint; rootgone/parentempty/38GiBfree.
New auto-submit repeat-red76811 is TERMINAL0 and confirmed the intended defect.
102 Node tests,5 Rust module tests and7 example tests pass; first ordinary auto
claim200 had one production-decoded exact PhaseA bound to distinct START session.
Fresh readiness/exact fresh-proof second claim409 left20 protected families and
frozen ATS bytes unchanged. Original-token heartbeat200/release204/fresh terminal
replay204 passed; zero effects and4-child/listener/PG/root cleanup passed,
parentempty/38GiBfree. No process is live. Source774 manifest
085263571c034f05406bfb4dc28367e27e3722391ae83afa25a3511d3eeb3538
and HEAD/status matched; privacy2921tracked/2645text passed before this run.
Production ATS/lease behavior is unchanged; FIX831 is an observed contract red,
not a correction. Bounded read-only arena proposals now compare stable PhaseA
reuse versus generation-bound immutable renewal. Do not weaken nonce/expiry
equality or call this red green; preserve prepared token/fence rotation and PhaseB.
The subsequent bounded arena chose stable PhaseA reuse for only an exact same-owner
unexpired prepared managed-cloud retry; DECISION-PHASE-638-PREPARED-ATS-RETRY.md
records the rationale, limits and required tests. Rust specialist is implementing
two production files/inline tests; parent owns the three Node green-case files
(regression worker made no edits). Independent reviewer is checking Node first,
then Rust. No source freeze/runtime acceptance yet.133 Node tests pass; launcher
self-test24516 terminal0 passed including residue checks; no test process is live.
No takeover/expired renewal or generic/local replay relaxation is authorized.
Next: D0.3 remaining actual-PG session reuse/terminalization/policy rotation and
negative matrix; separate immutable recovery seam; full-source/native/cloud and
release/economics gates. Do not redo qualified causal/prereq/protocol runs. This
review-first pass is NOT auto-submit ATS/employer/native execution acceptance.

## Authority and limits

Checkout /Users/uno/Downloads/cue-jobs-source-verifier-release-v3-20260831.
Branch feat/phase-638-jobs-runner-control-plane; HEAD409498589b8d2e62b5ef54ebdee1372aeb8d005f.
No Phase638 commit/push yet; preserve owned dirty work and inherited untracked
REVIEW-PHASE-628-JOBS-SOURCE-VERIFIER-RELEASE-V3.md. Never edit/start meeting-owned
/Users/uno/Downloads/cue, desktop/Pinky. Current code/local handoffs authoritative;
SSD unused, only a specific missing historical fact could justify it.

No push/PR mutation/merge/rebase/retarget/deploy, signing/publication, paid resource,
production flag, owner account, competitor probing/private sessions, real employer
application/email/calendar, browser/Temporal/VM start. Only private synthetic
loopback Jobs/PG tests. Synthetic Linux/x64 descriptors are not native measurement.
Embedded runtime reporters stay0; new runner/admission scope staging/us-east-1/general.
The reused positive-source helper's internal synthetic production-scope rows are
explicitly private fixture data, not live authority. No safeguard bypass/evasion.

Owner2026-10-28 separately authorized organizing the existing Bluey Jobs Asana
project using the toolkit and Pinky's delivery format. Only those planning writes
were made; no production/execution authority expanded. Asana is now the owner-facing
Jobs delivery board, not a second execution engine or deployment approval.

Project:https://app.asana.com/1/1205769749443087/project/1219992838857989
48 release-area tasks;9 numbered release stages plus Intake;32 original dependency
updates plus6 updates for LinkedIn/AI-routing/autonomy scope. No fabricated date,
assignee or completed feature. Current status at_risk; project overview and evidence
ledger explicitly say NOT production-ready. Pinky project metadata was readable,
but task export was forbidden; no bypass or Pinky task/code mutation occurred.
Toolkit was already installed and used; no installer, hook or new runtime was run.
Key cards: program1219993222870748; evidence1219993227698601;
PG5001219993283753688; protocol1219993182492842; recovery1219993283765686;
release1219993119960845. Keep task details/status evidence in Asana, code facts in
canonical IMPL/REVIEW; do not create a competing HQ/Beads board.

## Read first

bluey-ops; AGENTS.md; Round638; IMPL638; REVIEW638; FIX819/FIX821/FIX822.
Do not redo qualified Phase637. Its source099f3b97 and records40949858 remain the
base, not qualification for current dirty source. Full current-source gates still
remain. Latest focused run23220 is terminal101: private PG17.10/vector0.8.3 and
client preflight passed, compilation2m33s,8 pure tests passed/3 SQLite setup tests
failed beforeadmission. FIX823 corrects only undersized synthetic run IDs; no
production relaxation. Actual managed fixture/client tests were not reached.
Exact root removed/parent empty/38GiB free, source774 manifest/HEAD/status matched.
No runtime process remains live; repeat only after review/privacy gates.

Continuation checkpoint: reviewed FIX823 rerun1656 is terminal101.2m32s compile,
9 passed/2 failed/zero skips in10.01s. Both started cases used unmanaged admission,
so production dispatch correctly returnedNone. FIX824 now composes existing signed
staging runtimes, public inventory/cutover, managed admission/binding and real
managed request-start; new tests are serial with scoped env reset. Only test-helper
visibility changed in tests.rs/runner_volume_purge.rs; production is untouched.
Private PG exited/root gone/parent empty,38GiB free. Prospective privacy ab7284
passed2912 tracked/2636 text before that run; source774 manifest was
21265467cef51431249083d335fd456e00b17c703422ca23a71f062cb6516b35,
HEAD/scoped status matched before/after. No actual positive resolver/client result
is inferred. No live process remains. Repeat after FIX824 review/privacy freeze.

Latest runtime:84098 terminal1 passed all13 Rust tests (8 pure,3 SQLite,2 existing
managed authority); actual ordinary API/PG claim returned500 in1283ms. Diagnostic
rerun82341 terminal1 built API/fixture in1m52s and reproduced500 in1439ms; PG log
detected an error but its closed category wasother and SQLSTATE unavailable.
18 sampled PG clients wereidle/no blockers; no panic observed. Root cause and
positive prepared lifecycle remain unproven. Both API/PG runs cleaned their exact
roots; parents empty/38GiBfree. NO runtime process remains live.

Latest774-source manifestcb7cb3753e0e5e9e845ebad628403d6d8e34b04d2a778dde3dc74d41c4e2eeb3
and HEAD/scoped status matched before/after82341. API SHA
93e83177cac7e30fd2fbb5664094f2f6c5ea57a2e59f7a7165c4c4c1f1921a9b;
fixture SHA3c1786e0755fe7035bd90e17564b719c0e7c9c35cd43638254ab5d4ba9b8db3a.
FIX825 diagnostic code has57 passing Node tests. Complete prospective privacy
d28663 passed2914 tracked/2638 text before82341, real index unchanged.
Private PG17.10 parser86871 passed both FOR SHARE/LIMIT orders2/2 and cleaned:
the review's clause-order candidate is ruled OUT; do not alter valid SQL.
Next bounded action: capture explicit PG SQLSTATE/error leaf safely within the
physically verified private cluster before cleanup (e.g. reviewed verbose-error
metadata for only this temporary cluster), then fix only the observed cause.
Never print native SQL/error/values or infer syntax from500 alone. Repeat the
build-only diagnostic recipe before broader full gates; no passed mode needs
replaying merely to obtain a missing error category.

New latest diagnostic45305 is terminal1 (build1m40s; actualclaim500/1444ms).
Private role-scoped verbose metadata now explicitly reports SQLSTATE42804,
datatype mismatch/categoryother. No APIpanic or sampled blockers. APIcleanuptrue,
PGexited; no test process remains live. Source774 manifest
34dee63f3ce92a48fba349ae1beaaadaf28aa276b0ad8fb63b044863d21117eb
and HEAD/status matched before/after. Artifact/log hashes are in FIX825. Next
identify the exact statement/leaf; SQLSTATE alone is insufficient for a fix.

Subsequent source review found finalizer runner_volume_purge.rs INSERTFALSE into
migration024 SMALLINT legacy_unresolved. Prepare-only privatePG probe31410 exit0
confirmed exact original42804 and corrected0::SMALLINT success using only temporary
tables/rollback; PG/root cleanup passed and source774 manifest
bfa6c5dbcd22a4f90470c0c00d3ec2806276dee30fcf061828f2398f32494153 matched.
Parent changed only this literal. FIX826 records cause/evidence. Terra owns only
the inline cfg(test) configuredPG finalizer regression; parent owns all runtime.
Actual migrated finalizer and ordinary prepared client success are still pending.

New latest:89311 focused14/14 PASS, zero skips, actualPG finalizer4.34s, build2m45s.
PG/rootclean and source774 manifest0f33d8b9ea6a71d01613c969ed7d5d1fc961ef0da956f9231287872f1569bdf3
matchedbeforeafter. FIX826 narrow regression is qualified. API61106 is terminal1:
build1m23s, claim400/2ms, no observedPGerror/panic/blockers; cleanuptrue/PGexited,
rootgone/parentempty and same source. No process remains live. FIX826 has hashes.
FIX827 adds only closed public-error category/hash to the private adapter,58 Node
tests pass; review/privacy/runtime capture still pending.
FIX828 is a separate source-proven1/32 random proof-ID grammar defect, not a proven
attribution of61106. Terra owns shared prefixed TS generator/test, fixture reuse,
and pure Rust validator test. Parent owns runtime/docs. Do not relax Rust grammar
or claim the ordinary lifecycle passed. Final source/release gates remain open.

Newer latest69688 is terminal1: native request-ID validator256vectors PASS1/1;
Rustbuild2m38s, APIbuild2.84s, actualmanagedclaim200/1291ms. Then harness combined
exact_prepared_bindings assertion failed; individual counts were not retained.
Source774 manifest9e928d8c3a34d16bd78d5740254d1d99869735ceccec98bd1c94e1e72f00e4e8
matchedbeforeafter; API/PG/rootclean, no live process. TS59716 separatelypassed2
selected generator tests plusrunner tsc(noEmit),32 unrelated filteredout, rootclean.
FIX829 corrects a source-confirmed harness policy mismatch (review_first packet
but auto_submit-only ATS binding expected). Explicit persisted mode and exact
lease/volume/process/ATS/alias counts now reviewed; ATS counts account/application/
run allzero withoutfilters.64 Node tests pass. Do not seedATS, switchmode orchange
production predicates. Build-only private API rerun and auto-submit ATS/fullgates
remain pending. FIX829 holds hashes; original61106 attribution remains unproven.

Latest65097 terminal1: claim200/1256ms after1m29s build; new SQL mode-readbackcast
failed22P02 because application_json is encrypted TEXT. FIX830 uses existing
READ ONLY post_queue/productionparse_application_json to return only observed
review_first mode, removesJSONBcast and checks immediatepostclaim immutable
snapshot equality (includingapplication) plus existingfinalcheck.65 Node tests
pass. Source774 a7a3035d746dc2f7c473d494a8f97ff81a40b22ae018b7751293369d4abd08b3
matchedbeforeafter; API/PG/rootclean/no liveprocess; hashesFIX830. No production
or crypto change. Obtain implementationreview+privacyfreeze, then build-only
API rerun. Review-first lifecycle is not auto-submit ATS/native/cloud acceptance.

Superseding runtime80446 PASS: see Current checkpoint/FIX830. Source/cleanup
verified before documentation updates; no model, host, flag, production pricing,
real application/email, owner account or SSD action occurred. Overall goal active.

Owner's Tsenta cost request is recorded in existing Asana cost1219993283940797.
Primary https://tsenta.com/pricing checked2026-10-28 advertises monthly USD19/600,
39/1500,99/4500 (full-use price0.031667/0.026/0.022, not operating cost).
Compare separate customer price, measured variable cost and fully allocated cost,
including failed attempts/retries/idle overhead, per unique verified submission.
No claim of beating Tsenta, Bluey pricing change or production action follows.

## What actually passed

- Latest57 Node report/cleanup/privacy diagnostics tests, zero skips; earlier32
  test checkpoint589971 remains historical, not the current total.
- Latest focused84098:13 Rust passes (8 pure,3 SQLite lifecycle,2 existing managed
  authority). Subsequent actual PG claim500 means positive lifecycle is unproven.
- Launcher self-test43481 exit0 including deliberate active-group/cleanup-refusal
  negatives and residue checks. All actual run roots/parents were separately clean.
- Source-derived pre-fix PG cutover query returned42601(parser62355); corrected
  query passed8 predicate cases90815 in private temporary tables. Both cleaned.
- Focused Rust/actual-PG37153 exit0:9 passes, zero skipped. Actual migrated INT4
  cloud entitlement decoder (0/1/2/-1, BOOL/INT8/missing-column Err/no unwind),
  actual cutover lifecycle/replays/genuine stale-successor immutability,2 fixture
  model/schema tests,4 example guards/redaction,1 SQLite cutover parity.
- Ordinary real-client causal79181 exit0: real profile/source/Track/factual packet/
  approval/cloudqueue+replays; strict read-only exact postqueue; third replay fresh
  readiness; exactly1 signed real-client claim returned exact target404. Distinct
  run/session IDs/no alias,14 protected DB families unchanged (worker-auth nonce
  reservation intentionally excluded),1 owned-loopback503/no retry>=11s, no
  Temporal acceptance/effect,3 clean child exits/PIDs absent/listeners refused.
  Private report validated then removed by canonical cleanup; unprinted exact
  elapsed/PID values must not be invented. PG/root cleanup verified,38GiB free.

Historical pre-fix772-source manifest8758dc34013ed91b323dc72c87bc8b4ba0370b47fe14d395455257455fa401d3
and HEAD/scoped status matched before/after focused37153 and ordinary79181.
API SHA9e8a9beb9169c3062aab8c7a5d0d744249fee734ab810b99ecc1e5eadcee505f;
fixture SHA08a8afa5e4a61b87d44f7b38b1760e7c5726441f9aadcb4885c5e910a7b89ee3.
These are local debug artifacts, not promoted/release/cloud images.

## Implemented prerequisite corrections

FIX819: remove only extra PG fleet-cutover query parenthesis; SQLite has an extra
scalar outer query and stays unchanged. Actual-PG lifecycle regression added.
FIX821: one private applications.rs postgres_cloud_browser_enabled(&Row) using
named-column try_get<i32>/nonzero. Replaces five erroneous Boolean decodes plus
normal reader centralization; queries/locks/order/optional-row false untouched.
All prior setup failures, the esbuild shutdown fix and interrupted12156 (no tests
counted) are recorded in IMPL/REVIEW; do not rewrite failures as passes.

## Current source correction — implemented, runtime qualification pending

Normal queue generates40-hex run ID and cloud-applicationID session. Command and
runner preserve both, but execution_leases.rs target lookup and ATS PhaseA use
run ID as session at the base. Independent review accepted causal79181. Current
dirty source implements the following reviewed correction:

1. Mandatory exact durable START resolution whenever outer
   ManagedExecutionLeaseClaimContext is Some, even if context.input is None.
   The sole production worker API always takes this path. Context-None low-level
   debug/test/library fixtures retain legacy behavior; do not use OR/current/latest
   session fallback within real worker admission.
2. Private SQLite/PG transaction resolver, exact account/application/run and one
   START (LIMIT2/require1), frozen payload and exact request START/RESUME chain
   when supplied. Reuse workflow_command_from_stored; do not open a pool-level
   materialization transaction inside the existing claim transaction.
3. Carry resolved ID into ExecutionTargetAuthority, payload validation and
   CloudAtsPhaseAContext instead of substituting run_id. Apply same managed-context
   resolution at first irreversible transition. Preserve lock ordering/rechecks.
4. Existing click_started stored-receipt replay must stay BEFORE current START
   lookup. Do not add current command/session/head dependencies to heartbeat,
   finish, submitted receipt replay or trusted checkpoint reconciliation.
5. Eight pure tests cover material/cardinality/START-RESUME/frozen-application
   invariants; three SQLite DB tests cover distinct ID, pre-start denial and
   started exact-scope rejection. Existing managed fixture retains aligned IDs
   but now uses its exact approved packet and canonical request/result builder,
   not placeholder material. The ordinary API/PG driver still creates distinct
   actual IDs with no alias. Runtime and wider session-reuse/negative/concurrency
   coverage remain pending.

New helper:server/src/db/jobs/execution_session_authority.rs. It repeats account/
cleanup fences; Some(input) calls existing managed authority FIRST preserving
request-to-START PG lock order, then requires exact-one active START and real
request-start evidence. A plain read under existing prelocks bridges workflow
identity/profile to the exact frozen application packet. Both later PG session
FOR UPDATE locks now also use execution_target.browser_session_id. Reviewer found
no remaining source blocker in these bytes; that is not runtime acceptance.

Prepared driver --prepared-lifecycle is implemented with signed real claim,
observed heartbeat, safe release, fresh separately signed finish replay, exact
volume/ATS session binding and observed immutable-state/effect checks.54 Node
report/cleanup/privacy tests pass, zero skipped. Complete scratch-index privacy
c90771 passed2911 tracked/2635 text; real index unchanged.774-source manifest
4520f7d4473dc933dc9017b27fe8fcae0502f1c68a87b00ef53ed63479037347,
HEAD/status captured before live23220; verify after terminal cleanup.

Separate open P1 recovery seam: checkpoint reconciliation still selects/validates
session.id=run_id. Do not claim full cloud lifecycle from prepared release. Its
eventual fix must use retained frozen session authority, never current START/head.

Closest existing materialization model: workflow_commands.rs
get_materializable_jobs_workflow_command_by_request_id states delivering/
delivery_unknown/accepted, real request-start event, cleanup/account fences.
JobsWorkflowStartMaterial contains browser_session_id. Managed execution-command
and start-for-resume helpers intentionally support retained replay without active
state restrictions; do not broaden/change those historical resolvers.

## Then prove positive protocol and full gates

Real client prepared claim, observed heartbeat same token/fence, safe release,
fresh signed terminal HTTP replay(new nonce, not cached finish promise), explicit
negative matrix. Prepared reclaims may rotate token/fence. Wrong worker is a CLAIM
binding; heartbeat/finish use signed transport+retained token/fence/volume, not a
stronger worker-ID contract. No irreversible employer action is authorized.

Use existing COMMAND638 recipes/shared mac-build queue/canonical launcher,
private HOME/Cargo/PG/cache/data/output and complete prospective-index privacy/
whitespace including new files before source freeze. Assert every prerequisite
exit before dispatch; never continue on a failed gate. Root owns all builds and
runtime, workers never start services/tests. Do not restart an unobserved live
handle. Full Jobs/server all-target/Clippy/source/cleanup gates remain pending.

## Team/tool state

Existing native fixture/regression specialists and Sol reviewer can be reused with
bounded briefs; all current assignments are complete, no model switch. Reviewer
recorded the frozen ATS recovery-session recommendation and the ruled-out SQL
clause-order hypothesis. No worker or test process is live. Parent owns docs,
integration and all runtime. Ruflo project
task task-1793204589019-9to79y is running, not overall goal acceptance. Code graph
refused this checkout's project-root guard; direct rg/source fallback is approved,
never reconfigure the guard or index meeting checkout. Do not start framework CLI/
hooks/daemon or a competing project board. Broader MCP/C2C/source/OAuth/model/
monthly-billing/Linux-OCI/browser/receipt/release work remains unfinished.
