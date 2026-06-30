# Bluey 0.1.19

## Listen Sign-In Gate

- Listen/mic start now verifies the linked Bluey account before audio capture starts.
- If the desktop is not signed in, has an expired/deleted account token, or cannot verify the account, Listen stays off and no audio capture/STT billing begins.
- Expired or deleted-account tokens are cleared so the next sign-in can link the desktop cleanly.
- The macOS overlay no longer optimistically flips the Listen UI into Starting/Listening before the daemon accepts the request.

## Verification

- Release artifact scanned clean for configured secrets and visible-overlay dev flags.
- `latest.json` is signed with the Bluey Ed25519 release key.
- Live installer smoke from `https://bluey.sh/install.sh` completed successfully.
