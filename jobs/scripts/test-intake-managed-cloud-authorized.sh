#!/usr/bin/env bash
# Offline contract tests for intake-managed-cloud-authorized.sh.
set -euo pipefail
IFS=$'\n\t'
umask 077

root="$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd -P)"
bridge="$root/jobs/scripts/intake-managed-cloud-authorized.sh"
real_node="$(command -v node)"
tmp="$(mktemp -d "${TMPDIR:-/tmp}/jobs-intake-bridge-test.XXXXXX")"
trap 'rm -rf -- "$tmp"' EXIT HUP INT TERM
mkdir -p "$tmp/bin" "$tmp/runner-temp"

cp /dev/stdin "$tmp/bin/node" <<'NODE'
#!/usr/bin/env bash
set -euo pipefail
[[ -z "${GH_TOKEN+x}" ]] || exit 91
if [[ "${2-}" == "publication-api-capture" ]]; then
  out=""
  for ((i=1; i <= $#; i++)); do
    if [[ "${!i}" == "--out" ]]; then j=$((i + 1)); out="${!j}"; fi
  done
  [[ -n "$out" ]] || exit 2
  cat > "$out"
  exit 0
fi
if [[ "${2-}" == "authorization-artifact-select" || "${2-}" == "candidate-artifact-select" ]]; then
  if [[ "${2-}" == "candidate-artifact-select" ]]; then
    for argument in "$@"; do
      case "$argument" in
        --authorization-workflow-commit|--candidate-source-commit) exit 92 ;;
      esac
    done
  fi
  printf '%s\n' '{"artifactId":77,"artifactSizeBytes":3,"artifactDigest":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}'
  exit 0
fi
if [[ "${2-}" == "authorization-intake" || "${2-}" == "candidate-intake" ]]; then
  if [[ "${2-}" == "candidate-intake" ]]; then
    for argument in "$@"; do
      case "$argument" in
        --authorization-workflow-commit|--candidate-source-commit|--root-anchor-sha256|--approved-observability-policy-sha256) exit 92 ;;
      esac
    done
  fi
  printf '%s\n' "${2-}" >> "$TEST_LOG"
  printf '%s\n' "{\"audience\":\"bluey-jobs-managed-cloud-authorization-intake-v1\",\"root\":\"$TEST_ROOT\"}"
  exit 0
fi
exec "$REAL_NODE" "$@"
NODE
chmod 700 "$tmp/bin/node"

cp /dev/stdin "$tmp/bin/gh" <<'GH'
#!/usr/bin/env bash
set -euo pipefail
endpoint="${@: -1}"
[[ "${1-}" == api && "${2-}" == --hostname && "${3-}" == github.com && "${4-}" == --method && "${5-}" == GET ]] || exit 61
printf '%s\n' "$endpoint" >> "$TEST_LOG"
case "$endpoint" in
  */artifacts/77/zip)
    printf '%s' "${TEST_ARCHIVE_BODY:-abc}"
    [[ "${TEST_FAIL_ZIP:-}" != 1 ]] || exit 23
    ;;
  *)
    if [[ "${TEST_FAIL_RUN:-}" == 1 && "$endpoint" == *"/attempts/2" ]]; then
      printf '{}'
      exit 23
    fi
    printf '{}\n'
    ;;
esac
GH
chmod 700 "$tmp/bin/gh"

export PATH="$tmp/bin:$PATH" REAL_NODE="$real_node" TEST_LOG="$tmp/log" TEST_ROOT="$tmp/accepted" \
  GITHUB_WORKSPACE="$root" GITHUB_REPOSITORY="fixture/repository" GITHUB_REPOSITORY_ID=611 \
  RUNNER_TEMP="$tmp/runner-temp" GH_TOKEN=synthetic DEFAULT_BRANCH=main AUTHORIZED_RUN_ID=901 \
  AUTHORIZED_RUN_ATTEMPT=2 AUTHORIZATION_WORKFLOW_COMMIT=1111111111111111111111111111111111111111 \
  CANDIDATE_SOURCE_COMMIT=2222222222222222222222222222222222222222 \
  APPROVED_OBSERVABILITY_POLICY_SHA256=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa \
  ROOT_ANCHOR_SHA256=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb \
  AUTHORIZED_ARCHIVE_MAX_BYTES=8 AUTHORIZED_MAX_UNPACKED_BYTES=8 AUTHORIZED_RESERVE_BYTES=1

happy="$tmp/happy.json"
bash "$bridge" > "$happy"
[[ "$(cat "$TEST_LOG")" == "$(printf '%s\n' \
  '/repos/fixture/repository/actions/runs/901/attempts/2' \
  '/repos/fixture/repository/actions/runs/901/attempts/2/jobs?per_page=100' \
  '/repos/fixture/repository/actions/runs/901/artifacts?per_page=100' \
  '/repos/fixture/repository/actions/artifacts/77/zip' \
  'authorization-intake')" ]]
grep -qF "$TEST_ROOT" "$happy"
[[ "$(find "$tmp/runner-temp" -name authorization.zip -print | wc -l | tr -d '[:space:]')" == 0 ]]

: > "$TEST_LOG"
candidate="$tmp/candidate.json"
env -u AUTHORIZATION_WORKFLOW_COMMIT -u CANDIDATE_SOURCE_COMMIT \
  -u APPROVED_OBSERVABILITY_POLICY_SHA256 -u ROOT_ANCHOR_SHA256 \
  CANDIDATE_RUN_ID=902 CANDIDATE_RUN_ATTEMPT=3 bash "$bridge" candidate > "$candidate"
[[ "$(cat "$TEST_LOG")" == "$(printf '%s\n' \
  '/repos/fixture/repository/actions/runs/902/attempts/3' \
  '/repos/fixture/repository/actions/runs/902/attempts/3/jobs?per_page=100' \
  '/repos/fixture/repository/actions/runs/902/artifacts?per_page=100' \
  '/repos/fixture/repository/actions/artifacts/77/zip' \
  'candidate-intake')" ]]
grep -qF "$TEST_ROOT" "$candidate"
[[ "$(find "$tmp/runner-temp" -name candidate.zip -print | wc -l | tr -d '[:space:]')" == 0 ]]

: > "$TEST_LOG"
if env -u AUTHORIZATION_WORKFLOW_COMMIT -u CANDIDATE_SOURCE_COMMIT \
  -u APPROVED_OBSERVABILITY_POLICY_SHA256 -u ROOT_ANCHOR_SHA256 \
  CANDIDATE_RUN_ID=902 CANDIDATE_RUN_ATTEMPT=3 TEST_FAIL_ZIP=1 \
  bash "$bridge" candidate > "$tmp/candidate-failed-zip.out" 2> "$tmp/candidate-failed-zip.err"; then
  echo "candidate failed ZIP producer unexpectedly succeeded" >&2
  exit 1
fi
[[ ! -s "$tmp/candidate-failed-zip.out" ]]
! grep -qxF candidate-intake "$TEST_LOG"

: > "$TEST_LOG"
if TEST_FAIL_RUN=1 bash "$bridge" > "$tmp/failed.out" 2> "$tmp/failed.err"; then
  echo "failed producer unexpectedly succeeded" >&2
  exit 1
fi
[[ ! -s "$tmp/failed.out" ]]
! grep -qxF intake "$TEST_LOG"

: > "$TEST_LOG"
if TEST_FAIL_ZIP=1 bash "$bridge" > "$tmp/failed-zip.out" 2> "$tmp/failed-zip.err"; then
  echo "byte-complete failed ZIP producer unexpectedly succeeded" >&2
  exit 1
fi
[[ ! -s "$tmp/failed-zip.out" ]]
! grep -qxF intake "$TEST_LOG"

: > "$TEST_LOG"
if TEST_ARCHIVE_BODY=abcd bash "$bridge" > "$tmp/oversized.out" 2> "$tmp/oversized.err"; then
  echo "oversized archive unexpectedly succeeded" >&2
  exit 1
fi
[[ ! -s "$tmp/oversized.out" ]]
! grep -qxF intake "$TEST_LOG"
printf '%s\n' "intake-managed-cloud-authorized offline tests passed"
