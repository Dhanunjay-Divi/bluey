# FIX883 — Explicit current-source qualification admission

Preflight `bluey-ops`; Round646. No production or image execution in this fix.

## Issue / root cause

Old Phase639 tooling embeds32ca96 as the image source, while requiring HEAD to
equal that literal throughout. Porting it onto the modern Jobs graph and then
committing changes makes a source literal naming its own final commit impossible.
Silently keeping the old commit would qualify obsolete package/runtime bytes.

## Fix summary / files

Require exactly one explicit nonzero lowercase40-hex `--source-commit` in every
executing mode. Reject omission, duplicate/alternate args, zero/malformed/newline
values or environment/HEAD fallback. Verify exact HEAD before setup and retain
all later source/export/cleanup checks. Workload, report, inspection and artifact
validators receive the independently captured expected commit, never a value
read back from the untrusted report being validated.

Changed harness/workload and tests; ported the reviewed D0/D1 helper closure onto
Phase645 without overwriting modern dependency/quarantine rules. The selected
Git-tree cardinality is exactly797, measured from current tracked+explicitly
owned new selected paths before commit. Preserve strict equality and reject the
old778 or adjacent counts; confirm the final committed tree before execution.

The argument is configuration, not authority. After commit, the private parent
controller must hard-pin that real SHA, complete source/status/index/tool/harness
closure, exact argv, reader approval file digest and helper/runtime bindings.
Existing portable-reader approval does not itself bind Git commit; the controller
must bind that exact approval to the selected source and helper. No old report
or approval is automatically promoted because a new parser accepts its shape.

## Edge cases / tests

Missing/duplicate/zero/uppercase/nonhex/truncated/LF/CRLF commit; HEAD mismatch
before setup and every existing recheck; report/inspection/archive-source drift,
old-source replay and mixed expected values. Two distinct valid synthetic commits
must not cross-accept. Positive and failed-retention artifacts keep distinct
prefixes, full source/tool binding and false production/startup authority.

## Verification / limits

Implementation/source review accepted; syntax, focused/full gate24772 exit0,
privacy and cleanup passed as recorded in IMPL646. Final committed797-tree and
image-controller pins remain pending. No VM, image, managed-image browser/native
startup, registry, per-application cost or production proof follows. Old82100
tar-x failure and incomplete retention evidence remain failed. Do not admit a
PAX subset without actual retained bytes and a separately reviewed change.
