# FIX-003: Calendar webhook authentication

## Issue

The public Google Calendar and Microsoft Graph webhook routes accepted
insufficiently authenticated or weakly bounded change notifications. This could
allow spoofed calendar doorbells, reflection of hostile validation input, and
attacker-controlled values in application logs.

## Root Cause

- Google notifications checked the configured channel token but accepted any
  non-empty `X-Goog-Channel-ID`. Google echoes both the caller-created channel
  ID and token, so both must be compared with the values used by `events.watch`.
- Shared-secret verification trimmed the received value. That made values with
  attacker-added surrounding whitespace compare equal instead of requiring an
  exact match.
- Microsoft notifications checked `clientState`, but did not bound the body or
  batch, require a valid subscription ID/change type, or require the documented
  JSON media type.
- Microsoft validation echoed a public query value without anti-sniffing and
  cache headers or rejecting markup-like input.
- The success log included provider-supplied `changeType` and emitted one line
  per notification, allowing public input to shape and amplify logs.

## Fix Summary

- Require exact, constant-time matches for both
  `X-Goog-Channel-Token` and `X-Goog-Channel-ID`.
- Fail closed with `503 Service Unavailable` when any required server-side
  webhook credential is absent or invalid.
- Enforce provider-documented credential limits: Google channel ID 64
  characters, channel token 256 characters, and Microsoft subscription
  `clientState` 128 characters.
- Require a bounded Google resource ID after authenticating the channel.
- Bound Microsoft JSON notifications to 64 KiB and 256 items, validate every
  item's `clientState`, and require a GUID subscription ID plus a supported
  calendar change type.
- Require `text/plain` for Microsoft validation and `application/json` for
  notifications.
- Echo only bounded, control-free, non-markup validation tokens with
  `text/plain`, `Cache-Control: no-store`, and
  `X-Content-Type-Options: nosniff`.
- Log only a generic authenticated/rejected event and server-generated status
  code. No headers, tokens, subscription IDs, payload fields, or bodies are
  logged.

## Files Modified

| File | Change |
|------|--------|
| `server/src/api/calendar.rs` | Harden Google and Microsoft webhook verification and add focused tests. |
| `server/src/api/mod.rs` | Apply the Microsoft 64 KiB body limit before Axum buffers the request. |
| `docs/work/FIX-003-calendar-webhook-authentication.md` | Record the security boundary, setup contract, and limitations. |

## Edge Cases Handled

- Missing or whitespace-padded shared secrets.
- A valid Google token paired with a missing, wrong, or unconfigured channel ID.
- Missing or oversized Google authentication headers.
- A mixed Microsoft batch where only one item has the wrong `clientState`.
- Missing `clientState`, subscription ID, or change type.
- Empty, malformed, oversized, or wrong-media-type Microsoft payloads.
- Microsoft validation tokens containing controls, markup delimiters, or
  excessive data.

## How to Test

```bash
cd server
cargo test calendar::tests --lib
cargo clippy --lib -- -D warnings
```

For an external smoke test, configure the three required values:

```bash
export BLUEY_GOOGLE_CALENDAR_WEBHOOK_TOKEN='<random token used in events.watch>'
export BLUEY_GOOGLE_CALENDAR_WEBHOOK_CHANNEL_ID='<id used in events.watch>'
export BLUEY_MICROSOFT_CALENDAR_CLIENT_STATE='<random clientState used in subscription creation>'
```

Then expose the server through a publicly reachable HTTPS endpoint with a valid
certificate and use:

- `POST /webhook/calendar/google` as the Google `events.watch` address;
- `POST /webhook/calendar/microsoft` as the Microsoft Graph
  `notificationUrl`.

The Google watch request must use the exact configured `id` and `token`.
The Microsoft subscription request must use the exact configured
`clientState`. Rotate server configuration and recreate subscriptions together;
rotating only one side intentionally causes notifications to fail closed.

Provider contracts:

- [Google Calendar push notifications](https://developers.google.com/workspace/calendar/api/guides/push)
- [Microsoft Graph webhook delivery](https://learn.microsoft.com/en-us/graph/change-notifications-delivery-webhooks)

## Known Limitations

- These endpoints only authenticate and acknowledge change doorbells. They do
  not create, renew, or delete Google channels or Microsoft subscriptions, and
  they do not enqueue work or deliver a doorbell to a user's desktop daemon.
- OAuth authorization-code/token exchange is a separate native PKCE flow.
  Webhook validation cannot fix a failed Google or Microsoft token exchange.
- External setup still requires Google and Microsoft app registrations,
  provider-approved calendar scopes/consent, a public HTTPS callback with valid
  DNS/TLS, and separate subscription creation plus expiration renewal.
- The current environment-variable model represents one Google channel and one
  Microsoft `clientState`. A multi-tenant production service must persist
  per-subscription channel IDs, tokens/client states, account ownership, and
  expiration in server storage before these doorbells can safely route work.
- The Microsoft validation challenge is necessarily public before a
  subscription exists. It proves endpoint reachability, while notification
  authenticity comes from the later exact `clientState` comparison.
