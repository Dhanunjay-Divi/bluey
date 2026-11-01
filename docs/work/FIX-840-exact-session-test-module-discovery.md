# FIX-840: Exact session DB test filters used the filename instead of module name

Preflight: bluey-ops/current AGENTS/local handoff and independent native review.
Command-only correction; no product/test source change.

## Issue / root cause

Run62014 TERMINAL1. Format/strictbothfeaturealltargetClippy passed1m57s, testbuild
2m35s. First3 tests passed: original unmanaged red4.01s; actual authenticated
unmanaged claim/start/replay3.45s; managed claim/final-submit omission/wrong-worker
denial and legitimate start/replay5.53s. Each1passed/0failed/0ignored.

Fourth filter ran0 tests and the mandatory expected-count guard correctly failed
focused_test_count_mismatch. Remainingcase/preflights/PG/full were not reached.
Parent used execution_session_authority_tests as the module name, inferred from
the file name; jobs.rs2460 instead declares execution_session_authority_db_tests
for that path-loaded file. A different inline pure-test module has the former
name, which made filename-only review insufficient. Initial command review
missed this distinction and REVIEW638 records the oversight. No ignored or
zero-test invocation is accepted as a pass.

## Fix summary / files modified

- New COMMAND-PHASE-638-FIX840-RUST-GATES.md: correct both DB session-case prefixes
  to db::jobs::execution_session_authority_db_tests::. Keep --exact at execution.
- First discover all5 exact upfront cases on the actual same-feature/crypto test
  binary using --list. Require each returned set to equal its one full filter and
  all5 to be unique before ANY upfront case executes. Reuse compilation within
  this single isolated target; do not keep an old target after cleanup.
- New private launcher parent/event label only; all other focused/PG/full/count/
  lint/source/HEAD/status/isolation/cleanup gates remain. No source patch or weaker
  assertion, guard or test-selection exclusion.

## Edge cases / how to test

Missing, stale, overlapping and wrongly qualified filters must fail discovery,
not produce successful zero-test coverage. Runtime must observe5 focused passes,
all43 genuine fresh-DB PG cases,30retainedfocused and mandatory full alltargets.
Run the new exact command only after independent review and prospective privacy.

## Failed-run evidence and cleanup

Run62014 admitted774-source d5619475934fec80eda3dfc5b5d4dbfcd2649b768f355958916f66b94d9c1a46,
HEAD409498589b8d2e62b5ef54ebdee1372aeb8d005f/status6754505e. Parent final rehash
BEFORE dispatching edits matched EXACT source/HEAD/status. Exact root
run.1793233031.25828.25451104120 absent, parent/index empty,37GiBfree; no PG started.
No runtime remains live from62014. The3 bounded tests do not qualify the full gate.

## Known limitations

Full corrected-source qualification and browser/native/cloud/provider/OAuth/
cost/latency/release gates remain open. No production or real external effect.
