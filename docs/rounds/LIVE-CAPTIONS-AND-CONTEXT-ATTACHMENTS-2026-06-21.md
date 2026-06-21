# Live Captions And Context Attachments - 2026-06-21

## Problem

Bluey visible testing showed two production-facing issues:

- Clicking Listen did not visibly produce captions, even though the UI moved into a listening state.
- Attached documents and images needed a cleaner contract: chips/removal in the overlay, bounded context for answers, no large raw document dumps in the conversation, and no stale converted document copies after removal.

## Changes

- Fixed macOS microphone continuous capture by keeping the `AVAudioEngine` owner task alive instead of relying on `dispatchMain()` from the helper worker path.
- Shared one authenticated cloud client across mic/system live STT relay startup, including an `/account/me` preflight before spawning source relays. This avoids parallel refresh-token races and makes auth failure visible before capture starts.
- Made native live audio helpers report an error if a source exits before producing any audio bytes.
- Improved the live caption strip so the source marker is stable (`MIC`, `SYSTEM`, etc.) and the scrolling text contains only the caption body.
- Allowed supported image files (`png`, `jpg`, `jpeg`, `gif`, `webp`) through the document picker and daemon attachment filter as vision context.
- Kept unsupported image formats out of picker/attach flow so they do not become unusable context.
- Capped provider image uploads to four server-supported data URLs and skipped oversized images with a prompt note instead of failing the whole answer.
- Bounded attached document previews in answer prompts. Full converted Markdown stays in Bluey's local context store/RAG path; the live prompt gets a compact preview plus relevant RAG snippets.
- Cleaned up converted local Markdown copies when an attachment is removed or its session is deleted.
- Rendered short context/system/warning confirmations as overlay toasts instead of feeding them into the main conversation.

## Verification

- Direct helper check before the fix: system audio produced bytes; microphone continuous capture produced zero bytes.
- Direct helper check after the fix:
  - microphone continuous capture produced `114916` bytes in four seconds.
  - system continuous capture produced `116480` bytes in four seconds.
- Local `bluey audio status` after Listen smoke:
  - provider: `bluey-managed:deepgram/nova-3 live`
  - sources: system and microphone both `Capturing`
  - transcript segments emitted rose from zero to `62+`
  - system chunks and microphone chunks both advanced.

## Known Follow-Ups

- The live caption data path is working locally, but the user should visually confirm the overlay caption strip after reinstalling the refreshed overlay binary.
- RAG embedding logs showed `/router/embed` failures separately from captions. That is a server/provider configuration path and should be tested after the latest server deploy.
- The attachment flow now supports image files only in provider-supported formats. HEIC/BMP/TIFF need conversion before they should be accepted.
