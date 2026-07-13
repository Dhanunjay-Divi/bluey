#!/usr/bin/env bash
set -euo pipefail

ORIGIN_IP="${1:-${BLUEY_ORIGIN_IP:-}}"
if [ -z "$ORIGIN_IP" ]; then
    echo "usage: $0 <historical-origin-ip>" >&2
    exit 2
fi

BASE="${BLUEY_PUBLIC_ORIGIN:-https://bluey.sh}"
CURL="${CURL_BIN:-curl}"

expect_status() {
    local expected="$1"
    local url="$2"
    shift 2
    local actual
    actual="$($CURL -sS -o /dev/null -w '%{http_code}' "$@" "$url")"
    if [ "$actual" != "$expected" ]; then
        echo "FAIL $url expected=$expected actual=$actual" >&2
        return 1
    fi
    echo "PASS $url status=$actual"
}

expect_status 200 "$BASE/robots.txt"
expect_status 410 "$BASE/llms.txt"
expect_status 308 "$BASE/JobApply"
expect_status 404 "$BASE/jobs/assets/does-not-exist.js.map"
expect_status 404 "$BASE/api/jobs/internal/discovery/lease" -X POST
expect_status 401 "$BASE/api/jobs/workspace"

robots_type="$($CURL -sSI "$BASE/robots.txt" | tr -d '\r' | awk 'BEGIN{IGNORECASE=1} /^content-type:/{print tolower($0)}')"
case "$robots_type" in
    *text/plain*) echo "PASS robots.txt content type" ;;
    *) echo "FAIL robots.txt content type: $robots_type" >&2; exit 1 ;;
esac

jobs_robots="$($CURL -sSI "$BASE/jobs/" | tr -d '\r' | awk 'BEGIN{IGNORECASE=1} /^x-robots-tag:/{print tolower($0)}')"
case "$jobs_robots" in
    *noindex*nofollow*) echo "PASS Jobs X-Robots-Tag" ;;
    *) echo "FAIL Jobs X-Robots-Tag: $jobs_robots" >&2; exit 1 ;;
esac

for jobs_path in /jobs/ /jobs/applications; do
    jobs_cache="$($CURL -sSI "$BASE$jobs_path" | tr -d '\r' | awk 'BEGIN{IGNORECASE=1} /^cache-control:/{print tolower($0)}')"
    case "$jobs_cache" in
        *no-cache*) echo "PASS $jobs_path Cache-Control" ;;
        *) echo "FAIL $jobs_path Cache-Control: $jobs_cache" >&2; exit 1 ;;
    esac
done

jobs_asset="$($CURL -sS "$BASE/jobs/" | grep -Eo '/jobs/assets/[^" ]+-[A-Za-z0-9_-]{8}\.(js|css)' | head -1 || true)"
if [ -z "$jobs_asset" ]; then
    echo "FAIL could not find a hashed Jobs asset" >&2
    exit 1
fi
expect_status 200 "$BASE$jobs_asset"
jobs_asset_cache="$($CURL -sSI "$BASE$jobs_asset" | tr -d '\r' | awk 'BEGIN{IGNORECASE=1} /^cache-control:/{print tolower($0)}')"
case "$jobs_asset_cache" in
    *public*max-age=31536000*immutable*) echo "PASS Jobs asset Cache-Control" ;;
    *) echo "FAIL Jobs asset Cache-Control: $jobs_asset_cache" >&2; exit 1 ;;
esac

gpt_status="$($CURL -sS -o /dev/null -w '%{http_code}' -A GPTBot "$BASE/")"
if [ "$gpt_status" = "200" ]; then
    echo "FAIL GPTBot still receives 200" >&2
    exit 1
fi
echo "PASS GPTBot blocked status=$gpt_status"

if $CURL -kfsS --connect-timeout 5 --resolve "bluey.sh:443:$ORIGIN_IP" "https://bluey.sh/health" >/dev/null; then
    echo "FAIL direct HTTPS origin bypass still succeeds" >&2
    exit 1
fi
echo "PASS direct HTTPS origin bypass blocked"

direct_http_headers="$(
    $CURL -sS --connect-timeout 5 -D - -o /dev/null \
        -H 'Host: bluey.sh' "http://$ORIGIN_IP/health" 2>/dev/null || true
)"
if [ -z "$direct_http_headers" ]; then
    echo "PASS direct HTTP origin bypass blocked"
else
    direct_http_status="$(printf '%s\n' "$direct_http_headers" | tr -d '\r' | awk 'NR == 1 { print $2 }')"
    direct_http_location="$(printf '%s\n' "$direct_http_headers" | tr -d '\r' | awk 'tolower($1) == "location:" {print $2; exit}')"
    if [ "$direct_http_status" != "308" ] || [ "$direct_http_location" != "https://bluey.sh/health" ]; then
        echo "FAIL direct HTTP origin exposed content or an unsafe redirect status=$direct_http_status location=$direct_http_location" >&2
        exit 1
    fi
    echo "PASS direct HTTP origin is redirect-only for certificate renewal"
fi

echo "Bluey live edge verification passed"
