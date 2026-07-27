#!/usr/bin/env bash
# Verify that a staged BlueyAudio.app is complete, correctly provisioned, and
# actually launchable after copying/signing.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
APP="${1:-$SCRIPT_DIR/.build/BlueyAudio.app}"
REQUIRE_STABLE="${BLUEY_REQUIRE_STABLE_CODESIGN:-0}"
REQUIRE_PERSISTENT="${BLUEY_REQUIRE_PERSISTENT_CAPTURE_ENTITLEMENT:-0}"
VERIFY_LAUNCH="${BLUEY_VERIFY_LAUNCH:-0}"
EXPECTED_ARCHS="${BLUEY_EXPECTED_ARCHS:-}"
BIN="$APP/Contents/MacOS/BlueyAudio"
PLIST="$APP/Contents/Info.plist"
PROFILE="$APP/Contents/embedded.provisionprofile"
PROBE_SENTINEL="bluey-audio-launch-probe-v1"
TEMP_FILES=()
PROBE_MARKERS=()

cleanup_verifier() {
  # macOS ships Bash 3.2. With `set -u`, expanding an empty declared array as
  # `"${array[@]}"` raises "unbound variable". The verifier commonly exits
  # before creating launch-probe files, so guard both cleanup loops by length.
  if [ "${#PROBE_MARKERS[@]}" -gt 0 ]; then
    for marker in "${PROBE_MARKERS[@]}"; do
      while IFS= read -r probe_pid; do
        case "$probe_pid" in
          ''|*[!0-9]*) continue ;;
        esac
        if [ "$probe_pid" -ne "$$" ]; then
          kill "$probe_pid" 2>/dev/null || true
        fi
      done < <(/usr/bin/pgrep -f "$marker" 2>/dev/null || true)
    done
  fi
  if [ "${#TEMP_FILES[@]}" -gt 0 ]; then
    for temp_file in "${TEMP_FILES[@]}"; do
      rm -f "$temp_file"
    done
  fi
}
trap cleanup_verifier EXIT

for flag_name in REQUIRE_STABLE REQUIRE_PERSISTENT VERIFY_LAUNCH; do
  flag_value="${!flag_name}"
  case "$flag_value" in
    0|1) ;;
    *)
      echo "verify-app: $flag_name must be 0 or 1" >&2
      exit 2
      ;;
  esac
done

if [ ! -d "$APP" ]; then
  echo "verify-app: missing bundle: $APP" >&2
  exit 1
fi
if [ ! -x "$BIN" ]; then
  echo "verify-app: missing executable: $BIN" >&2
  exit 1
fi
if [ ! -f "$PLIST" ]; then
  echo "verify-app: missing Info.plist: $PLIST" >&2
  exit 1
fi

bundle_id="$(
  /usr/libexec/PlistBuddy -c "Print :CFBundleIdentifier" "$PLIST" 2>/dev/null
)"
if [ "$bundle_id" != "sh.bluey.audio" ]; then
  echo "verify-app: unexpected bundle identifier: $bundle_id" >&2
  exit 1
fi

codesign --verify --deep --strict "$APP"
details="$(codesign -dvv "$APP" 2>&1)"
identifier="$(printf '%s\n' "$details" | sed -n 's/^Identifier=//p' | head -1)"
if [ "$identifier" != "sh.bluey.audio" ]; then
  echo "verify-app: unexpected signature identifier: $identifier" >&2
  exit 1
fi

if [ -n "$EXPECTED_ARCHS" ]; then
  actual_archs="$(lipo -archs "$BIN")"
  for expected_arch in $EXPECTED_ARCHS; do
    case " $actual_archs " in
      *" $expected_arch "*) ;;
      *)
        echo "verify-app: missing $expected_arch slice (found: $actual_archs)" >&2
        exit 1
        ;;
    esac
  done
fi

signature="$(
  printf '%s\n' "$details" | sed -n 's/^Signature=//p' | head -1
)"
authority="$(
  printf '%s\n' "$details" | sed -n 's/^Authority=//p' | head -1
)"
team_id="$(
  printf '%s\n' "$details" | sed -n 's/^TeamIdentifier=//p' | head -1
)"

if [ "$REQUIRE_STABLE" = "1" ]; then
  if [ "$signature" = "adhoc" ] || [ -z "$authority" ] || \
    [ -z "$team_id" ] || [ "$team_id" = "not set" ]; then
    echo "verify-app: release bundle requires a certificate-backed signature" >&2
    echo "verify-app: set BLUEY_CODESIGN_IDENTITY to an installed identity" >&2
    exit 1
  fi
fi

requirement="$(codesign -dr - "$APP" 2>&1)"
if [ "$signature" != "adhoc" ] && \
  ! printf '%s\n' "$requirement" | grep -Fq 'identifier "sh.bluey.audio"'; then
  echo "verify-app: designated requirement is not pinned to sh.bluey.audio" >&2
  exit 1
fi

signed_entitlements="$(
  codesign -d --entitlements - --xml "$APP" 2>&1 || true
)"
compact_entitlements="$(
  printf '%s' "$signed_entitlements" | tr -d '[:space:]'
)"
has_persistent=0
if printf '%s' "$compact_entitlements" \
  | grep -Fq '<key>com.apple.developer.persistent-content-capture</key><true/>'; then
  has_persistent=1
fi

if [ "$has_persistent" = "1" ] && [ ! -f "$PROFILE" ]; then
  echo "verify-app: restricted persistent-content-capture entitlement lacks an embedded profile" >&2
  exit 1
fi
if [ "$REQUIRE_PERSISTENT" = "1" ] && [ "$has_persistent" != "1" ]; then
  echo "verify-app: persistent-content-capture was required but is not signed into the app" >&2
  exit 1
fi

if [ -f "$PROFILE" ]; then
  profile_plist="$(mktemp -t bluey-audio-profile)"
  TEMP_FILES+=("$profile_plist")

  if ! security cms -D -i "$PROFILE" >"$profile_plist"; then
    echo "verify-app: embedded provisioning profile is not valid CMS data" >&2
    exit 1
  fi
  profile_entitlement="$(
    /usr/libexec/PlistBuddy \
      -c "Print :Entitlements:com.apple.developer.persistent-content-capture" \
      "$profile_plist" 2>/dev/null || true
  )"
  if [ "$has_persistent" = "1" ] && [ "$profile_entitlement" != "true" ]; then
    echo "verify-app: embedded profile does not authorize persistent-content-capture" >&2
    exit 1
  fi
  profile_app_id="$(
    /usr/libexec/PlistBuddy \
      -c "Print :Entitlements:application-identifier" \
      "$profile_plist" 2>/dev/null || \
      /usr/libexec/PlistBuddy \
        -c "Print :Entitlements:com.apple.application-identifier" \
        "$profile_plist" 2>/dev/null || true
  )"
  case "$profile_app_id" in
    "$team_id.sh.bluey.audio") ;;
    *)
      echo "verify-app: embedded profile does not match team/bundle sh.bluey.audio" >&2
      exit 1
      ;;
  esac
fi

if [ "$VERIFY_LAUNCH" = "1" ]; then
  # Direct execution catches kernel/amfid rejection. The dedicated probe exits
  # before touching Screen/System Audio or Microphone and must write a unique
  # sentinel. Older helpers that ignore the flag time out instead of entering
  # real capture and being accepted accidentally.
  direct_probe="$(mktemp -t bluey-audio-direct-probe)"
  rm -f "$direct_probe"
  TEMP_FILES+=("$direct_probe")
  PROBE_MARKERS+=("$direct_probe")
  "$BIN" \
    --launch-probe \
    --launch-probe-output "$direct_probe" \
    >/dev/null 2>&1 &
  direct_pid=$!
  direct_finished=0
  direct_ok=0
  for _ in $(seq 1 100); do
    if ! kill -0 "$direct_pid" 2>/dev/null; then
      direct_finished=1
      if wait "$direct_pid"; then
        direct_ok=1
      fi
      break
    fi
    sleep 0.1
  done
  if [ "$direct_finished" != "1" ]; then
    kill "$direct_pid" 2>/dev/null || true
    wait "$direct_pid" 2>/dev/null || true
    echo "verify-app: direct no-capture launch probe timed out" >&2
  elif [ "$direct_ok" != "1" ]; then
    echo "verify-app: helper executable failed the no-capture launch probe" >&2
  fi
  direct_reported_pid="$(
    sed -n 's/^pid=//p' "$direct_probe" 2>/dev/null | head -1
  )"
  if [ "$direct_finished" != "1" ] || [ "$direct_ok" != "1" ] || \
    ! grep -Fxq "$PROBE_SENTINEL" "$direct_probe" 2>/dev/null || \
    [ "$direct_reported_pid" != "$direct_pid" ]; then
    echo "verify-app: direct launch probe did not return the expected handshake" >&2
    exit 1
  fi

  launch_log="$(mktemp -t bluey-audio-launch)"
  launch_probe="$(mktemp -t bluey-audio-launch-probe)"
  rm -f "$launch_probe"
  TEMP_FILES+=("$launch_log" "$launch_probe")
  PROBE_MARKERS+=("$launch_probe")
  # Do not use `open -W` here. The probe intentionally exits immediately, and
  # LaunchServices can race that exit while setting up its kevent waiter,
  # yielding "No such process" even though the app launched successfully.
  # The unique sentinel is the authoritative launch acknowledgement.
  if ! /usr/bin/open -n "$APP" --args \
    --launch-probe \
    --launch-probe-output "$launch_probe" \
    >"$launch_log" 2>&1; then
    echo "verify-app: LaunchServices rejected the helper bundle" >&2
    sed -n '1,40p' "$launch_log" >&2
    exit 1
  fi

  launch_reported_pid=0
  for _ in $(seq 1 100); do
    launch_reported_pid="$(
      sed -n 's/^pid=//p' "$launch_probe" 2>/dev/null | head -1 || true
    )"
    case "$launch_reported_pid" in
      ''|*[!0-9]*) launch_reported_pid=0 ;;
    esac
    if [ "$launch_reported_pid" -gt 1 ] && \
      grep -Fxq "$PROBE_SENTINEL" "$launch_probe" 2>/dev/null; then
      break
    fi
    sleep 0.1
  done
  if [ "$launch_reported_pid" -le 1 ] || \
    ! grep -Fxq "$PROBE_SENTINEL" "$launch_probe" 2>/dev/null; then
    if [ "$launch_reported_pid" -gt 1 ]; then
      kill "$launch_reported_pid" 2>/dev/null || true
    fi
    echo "verify-app: LaunchServices probe did not return the expected handshake" >&2
    sed -n '1,40p' "$launch_log" >&2
    exit 1
  fi

  helper_exited=0
  for _ in $(seq 1 50); do
    if ! kill -0 "$launch_reported_pid" 2>/dev/null; then
      helper_exited=1
      break
    fi
    sleep 0.1
  done
  if [ "$helper_exited" != "1" ]; then
    kill "$launch_reported_pid" 2>/dev/null || true
    echo "verify-app: LaunchServices probe helper did not exit cleanly" >&2
    exit 1
  fi
fi

if [ -n "$authority" ]; then
  echo "verified $APP ($bundle_id, team $team_id, authority $authority)"
else
  echo "verified $APP ($bundle_id, ad-hoc development signature)"
fi
