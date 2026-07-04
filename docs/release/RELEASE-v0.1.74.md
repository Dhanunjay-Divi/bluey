# Bluey 0.1.74

Live STT cleanup for coding/interview mishears.

## Changes

- Added deterministic live STT cleanup before transcript text is shown, saved, or sent into the answer pipeline.
- Repairs obvious coding/interview mishears such as `given acetone two numbers` to `given a set of two numbers`.
- Keeps unrelated phrases such as `acetone bottle` unchanged.
- Adds metadata-only cleanup logs without storing transcript text.
- Expanded managed Deepgram coding keyterms and raised the keyterm cap to `64`.

## Verification

```bash
cargo test --manifest-path server/Cargo.toml stt::tests -- --nocapture
cargo test -p cue-daemon live_stt_cleanup -- --nocapture
cargo test -p cue-daemon live_stt_ -- --nocapture
```

## Deployment

- Desktop release `0.1.74` published to `https://bluey.sh/latest.json`.
- Server API deployed with source commit `13dc58b3129f13eb49a335427a3e6e3f08ab6929`.
- Public release verification and installer smoke passed.
