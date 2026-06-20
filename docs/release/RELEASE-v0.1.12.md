# Bluey v0.1.12

Auto-update behavior release for the paid alpha path.

## Included

- `bluey on` now auto-installs verified signed updates by default.
- Users still see the update version and size before install and can press Esc
  within 5 seconds to skip that launch.
- Operator/dev escapes remain available:
  - `BLUEY_SKIP_UPDATE=1`
  - `BLUEY_UPDATE_CHECK_ONLY=1`
  - `BLUEY_AUTO_UPDATE=0`

## Operator Notes

- This behavior still requires a verified signed `latest.json.sig`.
- Unsigned manifests remain non-installable unless explicitly allowed for local
  development.
