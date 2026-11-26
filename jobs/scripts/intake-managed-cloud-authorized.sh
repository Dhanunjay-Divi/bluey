#!/usr/bin/env bash
# Fetch and stage one already-authorized managed-cloud release artifact.
set -euo pipefail
IFS=$'\n\t'
umask 077
export GH_PROMPT_DISABLED=1 GH_NO_UPDATE_NOTIFIER=1 GH_NO_EXTENSION_UPDATE_NOTIFIER=1

fail() { printf '%s\n' "authorization intake refused: $*" >&2; exit 1; }
require_env() {
  local name="$1" value="${!1-}"
  [[ -n "$value" && "$value" != *$'\n'* && "$value" != *$'\r'* ]] || fail "missing or malformed ${name}"
}
require_decimal() {
  local name="$1" value="${!1-}"
  [[ "$value" =~ ^(0|[1-9][0-9]*)$ ]] || fail "${name} must be a canonical decimal"
}

[[ "$#" -le 1 ]] || fail "usage: intake-managed-cloud-authorized.sh [candidate]"
mode="${1-authorized}"
case "$mode" in
  authorized)
    run_id_var=AUTHORIZED_RUN_ID
    run_attempt_var=AUTHORIZED_RUN_ATTEMPT
    select_command=authorization-artifact-select
    intake_command=authorization-intake
    ;;
  candidate)
    run_id_var=CANDIDATE_RUN_ID
    run_attempt_var=CANDIDATE_RUN_ATTEMPT
    select_command=candidate-artifact-select
    intake_command=candidate-intake
    ;;
  *) fail "usage: intake-managed-cloud-authorized.sh [candidate]" ;;
esac

for required in GITHUB_WORKSPACE GITHUB_REPOSITORY GITHUB_REPOSITORY_ID RUNNER_TEMP GH_TOKEN DEFAULT_BRANCH "$run_id_var" "$run_attempt_var" AUTHORIZED_ARCHIVE_MAX_BYTES AUTHORIZED_MAX_UNPACKED_BYTES AUTHORIZED_RESERVE_BYTES; do require_env "$required"; done
if [[ "$mode" == authorized ]]; then
  for required in AUTHORIZATION_WORKFLOW_COMMIT CANDIDATE_SOURCE_COMMIT APPROVED_OBSERVABILITY_POLICY_SHA256 ROOT_ANCHOR_SHA256; do require_env "$required"; done
fi
[[ "$GITHUB_REPOSITORY" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] || fail "malformed repository"
[[ "$DEFAULT_BRANCH" =~ ^[A-Za-z0-9._/-]+$ && "$DEFAULT_BRANCH" != -* ]] || fail "malformed default branch"
if [[ "$mode" == authorized ]]; then
  [[ "$AUTHORIZATION_WORKFLOW_COMMIT" =~ ^[0-9a-f]{40}$ ]] || fail "malformed authorization workflow commit"
  [[ "$CANDIDATE_SOURCE_COMMIT" =~ ^[0-9a-f]{40}$ ]] || fail "malformed candidate source commit"
  [[ "$APPROVED_OBSERVABILITY_POLICY_SHA256" =~ ^[0-9a-f]{64}$ ]] || fail "malformed observability policy digest"
  [[ "$ROOT_ANCHOR_SHA256" =~ ^[0-9a-f]{64}$ ]] || fail "malformed root anchor digest"
fi
for decimal in GITHUB_REPOSITORY_ID "$run_id_var" "$run_attempt_var" AUTHORIZED_ARCHIVE_MAX_BYTES AUTHORIZED_MAX_UNPACKED_BYTES AUTHORIZED_RESERVE_BYTES; do require_decimal "$decimal"; done
run_id="${!run_id_var}"
run_attempt="${!run_attempt_var}"
[[ "$GITHUB_REPOSITORY_ID" != 0 && "$run_id" != 0 && "$run_attempt" != 0 && "$AUTHORIZED_ARCHIVE_MAX_BYTES" != 0 && "$AUTHORIZED_MAX_UNPACKED_BYTES" != 0 && "$AUTHORIZED_RESERVE_BYTES" != 0 ]] || fail "zero-valued intake setting"

# Bash arithmetic is not a safe authority parser for untrusted-sized decimals.
# Reject overflow before the first remote request.
env -u GH_TOKEN node - "$GITHUB_REPOSITORY_ID" "$run_id" "$run_attempt" "$AUTHORIZED_ARCHIVE_MAX_BYTES" "$AUTHORIZED_MAX_UNPACKED_BYTES" "$AUTHORIZED_RESERVE_BYTES" <<'NODE'
for (const value of process.argv.slice(2)) {
  if (!/^(?:0|[1-9][0-9]*)$/.test(value) || BigInt(value) > BigInt(Number.MAX_SAFE_INTEGER)) {
    process.exit(1);
  }
}
NODE
[[ "$?" == 0 ]] || fail "numeric authorization setting is out of range"

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)"
repository_root="$(CDPATH= cd -- "$script_dir/../.." && pwd -P)"
workspace_root="$(CDPATH= cd -- "$GITHUB_WORKSPACE" 2>/dev/null && pwd -P)" || fail "workspace is not a physical directory"
runner_temp="$(CDPATH= cd -- "$RUNNER_TEMP" 2>/dev/null && pwd -P)" || fail "runner temp is not a physical directory"
[[ "$workspace_root" == "$repository_root" ]] || fail "script must run from the trusted workspace"
[[ -f "$repository_root/jobs/scripts/managed-cloud-release-gate.mjs" ]] || fail "release gate is unavailable"

scratch="$(mktemp -d "$runner_temp/jobs-authorization-intake.XXXXXX")" || fail "unable to create private scratch"
env -u GH_TOKEN node - "$scratch" <<'NODE'
const fs = require("fs");
const path = process.argv[2];
const stat = fs.lstatSync(path, { bigint: true });
if (!stat.isDirectory() || stat.uid !== BigInt(process.getuid()) || (stat.mode & 0o777n) !== 0o700n || fs.realpathSync(path) !== path) {
  process.exit(1);
}
NODE
[[ "$?" == 0 ]] || fail "new scratch is not a private physical directory"
# Preserve all scratch on any interruption or refusal for bounded runner-temporary
# forensics.  The exact accepted archive is removed only after a clean intake.
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

safe_node() { env -u GH_TOKEN node "$@"; }

capture_api() {
  local endpoint="$1" kind="$2" output="$3"
  if ! gh api --hostname github.com --method GET "$endpoint" | safe_node "$repository_root/jobs/scripts/managed-cloud-release-gate.mjs" publication-api-capture --kind "$kind" --out "$output"; then
    rm -f -- "$output" "$output.partial"
    fail "${kind} API capture producer failed"
  fi
}

run_capture="$scratch/run.json"
jobs_capture="$scratch/jobs.json"
artifacts_capture="$scratch/artifacts.json"
capture_api "/repos/$GITHUB_REPOSITORY/actions/runs/$run_id/attempts/$run_attempt" run "$run_capture"
capture_api "/repos/$GITHUB_REPOSITORY/actions/runs/$run_id/attempts/$run_attempt/jobs?per_page=100" jobs "$jobs_capture"
capture_api "/repos/$GITHUB_REPOSITORY/actions/runs/$run_id/artifacts?per_page=100" artifacts "$artifacts_capture"

selected="$scratch/selected.json"
select_args=("$select_command" --run "$run_capture" --jobs "$jobs_capture" --artifacts "$artifacts_capture" --artifact-max-bytes "$AUTHORIZED_ARCHIVE_MAX_BYTES" --default-branch "$DEFAULT_BRANCH" --repository "$GITHUB_REPOSITORY" --repository-id "$GITHUB_REPOSITORY_ID" --run-id "$run_id" --run-attempt "$run_attempt")
if [[ "$mode" == authorized ]]; then
  select_args+=(--authorization-workflow-commit "$AUTHORIZATION_WORKFLOW_COMMIT" --candidate-source-commit "$CANDIDATE_SOURCE_COMMIT")
fi
safe_node "$repository_root/jobs/scripts/managed-cloud-release-gate.mjs" "${select_args[@]}" > "$selected"

selection_fields="$scratch/selection.fields"
safe_node - "$selected" "$AUTHORIZED_ARCHIVE_MAX_BYTES" "$AUTHORIZED_MAX_UNPACKED_BYTES" "$AUTHORIZED_RESERVE_BYTES" "$scratch" "$selection_fields" <<'NODE'
const fs = require("fs");
const [selectedPath, archiveMaxText, unpackedText, reserveText, scratch, output] = process.argv.slice(2);
const decimal = (value, label, minimum) => {
  if (!/^(?:0|[1-9][0-9]*)$/.test(value)) throw new Error(`${label} is not canonical`);
  const parsed = BigInt(value);
  if (parsed < BigInt(minimum) || parsed > BigInt(Number.MAX_SAFE_INTEGER)) throw new Error(`${label} is out of range`);
  return parsed;
};
const selected = JSON.parse(fs.readFileSync(selectedPath, "utf8"));
const id = decimal(String(selected.artifactId), "artifact ID", 1);
const size = decimal(String(selected.artifactSizeBytes), "artifact size", 1);
const archiveMax = decimal(archiveMaxText, "archive cap", 1);
const unpacked = decimal(unpackedText, "unpacked cap", 1);
const reserve = decimal(reserveText, "reserve", 0);
if (size > archiveMax || typeof selected.artifactDigest !== "string" || !/^sha256:[0-9a-f]{64}$/.test(selected.artifactDigest)) throw new Error("selected artifact is malformed or exceeds its configured cap");
const stat = fs.statfsSync(scratch, { bigint: true });
// Reserve the one-byte overflow witness plus the stager's 1 MiB margin.
if (stat.bavail * stat.bsize < size + unpacked + reserve + 1_048_577n) {
  throw new Error("insufficient disk headroom for selected authorization archive");
}
fs.writeFileSync(output, `${id}\n${size}\n${selected.artifactDigest}\n`, { encoding: "utf8", flag: "wx", mode: 0o600 });
NODE

artifact_id="$(sed -n '1p' "$selection_fields")"
artifact_size="$(sed -n '2p' "$selection_fields")"
artifact_digest="$(sed -n '3p' "$selection_fields")"
[[ "$artifact_id" =~ ^[1-9][0-9]*$ && "$artifact_size" =~ ^[1-9][0-9]*$ && "$artifact_digest" =~ ^sha256:[0-9a-f]{64}$ ]] || fail "selected artifact fields are malformed"
download_limit="$(safe_node -e 'const n=BigInt(process.argv[1]); if(n>=BigInt(Number.MAX_SAFE_INTEGER)) process.exit(1); process.stdout.write(String(n+1n))' "$artifact_size")" || fail "selected artifact download bound is invalid"

archive="$scratch/${mode}.zip"
if ! gh api --hostname github.com --method GET "/repos/$GITHUB_REPOSITORY/actions/artifacts/$artifact_id/zip" | head -c "$download_limit" > "$archive"; then rm -f -- "$archive"; fail "selected authorization artifact producer failed or exceeded its bound"; fi
actual_size="$(wc -c < "$archive" | tr -d '[:space:]')"
[[ "$actual_size" == "$artifact_size" ]] || fail "selected authorization artifact size mismatch"

intake_result="$scratch/intake-result.json"
intake_args=("$intake_command" --run "$run_capture" --jobs "$jobs_capture" --artifacts "$artifacts_capture" --artifact-max-bytes "$AUTHORIZED_ARCHIVE_MAX_BYTES" --default-branch "$DEFAULT_BRANCH" --repository "$GITHUB_REPOSITORY" --repository-id "$GITHUB_REPOSITORY_ID" --run-id "$run_id" --run-attempt "$run_attempt")
if [[ "$mode" == authorized ]]; then
  intake_args+=(--authorization-workflow-commit "$AUTHORIZATION_WORKFLOW_COMMIT" --candidate-source-commit "$CANDIDATE_SOURCE_COMMIT" --root-anchor-sha256 "$ROOT_ANCHOR_SHA256" --approved-observability-policy-sha256 "$APPROVED_OBSERVABILITY_POLICY_SHA256")
fi
intake_args+=(--archive "$archive" --downloaded-artifact-id "$artifact_id" --max-unpacked-bytes "$AUTHORIZED_MAX_UNPACKED_BYTES" --reserve-bytes "$AUTHORIZED_RESERVE_BYTES" --parent "$scratch")
safe_node "$repository_root/jobs/scripts/managed-cloud-release-gate.mjs" "${intake_args[@]}" > "$intake_result"

# The gate verifies archive identity while holding its descriptor. Keep the accepted
# private parent because `root` is the next workflow step's handoff, but remove raw ZIP.
rm -f -- "$archive"
cat "$intake_result"
