#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="${1:-}"

if [[ -n "${ENV_FILE}" ]]; then
  if [[ ! -f "${ENV_FILE}" ]]; then
    echo "fail: env file not found: ${ENV_FILE}" >&2
    exit 1
  fi
  set -a
  # shellcheck disable=SC1090
  source "${ENV_FILE}"
  set +a
fi

strict="${BLUEY_SCALABLE_STRICT:-0}"
failures=0
warnings=0

ok() {
  printf 'ok: %s\n' "$1"
}

warn() {
  warnings=$((warnings + 1))
  printf 'warn: %s\n' "$1"
}

fail() {
  failures=$((failures + 1))
  printf 'fail: %s\n' "$1"
}

require_or_warn() {
  local condition="$1"
  local message="$2"
  if [[ "${condition}" == "1" ]]; then
    ok "${message}"
  elif [[ "${strict}" == "1" ]]; then
    fail "${message}"
  else
    warn "${message}"
  fi
}

echo "Bluey scalable architecture readiness"
echo "root: ${ROOT}"

if [[ -x "${ROOT}/scripts/check-server-sqlite-boundary.sh" ]]; then
  if "${ROOT}/scripts/check-server-sqlite-boundary.sh" >/tmp/bluey-sqlite-boundary.$$ 2>&1; then
    ok "server SQLite usage is isolated behind the DB layer"
  else
    cat /tmp/bluey-sqlite-boundary.$$ >&2
    fail "server still has direct SQLite usage outside the DB layer"
  fi
  rm -f /tmp/bluey-sqlite-boundary.$$
else
  warn "SQLite boundary check script is missing or not executable"
fi

if grep -q 'BLUEY_SERVER_DB_BACKEND=postgres is provisioned but this bluey-server build still uses the SQLite runtime adapter' "${ROOT}/server/src/main.rs"; then
  require_or_warn 0 "Postgres runtime adapter refusal guard is still present; this build cannot run postgres mode"
else
  ok "server binary no longer has the postgres-mode refusal guard"
fi

if grep -Eq 'tokio-postgres|sqlx|deadpool-postgres|postgres' "${ROOT}/server/Cargo.toml"; then
  ok "server has a Postgres-capable Rust dependency"
else
  require_or_warn 0 "server has no Postgres runtime dependency yet"
fi

if grep -q 'pub fn run_blocking_db' "${ROOT}/server/src/db/mod.rs" \
  && grep -q 'Postgres DB access must run inside db::run_blocking_db' "${ROOT}/server/src/db/mod.rs"; then
  ok "Postgres adapter has an enforced Tokio blocking boundary"
else
  require_or_warn 0 "Postgres adapter blocking boundary is missing"
fi

if [[ "${BLUEY_SERVER_DB_BACKEND:-}" == "postgres" && -n "${BLUEY_DATABASE_URL:-}" ]]; then
  ok "Postgres backend env is configured"
  if command -v psql >/dev/null 2>&1; then
    if psql "${BLUEY_DATABASE_URL}" -c 'select 1;' >/dev/null 2>&1; then
      ok "Postgres connection works"
    else
      require_or_warn 0 "Postgres env is set but psql connection failed"
    fi
  else
    warn "psql is not installed here, skipping live Postgres connection check"
  fi
else
  require_or_warn 0 "Postgres backend env is not fully configured"
fi

if [[ -n "${BLUEY_REDIS_URL:-}" ]]; then
  if [[ "${BLUEY_REDIS_URL}" == redis://127.0.0.1* || "${BLUEY_REDIS_URL}" == redis://localhost* ]]; then
    require_or_warn 0 "Redis is configured but local-only; acceptable for one-server alpha only"
  else
    ok "Redis/Valkey URL is configured for shared capacity state"
  fi
else
  require_or_warn 0 "BLUEY_REDIS_URL is missing"
fi

if [[ "${BLUEY_RATE_LIMIT_REDIS_STRICT:-0}" == "1" ]]; then
  ok "Redis strict mode is enabled"
else
  warn "Redis strict mode is not enabled; realtime fallback stays available if Redis fails"
fi

r2_ready=0
if [[ -n "${BLUEY_BACKUP_S3_BUCKET:-}" || -n "${BLUEY_R2_BUCKET:-}" || -n "${OFFSITE_DESTINATION:-}" ]]; then
  r2_ready=1
fi
if [[ "${r2_ready}" == "1" ]]; then
  ok "R2/S3-compatible off-host storage env is configured"
else
  require_or_warn 0 "R2/S3-compatible off-host storage env is missing"
fi

provider_ready=1
for name in OPENAI_API_KEYS ANTHROPIC_API_KEYS DEEPGRAM_API_KEYS; do
  if [[ -z "${!name:-}" ]]; then
    warn "${name} is missing from this environment"
    provider_ready=0
  fi
done
if [[ -z "${GEMINI_API_KEYS:-}" && -z "${GEMINI_API_KEY:-}" ]]; then
  warn "GEMINI_API_KEYS/GEMINI_API_KEY is missing from this environment"
  provider_ready=0
fi
if [[ -z "${ZAI_API_KEYS:-}" && -z "${ZAI_API_KEY:-}" ]]; then
  warn "ZAI_API_KEYS/ZAI_API_KEY is missing; GLM routes will be skipped"
else
  ok "Z.AI GLM server-side provider env is present"
fi
if [[ -z "${DEEPSEEK_API_KEYS:-}" && -z "${DEEPSEEK_API_KEY:-}" ]]; then
  warn "DEEPSEEK_API_KEYS/DEEPSEEK_API_KEY is missing; DeepSeek routes will be skipped"
else
  ok "DeepSeek server-side provider env is present"
fi
if [[ "${provider_ready}" == "1" ]]; then
  ok "OpenAI, Anthropic, Gemini, and Deepgram server-side provider env is present"
fi

if grep -q 'provider: "deepgram"' "${ROOT}/server/src/pricing/mod.rs"; then
  ok "Deepgram STT pricing is part of the billing table"
else
  fail "Deepgram STT pricing is missing from the billing table"
fi

echo
if [[ "${failures}" -gt 0 ]]; then
  echo "Result: NOT READY (${failures} failure(s), ${warnings} warning(s))"
  exit 1
fi

if [[ "${warnings}" -gt 0 ]]; then
  echo "Result: ALPHA-READY WITH WARNINGS (${warnings} warning(s))"
else
  echo "Result: READY"
fi
