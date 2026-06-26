#!/usr/bin/env bash
set -euo pipefail

# Apply or verify Square hosted-checkout branding for Bluey.
#
# Usage:
#   scripts/bluey-square-branding.sh /etc/bluey-api/bluey-api.env
#   scripts/bluey-square-branding.sh /etc/bluey-api/bluey-api.env --check
#
# The script does not print access tokens. It uses the active
# SQUARE_ENVIRONMENT and the matching SQUARE_* token/location values.

ENV_FILE=""
MODE="apply"

for arg in "$@"; do
  case "$arg" in
    --check)
      MODE="check"
      ;;
    --apply)
      MODE="apply"
      ;;
    *)
      ENV_FILE="$arg"
      ;;
  esac
done

if [ -n "$ENV_FILE" ]; then
  if [ ! -f "$ENV_FILE" ]; then
    echo "fatal: env file not found: $ENV_FILE" >&2
    exit 2
  fi
  set -a
  # shellcheck disable=SC1090
  . "$ENV_FILE"
  set +a
fi

if [ "${BLUEY_BILLING_PROVIDER:-}" != "square" ]; then
  echo "ok: billing provider is not Square; skipped checkout branding"
  exit 0
fi

SQUARE_API_VERSION="${SQUARE_API_VERSION:-2025-04-16}"
SQUARE_ENVIRONMENT="${SQUARE_ENVIRONMENT:-sandbox}"
EXPECTED_APPLICATION_ID="${BLUEY_SQUARE_APPLICATION_ID_EXPECTED:-}"
FORBIDDEN_LOCATION_IDS="${BLUEY_SQUARE_FORBIDDEN_LOCATION_IDS:-LRV41T44TT30E}"
case "$SQUARE_ENVIRONMENT" in
  production|prod|live)
    SQUARE_BASE_URL="https://connect.squareup.com"
    ACCESS_TOKEN="${SQUARE_ACCESS_TOKEN:-${SQUARE_PRODUCTION_ACCESS_TOKEN:-}}"
    LOCATION_ID="${SQUARE_LOCATION_ID:-${SQUARE_PRODUCTION_LOCATION_ID:-}}"
    APPLICATION_ID="${SQUARE_APPLICATION_ID:-${SQUARE_PRODUCTION_APPLICATION_ID:-}}"
    ;;
  *)
    SQUARE_BASE_URL="https://connect.squareupsandbox.com"
    ACCESS_TOKEN="${SQUARE_ACCESS_TOKEN:-${SQUARE_SANDBOX_ACCESS_TOKEN:-}}"
    LOCATION_ID="${SQUARE_LOCATION_ID:-${SQUARE_SANDBOX_LOCATION_ID:-}}"
    APPLICATION_ID="${SQUARE_APPLICATION_ID:-${SQUARE_SANDBOX_APPLICATION_ID:-}}"
    ;;
esac

if [ -z "$APPLICATION_ID" ]; then
  echo "fatal: Square application id missing for SQUARE_ENVIRONMENT=$SQUARE_ENVIRONMENT" >&2
  exit 2
fi
if [ -z "$EXPECTED_APPLICATION_ID" ]; then
  echo "fatal: BLUEY_SQUARE_APPLICATION_ID_EXPECTED is required before branding checkout" >&2
  exit 2
fi
if [ "$APPLICATION_ID" != "$EXPECTED_APPLICATION_ID" ]; then
  echo "fatal: refusing to brand checkout for Square app $APPLICATION_ID; expected Bluey app $EXPECTED_APPLICATION_ID" >&2
  exit 2
fi
if [ -z "$ACCESS_TOKEN" ]; then
  echo "fatal: Square access token missing for SQUARE_ENVIRONMENT=$SQUARE_ENVIRONMENT" >&2
  exit 2
fi
if [ -z "$LOCATION_ID" ]; then
  echo "fatal: Square location id missing for SQUARE_ENVIRONMENT=$SQUARE_ENVIRONMENT" >&2
  exit 2
fi
IFS="," read -r -a forbidden_locations <<<"$FORBIDDEN_LOCATION_IDS"
for forbidden_location in "${forbidden_locations[@]}"; do
  forbidden_location="${forbidden_location#"${forbidden_location%%[![:space:]]*}"}"
  forbidden_location="${forbidden_location%"${forbidden_location##*[![:space:]]}"}"
  if [ -n "$forbidden_location" ] && [ "$LOCATION_ID" = "$forbidden_location" ]; then
    echo "fatal: refusing to brand Square location $LOCATION_ID for Bluey; it is reserved for another product" >&2
    exit 2
  fi
done
if ! command -v curl >/dev/null 2>&1; then
  echo "fatal: curl is required" >&2
  exit 2
fi
if ! command -v python3 >/dev/null 2>&1; then
  echo "fatal: python3 is required" >&2
  exit 2
fi

DESIRED_NAME="${BLUEY_SQUARE_BUSINESS_NAME:-Bluey}"
DESIRED_WEBSITE="${BLUEY_SQUARE_WEBSITE_URL:-${BLUEY_PUBLIC_URL:-https://bluey.sh}}"
DESIRED_HEADER_TYPE="${BLUEY_SQUARE_HEADER_TYPE:-BUSINESS_NAME}"
DESIRED_BUTTON_COLOR="${BLUEY_SQUARE_BUTTON_COLOR:-#20c7ff}"
DESIRED_BUTTON_SHAPE="${BLUEY_SQUARE_BUTTON_SHAPE:-ROUNDED}"

tmp="$(mktemp)"
cleanup() {
  rm -f "$tmp"
}
trap cleanup EXIT

square_curl() {
  local method="$1"
  local path="$2"
  local payload="${3:-}"
  if [ -n "$payload" ]; then
    curl -fsS -X "$method" \
      -H "Authorization: Bearer $ACCESS_TOKEN" \
      -H "Square-Version: $SQUARE_API_VERSION" \
      -H "Content-Type: application/json" \
      "$SQUARE_BASE_URL$path" \
      -d "$payload"
  else
    curl -fsS -X "$method" \
      -H "Authorization: Bearer $ACCESS_TOKEN" \
      -H "Square-Version: $SQUARE_API_VERSION" \
      "$SQUARE_BASE_URL$path"
  fi
}

if [ "$MODE" = "apply" ]; then
  python3 - <<'PY' "$DESIRED_NAME" "$DESIRED_WEBSITE" >"$tmp"
import json
import sys

name = sys.argv[1]
website = sys.argv[2]
payload = {"location": {"name": name, "business_name": name}}
if website:
    payload["location"]["website_url"] = website
print(json.dumps(payload, separators=(",", ":")))
PY
  square_curl "PUT" "/v2/locations/$LOCATION_ID" "$(cat "$tmp")" >/dev/null

  if [ "$DESIRED_HEADER_TYPE" = "FRAMED_LOGO" ]; then
    location_for_logo_json="$(square_curl "GET" "/v2/locations/$LOCATION_ID")"
    python3 - <<'PY' "$location_for_logo_json"
import json
import sys

location = json.loads(sys.argv[1]).get("location", {})
if not (location.get("logo_url") or location.get("full_format_logo_url")):
    raise SystemExit(
        "fatal: Square FRAMED_LOGO checkout needs a location logo; "
        "upload the Bluey logo in Square Dashboard before switching header type"
    )
PY
  fi

  python3 - <<'PY' "$DESIRED_HEADER_TYPE" "$DESIRED_BUTTON_COLOR" "$DESIRED_BUTTON_SHAPE" >"$tmp"
import json
import sys

header_type, button_color, button_shape = sys.argv[1:4]
print(json.dumps({
    "location_settings": {
        "branding": {
            "header_type": header_type,
            "button_color": button_color,
            "button_shape": button_shape,
        }
    }
}, separators=(",", ":")))
PY
  square_curl "PUT" "/v2/online-checkout/location-settings/$LOCATION_ID" "$(cat "$tmp")" >/dev/null
fi

location_json="$(square_curl "GET" "/v2/locations/$LOCATION_ID")"
settings_json="$(square_curl "GET" "/v2/online-checkout/location-settings/$LOCATION_ID")"

python3 - <<'PY' "$MODE" "$DESIRED_NAME" "$DESIRED_WEBSITE" "$DESIRED_HEADER_TYPE" "$DESIRED_BUTTON_COLOR" "$DESIRED_BUTTON_SHAPE" "$location_json" "$settings_json"
import json
import sys

mode, desired_name, desired_website, desired_header, desired_color, desired_shape, location_raw, settings_raw = sys.argv[1:]
location = json.loads(location_raw).get("location", {})
settings = json.loads(settings_raw).get("location_settings", {})
branding = settings.get("branding", {})

problems = []
if location.get("name") != desired_name:
    problems.append(f"location.name is {location.get('name')!r}, expected {desired_name!r}")
if location.get("business_name") != desired_name:
    problems.append(f"location.business_name is {location.get('business_name')!r}, expected {desired_name!r}")
if desired_website and location.get("website_url") != desired_website:
    problems.append(f"location.website_url is {location.get('website_url')!r}, expected {desired_website!r}")
if branding.get("header_type") != desired_header:
    problems.append(f"branding.header_type is {branding.get('header_type')!r}, expected {desired_header!r}")
if desired_header == "FRAMED_LOGO" and not (location.get("logo_url") or location.get("full_format_logo_url")):
    problems.append("location logo is missing; upload the Bluey logo in Square Dashboard before using FRAMED_LOGO")
if (branding.get("button_color") or "").lower() != desired_color.lower():
    problems.append(f"branding.button_color is {branding.get('button_color')!r}, expected {desired_color!r}")
if branding.get("button_shape") != desired_shape:
    problems.append(f"branding.button_shape is {branding.get('button_shape')!r}, expected {desired_shape!r}")

if problems:
    for problem in problems:
        print(f"fail: {problem}", file=sys.stderr)
    sys.exit(1)

verb = "verified" if mode == "check" else "applied"
print(
    f"ok: Square checkout branding {verb}: "
    f"name={desired_name}, header={desired_header}, button={desired_color}/{desired_shape}"
)
PY
