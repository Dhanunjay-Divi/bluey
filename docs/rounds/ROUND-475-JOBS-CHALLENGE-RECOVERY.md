# Round 475: Jobs Challenge Recovery

Date: 2026-07-10

## Goal

Reduce avoidable application interruptions without weakening account-owner checks. CAPTCHA and email verification are separate flows: CAPTCHA stays a browser takeover, while a matching email code from a connected inbox can be approved with one click and used only for the active application.

## Challenge Policy

- CAPTCHA preserves the exact browser page for user takeover, then resumes the same run.
- Email OTP may use one-click approval when a connected Gmail or Outlook inbox receives a matching, unexpired message.
- SMS, authenticator, and push verification remain account-owner takeover flows.
- Assessments remain user takeover flows without losing the application state.
- Unknown required questions and missing facts continue through answer memory so a confirmed answer can be reused.

## Email OTP Matching

The automation package now matches an email OTP only when all of these are true:

- the inbox is connected;
- the recipient belongs to that connected inbox;
- the provider is Gmail or Outlook;
- the message arrived after the active challenge began and is no more than ten minutes old;
- the message contains verification-code language;
- the sender domain or message content matches the active company/application;
- the code is still unexpired when the user approves it.

The raw code exists only in the worker's ephemeral candidate. The persisted intervention contains the encrypted provider message ID, provider, masked destination, resolution type, and expiry. Server validation rejects metadata containing OTPs, passwords, access tokens, refresh tokens, secrets, or credentials.

## Portal Changes

- Browser challenge controls distinguish `Use email code` from `Take over`.
- Email-code approval returns the run to `Resuming application`.
- The cloud runner summary explains email-code approval and preserved owner handoffs.
- Settings replaces the broad pause list with a compact challenge matrix covering CAPTCHA, email code, phone/app 2FA, assessments, and required questions.
- Privacy and Terms explain transient email-code handling and state that Bluey does not circumvent CAPTCHA or account-owner checks.

## Verification

- Automation tests cover CAPTCHA takeover, fresh email-code matching, unrelated/old/disconnected message rejection, expiry, and secret-free intervention serialization.
- Rust Jobs tests cover encrypted intervention persistence and reject raw OTP metadata.
- Portal type checks and tests pass.
- The Browser approval flow was exercised in the local preview.
- Settings and Browser layouts were reviewed in light and dark themes and at a 390px mobile viewport with no horizontal overflow.
- No Bluey host overlay, meeting runtime, audio, or native session files were changed.
