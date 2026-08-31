# FIX-716: Original-Source Provider Protocols Could Diverge Across Acquisition And Verification

> **Codex preflight:** Load `$bluey-ops` before diagnosis, implementation, or review and reconcile
> this record with Round 614 and the final branch diff.

**Status:** Implemented; focused provider, format, typecheck, and lifecycle checkpoints green;
full Rust and external aggregates remain conditional

## Issue

Discovery, Rust assignment validation, and the TypeScript verifier did not initially share one
exact provider URL and normalization contract. Valid Lever application-form URLs and standard
slugged SmartRecruiters URLs could be imported but later fail publication. Loose HTTP success,
redirect, destination, parser, or address handling could also let one provider family produce
different identity evidence from another.

## Root Cause

The public-ATS acquisition normalizers predated the managed verifier and provider variants were
reimplemented at several boundaries. The initial verifier contract did not bind the observed
canonical application URL/domain in storage and did not exhaustively exclude IPv4/IPv6
special-purpose resolution ranges.

## Fix Summary

- Freeze the supported set to Greenhouse, Lever, Ashby, SmartRecruiters, and Workday.
- Share discovery normalization where semantics must be identical and require exact host, tenant,
  path, record, variant, canonical URL, application URL, and application-domain agreement.
- Accept both canonical Lever posting and application variants without broadening path grammar.
- Accept a SmartRecruiters title slug only when its exact posting-ID prefix is valid, then project
  the stable tenant/posting identity.
- Require exact 2xx JSON responses, reject redirects, omit credentials/cookies, pin DNS results to
  the TLS connection with hostname verification, and reject non-public IPv4/IPv6 ranges.
- Accept only exact `application/json` with optional UTF-8 charset and absent/`identity`
  `Content-Encoding`; reject hostile substring/JSONP media types, other parameters, and overlong
  headers while digest-binding encoding and bounded/over-limit header fingerprints.
- Bound attempts, DNS results, time, response bytes, JSON depth/nodes/arrays/fields/strings, and
  observed text before parsing or receipt publication.
- Add exact positive and adversarial fixtures for every provider family and variant.

## Review Correction

Independent review found that ordinary `JSON.parse` overwrites duplicate object members. A hostile
response containing duplicate or conflicting keys could therefore hide one value before semantic
validation. The verifier now requires a bounded raw byte stream, decodes UTF-8 fatally, hashes the
exact octets, and pre-scans object members before ordinary parsing. It rejects duplicate decoded
keys across all five provider families, including escaped-equivalent and nested duplicates. It also
rejects 14 contradictory alias combinations across the five families while accepting three typed
equivalent-alias fixtures. Distinct malformed byte sequences retain distinct evidence digests.
The shared TypeScript/Rust observation vector is repinned from the exact raw-body/header fixture.
Another review found that recognized-but-malformed aliases and nested records could collapse to
absent. The parser now requires exact scalar IDs/provider fields, nested records,
workplace/timestamps, Lever list and salary shapes, SmartRecruiters job-ad sections, and Ashby
`isListed`. Seventeen adversarial cases span all five providers.
Streamed over-limit failures now hash the exact bounded `max+1` prefix, and the oversized fixture
proves two distinct bodies produce distinct content digests. A declared oversized response remains
bound by its exact header evidence.

## Files Modified

| File                                                                | Change                                                                                                                                                                                |
| ------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `jobs/automation/src/original-source-verification.ts`               | Closed provider transport, exact media/encoding/header semantics, raw-byte/UTF-8/duplicate-key bounds, strict recognized-field shapes, alias reconciliation, and observation evidence |
| `jobs/automation/tests/original-source-verification.test.ts`        | Five-family positive/adversarial, raw duplicate, malformed-byte, media/header digest, alias, and recognized-shape fixtures                                                            |
| `jobs/automation/src/public-ats.ts`                                 | Shared Lever and SmartRecruiters normalization parity                                                                                                                                 |
| `jobs/workflows/tests/original-source-verification-runtime.test.ts` | Replace text-only pseudo-response with a real `Response` carrying the exact byte stream required by the hardened verifier                                                             |
| `server/src/db/jobs/original_source_verification.rs`                | Server-side provider subject, destination, and observation/result tuple validation                                                                                                    |
| Paired Phase 614 migrations                                         | Persist exact canonical application URL/domain and closed receipt vocabulary                                                                                                          |

## Verification

Observed after the review correction:

```text
Provider verifier fixture/adversarial matrix       27 / 27 (1 file)
Automation TypeScript typecheck                    passed
Provider source/test Prettier                      passed with 3.6.2
Scoped diff check                                  passed
Rust original-source authority suite              25 / 25; normal-parallel twice
Workflows workspace                               300 / 300
Full Jobs final-parser aggregate                  1,884 passed / 1 skipped;
                                                    146 files / 1 skipped file
```

The accepted TypeScript source SHA-256 is
`934b176fd06c2bb7782c63fa9f0d6c53e42503c013af4cc9c5e4152442c1a165`; the accepted test SHA-256 is
`d2cce7d88f3675c009e9853ac201a69a77013d3f1bac767db49d06ec8895a3fe`.

The 27-test matrix covers all five families across positive, closed `404`/`410`, identity and
destination mismatch, redirect, malformed response, auth, CAPTCHA, rate limit, timeout, URL
userinfo, private-address/DNS rejection, duplicate raw members, malformed UTF-8, absent raw bodies,
exact-octet digest distinction, conflicting/equivalent aliases, exact JSON media/encoding, hostile
media parameters/JSONP, and bounded/over-limit header fingerprints. The additional 17-case
recognized-shape matrix rejects malformed present values rather than treating them as absent. The
clean Jobs aggregate rebuilt automation first and passed 1,884 plus one explicit skip. No live or
authenticated provider action is evidence for this fix.

## Limits

The verifier performs anonymous semantic reads only. It does not scrape private sessions, solve
challenges, write forms, contact employers, or certify provider legal/operational rollout.
