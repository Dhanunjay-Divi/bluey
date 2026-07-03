# Bluey 0.1.53

- Removes the misleading default saved-memory/checking status before normal answers stream.
- Looks up saved conversation memory only for explicit memory requests or clear follow-ups.
- Adds short request refs on failed answer cards so screenshots can be traced without exposing prompt text.
- Keeps the production server on the same memory-lookup behavior as the desktop overlay.
