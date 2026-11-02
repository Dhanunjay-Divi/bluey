# FIX869 — Match startup preflight to ordinary grant reference grammar

## Issue / observed failure

Phase641 gate32702 terminated1 during six focused library tests:5 passed,
the three-grant unit case failed with `invalid managed cloud request`. Example
tests and the full server suite were not reached. Launcher self-test, formatting
and strict both-feature all-target Clippy passed; final source equality and all
owned process/root/parent cleanup passed. Journal SHA:
`fdc5fbcb16a6ff7f63459cab90cb5ed2031371aa001e87404a2004b457670511`.
The failed attempt remains failed.

## Root cause

The ordinary managed grant API requires both issuance and authorization
references to satisfy `managed_cloud_route_id`:20–128 ASCII alphanumeric, hyphen
or underscore characters. The new fixture header instead accepted the weaker
general token grammar, including an18-byte synthetic authorization reference.
That allowed invalid grant input past preflight and could leave earlier normal
authority imports committed before the later ordinary API refused it.

## Fix / files

`server/src/db/jobs/runner_control_plane_fixture.rs` now checks both fields with
the same ordinary route-ID validator before any import. The unit reference is a
valid28-byte explicit synthetic label. Existing boundary tests add20/128-length
positives and19/129-length, colon and dot negatives for both fields. No production
validator, privilege, TTL, worker identity, signature or state guard is loosened.

## Verification / limits

Independent static review confirms the narrow correction and no loosening.
Post-fix18348 terminated0: launcher self-test, formatting, strict both-feature
all-target Clippy, all6 library+3 example cases and full all-target1942 tests
passed/zero failures/ignored. Source778/23f93 and empty index unchanged; owned
root/parent/process cleanup verified and shared lock released. Journal SHA256
`2ba2b174d9de0493dcc0d53d46f21a29bb64ba32f331b26cb9f8239e3bae8238`.
Focused repetitions are included in the full count, not extra unique cases.

Late DB/clock failures can still preserve partial private state; retries remain
refused. This is not an atomic-import/rollback claim. Actual D1/package/normal
startup, saved-account, cost/latency and release evidence remain pending.
