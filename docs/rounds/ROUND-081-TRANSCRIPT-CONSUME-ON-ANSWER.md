# Round 081 - Transcript Consume On Answer - 2026-06-20

## Goal

When the user presses Enter or Answer, Bluey should send the current typed text plus the live transcript preview available at that moment. That same preview text should not be reused by the next Answer press unless new transcript arrives.

## Change

- The macOS overlay now drains the live transcript answer buffer after a successful Answer submit.
- It records normalized fingerprints for the consumed transcript snippets.
- Late matching partial/final STT events from the same utterance are suppressed from the next answer buffer so Deepgram finalization does not re-queue the same sentence.
- The saved session transcript is not deleted. It remains available for session history, support/debugging, sync, and RAG memory.

## User Flow

1. User clicks Listen.
2. Mic/system captions appear in the live preview strip.
3. User presses Enter or Answer.
4. Bluey sends typed text plus current caption context.
5. The caption buffer is cleared for the next answer.
6. Later captions must be new audio before they are included in another answer.

## Verification

- `swift build -c release --package-path native/macos/cue-overlay`

## Remaining Risk

This is an overlay-side consumption guard. The full session transcript still stores the original audio turns, so older context can still be found through saved-session/RAG retrieval when it is genuinely relevant.
