# Settings UI Contract

Bluey settings should be driven by a single cloud profile plus local machine capability state. The desktop settings app can render this contract without knowing provider secrets or backend implementation details.

## Settings Sections

- Account: login state, email, workspace, plan, billing portal.
- Permissions: microphone, system audio, screen capture, accessibility, notifications.
- Capture: selected mic, selected system source, visible capture indicator, active-page capture defaults, optional periodic capture interval.
- Shortcuts: show/hide, ask, capture, attach, mute mic, pause sync.
- Privacy: retention days, artifact retention, cloud sync toggle, local cache clear, export/delete.
- Providers: managed routing health, current answer mode, optional development-key status.
- Workspace: members, role, admin controls, audit log link.
- Diagnostics: app version, device id, sync cursor, queue backlog, provider health, logs export.

## Cloud Payload

`GET /settings` returns:

```json
{
  "workspace_id": "wsp_01",
  "profile_version": 7,
  "account": {
    "email": "user@example.com",
    "role": "admin",
    "plan": "pro",
    "billing_portal_available": true
  },
  "capture": {
    "cloud_sync_enabled": true,
    "retain_audio": false,
    "periodic_capture_interval_seconds": 12,
    "visible_capture_indicator": true
  },
  "privacy": {
    "retention_days": 90,
    "artifact_retention_days": 90,
    "allow_workspace_memory": true,
    "allow_provider_training": false
  },
  "answering": {
    "route": "managed_auto",
    "mode": "meeting",
    "citation_level": "source",
    "latency_budget_ms": 7000
  },
  "shortcuts": {
    "toggle_overlay": "CmdOrCtrl+Shift+Space",
    "ask": "CmdOrCtrl+Shift+A",
    "capture": "CmdOrCtrl+Shift+C",
    "attach": "CmdOrCtrl+Shift+U"
  },
  "limits": {
    "max_local_queue_mb": 512,
    "max_artifact_mb": 50,
    "monthly_minutes_remaining": 1200
  }
}
```

`PUT /settings` accepts partial updates with `profile_version` for optimistic concurrency. The server returns the full merged profile and a new version.

## Local Capability Payload

The desktop combines cloud settings with local capability status:

```json
{
  "device_id": "dev_01",
  "app_version": "0.1.0",
  "platform": "macos",
  "permissions": {
    "microphone": "granted",
    "system_audio": "needs_setup",
    "screen_capture": "granted",
    "accessibility": "not_requested",
    "notifications": "denied"
  },
  "devices": {
    "microphones": [
      { "id": "default", "label": "Default Microphone", "selected": true }
    ],
    "system_sources": [
      { "id": "system-default", "label": "System Audio", "selected": true }
    ]
  },
  "health": {
    "cloud": "healthy",
    "sync": "backlogged",
    "stt": "healthy",
    "answers": "degraded",
    "rag": "healthy"
  },
  "sync": {
    "pending_events": 14,
    "pending_bytes": 2048000,
    "last_ack_cursor": "cur_01",
    "last_error": null
  }
}
```

## UI Behavior Requirements

- Settings must show visible consent state before capture can start.
- Disabling cloud sync pauses uploads but keeps local meeting capture available.
- Changing retention warns admins that expired data will be queued for deletion.
- Provider settings show managed route health, not raw provider keys.
- Development provider-key mode is hidden in production builds.
- Export and deletion actions require confirmation and show request status.
- Workspace admin controls are hidden unless the role is `admin` or `owner`.
- Any permission blocked by the OS links to the correct system settings page.

## Validation

- `retention_days`: 7 to 3650, or workspace enterprise override.
- `periodic_capture_interval_seconds`: 5 to 300.
- `latency_budget_ms`: 1000 to 30000.
- Shortcut values must be unique after platform normalization.
- Cloud sync cannot be enabled when account auth is missing.
- Workspace memory cannot be enabled when retention is zero or deletion is pending.
