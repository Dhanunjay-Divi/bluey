# Round 154 - STT 200 Percent Usage Pricing - 2026-06-23

## What changed

- Updated server STT pricing so Deepgram `nova-3` uses the current accuracy-first streaming rate of `$0.0092/min`.
- Changed managed STT markup from `150%` to `200%`, so the customer pays `3x` the provider cost for active STT usage.
- Applied the same `200%` STT markup to the OpenAI transcription fallback.
- Kept billing measured by actual elapsed seconds, with upfront reservation and unused-time refund on session close.

## Expected math

- One minute of Deepgram `nova-3` streaming:
  - Bluey provider cost: about `1` cent after whole-cent rounding.
  - Customer charge: `3` cents after whole-cent rounding.
- Two hours with mic plus system as separated sources:
  - Metered audio: `240` source-minutes.
  - Bluey provider cost: about `$2.21`.
  - Customer charge: about `$6.63`.

If Bluey later enables Deepgram Keyterm Prompting or another paid add-on in the relay URL, update `server/src/pricing/mod.rs` and this doc so the customer charge follows the actual enabled provider cost.

## Verification

- `server/src/pricing/mod.rs` has a test for the two-hour dual-source STT math.
- `server/src/api/stt.rs` has an exact one-minute customer estimate test.
- `server/src/db/stt_accounting.rs` reservation tests now use the new 10-minute reserve and refund values.
