# Round 393 - Landing Hero Copy Tighten

## Goal

Tune the Bluey landing copy around "Stay present, stay unseen" with a clearer, more premium description of what Bluey helps with.

## Changes

- Updated the hero lede to cover meetings, calls, code, docs, and screen context from the terminal.
- Replaced the proof line with a shorter context-and-answer statement.
- Updated meta, Open Graph, Twitter, and JSON-LD descriptions so shared previews match the new positioning.

## Notes

- Kept "interviews" out of the public landing wording because paired with "stay unseen" it can read like covert third-party assessment help.

## Verification

- `git diff --check -- web/index.html docs/rounds/ROUND-393-LANDING-HERO-COPY-TIGHTEN.md`
- `awk '/[ \t]$/{print FILENAME ":" FNR ": trailing whitespace"; bad=1} END{exit bad}' docs/rounds/ROUND-393-LANDING-HERO-COPY-TIGHTEN.md`
