# Calendar OAuth Runtime Configuration

Bluey's Google and Microsoft calendar integrations use the native public-client
OAuth flow: authorization code + PKCE + a loopback redirect. The desktop app
needs a registered public client ID for each provider. It must never receive or
store a client secret.

## Configuration Precedence

The calendar provider resolves each public client ID in this order:

1. A non-empty runtime environment variable on the `bluey-daemon` process.
2. The same variable captured when the release binary was compiled.
3. An invalid placeholder, which makes the UI report that calendar OAuth is not
   configured before it opens a browser.

| Provider | Runtime/build variable | Required registration |
|----------|------------------------|-----------------------|
| Google | `BLUEY_GOOGLE_CLIENT_ID` | Google OAuth Desktop app client ID ending in `.apps.googleusercontent.com` |
| Microsoft | `BLUEY_MICROSOFT_CLIENT_ID` | Entra mobile/desktop public-client application GUID |

Runtime values intentionally override baked release values. This supports
staging or enterprise distributions without rebuilding the binary. An empty
runtime value does not erase a valid baked value. A malformed non-empty
override fails visibly instead of silently falling back to a different OAuth
application.

Environment variables are process configuration. Set them in the service,
launcher, terminal, or supervisor that starts `bluey-daemon`, then restart
Bluey. Changing a shell profile does not change an already-running daemon, and
Finder-launched macOS apps do not generally inherit interactive-shell exports.
Normal customer releases should therefore bake Bluey's registered public IDs;
runtime configuration is primarily for development, staging, and managed
deployments.

Example for a locally launched daemon:

```bash
BLUEY_GOOGLE_CLIENT_ID='<registered-google-desktop-client-id>' \
BLUEY_MICROSOFT_CLIENT_ID='<registered-microsoft-application-guid>' \
bluey-daemon
```

Do not add either value to source control. Although OAuth client IDs are public
identifiers rather than credentials, keeping deployment-specific identifiers
in release variables or the process environment prevents accidental
cross-environment configuration. Do not define
`BLUEY_GOOGLE_CLIENT_SECRET` or `BLUEY_MICROSOFT_CLIENT_SECRET`; Bluey's PKCE
desktop flow does not use them.

## Provider Registration

Google:

- Register an OAuth 2.0 Desktop app.
- Enable Google Calendar API.
- Allow `calendar.readonly`, `openid`, and `email`.
- Google desktop clients accept the loopback redirect Bluey creates on
  `127.0.0.1` with an ephemeral port.

Microsoft:

- Register a mobile/desktop public client and enable public client flows.
- Support the intended account audience through the `common` endpoint.
- Register `http://localhost` as the mobile/desktop redirect.
- Grant delegated `Calendars.Read`, `User.Read`, `offline_access`, `openid`, and
  `profile`.

Provider registration and consent-screen approval happen in Google Cloud and
Microsoft Entra. They cannot be generated or completed from this repository.

Official provider references:

- [Google OAuth 2.0 installed applications](https://developers.google.com/identity/protocols/oauth2)
- [Google desktop loopback flow](https://developers.google.com/identity/protocols/oauth2/resources/loopback-migration)
- [Google OAuth security best practices](https://developers.google.com/identity/protocols/oauth2/resources/best-practices)
- [Microsoft redirect URI rules](https://learn.microsoft.com/en-us/entra/identity-platform/reply-url)
- [Microsoft Graph permissions](https://learn.microsoft.com/en-us/graph/permissions-reference)

## Failure Behavior

Onboarding disables a provider whose public client ID is missing or malformed
and explains both recovery paths:

- install a calendar-enabled Bluey release; or
- set the provider's runtime variable on the daemon and restart Bluey.

The browser is not opened until configuration validation passes. OAuth tokens
are stored separately in the operating-system credential vault and never
returned to the UI.
