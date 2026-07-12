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

if $CURL -fsS --connect-timeout 5 -H 'Host: bluey.sh' "http://$ORIGIN_IP/health" >/dev/null; then
    echo "FAIL direct HTTP origin bypass still succeeds" >&2
    exit 1
fi
echo "PASS direct HTTP origin bypass blocked"

echo "Bluey live edge verification passed"
