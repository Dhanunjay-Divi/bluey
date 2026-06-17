# Admin User Flow - 2026-06-04

## Goal

Let the operator test Bluey as a normal customer account while also getting admin access on the server. The desktop app remains normal: users still run `bluey on`, link/login in the browser when needed, and tokens stay in Bluey's private local account profile. Legacy installs may also clear OS keychain tokens when `BLUEY_LEGACY_KEYRING_FALLBACK=1`.

## Server Switch

Set admin emails on the server:

```bash
BLUEY_ADMIN_EMAILS="owner@bluey.sh,ops@bluey.sh"
```

Behavior:

- New signup with a matching email is created with `is_admin = true`.
- Existing matching accounts are promoted on the next successful login.
- Matching is trim-normalized and case-insensitive.
- No account becomes admin unless its email is explicitly listed in server config.

## Customer-Facing Flow

1. User installs Bluey.
2. User runs `bluey on`.
3. If cloud features need an account, Bluey opens/points to the browser login/link flow.
4. User signs up or logs in with the configured admin email.
5. The desktop receives normal account tokens through the existing link/device flow.
6. Server-side admin endpoints accept that account token because the account is admin.

## Test Proof

Integration tests cover:

- Configured admin email signup can call `/admin/customers`.
- Configured admin email login promotes a pre-existing non-admin account.
- `/account/me` exposes `is_admin` so the dashboard/CLI can show operator state.

Run:

```bash
cd /Users/uno/Downloads/cue
cd server
cargo test configured_admin_email --test integration_e2e
```

## Security Notes

- This is not a first-user-admin flow.
- This is not a desktop flag.
- Admin bootstrap lives only in server environment/configuration.
- Do not put upstream provider keys on customer machines; managed provider keys stay server-side.
