#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DIRECT="$ROOT/ops/bluey-jobs-discovery.service.example"
GLOBAL="$ROOT/ops/bluey-jobs-global-discovery.service.example"
HEALTH_SERVICE="$ROOT/ops/bluey-jobs-discovery-health.service.example"
HEALTH_TIMER="$ROOT/ops/bluey-jobs-discovery-health.timer.example"

fail() {
    echo "test-bluey-jobs-discovery-units: $*" >&2
    exit 1
}

for unit in "$DIRECT" "$GLOBAL"; do
    grep -qx 'Restart=always' "$unit" ||
        fail "$(basename "$unit") must always restart"
    grep -qx 'WantedBy=multi-user.target' "$unit" ||
        fail "$(basename "$unit") must remain enabled independently"
    if grep -Eq '^(PartOf|Requires)=bluey-jobs-api\.service$' "$unit"; then
        fail "$(basename "$unit") must not stop with API maintenance"
    fi
done

grep -qx 'ExecStart=/usr/local/sbin/check-bluey-jobs-discovery.sh' "$HEALTH_SERVICE" ||
    fail "health service must call the installed checker"
grep -qx 'OnUnitActiveSec=15min' "$HEALTH_TIMER" ||
    fail "health timer must run every 15 minutes"
grep -qx 'Persistent=true' "$HEALTH_TIMER" ||
    fail "health timer must catch up after downtime"

echo "test-bluey-jobs-discovery-units: PASS"
