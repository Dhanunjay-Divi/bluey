# Bluey v0.1.16

This release restores reliable overlay interaction after the click-through hardening pass.

- Makes the expanded macOS overlay mouse-active by default again so controls remain clickable.
- Restores blank-surface drag behavior in the expanded macOS overlay.
- Aligns Windows expanded overlay behavior so controls click normally and blank surface moves the panel.
- Keeps production overlay capture-hidden; visible overlay/capture debug flags remain disabled in shipped binaries.
