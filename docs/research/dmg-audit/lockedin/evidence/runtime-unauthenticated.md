# Approved unauthenticated runtime observation

## Authorization and guardrails

The coordinator authorized one short unauthenticated launch after the static audit. The exact DMG was reattached with `hdiutil attach -readonly -nobrowse`. The app was launched directly from the mounted bundle with a disposable HOME, Electron user-data directory, and blocked external egress/update posture. No credentials or account were used; no button was clicked; no permission was granted; no updater/install/helper/remote-control action was allowed.

Runtime window: approximately `2026-07-12T08:59:48Z` through `2026-07-12T09:00:29Z`.

Disposable root: `/tmp/lockedin-runtime.dWshln` (mode `0700`; removed after inventory).

## Exact launch command

```text
env -i HOME='/tmp/lockedin-runtime.dWshln/home' TMPDIR='/tmp/lockedin-runtime.dWshln/tmp/' XDG_CONFIG_HOME='/tmp/lockedin-runtime.dWshln/home/.config' XDG_CACHE_HOME='/tmp/lockedin-runtime.dWshln/home/.cache' USER='runtime' LOGNAME='runtime' PATH='/usr/bin:/bin:/usr/sbin:/sbin' LANG='en_US.UTF-8' ELECTRON_NO_UPDATER='1' ELECTRON_ENABLE_LOGGING='1' HTTP_PROXY='http://127.0.0.1:9' HTTPS_PROXY='http://127.0.0.1:9' ALL_PROXY='http://127.0.0.1:9' NO_PROXY='localhost,127.0.0.1' '/Volumes/LockedIn 1.7.5-universal/LockedIn.app/Contents/MacOS/LockedIn' --user-data-dir='/tmp/lockedin-runtime.dWshln/user-data' --host-resolver-rules='MAP * 0.0.0.0, EXCLUDE localhost, EXCLUDE 127.0.0.1' --proxy-server='http://127.0.0.1:9' --disable-component-update --disable-background-networking --disable-breakpad --disable-crash-reporter --no-default-browser-check
```

The proxy endpoint was deliberately closed. `ELECTRON_NO_UPDATER=1` did not suppress this app's explicit updater call; the closed proxy prevented a remote connection or download.

## Initial route and UI identity

Runtime renderer logging identified the first window as:

- URL: `file:///Volumes/LockedIn%201.7.5-universal/LockedIn.app/Contents/Resources/app.asar/build/index.html#/sign-in`
- title: `LockedIn AI`
- type: `window`

No authentication or route transition was attempted. A separate investigator window-metadata command produced no usable UI content and is not attributed to LockedIn.

## Runtime console observations

- The main process detected `arm64` and found the packaged arm64 audio addon. It did not start audio capture.
- Crash reporting initialized with database `/private/tmp/lockedin-runtime.dWshln/user-data/Crashpad` and submit URL `https://crash-report.invalid`; no crash was submitted.
- The app logged its complete executable path and `argv` to `/tmp/lockedin-runtime.dWshln/home/Library/Logs/lockedin_desktop_app/main.log`.
- Protocol registration returned `true`. After termination, a scoped `lsregister -u` for the exact mounted bundle initially exited 0. A final LaunchServices dump still retained stale, non-runnable path records for the unmounted main app and GPU helper; a second scoped unregister attempt returned `-10814`. No global LaunchServices reset/garbage collection was run because it could affect unrelated app state.
- Preload reported successful loading.
- Firestore 11.2.0 entered offline mode and repeatedly reported failed listen streams because egress was blocked.
- Model-tier configuration failed to load and the renderer reported falling back to static model tiers.
- The app started an update check despite `ELECTRON_NO_UPDATER=1`, created `.updaterId`, and failed with `net::ERR_PROXY_CONNECTION_FAILED`. No update metadata/package was received, downloaded, or installed.
- A process/socket snapshot found no live TCP or UDP socket for any LockedIn process.

No environment value, token, cookie, payload, or generated updater identifier is reproduced.

## Process tree snapshot

PIDs are ephemeral. Arguments are recorded because the disposable invocation contained no credentials.

```text
46240  parent=shell  LockedIn main --user-data-dir=/tmp/lockedin-runtime.dWshln/user-data --host-resolver-rules=... --proxy-server=http://127.0.0.1:9 ...
47057  parent=1      chrome_crashpad_handler --database=/private/tmp/lockedin-runtime.dWshln/user-data/Crashpad --url=https://crash-report.invalid ...
47066  parent=46240  LockedIn Helper (GPU) --type=gpu-process --user-data-dir=/private/tmp/lockedin-runtime.dWshln/user-data ... --seatbelt-client=38
47067  parent=46240  LockedIn Helper --type=utility --utility-sub-type=network.mojom.NetworkService --service-sandbox-type=network ... --seatbelt-client=38
47077  parent=46240  LockedIn Helper (Renderer) --type=renderer --user-data-dir=/private/tmp/lockedin-runtime.dWshln/user-data --app-path=.../app.asar --enable-sandbox ... --seatbelt-client=58
```

The `--enable-sandbox` renderer argument is runtime evidence that Electron's renderer sandbox was active even though the BrowserWindow options do not explicitly contain `sandbox: true`. This corrects the static uncertainty; it does not add the macOS App Sandbox entitlement, which remains absent.

An investigator-created `swift-frontend -interpret` process and `tmp/.../main.swift` were part of a CGWindowList/preflight inspection attempt, not app children. They are excluded from the LockedIn process tree and application-profile inventory.

## Complete application-profile file inventory

At termination, `home/` plus `user-data/` contained 51 files totaling 2,916,515 bytes. Contents were not copied. Paths are relative to `/tmp/lockedin-runtime.dWshln`; sizes are bytes.

```text
home/Library/Logs/lockedin_desktop_app/main.log|9041|0644
user-data/.updaterId|36|0644
user-data/Cache/Cache_Data/index-dir/the-real-index|48|0600
user-data/Cache/Cache_Data/index|24|0600
user-data/Code Cache/js/index-dir/the-real-index|48|0600
user-data/Code Cache/js/index|24|0600
user-data/Code Cache/wasm/index-dir/the-real-index|48|0600
user-data/Code Cache/wasm/index|24|0600
user-data/Cookies-journal|0|0600
user-data/Cookies|20480|0600
user-data/Crashpad/settings.dat|40|0600
user-data/DawnGraphiteCache/data_0|8192|0600
user-data/DawnGraphiteCache/data_1|270336|0600
user-data/DawnGraphiteCache/data_2|8192|0600
user-data/DawnGraphiteCache/data_3|8192|0600
user-data/DawnGraphiteCache/index|262512|0600
user-data/DawnWebGPUCache/data_0|8192|0600
user-data/DawnWebGPUCache/data_1|270336|0600
user-data/DawnWebGPUCache/data_2|8192|0600
user-data/DawnWebGPUCache/data_3|8192|0600
user-data/DawnWebGPUCache/index|262512|0600
user-data/GPUCache/data_0|45056|0600
user-data/GPUCache/data_1|270336|0600
user-data/GPUCache/data_2|1056768|0600
user-data/GPUCache/data_3|8192|0600
user-data/GPUCache/index|262512|0600
user-data/IndexedDB/file__0.indexeddb.leveldb/000003.log|4440|0600
user-data/IndexedDB/file__0.indexeddb.leveldb/CURRENT|16|0600
user-data/IndexedDB/file__0.indexeddb.leveldb/LOCK|0|0600
user-data/IndexedDB/file__0.indexeddb.leveldb/LOG|455|0600
user-data/IndexedDB/file__0.indexeddb.leveldb/MANIFEST-000001|23|0600
user-data/Local Storage/leveldb/000003.log|81|0600
user-data/Local Storage/leveldb/CURRENT|16|0600
user-data/Local Storage/leveldb/LOCK|0|0600
user-data/Local Storage/leveldb/LOG|263|0600
user-data/Local Storage/leveldb/MANIFEST-000001|41|0600
user-data/Network Persistent State|111|0600
user-data/Preferences|41|0600
user-data/Session Storage/000003.log|243|0600
user-data/Session Storage/CURRENT|16|0600
user-data/Session Storage/LOCK|0|0600
user-data/Session Storage/LOG|251|0600
user-data/Session Storage/MANIFEST-000001|41|0600
user-data/Shared Dictionary/cache/index-dir/the-real-index|48|0600
user-data/Shared Dictionary/cache/index|24|0600
user-data/Shared Dictionary/db-journal|0|0600
user-data/Shared Dictionary/db|45056|0600
user-data/Trust Tokens-journal|0|0600
user-data/Trust Tokens|36864|0600
user-data/WebStorage/QuotaManager-journal|0|0600
user-data/WebStorage/QuotaManager|40960|0600
```

The empty `Crashpad/{attachments,completed,new,pending}` and `blob_storage/<generated-id>` directories are not files and are therefore not in the list. The main log and updater ID were mode `0644`; Chromium database/cache files were mode `0600`. The disposable root and HOME/user-data parents were mode `0700`.

The investigator's `swift -e` attempt created 108 compiler/temp files under `tmp/`; these were not created by LockedIn and were deleted with the same disposable root.

## Termination and cleanup

- Sent SIGINT through the controlling terminal; the launch session exited with status 0.
- At `2026-07-12T09:00:41Z`, no main, renderer, GPU, network-service, crashpad, or other LockedIn process remained.
- Attempted scoped LaunchServices cleanup for the exact app/helper paths. The mount and processes are gone, but LaunchServices retained stale path records as described above; this residual is explicitly recorded rather than using a global cleanup operation.
- No permission was granted, no account/auth state was created, and no external socket was established.
- The disposable root was inventoried and then removed.
- The exact DMG was detached after cleanup; mount absence was verified.
