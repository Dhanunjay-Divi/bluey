import assert from "node:assert/strict";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

import {
  checkPhase613MigrationRegistration,
  checkPhase614MigrationRegistration,
  checkPhase614BMigrationRegistration,
  checkPhase621MigrationRegistration,
  checkPhase628MigrationRegistration,
  compareJobsSchemas,
} from "./check-jobs-schema-parity.mjs";
import {
  inventoryPackageLock,
  parseProvenanceRows,
  validateProvenance,
  validateWorkspaceLock,
} from "./check-provenance-licenses.mjs";
import { classifyTrackedPath, scanTextForSecrets } from "./privacy-gate.mjs";
import { checkBusinessMessagingSimulatorContainment } from "./check-business-messaging-simulator-containment.mjs";

const repoRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../..",
);

const JOBS_CI_TIMEOUT_MINUTES = 90;
const ISOLATED_TEST_LAUNCHER = "scripts/run-bluey-tests.sh";
const MIC_DEVICE_SELECTION_TEST =
  "crates/cue-daemon/tests/mic_device_selection.rs";
const MARKER_FIELD_COMPARISONS = [
  ["magic", '"$marker_magic" == "$1"'],
  ["run_root", '"$marker_run_root" == "$2"'],
  ["run_name", '"$marker_run_name" == "$3"'],
  ["owner_uid", '"$marker_owner_uid" == "$4"'],
  ["owner_pid", '"$marker_owner_pid" == "$5"'],
  ["process_group_id", '"$marker_process_group_id" == "$6"'],
  ["launcher_group_id", '"$marker_launcher_group_id" == "$7"'],
  ["created_epoch", '"$marker_created_epoch" == "$8"'],
  ["state", '"$marker_state" == "$9"'],
];
const CURRENT_MARKER_SNAPSHOT_BINDINGS = [
  ["magic", 'current_marker_magic="$marker_magic"'],
  ["run_root", 'current_marker_run_root="$marker_run_root"'],
  ["run_name", 'current_marker_run_name="$marker_run_name"'],
  ["owner_uid", 'current_marker_owner_uid="$marker_owner_uid"'],
  ["owner_pid", 'current_marker_owner_pid="$marker_owner_pid"'],
  [
    "process_group_id",
    'current_marker_process_group_id="$marker_process_group_id"',
  ],
  [
    "launcher_group_id",
    'current_marker_launcher_group_id="$marker_launcher_group_id"',
  ],
  ["created_epoch", 'current_marker_created_epoch="$marker_created_epoch"'],
  ["state", 'current_marker_state="$marker_state"'],
];
const CURRENT_MARKER_REVALIDATION_ARGUMENTS = [
  ["magic", '"$current_marker_magic"'],
  ["run_root", '"$current_marker_run_root"'],
  ["run_name", '"$current_marker_run_name"'],
  ["owner_uid", '"$current_marker_owner_uid"'],
  ["owner_pid", '"$current_marker_owner_pid"'],
  ["process_group_id", '"$current_marker_process_group_id"'],
  ["launcher_group_id", '"$current_marker_launcher_group_id"'],
  ["created_epoch", '"$current_marker_created_epoch"'],
  ["state", '"$current_marker_state"'],
];
const EXPECTED_MARKER_REVALIDATION_ARGUMENTS = [
  ["magic", '"$expected_magic"'],
  ["run_root", '"$expected_run_root"'],
  ["run_name", '"$expected_run_name"'],
  ["owner_uid", '"$expected_owner_uid"'],
  ["owner_pid", '"$expected_owner_pid"'],
  ["process_group_id", '"$expected_process_group_id"'],
  ["launcher_group_id", '"$expected_launcher_group_id"'],
  ["created_epoch", '"$expected_created_epoch"'],
  ["state", '"$expected_state"'],
];

function replaceOccurrence(source, needle, replacement, occurrence) {
  let start = 0;
  let index = -1;
  for (let count = 0; count < occurrence; count += 1) {
    index = source.indexOf(needle, start);
    if (index < 0) return source;
    start = index + needle.length;
  }
  return source.slice(0, index) + replacement + source.slice(index + needle.length);
}

const managedReleasePermissionContracts = new Map([
  ["CANDIDATE-BUILD", ["actions: read", "contents: read"]],
  ["VERIFY-NO-REBUILD", ["actions: read", "contents: read"]],
  ["AUTHORIZE-NO-REBUILD", ["actions: read", "contents: read"]],
  ["PUBLISH-REMOTE", ["actions: read", "contents: read", "packages: write"]],
  ["PROMOTE-NO-REBUILD", ["actions: read", "contents: read", "packages: read"]],
  ["ROLLBACK-NO-REBUILD", ["actions: read", "contents: read"]],
]);

function managedReleaseMarkedSection(text, marker) {
  const begin = `# RELEASE-GATE: ${marker}-BEGIN`;
  const end = `# RELEASE-GATE: ${marker}-END`;
  const start = text.indexOf(begin);
  const finish = text.indexOf(end);
  assert(start >= 0 && finish > start, `${marker} must have one marked section`);
  return {
    begin: start + begin.length,
    end: finish,
    text: text.slice(start + begin.length, finish),
  };
}

function assertManagedReleasePermissionContract(text) {
  const globalPermissions = text.match(
    /(?:^|\n)permissions:\n((?:  [^\n]+\n)+)/,
  );
  assert.deepEqual(
    globalPermissions?.[1]
      .trim()
      .split("\n")
      .map((line) => line.trim()),
    ["contents: read"],
    "Managed-cloud global permissions must be contents: read only",
  );
  for (const [marker, expected] of managedReleasePermissionContracts) {
    const section = managedReleaseMarkedSection(text, marker).text;
    const permissions = section.match(/\n    permissions:\n((?:      [^\n]+\n)+)/);
    assert.equal(
      section.split("\n    permissions:").length - 1,
      1,
      `${marker} must have exactly one job-local permissions map`,
    );
    assert.deepEqual(
      permissions?.[1]
        .trim()
        .split("\n")
        .map((line) => line.trim()),
      expected,
      `${marker} must retain exact job-local permissions`,
    );
  }
}

function mutateManagedReleaseSection(text, marker, mutate) {
  const section = managedReleaseMarkedSection(text, marker);
  const mutated = mutate(section.text);
  assert.notEqual(mutated, section.text, `${marker} mutation must alter its section`);
  return text.slice(0, section.begin) + mutated + text.slice(section.end);
}

function managedReleaseNamedStep(section, name) {
  const marker = `      - name: ${name}`;
  const begin = section.indexOf(marker);
  assert(begin >= 0, `${name} step must exist`);
  assert.equal(
    section.indexOf(marker, begin + marker.length),
    -1,
    `${name} step must be unambiguous`,
  );
  const next = section.indexOf("\n      - ", begin + marker.length);
  const end = next < 0 ? section.length : next;
  return { begin, end, text: section.slice(begin, end) };
}

function mutateManagedReleaseNamedStep(text, marker, name, mutate) {
  return mutateManagedReleaseSection(text, marker, (section) => {
    const step = managedReleaseNamedStep(section, name);
    const mutated = mutate(step.text);
    assert.notEqual(mutated, step.text, `${name} mutation must alter only that step`);
    return section.slice(0, step.begin) + mutated + section.slice(step.end);
  });
}

function assertManagedPublicationNetworkContract(text) {
  const candidate = managedReleaseMarkedSection(text, "CANDIDATE-BUILD").text;
  const publication = managedReleaseMarkedSection(text, "PUBLISH-REMOTE").text;
  const promotion = managedReleaseMarkedSection(
    text,
    "PROMOTE-NO-REBUILD",
  ).text;
  for (const [label, section, expectedBindings] of [
    ["candidate", candidate, 1],
    ["publication", publication, 1],
    ["promotion", promotion, 2],
  ]) {
    assert(
      section.split(
        "DEFAULT_BRANCH: ${{ github.event.repository.default_branch }}",
      ).length -
        1 ===
        expectedBindings &&
        section.split('--default-branch "$DEFAULT_BRANCH"').length - 1 ===
          expectedBindings,
      `${label} must take default branch from trusted GitHub event context`,
    );
  }
  const candidateRunCaptures = [
    {
      endpoint:
        'gh api "/repos/$GITHUB_REPOSITORY/actions/runs/$JOBS_CI_RUN_ID"',
      output: "release/prepared/jobs-ci-run.json",
    },
    {
      endpoint:
        'gh api "/repos/$GITHUB_REPOSITORY/actions/runs/$OBSERVABILITY_RUN_ID"',
      output: "release/prepared/observability-run.json",
    },
  ];
  const candidateRunCaptureStep = managedReleaseNamedStep(
    candidate,
    "Bind successful exact-tip CI instead of synthetic candidate evidence",
  ).text;
  assert.equal(
    candidate.split("publication-api-capture").length - 1,
    2,
    "Candidate must use exactly two atomic bounded run-response captures",
  );
  assert.equal(
    candidateRunCaptureStep.split("publication-api-capture").length - 1,
    2,
    "Candidate exact-tip CI step must contain both bounded captures",
  );
  assert.equal(
    candidate.split("--kind run --out").length - 1,
    2,
    "Candidate must fix both CI evidence captures to the 128 KiB run kind",
  );
  assert.equal(
    candidateRunCaptureStep.split("--kind run --out").length - 1,
    2,
    "Candidate exact-tip CI step must fix both captures to the run kind",
  );
  assert.equal(
    candidateRunCaptureStep.split(
      "        run: |\n          set -euo pipefail\n",
    ).length - 1,
    1,
    "Candidate exact-tip CI capture step must enable pipefail before both producer pipelines",
  );
  for (const { endpoint, output } of candidateRunCaptures) {
    const pipeline =
      `if ! ${endpoint} \\\n` +
      "            | node jobs/scripts/managed-cloud-release-gate.mjs " +
      "publication-api-capture \\\n" +
      `                --kind run --out ${output}`;
    const cleanup = `rm -f -- ${output} \\\n              ${output}.partial`;
    assert(
      candidate.split(endpoint).length - 1 === 1 &&
        candidate.split(output).length - 1 === 4 &&
        candidateRunCaptureStep.includes(pipeline) &&
        candidateRunCaptureStep.includes(cleanup) &&
        !candidate.includes(`> ${output}`),
      `Candidate CI run capture must be atomic and fail closed: ${output}`,
    );
  }
  const runtimeMarkers = [
    "publication-runtime",
    "--authorized authorized",
    '--root-anchor-sha256 "$ROOT_ANCHOR_SHA256"',
    '--default-branch "$DEFAULT_BRANCH"',
    '--source-commit "$GITHUB_SHA"',
    '--repository "$GITHUB_REPOSITORY"',
    '--repository-id "$GITHUB_REPOSITORY_ID"',
  ];
  for (const marker of runtimeMarkers) {
    assert(
      publication.includes(marker),
      `Publication pre-write runtime identity is missing: ${marker}`,
    );
  }
  assert(
    publication.indexOf("publication-runtime") <
      publication.indexOf("oras login ghcr.io"),
    "Publication runtime identity must pass before registry login or write",
  );
  const portalMarkers = [
    'oras manifest fetch "$PORTAL_REGISTRY_REF"',
    "publication-portal-capture",
    "--out publication-readback/jobs-portal.oci-manifest.json",
    "rm -f -- publication-readback/jobs-portal.oci-manifest.json",
    "publication-readback/jobs-portal.oci-manifest.json.partial",
    "publication-portal-manifest",
    "publication-portal-readback",
    "--manifest publication-readback/jobs-portal.oci-manifest.json",
    "--download portal-download",
    "--out publication-readback/jobs-portal.tar",
  ];
  for (const [label, section] of [
    ["publication", publication],
    ["promotion", promotion],
  ]) {
    for (const marker of portalMarkers) {
      assert(
        section.includes(marker),
        `${label} portal manifest/readback is missing: ${marker}`,
      );
    }
    const fetchIndex = section.indexOf(
      'oras manifest fetch "$PORTAL_REGISTRY_REF"',
    );
    const captureIndex = section.indexOf("publication-portal-capture");
    const manifestIndex = section.indexOf("publication-portal-manifest");
    const pullIndex = section.indexOf(
      'oras pull "$PORTAL_REGISTRY_REF" --output portal-download',
    );
    const readbackIndex = section.indexOf("publication-portal-readback");
    assert(
      fetchIndex >= 0 &&
        captureIndex > fetchIndex &&
        manifestIndex > captureIndex &&
        pullIndex > manifestIndex &&
        readbackIndex > pullIndex &&
        !section.includes(
          "--output publication-readback/jobs-portal.oci-manifest.json",
        ),
      `${label} must validate the bounded manifest before pull and the tree after pull`,
    );
  }
  assert(
    publication.includes(
      '--annotation "org.opencontainers.image.created=$PORTAL_MANIFEST_CREATED_AT"',
    ) &&
      publication.includes(
        "process.stdout.write(plan.portalManifestCreatedAt)",
      ),
    "Publication must bind the deterministic portal manifest timestamp",
  );
  const exactRunIndex = promotion.indexOf(
    '/attempts/$PUBLICATION_RUN_ATTEMPT"',
  );
  const jobsReadIndex = promotion.indexOf(
    '/attempts/$PUBLICATION_RUN_ATTEMPT/jobs?per_page=100"',
  );
  const artifactsReadIndex = promotion.indexOf('/artifacts?per_page=100"');
  const selectionIndex = promotion.indexOf("publication-artifact-select");
  const downloadIndex = promotion.indexOf(
    "artifact-ids: ${{ steps.select-publication.outputs.artifact-id }}",
  );
  const preflightIndex = promotion.indexOf(
    "managed-cloud-release-gate.mjs publication-preflight",
  );
  const loginIndex = promotion.indexOf("oras login ghcr.io");
  const registryReadIndex = promotion.indexOf("oras cp --to-oci-layout");
  assert(
    exactRunIndex >= 0 &&
      jobsReadIndex > exactRunIndex &&
      artifactsReadIndex > jobsReadIndex &&
      selectionIndex > artifactsReadIndex &&
      downloadIndex > selectionIndex &&
      preflightIndex > downloadIndex &&
      loginIndex > preflightIndex &&
      registryReadIndex > preflightIndex,
    "Promotion canonical preflight must pass before registry login or readback",
  );
  const exactAttemptMarkers = [
    "publication_run_attempt",
    "PUBLICATION_RUN_ATTEMPT: ${{ github.event.inputs.publication_run_attempt }}",
    "canonical positive safe integer",
    "Number.isSafeInteger(Number(value))",
    '/attempts/$PUBLICATION_RUN_ATTEMPT"',
    '/attempts/$PUBLICATION_RUN_ATTEMPT/jobs?per_page=100"',
    '/artifacts?per_page=100"',
    "publication-artifact-select",
    "--run-attempt \"$PUBLICATION_RUN_ATTEMPT\"",
    "--publication-workflow-evidence publication-workflow-evidence.json",
    "artifact-ids: ${{ steps.select-publication.outputs.artifact-id }}",
    "merge-multiple: true",
    '--workflow-run-attempt "$PUBLICATION_RUN_ATTEMPT"',
  ];
  for (const marker of exactAttemptMarkers) {
    assert(
      promotion.includes(marker),
      `Promotion exact-attempt artifact authority is missing: ${marker}`,
    );
  }
  const apiCaptureMarkers = [
    "--kind run --out publication-workflow-run.json",
    "--kind jobs --out publication-workflow-jobs.json",
    "--kind artifacts --out publication-workflow-artifacts.json",
    "rm -f -- publication-workflow-run.json publication-workflow-run.json.partial",
    "rm -f -- publication-workflow-jobs.json publication-workflow-jobs.json.partial",
    "rm -f -- publication-workflow-artifacts.json publication-workflow-artifacts.json.partial",
  ];
  assert.equal(
    promotion.split("publication-api-capture").length - 1,
    3,
    "Promotion must use exactly three fixed-kind bounded API captures",
  );
  for (const marker of apiCaptureMarkers) {
    assert(
      promotion.includes(marker),
      `Promotion bounded API capture is missing: ${marker}`,
    );
  }
  assert(
    publication.includes(
      "name: jobs-managed-cloud-publication-${{ github.run_id }}-${{ github.run_attempt }}",
    ) &&
      publication.split('--workflow-run-attempt "$GITHUB_RUN_ATTEMPT"').length -
        1 ===
        2,
    "Publication must bind and upload one run-and-attempt-qualified receipt",
  );
  assert(
    !promotion.includes("filter=latest") &&
      !promotion.includes("name: jobs-managed-cloud-publication\n") &&
      !promotion.includes("> publication-workflow-run.json") &&
      !promotion.includes("> publication-workflow-jobs.json") &&
      !promotion.includes("> publication-workflow-artifacts.json"),
    "Promotion must reject latest-attempt, reusable-name, and unbounded API transports",
  );
  assert(
    promotion.includes("--publication-preflight publication-preflight.json") &&
      promotion.includes('--repository "$GITHUB_REPOSITORY"') &&
      promotion.includes('--repository-id "$GITHUB_REPOSITORY_ID"') &&
      !promotion.includes('require("./publication/publication-plan.json")') &&
      !promotion.includes('require("./publication/publication-set.json")'),
    "Promotion must consume only canonical preflight registry authority",
  );
  assert(
    !text.includes("mv portal-download/jobs-portal.tar"),
    "Portal pull must pass closed inspection before a new readback file is written",
  );
}

function runnerVolumeParitySchema(integerType) {
  const migrationPath =
    integerType === "INTEGER"
      ? "infra/sqlite/server-runtime/046_jobs_runner_volume_purge.sql"
      : "infra/postgres/server-runtime/024_jobs_runner_volume_purge.sql";
  return fs.readFileSync(path.join(repoRoot, migrationPath), "utf8");
}

function browserReleaseAuthorityParitySchema(integerType) {
  const migrationPath =
    integerType === "INTEGER"
      ? "infra/sqlite/server-runtime/047_jobs_browser_release_authority.sql"
      : "infra/postgres/server-runtime/025_jobs_browser_release_authority.sql";
  return fs.readFileSync(path.join(repoRoot, migrationPath), "utf8");
}

function atsCertificationParitySchema(integerType) {
  const migrations =
    integerType === "INTEGER"
      ? [
          "infra/sqlite/server-runtime/048_jobs_ats_certification_authority.sql",
          "infra/sqlite/server-runtime/049_jobs_browser_release_runtime_components.sql",
          "infra/sqlite/server-runtime/050_jobs_runner_process_runtime_authority.sql",
        ]
      : [
          "infra/postgres/server-runtime/026_jobs_ats_certification_authority.sql",
          "infra/postgres/server-runtime/027_jobs_browser_release_runtime_components.sql",
          "infra/postgres/server-runtime/028_jobs_runner_process_runtime_authority.sql",
        ];
  return migrations
    .map((migrationPath) =>
      fs.readFileSync(path.join(repoRoot, migrationPath), "utf8"),
    )
    .join("\n");
}

function communicationExecutionParitySchema(integerType) {
  const migrationPath =
    integerType === "INTEGER"
      ? "infra/sqlite/server-runtime/051_jobs_communication_execution.sql"
      : "infra/postgres/server-runtime/029_jobs_communication_execution.sql";
  return fs.readFileSync(path.join(repoRoot, migrationPath), "utf8");
}

function operationalHoldsParitySchema(integerType) {
  const migrationPath =
    integerType === "INTEGER"
      ? "infra/sqlite/server-runtime/052_jobs_operational_holds.sql"
      : "infra/postgres/server-runtime/030_jobs_operational_holds.sql";
  return fs.readFileSync(path.join(repoRoot, migrationPath), "utf8");
}

function phase613ParitySchema(integerType) {
  const migrationPath =
    integerType === "INTEGER"
      ? "infra/sqlite/server-runtime/056_jobs_canonical_taxonomy_authority.sql"
      : "infra/postgres/server-runtime/034_jobs_canonical_taxonomy_authority.sql";
  return fs.readFileSync(path.join(repoRoot, migrationPath), "utf8");
}

function testPrivacyPaths() {
  const rejected = [
    ["jobs/candidates/alice/resume.pdf", "candidate or user data directory"],
    [
      "jobs/data/candidate-data/profile.json",
      "candidate or user data directory",
    ],
    [
      "jobs/browser/profiles/1234567890abcdef12345678/Default/Cookies",
      "generated browser profile",
    ],
    [
      "jobs/runner/snapshots/1234567890abcdef1234567890abcdef12345678.tar.gz.enc",
      "generated browser profile",
    ],
    ["jobs/runner/receipts/run-42.json", "receipt or screenshot artifact"],
    ["jobs/runner/screenshot-run-42.png", "receipt or screenshot artifact"],
    ["jobs/users/alice/profile.json", "candidate or user data directory"],
    ["jobs/users.csv", "candidate or user data file"],
    [
      "jobs/automation/tests/fixtures/dummy-token.json",
      "credential-shaped fixture file",
    ],
    [".env.local", "credential-bearing file path"],
    ["local/service-account.json", "credential-bearing file path"],
    ["release/jobs-export.zip", "archive artifact"],
  ];
  for (const [filePath, expectedFinding] of rejected) {
    assert(
      classifyTrackedPath(filePath).includes(expectedFinding),
      `${filePath} should be rejected as ${expectedFinding}`,
    );
  }

  const permitted = [
    ".env.example",
    "assets/brand/screenshot-frame.png",
    "docs/rounds/example/screenshots/ui.png",
    "jobs/automation/src/receipts.ts",
    "jobs/automation/tests/fixtures/greenhouse/public-modern.json",
    "jobs/automation/tests/receipts.test.ts",
    "jobs/browser/src/profile.ts",
    "jobs/runner/tests/fixtures/synthetic-receipt.json",
  ];
  for (const filePath of permitted) {
    assert.deepEqual(
      classifyTrackedPath(filePath),
      [],
      `${filePath} should be permitted`,
    );
  }
}

function testSecretScanning() {
  const awsKey = ["AKIA", "7QWERTYUIOP9ZXCV"].join("");
  const privateKey = ["-----BEGIN", "PRIVATE KEY-----"].join(" ");
  const stripeKey = ["sk_live_", "51QwertyUiopAsdfGhjkLzxc"].join("");
  const anthropicKey = [
    "sk-ant-api03-",
    "QwertyUiopAsdfGhjkLzxcVbnm123456",
  ].join("");
  const findings = scanTextForSecrets(
    "config/production.env",
    [
      `AWS_ACCESS_KEY_ID=${awsKey}`,
      privateKey,
      `client_secret = "v8F3mQ2zL9pR6sT1"`,
      stripeKey,
      anthropicKey,
      "BLUEY_TOKEN=Q7w9Er2Ty4Ui6Op8As1Df3Gh",
      "https://runner:Q7w9Er2Ty4Ui6Op8@example.com/jobs",
    ].join("\n"),
  );
  assert.deepEqual(
    findings.map((finding) => finding.kind),
    [
      "AWS access key",
      "private key material",
      "raw client_secret assignment",
      "Stripe live key",
      "Anthropic API key",
      "raw bluey_token assignment",
      "credential embedded in URL",
    ],
  );

  const canonicalJwt = [
    "eyJhbGciOiJIUzI1NiJ9",
    "eyJzdWIiOiJ0ZXN0IiwiaWF0IjoxNjE2MjM5MDIyfQ",
    "SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c",
  ].join(".");
  const placeholders = [
    'client_secret = "${CLIENT_SECRET}"',
    'password = "Password123!"',
    `token=${canonicalJwt}`,
    "AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE",
  ].join("\n");
  assert.deepEqual(
    scanTextForSecrets("tests/fixtures/synthetic-auth.txt", placeholders),
    [],
  );
}

function testPortalBundleFreshnessWorkflowGuard() {
  for (const workflowPath of [
    ".github/workflows/jobs-ci.yml",
    ".github/workflows/release.yml",
  ]) {
    const workflow = fs.readFileSync(path.join(repoRoot, workflowPath), "utf8");
    const buildIndex = workflow.indexOf("npm run build --prefix jobs");
    const freshnessIndex = workflow.indexOf("git diff --exit-code -- web/jobs");
    assert(buildIndex >= 0, `${workflowPath} must build the Jobs workspaces`);
    assert(
      freshnessIndex > buildIndex,
      `${workflowPath} must reject a stale checked-in Jobs portal bundle after the build`,
    );
  }
}

function testDependencySecurityWorkflowGuard() {
  for (const workflowPath of [
    ".github/workflows/jobs-ci.yml",
    ".github/workflows/release.yml",
  ]) {
    const workflow = fs.readFileSync(path.join(repoRoot, workflowPath), "utf8");
    const installIndex = workflow.indexOf("npm ci --prefix jobs --no-audit --no-fund");
    const auditIndex = workflow.indexOf("npm audit --prefix jobs --audit-level=moderate");
    assert(installIndex >= 0, `${workflowPath} must install the locked Jobs dependencies`);
    assert(
      auditIndex > installIndex,
      `${workflowPath} must audit the installed Jobs dependency graph before release checks`,
    );
  }
  const browserRelease = fs.readFileSync(
    path.join(repoRoot, ".github/workflows/jobs-browser-release.yml"),
    "utf8",
  );
  assert.equal(
    browserRelease.split("npm audit --prefix candidate-source/jobs --audit-level=moderate").length - 1,
    2,
    "Browser candidate preparation and isolated packaging must both audit candidate dependencies",
  );
  assert.equal(
    browserRelease.split("npm audit --prefix trusted-release-tools/jobs --audit-level=moderate").length - 1,
    1,
    "Browser isolated packaging must audit trusted tooling before receiving credentials",
  );
  const managedRelease = fs.readFileSync(
    path.join(repoRoot, ".github/workflows/jobs-managed-cloud-release.yml"),
    "utf8",
  );
  assertManagedReleasePermissionContract(managedRelease);
  assertManagedPublicationNetworkContract(managedRelease);
  for (const [marker, permissions] of managedReleasePermissionContracts) {
    for (const permission of permissions) {
      const mutated = mutateManagedReleaseSection(managedRelease, marker, (section) =>
        section.replace(`      ${permission}`, `      ${permission.split(":")[0]}: none`),
      );
      assert.throws(
        () => assertManagedReleasePermissionContract(mutated),
        new RegExp(`${marker} must retain exact job-local permissions`),
      );
    }
  }
  for (const marker of [
    "CANDIDATE-BUILD",
    "VERIFY-NO-REBUILD",
    "AUTHORIZE-NO-REBUILD",
    "ROLLBACK-NO-REBUILD",
  ]) {
    const mutated = mutateManagedReleaseSection(managedRelease, marker, (section) =>
      section.replace(
        "      contents: read",
        "      contents: read\n      packages: read",
      ),
    );
    assert.throws(
      () => assertManagedReleasePermissionContract(mutated),
      new RegExp(`${marker} must retain exact job-local permissions`),
    );
  }
  for (const replacement of [
    "permissions:\n  contents: none",
    "permissions:\n  actions: read\n  contents: read",
    "permissions:\n  contents: read\n  packages: read",
  ]) {
    const mutated = managedRelease.replace(
      "permissions:\n  contents: read",
      replacement,
    );
    assert.notEqual(mutated, managedRelease);
    assert.throws(
      () => assertManagedReleasePermissionContract(mutated),
      /Managed-cloud global permissions must be contents: read only/,
    );
  }
  for (const [marker, replacement] of [
    ["--authorized authorized", "--authorized unchecked"],
    [
      '--root-anchor-sha256 "$ROOT_ANCHOR_SHA256"',
      '--root-anchor-sha256 "$MUTATED_ROOT_ANCHOR_SHA256"',
    ],
    [
      '--default-branch "$DEFAULT_BRANCH"',
      '--default-branch "$MUTATED_DEFAULT_BRANCH"',
    ],
    ['--source-commit "$GITHUB_SHA"', '--source-commit "$MUTATED_SHA"'],
    [
      '--repository "$GITHUB_REPOSITORY"',
      '--repository "$MUTATED_REPOSITORY"',
    ],
    [
      '--repository-id "$GITHUB_REPOSITORY_ID"',
      '--repository-id "$MUTATED_REPOSITORY_ID"',
    ],
  ]) {
    const mutated = mutateManagedReleaseSection(
      managedRelease,
      "PUBLISH-REMOTE",
      (section) => section.replaceAll(marker, replacement),
    );
    assert.throws(
      () => assertManagedPublicationNetworkContract(mutated),
      /Publication pre-write runtime identity is missing|trusted GitHub event context/,
    );
  }
  for (const marker of ["PUBLISH-REMOTE", "PROMOTE-NO-REBUILD"]) {
    const missingPortalManifest = mutateManagedReleaseSection(
      managedRelease,
      marker,
      (section) => section.replace(
        "publication-portal-readback",
        "publication-portal-unchecked",
      ),
    );
    assert.throws(
      () => assertManagedPublicationNetworkContract(missingPortalManifest),
      /portal manifest\/readback is missing/,
    );
    const lateManifestValidation = mutateManagedReleaseSection(
      managedRelease,
      marker,
      (section) => {
        const line = section.match(
          /\n\s*publication-portal-manifest[\s\S]*?--manifest publication-readback\/jobs-portal\.oci-manifest\.json\n/,
        )?.[0];
        assert(line, `${marker} must contain a manifest-only validation block`);
        return section.replace(line, "").replace(
          'oras pull "$PORTAL_REGISTRY_REF" --output portal-download',
          'oras pull "$PORTAL_REGISTRY_REF" --output portal-download' + line,
        );
      },
    );
    assert.throws(
      () => assertManagedPublicationNetworkContract(lateManifestValidation),
      /validate the bounded manifest before pull/,
    );
    const unboundedPortalManifest = mutateManagedReleaseSection(
      managedRelease,
      marker,
      (section) => section.replace(
        "publication-portal-capture",
        "publication-portal-unbounded",
      ),
    );
    assert.throws(
      () => assertManagedPublicationNetworkContract(unboundedPortalManifest),
      /portal manifest\/readback is missing/,
    );
    const missingPortalCleanup = mutateManagedReleaseSection(
      managedRelease,
      marker,
      (section) => section.replace(
        "publication-readback/jobs-portal.oci-manifest.json.partial",
        "publication-readback/jobs-portal.oci-manifest.json.unchecked",
      ),
    );
    assert.throws(
      () => assertManagedPublicationNetworkContract(missingPortalCleanup),
      /portal manifest\/readback is missing/,
    );
  }
  const unboundedPublicationApi = mutateManagedReleaseSection(
    managedRelease,
    "PROMOTE-NO-REBUILD",
    (section) => section.replace(
      "publication-api-capture",
      "publication-api-unbounded",
    ),
  );
  assert.throws(
    () => assertManagedPublicationNetworkContract(unboundedPublicationApi),
    /exactly three fixed-kind bounded API captures/,
  );
  const unsafePublicationInput = mutateManagedReleaseSection(
    managedRelease,
    "PROMOTE-NO-REBUILD",
    (section) => section.replace(
      "Number.isSafeInteger(Number(value))",
      "Number.isInteger(Number(value))",
    ),
  );
  assert.throws(
    () => assertManagedPublicationNetworkContract(unsafePublicationInput),
    /exact-attempt artifact authority is missing/,
  );
  const missingPublicationApiCleanup = mutateManagedReleaseSection(
    managedRelease,
    "PROMOTE-NO-REBUILD",
    (section) => section.replace(
      "publication-workflow-run.json.partial",
      "publication-workflow-run.json.unchecked",
    ),
  );
  assert.throws(
    () => assertManagedPublicationNetworkContract(missingPublicationApiCleanup),
    /bounded API capture is missing/,
  );
  const latestAttemptJobs = mutateManagedReleaseSection(
    managedRelease,
    "PROMOTE-NO-REBUILD",
    (section) => section.replace(
      '/attempts/$PUBLICATION_RUN_ATTEMPT/jobs?per_page=100',
      '/jobs?filter=latest&per_page=100',
    ),
  );
  assert.throws(
    () => assertManagedPublicationNetworkContract(latestAttemptJobs),
    /canonical preflight must pass|exact-attempt artifact authority is missing|reject latest-attempt/,
  );
  const uncheckedArtifactSelection = mutateManagedReleaseSection(
    managedRelease,
    "PROMOTE-NO-REBUILD",
    (section) => section.replace(
      "publication-artifact-select",
      "publication-artifact-unchecked",
    ),
  );
  assert.throws(
    () => assertManagedPublicationNetworkContract(uncheckedArtifactSelection),
    /canonical preflight must pass|exact-attempt artifact authority is missing/,
  );
  const reusableArtifactName = mutateManagedReleaseSection(
    managedRelease,
    "PROMOTE-NO-REBUILD",
    (section) => section.replace(
      "artifact-ids: ${{ steps.select-publication.outputs.artifact-id }}",
      "name: jobs-managed-cloud-publication",
    ),
  );
  assert.throws(
    () => assertManagedPublicationNetworkContract(reusableArtifactName),
    /canonical preflight must pass|exact-attempt artifact authority is missing|reject latest-attempt/,
  );
  const staticPublicationArtifactName = mutateManagedReleaseSection(
    managedRelease,
    "PUBLISH-REMOTE",
    (section) => section.replace(
      "name: jobs-managed-cloud-publication-${{ github.run_id }}-${{ github.run_attempt }}",
      "name: jobs-managed-cloud-publication",
    ),
  );
  assert.throws(
    () => assertManagedPublicationNetworkContract(staticPublicationArtifactName),
    /run-and-attempt-qualified receipt/,
  );
  const unboundPortalCreated = mutateManagedReleaseSection(
    managedRelease,
    "PUBLISH-REMOTE",
    (section) => section.replace(
      '--annotation "org.opencontainers.image.created=$PORTAL_MANIFEST_CREATED_AT"',
      '--annotation "org.opencontainers.image.created=$RUNNER_NOW"',
    ),
  );
  assert.throws(
    () => assertManagedPublicationNetworkContract(unboundPortalCreated),
    /deterministic portal manifest timestamp/,
  );
  const rawPromotionReceipt = mutateManagedReleaseSection(
    managedRelease,
    "PROMOTE-NO-REBUILD",
    (section) => section.replace(
      'const preflight=require("./publication-preflight.json");',
      'const preflight=require("./publication/publication-plan.json");',
    ),
  );
  assert.throws(
    () => assertManagedPublicationNetworkContract(rawPromotionReceipt),
    /only canonical preflight registry authority/,
  );
  const missingPromotionPreflight = mutateManagedReleaseSection(
    managedRelease,
    "PROMOTE-NO-REBUILD",
    (section) => section.replace(
      "managed-cloud-release-gate.mjs publication-preflight",
      "managed-cloud-release-gate.mjs publication-unchecked",
    ),
  );
  assert.throws(
    () => assertManagedPublicationNetworkContract(missingPromotionPreflight),
    /canonical preflight must pass before registry login or readback/,
  );
  for (const marker of ["CANDIDATE-BUILD", "PROMOTE-NO-REBUILD"]) {
    const untrustedDefaultBranch = mutateManagedReleaseSection(
      managedRelease,
      marker,
      (section) => section.replace(
        '--default-branch "$DEFAULT_BRANCH"',
        "--default-branch main",
      ),
    );
    assert.throws(
      () => assertManagedPublicationNetworkContract(untrustedDefaultBranch),
      /trusted GitHub event context/,
    );
  }
  for (const [label, runId, output] of [
    ["jobs-ci", "$JOBS_CI_RUN_ID", "release/prepared/jobs-ci-run.json"],
    [
      "observability",
      "$OBSERVABILITY_RUN_ID",
      "release/prepared/observability-run.json",
    ],
  ]) {
    const endpoint =
      `gh api "/repos/$GITHUB_REPOSITORY/actions/runs/${runId}"`;
    const directRedirect = mutateManagedReleaseSection(
      managedRelease,
      "CANDIDATE-BUILD",
      (section) => section.replace(
        `${endpoint} \\\n` +
          "            | node jobs/scripts/managed-cloud-release-gate.mjs " +
          "publication-api-capture \\\n" +
          `                --kind run --out ${output}`,
        `${endpoint} > ${output}`,
      ),
    );
    assert.throws(
      () => assertManagedPublicationNetworkContract(directRedirect),
      /exactly two atomic bounded run-response captures|atomic and fail closed/,
      `${label} direct redirection must fail the independent CI guard`,
    );
    const bypassCapture = mutateManagedReleaseSection(
      managedRelease,
      "CANDIDATE-BUILD",
      (section) => section.replace(
        `--kind run --out ${output}`,
        `--kind run-unbounded --out ${output}`,
      ),
    );
    assert.throws(
      () => assertManagedPublicationNetworkContract(bypassCapture),
      /128 KiB run kind|atomic and fail closed/,
      `${label} cap bypass must fail the independent CI guard`,
    );
    const missingCleanup = mutateManagedReleaseSection(
      managedRelease,
      "CANDIDATE-BUILD",
      (section) => section.replace(`${output}.partial`, `${output}.unchecked`),
    );
    assert.throws(
      () => assertManagedPublicationNetworkContract(missingCleanup),
      /atomic and fail closed/,
      `${label} partial-file cleanup bypass must fail the independent CI guard`,
    );
  }
  const missingCandidateCapturePipefail = mutateManagedReleaseNamedStep(
    managedRelease,
    "CANDIDATE-BUILD",
    "Bind successful exact-tip CI instead of synthetic candidate evidence",
    (step) =>
      step.replace(
        "        run: |\n          set -euo pipefail\n",
        "        run: |\n          set -eu\n",
      ),
  );
  assert.throws(
    () => assertManagedPublicationNetworkContract(missingCandidateCapturePipefail),
    /exact-tip CI capture step must enable pipefail/,
    "Removing pipefail only from the live candidate capture step must fail the independent guard",
  );
  const managedAuditIndex = managedRelease.indexOf("npm audit --audit-level=moderate");
  const managedBuildIndex = managedRelease.indexOf("docker buildx build --platform");
  assert(
    managedAuditIndex >= 0 &&
      managedBuildIndex > managedAuditIndex &&
      managedRelease.includes("major===22&&minor<13"),
    "Managed-cloud candidate must prove Node compatibility and audit before building artifacts",
  );
  const managedSetupNode =
    "uses: actions/setup-node@49933ea5288caeca8642d1e84afbd3f7d6820020";
  const managedNodeVersion = "node-version: 22.23.2";
  const managedNodeAssertion = 'test "$(node --version)" = "v22.23.2"';
  const managedDefaultBranchType = "github.ref_type == 'branch'";
  const managedDefaultBranchName =
    "github.ref_name == github.event.repository.default_branch";
  const managedNodeCommand =
    /(?:^|\n)\s*(?:run:\s*)?node\s+(?:jobs\/scripts\/managed-cloud-release-gate\.mjs|--test|-e|--input-type=module)/m;
  for (const marker of [
    "CANDIDATE-BUILD",
    "VERIFY-NO-REBUILD",
    "AUTHORIZE-NO-REBUILD",
    "PUBLISH-REMOTE",
    "PROMOTE-NO-REBUILD",
    "ROLLBACK-NO-REBUILD",
  ]) {
    const begin = `# RELEASE-GATE: ${marker}-BEGIN`;
    const end = `# RELEASE-GATE: ${marker}-END`;
    const section = managedRelease.slice(
      managedRelease.indexOf(begin) + begin.length,
      managedRelease.indexOf(end),
    );
    const setupIndex = section.indexOf(managedSetupNode);
    const assertionIndex = section.indexOf(managedNodeAssertion);
    const commandIndex = section.search(managedNodeCommand);
    const stepsIndex = section.indexOf("steps:");
    assert.equal(
      section.split(managedDefaultBranchType).length - 1,
      1,
      `${marker} must require a branch ref`,
    );
    assert.equal(
      section.split(managedDefaultBranchName).length - 1,
      1,
      `${marker} must require the repository default branch`,
    );
    assert.equal(
      section.split(managedSetupNode).length - 1,
      1,
      `${marker} must contain one pinned setup-node action`,
    );
    assert.equal(
      section.split(managedNodeVersion).length - 1,
      1,
      `${marker} must contain one exact Node patch`,
    );
    assert.equal(
      section.split(managedNodeAssertion).length - 1,
      1,
      `${marker} must assert the exact Node patch`,
    );
    assert(
      section.indexOf(managedDefaultBranchType) < stepsIndex &&
        section.indexOf(managedDefaultBranchName) < stepsIndex &&
        setupIndex >= 0 &&
        assertionIndex > setupIndex &&
        commandIndex > assertionIndex,
      `${marker} must bind the default branch and Node before release evidence`,
    );
  }
  const exactWorkflowRoles =
    "discovery_worker,global_discovery_worker,original_source_verifier," +
    "workflow_gateway,workflow_worker";
  const workflowRoleAssignments = (source) => [
    ...source.matchAll(/BLUEY_JOBS_WORKFLOW_RUNTIME_ROLES=([a-z_,]+)/g),
  ].map((match) => match[1]);
  for (const required of [
    "--version 3",
    "version: 3",
    "bluey-jobs-managed-cloud-release-gate-tests-v3",
    "bluey-jobs-managed-cloud-release-descriptor-v3",
    "sourceVerification: true",
    "directDiscovery: true",
    "globalDiscovery: true",
    "BLUEY_JOBS_WORKFLOW_RUNTIME_ROLES=" + exactWorkflowRoles,
  ]) {
    assert(
      managedRelease.includes(required),
      `Managed-cloud candidate is missing the release-v3 authority binding: ${required}`,
    );
  }
  assert(
    !managedRelease.includes("sourceVerification: false") &&
      !managedRelease.includes("directDiscovery: false") &&
      !managedRelease.includes("globalDiscovery: false") &&
      !managedRelease.includes("bluey-jobs-managed-cloud-release-descriptor-v1") &&
      !managedRelease.includes("bluey-jobs-managed-cloud-release-descriptor-v2"),
    "Managed-cloud candidate must not silently fall back from release v3",
  );
  assert.equal(
    managedRelease.split("packages: write").length - 1,
    1,
    "Managed-cloud publication must have exactly one package writer",
  );
  for (const required of [
    "environment: jobs-managed-cloud-artifact-publication",
    "uses: oras-project/setup-oras@38de303aac69abb66f3e6255b7198bff35f323e3",
    "vars.BLUEY_JOBS_ARTIFACT_NAMESPACE",
    "vars.BLUEY_JOBS_PORTAL_ARTIFACT_ORIGIN",
    "ci-evidence",
    "oras cp --from-oci-layout",
    "oras cp --to-oci-layout",
    'tar --extract --file "authorized/candidate/payloads/$component.oci.tar"',
    '"publication-source/$component@sha256:$DIGEST" "$TAG_REF"',
    '"publication-layout/$component:$GITHUB_RUN_ID"',
    "publication-readback/$component.oci.tar",
    "publication-attest",
    "publication-preflight",
    "publication-portal-manifest",
    "publication-verify",
    "--publication-readback publication-readback",
    'PUBLICATION_RUN_ID: ${{ github.event.inputs.publication_run_id }}',
    'PUBLICATION_RUN_ATTEMPT: ${{ github.event.inputs.publication_run_attempt }}',
    '--workflow-run-id "$PUBLICATION_RUN_ID"',
    '--workflow-run-attempt "$PUBLICATION_RUN_ATTEMPT"',
    '/attempts/$PUBLICATION_RUN_ATTEMPT"',
    '/attempts/$PUBLICATION_RUN_ATTEMPT/jobs?per_page=100"',
    '/artifacts?per_page=100"',
    "publication-api-capture",
    "publication-artifact-select",
    "--publication-workflow-evidence publication-workflow-evidence.json",
    "artifact-ids: ${{ steps.select-publication.outputs.artifact-id }}",
    "name: jobs-managed-cloud-publication-${{ github.run_id }}-${{ github.run_attempt }}",
    '--root-anchor-sha256 "$ROOT_ANCHOR_SHA256"',
    '--default-branch "$DEFAULT_BRANCH"',
    '--source-commit "$GITHUB_SHA"',
    '--repository "$GITHUB_REPOSITORY"',
    '--repository-id "$GITHUB_REPOSITORY_ID"',
    '--annotation "org.opencontainers.image.created=$PORTAL_MANIFEST_CREATED_AT"',
  ]) {
    assert(
      managedRelease.includes(required),
      `Managed-cloud remote publication authority is missing: ${required}`,
    );
  }
  assert(
    !managedRelease.includes("`ghcr.io/bluey/") &&
      !managedRelease.includes("local_fixture") &&
      !managedRelease.includes("filter=latest"),
    "Managed-cloud production workflow must not use the old namespace or test adapter",
  );
  const workflowsDockerfile = fs.readFileSync(
    path.join(repoRoot, "jobs/workflows/Dockerfile"),
    "utf8",
  );
  assert.deepEqual(
    [
      ...workflowsDockerfile.matchAll(
        /^ARG BLUEY_JOBS_WORKFLOW_RUNTIME_ROLES=([^\r\n]+)$/gm,
      ),
    ].map((match) => match[1]),
    [exactWorkflowRoles, exactWorkflowRoles],
    "The workflows build and runtime stages must default to the exact release-v3 role set",
  );
  assert.deepEqual(
    workflowRoleAssignments(managedRelease),
    [exactWorkflowRoles],
    "Workflow and Docker must share the exact five-role workflows authority",
  );
  for (const [label, original, replacement] of [
    ["contract", "--version 3", "--version 2"],
    ["descriptor", "release-descriptor-v3", "release-descriptor-v2"],
    ["descriptor version", "version: 3", "version: 2"],
    ["self-test", "release-gate-tests-v3", "release-gate-tests-v2"],
    ["direct discovery", "directDiscovery: true", "directDiscovery: false"],
    ["global discovery", "globalDiscovery: true", "globalDiscovery: false"],
    ["source verifier", "sourceVerification: true", "sourceVerification: false"],
    [
      "workflows role",
      exactWorkflowRoles,
      "global_discovery_worker,original_source_verifier,workflow_gateway,workflow_worker",
    ],
    [
      "extra workflows role",
      exactWorkflowRoles,
      exactWorkflowRoles + ",unexpected_worker",
    ],
  ]) {
    const mutated = managedRelease.replace(original, replacement);
    assert.notEqual(mutated, managedRelease, `${label} mutation must alter workflow`);
    const retainsRequiredAuthority = [
      "--version 3",
      "version: 3",
      "bluey-jobs-managed-cloud-release-gate-tests-v3",
      "bluey-jobs-managed-cloud-release-descriptor-v3",
      "sourceVerification: true",
      "directDiscovery: true",
      "globalDiscovery: true",
      "BLUEY_JOBS_WORKFLOW_RUNTIME_ROLES=" + exactWorkflowRoles,
    ].every((required) => mutated.includes(required));
    const retainsExactRoles =
      JSON.stringify(workflowRoleAssignments(mutated)) ===
      JSON.stringify([exactWorkflowRoles]);
    assert(
      !(retainsRequiredAuthority && retainsExactRoles),
      `${label} mutation must break release-v3 authority`,
    );
  }
}

function testBuiltPortalPublicBetaTruth() {
  const assetsDir = path.join(repoRoot, "web/jobs/assets");
  const bundle = fs
    .readdirSync(assetsDir)
    .filter((name) => name.endsWith(".js"))
    .map((name) => fs.readFileSync(path.join(assetsDir, name), "utf8"))
    .join("\n");
  for (const forbidden of [
    "invited beta accounts",
    "invited local/cloud runner beta",
    "join runner beta when it opens",
    "cloud automation then runs for admitted public-beta accounts",
  ]) {
    assert(!bundle.toLowerCase().includes(forbidden), `built Jobs portal contains stale claim: ${forbidden}`);
  }
  assert(
    bundle.includes("Public-beta admission opens the Jobs workspace, not cloud automation"),
    "built Jobs portal is missing the cohort-versus-runner authority statement",
  );
}

function validateJobsCiTimeBudget(workflow) {
  const lines = workflow.split(/\r?\n/);
  const jobStart = lines.findIndex((line) => line === "  jobs-ci:");
  if (jobStart < 0) {
    return ["Jobs CI workflow must define the jobs-ci job"];
  }

  const nextJobOffset = lines
    .slice(jobStart + 1)
    .findIndex((line) => /^  [^\s].*:$/.test(line));
  const jobEnd =
    nextJobOffset < 0 ? lines.length : jobStart + 1 + nextJobOffset;
  const timeoutLines = lines
    .slice(jobStart + 1, jobEnd)
    .filter((line) => /^    timeout-minutes:\s*/.test(line));
  if (timeoutLines.length !== 1) {
    return ["Jobs CI jobs-ci job must define exactly one timeout-minutes value"];
  }

  const match = timeoutLines[0].match(/^    timeout-minutes:\s*(\d+)\s*$/);
  if (!match || Number(match[1]) !== JOBS_CI_TIMEOUT_MINUTES) {
    return [
      `Jobs CI jobs-ci timeout must remain exactly ${JOBS_CI_TIMEOUT_MINUTES} minutes`,
    ];
  }
  return [];
}

function testJobsCiTimeBudgetGuard() {
  const workflow = fs.readFileSync(
    path.join(repoRoot, ".github/workflows/jobs-ci.yml"),
    "utf8",
  );
  assert.deepEqual(validateJobsCiTimeBudget(workflow), []);

  const lowered = workflow.replace(
    `    timeout-minutes: ${JOBS_CI_TIMEOUT_MINUTES}`,
    "    timeout-minutes: 45",
  );
  assert(
    validateJobsCiTimeBudget(lowered).some((issue) =>
      issue.includes(`exactly ${JOBS_CI_TIMEOUT_MINUTES} minutes`),
    ),
    "Jobs CI guard must reject a regression to the exhausted 45-minute budget",
  );
}

function validateIsolatedTestDiskAuthority(workflow, launcher) {
  const issues = [];
  const occurrences = (value, needle) => value.split(needle).length - 1;
  const selfTestCommand = `bash ${ISOLATED_TEST_LAUNCHER} --self-test`;

  if (occurrences(workflow, selfTestCommand) !== 1) {
    issues.push(
      `Jobs CI must run ${ISOLATED_TEST_LAUNCHER} --self-test exactly once`,
    );
  }

  for (const required of [
    'export CARGO_TARGET_DIR="$run_root/cargo-target"',
    'export TMPDIR="$run_root/tmp"',
    'export TMP="$run_root/tmp"',
    'export TEMP="$run_root/tmp"',
    'export SQLITE_TMPDIR="$run_root/tmp"',
    'export BLUEY_DB_PATH="$run_root/data/bluey-test.sqlite"',
    "export BLUEY_SERVER_DB_BACKEND=sqlite",
    "unset BLUEY_DATABASE_URL BLUEY_TEST_POSTGRES_URL",
    'export BLUEY_LOG_DIR="$run_root/logs"',
    'export CUE_LOG_DIR="$BLUEY_LOG_DIR"',
    "export CARGO_INCREMENTAL=0",
    "export CARGO_PROFILE_DEV_INCREMENTAL=false",
    "export CARGO_PROFILE_TEST_INCREMENTAL=false",
    "export CARGO_PROFILE_DEV_DEBUG=0",
    "export CARGO_PROFILE_TEST_DEBUG=0",
    "trap on_exit EXIT",
    "trap 'on_signal HUP 129' HUP",
    "trap 'on_signal INT 130' INT",
    "trap 'on_signal TERM 143' TERM",
    "install_deferred_signal_traps",
    'readonly COMMAND_READY_POLL_ATTEMPTS=500',
    'readonly STOP_POLL_ATTEMPTS=50',
    "run_root_created=1",
    "dispatch_deferred_signal",
    'marker_established=1\n  write_marker starting 0',
    'if [[ "$marker_established" -eq 1 ]]; then',
    'readonly STALE_GRACE_SECONDS=60',
    'readonly MAX_CANONICAL_DECIMAL_DIGITS=15',
    'readonly WORKTREE_LIST_MAX_BYTES=1048576',
    'readonly WORKTREE_LIST_CAPTURE_BYTES=1048578',
    'readonly WORKTREE_LIST_TIMEOUT_SECONDS=10',
    "set -o pipefail",
    "is_canonical_decimal() {",
    'is_canonical_decimal "$marker_created_epoch"',
    '[[ "$run_epoch" == "$marker_created_epoch" && "$run_pid" == "$marker_owner_pid" ]]',
    '[[ "$marker_process_group_id" == "0" ]] || return 1',
    'is_canonical_decimal_gt_one "$marker_process_group_id"',
    "run parent must not contain dot segments",
    "run parent must stay under local /tmp",
    "existing run parent must already have mode 0700",
    'validate_no_git_ancestor "$desired"',
    'validate_no_known_worktree_overlap "$desired"',
    '[[ ! -e "$probe/.git" && ! -L "$probe/.git" ]] || return 1',
    'local producer_status=0 LC_ALL=C sentinel=$\'\\034\'',
    'alarm $timeout;',
    'git -C "$repo_root" worktree list --porcelain 2>/dev/null',
    'producer_status=$?',
    'exit "$producer_status"',
    '} | /usr/bin/head -c "$WORKTREE_LIST_CAPTURE_BYTES")" || return 1',
    '[[ "$listing" == *"$sentinel" ]] || return 1',
    'listing="${listing%"$sentinel"}"',
    '[[ "${#listing}" -le "$WORKTREE_LIST_MAX_BYTES" ]] || return 1',
    "run parent must not overlap any registered Git worktree",
    "/usr/bin/perl -MPOSIX=setsid",
    "-MFcntl=O_WRONLY,O_CREAT,O_EXCL",
    '$SIG{HUP} = "DEFAULT";',
    '$SIG{INT} = "DEFAULT";',
    '$SIG{TERM} = "DEFAULT";',
    "exit 125 if setsid() == -1;",
    "O_WRONLY | O_CREAT | O_EXCL, 0600",
    'ready="$run_root/.command-ready"',
    '[[ "$ready_pid" == "$command_pid" ]]',
    '[[ "$observed_group_id" == "$command_pid" ]]',
    '[[ "$observed_group_id" == "$command_pid" ]] && break',
    '/bin/kill -s "$signal_name" -- "-$process_group_id"',
    '/bin/kill -s KILL -- "-$process_group_id"',
    '/bin/kill -s KILL -- "$command_pid"',
    'pid_can_execute "$command_pid" && return 1',
    'group_is_active "$marker_process_group_id"',
    'output="$(ps -eo pgid=,stat= 2>/dev/null)"',
    '[[ "$observed_pgid" == "$pgid" && "$observed_state" != Z* ]]',
    '[[ "$status" -eq 0 ]] || return 0',
    '[[ -d "$candidate" && ! -L "$candidate" && -O "$candidate" ]] || continue',
    '[[ -f "$marker" && ! -L "$marker" && -O "$marker" ]] || return 1',
    'if ! read_marker "$marker"; then',
    "stale-run recovery blocked by missing or malformed owned metadata",
    "eligible stale-run deletion failed for exact root",
    "No test command was started.",
    "readonly STALE_RECOVERY_FAILURE_STATUS=75",
    "reap_stale_runs ||",
    'read_path_identity "$candidate"',
    'accepted_root_device_inode="$observed_path_device_inode"',
    'accepted_marker_device_inode="$observed_path_device_inode"',
    'run_parent_device_inode="$observed_path_device_inode"',
    'current_root_device_inode="$observed_path_device_inode"',
    'current_marker_device_inode="$observed_path_device_inode"',
    'current_marker_modified_epoch="$observed_path_modified_epoch"',
    "create_delete_quarantine",
    "remove_empty_delete_quarantine",
    "verify_quarantined_payload",
    "delete_quarantined_payload",
    "quarantine_and_delete_root",
    "/usr/bin/perl -e 'exit(rename($ARGV[0], $ARGV[1]) ? 0 : 1)'",
    'if [[ -e "$source" || -L "$source" ]]',
    'Darwin) /usr/bin/find -x . -depth -mindepth 1 -delete ;;',
    'Linux) /usr/bin/find . -xdev -depth -mindepth 1 -delete ;;',
    'hook="${BLUEY_TEST_INJECT_AFTER_QUARANTINE_VERIFY_HOOK:-}"',
    "set -o noclobber",
    'next_device_inode="$observed_path_device_inode"',
    'next_modified_epoch="$observed_path_modified_epoch"',
    'hook="${BLUEY_TEST_INJECT_AFTER_MARKER_RENAME_HOOK:-}"',
    "quarantined run-root identity changed",
    "final quarantined authority changed",
    "quarantine deletion failed",
    "marker_fields_match",
    'second_now="$(date +%s)"',
    "stale-run path identity changed during recovery",
    "stale-run age eligibility changed during recovery",
    'pid_is_active "$marker_owner_pid" && continue',
    "command passed but cleanup failed",
    "cleanup failed; preserving command status",
    "exit 125 unless chdir $repo_root;",
    'if [[ "${1:-}" == "--self-test" ]]',
    'if [[ "${1:-}" == "all" ]]',
  ]) {
    if (!launcher.includes(required)) {
      issues.push(`${ISOLATED_TEST_LAUNCHER} is missing authority: ${required}`);
    }
  }

  for (const requiredScenario of [
    "poisoned-environment isolation failed",
    "ordinary descendant survived",
    "reaper touched an active process group",
    "inactive marker-valid stale root survived",
    "reaper followed a candidate symlink",
    "reaper followed a retained marker symlink",
    "unsafe parent was mutated before rejection",
    "post-mkdir signal run root survived",
    "for signal_spec in HUP:129 INT:130 TERM:143; do",
    "child did not receive $signal_name",
    "all mapping did not run from repository root",
    "direct command did not run from repository root",
    "reaper treated ps failure as inactive",
    "zombie-only process group blocked cleanup",
    "marker-missing cleanup did not fail closed",
    "marker-corrupt cleanup did not fail closed",
    "stale deletion failure started child command or lost evidence",
    "retained marker did not block a new command",
    "linked retained marker did not block a new command",
    "future-dated marker did not block before child start",
    "for marker_case in",
    "overflow leading-zero epoch-drift pid-drift starting-group running-zero",
    "$marker_case marker authority did not block before child start",
    "young root with stale marker did not block before child start",
    "same-field marker inode replacement did not block before child start",
    "same-field marker inode replacement missed path-identity authority",
    "for current_identity_case in marker-inode marker-fields root-swap; do",
    "current $current_identity_case cleanup did not fail closed",
    "current root substitution removed the original command data",
    "current root substitution lacked retention guidance",
    "current marker inode replacement lacked retention guidance",
    "current marker field rewrite lacked retention guidance",
    "pre-running-marker root substitution gained command authority",
    "pre-running-marker root substitution deleted retained evidence",
    "pre-running-marker root substitution lacked retention guidance",
    "post-rename marker substitution gained command authority",
    "post-rename marker substitution deleted retained evidence",
    "post-rename marker substitution lacked retention guidance",
    "pre-delete root substitution did not fail closed",
    "pre-delete root substitution removed original or substitute data",
    "pre-delete root substitution lacked quarantine guidance",
    "final-delete current substitution did not fail closed",
    "final-delete current substitution removed retained evidence",
    "final-delete current substitution lacked quarantine guidance",
    "retained quarantine did not block the next command",
    "final-delete stale substitution did not block the next command",
    "final-delete stale substitution removed retained evidence",
    "final-delete stale substitution lacked quarantine guidance",
    "for exact_git_kind in directory file symlink; do",
    "existing exact Git $exact_git_kind parent was accepted",
    "worktree enumeration failure did not block the command",
    "worktree enumeration failure created a test root",
    "oversized worktree enumeration did not block the command",
    "oversized worktree enumeration created a test root",
    "pre-gate $pre_gate_signal status was not preserved",
    "pre-gate $pre_gate_signal started the user command or lacked handshake proof",
    "pre-gate $pre_gate_signal wrapper survived",
    "pre-gate $pre_gate_signal exact run root survived",
    "cleanup failure did not fail a successful command",
    "cleanup failure hid child status",
    "cleanup failure hid signal status",
    "test run residue remains",
  ]) {
    if (!launcher.includes(requiredScenario)) {
      issues.push(`${ISOLATED_TEST_LAUNCHER} is missing scenario: ${requiredScenario}`);
    }
  }

  for (const [field, comparison] of MARKER_FIELD_COMPARISONS) {
    if (!launcher.includes(comparison)) {
      issues.push(`marker reread is missing exact field comparison: ${field}`);
    }
  }

  for (const [field, binding] of CURRENT_MARKER_SNAPSHOT_BINDINGS) {
    if (occurrences(launcher, binding) !== 1) {
      issues.push(`current marker snapshot is missing exact field binding: ${field}`);
    }
  }

  for (const [field, argument] of CURRENT_MARKER_REVALIDATION_ARGUMENTS) {
    if (occurrences(launcher, argument) !== 2) {
      issues.push(`current marker cleanup is missing exact field revalidation: ${field}`);
    }
  }

  for (const [field, argument] of EXPECTED_MARKER_REVALIDATION_ARGUMENTS) {
    if (occurrences(launcher, argument) !== 5) {
      issues.push(`quarantined marker is missing exact field revalidation: ${field}`);
    }
  }

  if (
    occurrences(
      launcher,
      'current_root_device_inode="$observed_path_device_inode"',
    ) !== 1 ||
    occurrences(
      launcher,
      '"$observed_path_device_inode" == "$current_root_device_inode"',
    ) !== 3 ||
    occurrences(
      launcher,
      '"$observed_path_device_inode" != "$current_root_device_inode"',
    ) !== 2 ||
    occurrences(
      launcher,
      '"$first_marker_device_inode" != "$current_marker_device_inode"',
    ) !== 1 ||
    occurrences(
      launcher,
      '"$first_marker_modified_epoch" != "$current_marker_modified_epoch"',
    ) !== 1 ||
    occurrences(
      launcher,
      '"$observed_path_device_inode" != "$current_marker_device_inode"',
    ) !== 1 ||
    occurrences(
      launcher,
      '"$observed_path_modified_epoch" != "$current_marker_modified_epoch"',
    ) !== 1
  ) {
    issues.push(
      "current cleanup must bind established root, marker device/inode/mtime, and fields",
    );
  }

  const writeMarkerStart = launcher.indexOf("write_marker() {");
  const stopProcessGroupStart = launcher.indexOf(
    "stop_process_group() {",
    writeMarkerStart,
  );
  const writeMarkerAuthority =
    writeMarkerStart >= 0 && stopProcessGroupStart > writeMarkerStart
      ? launcher.slice(writeMarkerStart, stopProcessGroupStart)
      : "";
  const markerTempBindIndex = writeMarkerAuthority.indexOf(
    'next_device_inode="$observed_path_device_inode"',
  );
  const markerTempMtimeBindIndex = writeMarkerAuthority.indexOf(
    'next_modified_epoch="$observed_path_modified_epoch"',
  );
  const markerRenameIndex = writeMarkerAuthority.indexOf(
    "/usr/bin/perl -e 'exit(rename($ARGV[0], $ARGV[1]) ? 0 : 1)'",
  );
  const markerTransitionHookIndex = writeMarkerAuthority.indexOf(
    'hook="${BLUEY_TEST_INJECT_AFTER_MARKER_RENAME_HOOK:-}"',
  );
  const markerIdentityIndex = writeMarkerAuthority.indexOf(
    '"$observed_path_device_inode" == "$next_device_inode"',
  );
  const markerSnapshotIndex = writeMarkerAuthority.indexOf(
    'current_marker_device_inode="$observed_path_device_inode"',
  );
  if (
    markerTempBindIndex < 0 ||
    markerTempMtimeBindIndex < markerTempBindIndex ||
    markerRenameIndex < markerTempMtimeBindIndex ||
    markerTransitionHookIndex < markerRenameIndex ||
    markerIdentityIndex < markerTransitionHookIndex ||
    markerSnapshotIndex < markerIdentityIndex ||
    occurrences(
      writeMarkerAuthority,
      '"$observed_path_device_inode" == "$current_root_device_inode"',
    ) !== 3 ||
    occurrences(
      writeMarkerAuthority,
      '"$observed_path_device_inode" == "$next_device_inode"',
    ) !== 1 ||
    occurrences(
      writeMarkerAuthority,
      '"$observed_path_modified_epoch" == "$next_modified_epoch"',
    ) !== 1 ||
    writeMarkerAuthority.includes(
      'current_root_device_inode="$observed_path_device_inode"',
    )
  ) {
    issues.push(
      "marker transition must preserve immutable root and pre-rename marker identity",
    );
  }

  if (
    occurrences(
      launcher,
      '"$first_marker_device_inode" != "$expected_marker_device_inode"',
    ) !== 1 ||
    occurrences(
      launcher,
      '"$first_marker_modified_epoch" != "$expected_marker_modified_epoch"',
    ) !== 1 ||
    occurrences(
      launcher,
      '"$observed_path_device_inode" == "$expected_marker_device_inode"',
    ) !== 2 ||
    occurrences(
      launcher,
      '"$observed_path_modified_epoch" == "$expected_marker_modified_epoch"',
    ) !== 2
  ) {
    issues.push(
      "quarantined cleanup must bind the moved marker device/inode/mtime and fields",
    );
  }

  if (
    occurrences(launcher, 'validate_no_git_ancestor "$desired"') !== 1 ||
    launcher.includes('validate_no_git_ancestor "$parent_physical"') ||
    occurrences(launcher, "set -o pipefail") !== 1 ||
    !launcher.includes('alarm $timeout;') ||
    !launcher.includes('producer_status=$?') ||
    !launcher.includes('exit "$producer_status"') ||
    !launcher.includes(
      '} | /usr/bin/head -c "$WORKTREE_LIST_CAPTURE_BYTES")" || return 1',
    ) ||
    !launcher.includes('[[ "$listing" == *"$sentinel" ]] || return 1') ||
    !launcher.includes('listing="${listing%"$sentinel"}"') ||
    !launcher.includes('[[ "${#listing}" -le "$WORKTREE_LIST_MAX_BYTES" ]] || return 1') ||
    launcher.includes(
      'listing="$(git -C "$repo_root" worktree list --porcelain 2>/dev/null)"',
    ) ||
    launcher.includes(
      '< <(git -C "$repo_root" worktree list --porcelain)',
    )
  ) {
    issues.push(
      "exact requested parent Git ancestry and producer-status-checked worktree enumeration must fail closed",
    );
  }

  const worktreeProducerIndex = launcher.indexOf(
    'git -C "$repo_root" worktree list --porcelain 2>/dev/null',
  );
  const worktreeCapIndex = launcher.indexOf(
    '} | /usr/bin/head -c "$WORKTREE_LIST_CAPTURE_BYTES")" || return 1',
  );
  const worktreeSentinelIndex = launcher.indexOf(
    '[[ "$listing" == *"$sentinel" ]] || return 1',
  );
  const worktreeLengthIndex = launcher.indexOf(
    '[[ "${#listing}" -le "$WORKTREE_LIST_MAX_BYTES" ]] || return 1',
  );
  const worktreeParseIndex = launcher.indexOf(
    'while IFS= read -r line; do',
    worktreeLengthIndex,
  );
  if (
    worktreeProducerIndex < 0 ||
    worktreeCapIndex < worktreeProducerIndex ||
    worktreeSentinelIndex < worktreeCapIndex ||
    worktreeLengthIndex < worktreeSentinelIndex ||
    worktreeParseIndex < worktreeLengthIndex
  ) {
    issues.push(
      "worktree discovery must preserve producer status before bounded parsing",
    );
  }

  const quarantineStart = launcher.indexOf("quarantine_and_delete_root() {");
  const reaperStart = launcher.indexOf("reap_stale_runs() {", quarantineStart);
  const reaperEnd = launcher.indexOf('\nrun_root=""', reaperStart);
  const quarantineAuthority =
    quarantineStart >= 0 && reaperStart > quarantineStart
      ? launcher.slice(quarantineStart, reaperStart)
      : "";
  const reaperAuthority =
    reaperStart >= 0 && reaperEnd > reaperStart
      ? launcher.slice(reaperStart, reaperEnd)
      : "";
  const quarantineRenameIndex = quarantineAuthority.indexOf(
    "/usr/bin/perl -e 'exit(rename($ARGV[0], $ARGV[1]) ? 0 : 1)'",
  );
  const sourceAbsenceIndex = quarantineAuthority.indexOf(
    'if [[ -e "$source" || -L "$source" ]]',
  );
  const firstQuarantineVerifyIndex = quarantineAuthority.indexOf(
    "! verify_quarantined_payload \\",
  );
  const finalHookIndex = quarantineAuthority.indexOf(
    'hook="${BLUEY_TEST_INJECT_AFTER_QUARANTINE_VERIFY_HOOK:-}"',
  );
  const secondQuarantineVerifyIndex = quarantineAuthority.indexOf(
    "if ! verify_quarantined_payload \\",
    finalHookIndex,
  );
  const quarantineDeleteIndex = quarantineAuthority.indexOf(
    "if ! delete_quarantined_payload \\",
  );
  if (
    occurrences(launcher, "quarantine_and_delete_root() {") !== 1 ||
    occurrences(launcher, "quarantine_and_delete_root \\") !== 2 ||
    occurrences(launcher, "verify_quarantined_payload() {") !== 1 ||
    occurrences(launcher, "delete_quarantined_payload() {") !== 1 ||
    quarantineRenameIndex < 0 ||
    sourceAbsenceIndex < quarantineRenameIndex ||
    firstQuarantineVerifyIndex < sourceAbsenceIndex ||
    finalHookIndex < firstQuarantineVerifyIndex ||
    secondQuarantineVerifyIndex < finalHookIndex ||
    quarantineDeleteIndex < secondQuarantineVerifyIndex ||
    !quarantineAuthority.includes(
      '"$observed_path_device_inode" == "$expected_root_device_inode"',
    ) ||
    occurrences(
      launcher,
      '"$observed_path_device_inode" == "$expected_root_device_inode"',
    ) !== 5 ||
    occurrences(
      launcher,
      '"$observed_path_device_inode" == "$run_parent_device_inode"',
    ) !== 3 ||
    occurrences(
      launcher,
      '"$observed_path_device_inode" == "$delete_quarantine_device_inode"',
    ) !== 3 ||
    launcher.includes('rm -rf -- "$run_root"') ||
    launcher.includes('rm -rf -- "$candidate"') ||
    launcher.includes('rm -rf -- "$physical"') ||
    launcher.includes('rm -rf -- "$moved_root"')
  ) {
    issues.push(
      "current and stale deletion must use one atomic quarantine authority",
    );
  }

  const exactGitParentIndex = launcher.indexOf(
    'validate_no_git_ancestor "$desired"',
  );
  const exactWorktreeIndex = launcher.indexOf(
    'validate_no_known_worktree_overlap "$desired"',
  );
  const parentMutationIndex = launcher.indexOf(
    'if [[ -e "$desired" || -L "$desired" ]]; then',
  );
  if (
    exactGitParentIndex < 0 ||
    exactWorktreeIndex < exactGitParentIndex ||
    parentMutationIndex < exactWorktreeIndex
  ) {
    issues.push(
      "exact Git and registered-worktree rejection must precede run-parent mutation",
    );
  }

  const wrapperResetIndex = launcher.indexOf('$SIG{HUP} = "DEFAULT";');
  const wrapperSetsidIndex = launcher.indexOf("exit 125 if setsid() == -1;");
  const wrapperReadyIndex = launcher.indexOf("sysopen(my $handle, $ready");
  const wrapperGateWaitIndex = launcher.indexOf("while (!-e $gate)");
  const wrapperSpawnIndex = launcher.indexOf("/usr/bin/perl -MPOSIX=setsid");
  const preSpawnDeferredIndex = launcher.lastIndexOf(
    "install_deferred_signal_traps",
    wrapperSpawnIndex,
  );
  const readyPidIndex = launcher.indexOf(
    '[[ "$ready_pid" == "$command_pid" ]]',
    wrapperSpawnIndex,
  );
  const pgidIndex = launcher.indexOf(
    '[[ "$observed_group_id" == "$command_pid" ]]',
    wrapperSpawnIndex,
  );
  const runningMarkerIndex = launcher.indexOf(
    'write_marker running "$process_group_id"',
    wrapperSpawnIndex,
  );
  const deferredDispatchIndex = launcher.indexOf(
    "dispatch_deferred_signal",
    runningMarkerIndex,
  );
  const openGateIndex = launcher.indexOf(
    "printf 'start\\n' > \"$gate\"",
    deferredDispatchIndex,
  );
  if (
    wrapperResetIndex < wrapperSpawnIndex ||
    wrapperSetsidIndex < wrapperResetIndex ||
    wrapperReadyIndex < wrapperSetsidIndex ||
    wrapperGateWaitIndex < wrapperReadyIndex ||
    preSpawnDeferredIndex < 0 ||
    !launcher.includes(
      'ready="$run_root/.command-ready"\n  install_deferred_signal_traps\n  /usr/bin/perl',
    ) ||
    readyPidIndex < wrapperSpawnIndex ||
    pgidIndex < readyPidIndex ||
    runningMarkerIndex < pgidIndex ||
    deferredDispatchIndex < runningMarkerIndex ||
    !launcher.includes(
      "  dispatch_deferred_signal\n  printf 'start\\n' > \"$gate\"",
    ) ||
    openGateIndex < deferredDispatchIndex
  ) {
    issues.push(
      "private wrapper handshake and deferred pre-gate signal order must remain exact",
    );
  }

  if (
    occurrences(launcher, "for signal_spec in HUP:129 INT:130 TERM:143; do") !== 2
  ) {
    issues.push("pre-gate and running child receipt tests must cover HUP, INT, and TERM");
  }

  const trapIndex = launcher.indexOf("trap on_exit EXIT");
  const deferredMkdirIndex = launcher.indexOf(
    'install_deferred_signal_traps\n    if mkdir -m 0700 -- "$run_root"',
  );
  const postMkdirHookIndex = launcher.indexOf(
    "BLUEY_TEST_INJECT_AFTER_RUN_ROOT_MKDIR",
    deferredMkdirIndex,
  );
  const ownedIndex = launcher.indexOf("run_root_created=1", deferredMkdirIndex);
  const dispatchIndex = launcher.indexOf("dispatch_deferred_signal", ownedIndex);
  if (
    trapIndex < 0 ||
    deferredMkdirIndex < 0 ||
    postMkdirHookIndex < deferredMkdirIndex ||
    postMkdirHookIndex > ownedIndex ||
    ownedIndex < deferredMkdirIndex ||
    dispatchIndex < ownedIndex
  ) {
    issues.push("mkdir ownership window must defer signals until the root is owned");
  }

  if (
    !/if ! quarantine_and_delete_root \\\n[\s\S]{0,1000}return 1/.test(reaperAuthority) ||
    !/if ! read_marker "\$marker"; then[\s\S]{0,500}return 1/.test(
      launcher,
    )
  ) {
    issues.push("stale-run deletion and unknown metadata must block a new command");
  }

  if (
    !/is_canonical_decimal\(\) \{[\s\S]{0,300}\^\[1-9\]\[0-9\]\*\$[\s\S]{0,200}MAX_CANONICAL_DECIMAL_DIGITS/.test(
      launcher,
    )
  ) {
    issues.push("marker arithmetic must use canonical bounded decimals");
  }

  if (
    !launcher.includes(
      '[[ "$run_epoch" == "$marker_created_epoch" && "$run_pid" == "$marker_owner_pid" ]]',
    )
  ) {
    issues.push("marker run name must bind its exact epoch and owner PID");
  }

  if (
    !launcher.includes(
      '[[ "$marker_process_group_id" == "0" ]] || return 1',
    ) ||
    !/else\s+is_canonical_decimal_gt_one "\$marker_process_group_id" \|\| return 1/.test(
      launcher,
    )
  ) {
    issues.push("marker state must bind its process-group identity");
  }

  const ageRevalidations = launcher.match(
    /validate_stale_candidate_age \\\n\s+"\$(?:now|second_now)"/g,
  ) ?? [];
  const initialAgeRevalidations = launcher.match(
    /validate_stale_candidate_age \\\n\s+"\$now"/g,
  ) ?? [];
  const finalAgeRevalidations = launcher.match(
    /validate_stale_candidate_age \\\n\s+"\$second_now"/g,
  ) ?? [];
  const ownerRevalidations = launcher.match(
    /pid_is_active "\$marker_owner_pid" && continue/g,
  ) ?? [];
  const groupRevalidations = launcher.match(
    /\[\[ "\$marker_state" == "running" \]\] && group_is_active "\$marker_process_group_id"/g,
  ) ?? [];
  if (
    ageRevalidations.length !== 2 ||
    initialAgeRevalidations.length !== 1 ||
    finalAgeRevalidations.length !== 1 ||
    ownerRevalidations.length !== 2 ||
    groupRevalidations.length !== 2
  ) {
    issues.push("stale deletion must rerun full age, owner, and group eligibility");
  }

  if (
    !launcher.includes(
      'if [[ "$marker_created_epoch" -gt "$now" || "$root_modified" -gt "$now" ||',
    ) ||
    !launcher.includes(
      'if [[ "$root_age" -lt "$STALE_GRACE_SECONDS" ||',
    ) ||
    !launcher.includes(
      '"$marker_age" -lt "$STALE_GRACE_SECONDS" ]];',
    )
  ) {
    issues.push("stale age must reject future metadata and young copied roots");
  }

  if (
    !launcher.includes("marker_fields_match") ||
    !launcher.includes(
      '"$second_root_device_inode" != "$accepted_root_device_inode"',
    ) ||
    !launcher.includes(
      '"$second_marker_device_inode" != "$accepted_marker_device_inode"',
    ) ||
    !launcher.includes(
      '"$second_root_modified_epoch" != "$accepted_root_modified_epoch"',
    ) ||
    !launcher.includes(
      '"$second_marker_modified_epoch" != "$accepted_marker_modified_epoch"',
    )
  ) {
    issues.push("stale deletion must retain exact field, device, inode, and mtime identity");
  }

  return issues;
}

function validateMicDeviceTempAuthority(source) {
  const issues = [];
  for (const required of [
    "tempfile::Builder::new()",
    '.prefix("bluey-mic-device-")',
    ".tempdir_in(std::env::temp_dir())",
  ]) {
    if (!source.includes(required)) {
      issues.push(`${MIC_DEVICE_SELECTION_TEST} is missing: ${required}`);
    }
  }
  if (/['"]\/tmp\/cue_test_mic_device_/.test(source)) {
    issues.push(`${MIC_DEVICE_SELECTION_TEST} must not hardcode a shared /tmp database`);
  }
  return issues;
}

function testIsolatedTestDiskAuthorityGuard() {
  const workflow = fs.readFileSync(
    path.join(repoRoot, ".github/workflows/jobs-ci.yml"),
    "utf8",
  );
  const launcher = fs.readFileSync(
    path.join(repoRoot, ISOLATED_TEST_LAUNCHER),
    "utf8",
  );
  const micDeviceTest = fs.readFileSync(
    path.join(repoRoot, MIC_DEVICE_SELECTION_TEST),
    "utf8",
  );
  assert.deepEqual(
    validateIsolatedTestDiskAuthority(workflow, launcher),
    [],
  );
  assert.deepEqual(validateMicDeviceTempAuthority(micDeviceTest), []);

  const unwiredWorkflow = workflow.replace(
    `        run: bash ${ISOLATED_TEST_LAUNCHER} --self-test`,
    "        run: true",
  );
  assert(
    validateIsolatedTestDiskAuthority(unwiredWorkflow, launcher).some((issue) =>
      issue.includes("exactly once"),
    ),
    "Jobs CI guard must reject removal of the isolated launcher self-test",
  );

  const sharedCargoTarget = launcher.replace(
    'export CARGO_TARGET_DIR="$run_root/cargo-target"',
    'export CARGO_TARGET_DIR="$repo_root/target"',
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, sharedCargoTarget).some((issue) =>
      issue.includes("CARGO_TARGET_DIR"),
    ),
    "Jobs CI guard must reject loss of Cargo target isolation",
  );

  const poisonedDatabase = launcher.replace(
    "unset BLUEY_DATABASE_URL BLUEY_TEST_POSTGRES_URL",
    ": # inherited database URLs retained",
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, poisonedDatabase).some(
      (issue) => issue.includes("BLUEY_DATABASE_URL"),
    ),
    "Jobs CI guard must reject inherited database authority",
  );

  const unsafeSymlinkReaper = launcher.replace(
    '[[ -d "$candidate" && ! -L "$candidate" && -O "$candidate" ]] || continue',
    '[[ -d "$candidate" && -O "$candidate" ]] || continue',
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, unsafeSymlinkReaper).some(
      (issue) => issue.includes('! -L "$candidate"'),
    ),
    "Jobs CI guard must reject loss of stale-run symlink safety",
  );

  const unsafeMkdirWindow = launcher.replace(
    'install_deferred_signal_traps\n    if mkdir -m 0700 -- "$run_root"',
    'if mkdir -m 0700 -- "$run_root"',
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, unsafeMkdirWindow).some((issue) =>
      issue.includes("mkdir ownership window"),
    ),
    "Jobs CI guard must reject an unprotected mkdir ownership window",
  );

  const unsafeMarkerCleanup = launcher.replace(
    'if [[ "$marker_established" -eq 1 ]]; then',
    "if false; then",
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, unsafeMarkerCleanup).some((issue) =>
      issue.includes("marker_established"),
    ),
    "Jobs CI guard must reject cleanup that ignores an established marker",
  );

  const unsafeProcessInspection = launcher.replace(
    '[[ "$status" -eq 0 ]] || return 0',
    '[[ "$status" -eq 1 ]] && return 1',
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, unsafeProcessInspection).some((issue) =>
      issue.includes("status"),
    ),
    "Jobs CI guard must reject process inspection that treats ps errors as inactive",
  );

  const builtinGroupSignal = launcher.replace(
    '/bin/kill -s "$signal_name" -- "-$process_group_id"',
    'kill -s "$signal_name" "-$process_group_id"',
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, builtinGroupSignal).some((issue) =>
      issue.includes("/bin/kill"),
    ),
    "Jobs CI guard must reject non-portable builtin process-group signaling",
  );

  const incompleteSignalReceipt = launcher.replace(
    "for signal_spec in HUP:129 INT:130 TERM:143; do",
    "for signal_spec in TERM:143; do",
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, incompleteSignalReceipt).some((issue) =>
      issue.includes("receipt tests must cover HUP, INT, and TERM"),
    ),
    "Jobs CI guard must require child receipt tests for HUP, INT, and TERM",
  );

  const ignoredStaleDeletionFailure = replaceOccurrence(
    launcher,
    "    if ! quarantine_and_delete_root \\",
    "    if false && ! quarantine_and_delete_root \\",
    1,
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, ignoredStaleDeletionFailure).some(
      (issue) => issue.includes("stale-run deletion"),
    ),
    "Jobs CI guard must reject proceeding after stale deletion failure",
  );

  const ignoredUnknownMetadata = launcher.replace(
    /if ! read_marker "\$marker"; then([\s\S]{0,500}?)return 1/,
    'if ! read_marker "$marker"; then$1continue',
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, ignoredUnknownMetadata).some(
      (issue) => issue.includes("unknown metadata"),
    ),
    "Jobs CI guard must reject proceeding past missing retained-run metadata",
  );

  const unboundedMarkerDecimal = launcher.replace(
    '[[ "${#value}" -le "$MAX_CANONICAL_DECIMAL_DIGITS" ]]',
    "return 0",
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, unboundedMarkerDecimal).some(
      (issue) => issue.includes("canonical bounded decimals"),
    ),
    "Jobs CI guard must reject unbounded or non-canonical marker arithmetic",
  );

  const uncoupledRunName = launcher.replace(
    '[[ "$run_epoch" == "$marker_created_epoch" && "$run_pid" == "$marker_owner_pid" ]]',
    '[[ -n "$run_nonce" ]]',
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, uncoupledRunName).some((issue) =>
      issue.includes("exact epoch and owner PID"),
    ),
    "Jobs CI guard must reject loss of run-name epoch/PID binding",
  );

  const uncoupledMarkerState = launcher.replace(
    '[[ "$marker_process_group_id" == "0" ]] || return 1',
    ": # starting state accepted with any process group",
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, uncoupledMarkerState).some((issue) =>
      issue.includes("state must bind"),
    ),
    "Jobs CI guard must reject loss of marker state/process-group binding",
  );

  const trustedYoungCopiedRoot = launcher.replace(
    '"$marker_age" -lt "$STALE_GRACE_SECONDS" ]];',
    '"$marker_age" -lt "0" ]];',
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, trustedYoungCopiedRoot).some((issue) =>
      issue.includes("young copied roots"),
    ),
    "Jobs CI guard must reject deletion authority from an old marker in a young root",
  );

  const acceptedFutureMarker = launcher.replace(
    'if [[ "$marker_created_epoch" -gt "$now" || "$root_modified" -gt "$now" ||',
    'if [[ "$marker_created_epoch" -lt "0" || "$root_modified" -gt "$now" ||',
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, acceptedFutureMarker).some((issue) =>
      issue.includes("future metadata"),
    ),
    "Jobs CI guard must reject loss of future-dated marker blocking",
  );

  const skippedFinalAgeRevalidation = launcher.replace(
    '      "$second_now" "$second_root_modified_epoch" "$second_marker_modified_epoch"',
    '      "$now" "$second_root_modified_epoch" "$second_marker_modified_epoch"',
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, skippedFinalAgeRevalidation).some(
      (issue) => issue.includes("rerun full age"),
    ),
    "Jobs CI guard must reject loss of fresh second age validation",
  );

  const ignoredMarkerIdentitySwap = launcher.replace(
    '"$second_marker_device_inode" != "$accepted_marker_device_inode"',
    '"$second_marker_device_inode" != "$second_marker_device_inode"',
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, ignoredMarkerIdentitySwap).some(
      (issue) => issue.includes("device, inode, and mtime identity"),
    ),
    "Jobs CI guard must reject loss of same-field marker inode binding",
  );

  for (const [field, comparison] of MARKER_FIELD_COMPARISONS) {
    const ignoredField = launcher.replace(comparison, '"ignored" == "ignored"');
    assert(
      validateIsolatedTestDiskAuthority(workflow, ignoredField).some((issue) =>
        issue.includes(`exact field comparison: ${field}`),
      ),
      `Jobs CI guard must reject loss of the ${field} reread comparison`,
    );
  }

  const unboundCurrentRootSnapshot = launcher
    .split('current_root_device_inode="$observed_path_device_inode"')
    .join('current_root_device_inode="unbound"');
  assert(
    validateIsolatedTestDiskAuthority(workflow, unboundCurrentRootSnapshot).some(
      (issue) => issue.includes("current cleanup must bind established root"),
    ),
    "Jobs CI guard must reject loss of the current run-root identity snapshot",
  );

  const incompleteCurrentIdentityCoverage = launcher.replace(
    "for current_identity_case in marker-inode marker-fields root-swap; do",
    "for current_identity_case in marker-inode; do",
  );
  assert(
    validateIsolatedTestDiskAuthority(
      workflow,
      incompleteCurrentIdentityCoverage,
    ).some((issue) => issue.includes("marker-inode marker-fields root-swap")),
    "Jobs CI guard must run all current root/marker substitution fixtures",
  );

  for (const [comparison, replacement, count] of [
    [
      '"$observed_path_device_inode" == "$current_root_device_inode"',
      '"$observed_path_device_inode" == "$observed_path_device_inode"',
      3,
    ],
    [
      '"$observed_path_device_inode" != "$current_root_device_inode"',
      '"$observed_path_device_inode" != "$observed_path_device_inode"',
      2,
    ],
  ]) {
    for (let occurrence = 1; occurrence <= count; occurrence += 1) {
      const skippedCurrentRootRevalidation = replaceOccurrence(
        launcher,
        comparison,
        replacement,
        occurrence,
      );
      assert(
        validateIsolatedTestDiskAuthority(
          workflow,
          skippedCurrentRootRevalidation,
        ).some((issue) => issue.includes("current cleanup must bind established root")),
        `Jobs CI guard must reject loss of current run-root revalidation ${occurrence}`,
      );
    }
  }

  const skippedCurrentMarkerIdentity = launcher.replace(
    '"$first_marker_device_inode" != "$current_marker_device_inode"',
    '"$first_marker_device_inode" != "$first_marker_device_inode"',
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, skippedCurrentMarkerIdentity).some(
      (issue) => issue.includes("current cleanup must bind established root"),
    ),
    "Jobs CI guard must reject loss of current marker device/inode revalidation",
  );

  const skippedCurrentMarkerMtime = launcher.replace(
    '"$first_marker_modified_epoch" != "$current_marker_modified_epoch"',
    '"$first_marker_modified_epoch" != "$first_marker_modified_epoch"',
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, skippedCurrentMarkerMtime).some(
      (issue) => issue.includes("current cleanup must bind established root"),
    ),
    "Jobs CI guard must reject loss of current marker mtime revalidation",
  );

  for (const [comparison, replacement] of [
    [
      '"$observed_path_device_inode" != "$current_marker_device_inode"',
      '"$observed_path_device_inode" != "$observed_path_device_inode"',
    ],
    [
      '"$observed_path_modified_epoch" != "$current_marker_modified_epoch"',
      '"$observed_path_modified_epoch" != "$observed_path_modified_epoch"',
    ],
  ]) {
    const skippedFinalCurrentMarkerIdentity = launcher.replace(
      comparison,
      replacement,
    );
    assert(
      validateIsolatedTestDiskAuthority(
        workflow,
        skippedFinalCurrentMarkerIdentity,
      ).some((issue) => issue.includes("current cleanup must bind established root")),
      "Jobs CI guard must reject loss of final current-marker revalidation",
    );
  }

  for (const [binding, replacement] of [
    [
      'next_device_inode="$observed_path_device_inode"',
      'next_device_inode="unbound"',
    ],
    [
      'next_modified_epoch="$observed_path_modified_epoch"',
      'next_modified_epoch="unbound"',
    ],
  ]) {
    const skippedMarkerTransitionBinding = launcher.replace(binding, replacement);
    assert(
      validateIsolatedTestDiskAuthority(
        workflow,
        skippedMarkerTransitionBinding,
      ).some((issue) => issue.includes("marker transition")),
      "Jobs CI guard must reject loss of marker-transition identity binding",
    );
  }

  const nonAtomicMarkerTransition = replaceOccurrence(
    launcher,
    "/usr/bin/perl -e 'exit(rename($ARGV[0], $ARGV[1]) ? 0 : 1)'",
    '/bin/mv -f -- "$next" "$marker_path"',
    2,
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, nonAtomicMarkerTransition).some(
      (issue) => issue.includes("marker transition"),
    ),
    "Jobs CI guard must reject loss of atomic marker transition rename",
  );

  for (const [comparison, replacement] of [
    [
      '"$observed_path_device_inode" == "$next_device_inode"',
      '"$observed_path_device_inode" == "$observed_path_device_inode"',
    ],
    [
      '"$observed_path_modified_epoch" == "$next_modified_epoch"',
      '"$observed_path_modified_epoch" == "$observed_path_modified_epoch"',
    ],
  ]) {
    const ignoredMarkerTransitionIdentity = launcher.replace(
      comparison,
      replacement,
    );
    assert(
      validateIsolatedTestDiskAuthority(
        workflow,
        ignoredMarkerTransitionIdentity,
      ).some((issue) => issue.includes("marker transition")),
      "Jobs CI guard must reject loss of pre-rename marker identity",
    );
  }

  const missingMarkerTransitionHook = launcher.replace(
    'hook="${BLUEY_TEST_INJECT_AFTER_MARKER_RENAME_HOOK:-}"',
    'hook=""',
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, missingMarkerTransitionHook).some(
      (issue) => issue.includes("marker transition"),
    ),
    "Jobs CI guard must preserve the deterministic marker-transition fixture",
  );

  for (const [field, binding] of CURRENT_MARKER_SNAPSHOT_BINDINGS) {
    const ignoredBinding = launcher.replace(binding, `current_marker_${field}="ignored"`);
    assert(
      validateIsolatedTestDiskAuthority(workflow, ignoredBinding).some((issue) =>
        issue.includes(`exact field binding: ${field}`),
      ),
      `Jobs CI guard must reject loss of the current ${field} marker snapshot`,
    );
  }

  for (const [field, argument] of CURRENT_MARKER_REVALIDATION_ARGUMENTS) {
    for (let occurrence = 1; occurrence <= 2; occurrence += 1) {
      const ignoredArgument = replaceOccurrence(
        launcher,
        argument,
        '"ignored"',
        occurrence,
      );
      assert(
        validateIsolatedTestDiskAuthority(workflow, ignoredArgument).some((issue) =>
          issue.includes(`exact field revalidation: ${field}`),
        ),
        `Jobs CI guard must reject current ${field} revalidation ${occurrence}`,
      );
    }
  }

  for (const [field, argument] of EXPECTED_MARKER_REVALIDATION_ARGUMENTS) {
    for (let occurrence = 1; occurrence <= 5; occurrence += 1) {
      const ignoredArgument = replaceOccurrence(
        launcher,
        argument,
        '"ignored"',
        occurrence,
      );
      assert(
        validateIsolatedTestDiskAuthority(workflow, ignoredArgument).some((issue) =>
          issue.includes(`exact field revalidation: ${field}`),
        ),
        `Jobs CI guard must reject quarantined ${field} revalidation ${occurrence}`,
      );
    }
  }

  for (let occurrence = 1; occurrence <= 2; occurrence += 1) {
    const skippedQuarantineCall = replaceOccurrence(
      launcher,
      "quarantine_and_delete_root \\",
      "unsafe_delete_root \\",
      occurrence,
    );
    assert(
      validateIsolatedTestDiskAuthority(workflow, skippedQuarantineCall).some(
        (issue) => issue.includes("atomic quarantine authority"),
      ),
      `Jobs CI guard must require quarantine call ${occurrence}`,
    );
  }

  const nonAtomicQuarantineRename = replaceOccurrence(
    launcher,
    "/usr/bin/perl -e 'exit(rename($ARGV[0], $ARGV[1]) ? 0 : 1)'",
    '/bin/cp -R "$source" "$moved_root"',
    1,
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, nonAtomicQuarantineRename).some(
      (issue) => issue.includes("atomic quarantine authority"),
    ),
    "Jobs CI guard must reject copy-based quarantine transitions",
  );

  const skippedSourceAbsence = launcher.replace(
    'if [[ -e "$source" || -L "$source" ]]',
    "if false",
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, skippedSourceAbsence).some(
      (issue) => issue.includes("atomic quarantine authority"),
    ),
    "Jobs CI guard must require original-source absence after quarantine rename",
  );

  for (let occurrence = 1; occurrence <= 2; occurrence += 1) {
    const skippedQuarantineVerification = replaceOccurrence(
      launcher,
      "verify_quarantined_payload \\",
      "unsafe_verify_quarantined_payload \\",
      occurrence,
    );
    assert(
      validateIsolatedTestDiskAuthority(
        workflow,
        skippedQuarantineVerification,
      ).some((issue) => issue.includes("atomic quarantine authority")),
      `Jobs CI guard must require quarantine verification ${occurrence}`,
    );
  }

  const skippedAnchoredDelete = launcher.replace(
    "delete_quarantined_payload \\",
    "unsafe_delete_quarantined_payload \\",
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, skippedAnchoredDelete).some(
      (issue) => issue.includes("atomic quarantine authority"),
    ),
    "Jobs CI guard must require anchored quarantined deletion",
  );

  const missingFinalDeleteHook = launcher.replace(
    'hook="${BLUEY_TEST_INJECT_AFTER_QUARANTINE_VERIFY_HOOK:-}"',
    'hook=""',
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, missingFinalDeleteHook).some(
      (issue) => issue.includes("atomic quarantine authority"),
    ),
    "Jobs CI guard must preserve the final-delete substitution fixture",
  );

  for (const [comparison, replacement, count] of [
    [
      '"$observed_path_device_inode" == "$expected_root_device_inode"',
      '"$observed_path_device_inode" == "$observed_path_device_inode"',
      5,
    ],
    [
      '"$observed_path_device_inode" == "$run_parent_device_inode"',
      '"$observed_path_device_inode" == "$observed_path_device_inode"',
      3,
    ],
    [
      '"$observed_path_device_inode" == "$delete_quarantine_device_inode"',
      '"$observed_path_device_inode" == "$observed_path_device_inode"',
      3,
    ],
  ]) {
    for (let occurrence = 1; occurrence <= count; occurrence += 1) {
      const ignoredQuarantineIdentity = replaceOccurrence(
        launcher,
        comparison,
        replacement,
        occurrence,
      );
      assert(
        validateIsolatedTestDiskAuthority(
          workflow,
          ignoredQuarantineIdentity,
        ).some((issue) => issue.includes("atomic quarantine authority")),
        `Jobs CI guard must reject quarantine identity loss ${occurrence}`,
      );
    }
  }

  for (const [comparison, replacement, count] of [
    [
      '"$first_marker_device_inode" != "$expected_marker_device_inode"',
      '"$first_marker_device_inode" != "$first_marker_device_inode"',
      1,
    ],
    [
      '"$first_marker_modified_epoch" != "$expected_marker_modified_epoch"',
      '"$first_marker_modified_epoch" != "$first_marker_modified_epoch"',
      1,
    ],
    [
      '"$observed_path_device_inode" == "$expected_marker_device_inode"',
      '"$observed_path_device_inode" == "$observed_path_device_inode"',
      2,
    ],
    [
      '"$observed_path_modified_epoch" == "$expected_marker_modified_epoch"',
      '"$observed_path_modified_epoch" == "$observed_path_modified_epoch"',
      2,
    ],
  ]) {
    for (let occurrence = 1; occurrence <= count; occurrence += 1) {
      const ignoredQuarantinedMarkerIdentity = replaceOccurrence(
        launcher,
        comparison,
        replacement,
        occurrence,
      );
      assert(
        validateIsolatedTestDiskAuthority(
          workflow,
          ignoredQuarantinedMarkerIdentity,
        ).some((issue) => issue.includes("quarantined cleanup")),
        `Jobs CI guard must reject quarantined marker identity loss ${occurrence}`,
      );
    }
  }

  const ancestorCheckOnParentOnly = launcher.replace(
    'validate_no_git_ancestor "$desired"',
    'validate_no_git_ancestor "$parent_physical"',
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, ancestorCheckOnParentOnly).some(
      (issue) => issue.includes("exact requested parent Git ancestry"),
    ),
    "Jobs CI guard must reject checking only the requested parent's ancestor",
  );

  const incompleteExactGitCoverage = launcher.replace(
    "for exact_git_kind in directory file symlink; do",
    "for exact_git_kind in directory file; do",
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, incompleteExactGitCoverage).some(
      (issue) => issue.includes("directory file symlink"),
    ),
    "Jobs CI guard must run exact Git directory, file, and symlink fixtures",
  );

  const lateExactParentChecks = launcher.replace(
    'validate_no_git_ancestor "$desired"',
    'if [[ -e "$desired" || -L "$desired" ]]; then\n  fi\n  ' +
      'validate_no_git_ancestor "$desired"',
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, lateExactParentChecks).some(
      (issue) => issue.includes("precede run-parent mutation"),
    ),
    "Jobs CI guard must reject exact Git checks moved after parent mutation",
  );

  const uncheckedWorktreeEnumeration = launcher.replace(
    '    exit "$producer_status"',
    "    exit 0",
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, uncheckedWorktreeEnumeration).some(
      (issue) => issue.includes("producer-status-checked worktree enumeration"),
    ),
    "Jobs CI guard must reject treating worktree enumeration failure as success",
  );

  const processSubstitutionWorktreeEnumeration = launcher.replace(
    'done <<< "$listing"',
    'done < <(git -C "$repo_root" worktree list --porcelain)',
  );
  assert(
    validateIsolatedTestDiskAuthority(
      workflow,
      processSubstitutionWorktreeEnumeration,
    ).some((issue) => issue.includes("status-checked worktree enumeration")),
    "Jobs CI guard must reject process substitution that discards Git status",
  );

  const unboundedWorktreeEnumeration = launcher.replace(
    '[[ "${#listing}" -le "$WORKTREE_LIST_MAX_BYTES" ]] || return 1',
    ': # worktree enumeration response accepted without a bound',
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, unboundedWorktreeEnumeration).some(
      (issue) => issue.includes("worktree enumeration"),
    ),
    "Jobs CI guard must reject an unbounded worktree enumeration response",
  );

  for (const [needle, replacement, label] of [
    ["set -o pipefail", ": # pipeline status ignored", "pipeline status"],
    ["      alarm $timeout;", "      alarm 0;", "producer timeout"],
    [
      '} | /usr/bin/head -c "$WORKTREE_LIST_CAPTURE_BYTES")" || return 1',
      '})" || return 1',
      "producer byte cap",
    ],
    [
      '[[ "$listing" == *"$sentinel" ]] || return 1',
      ": # producer completion sentinel ignored",
      "producer completion sentinel",
    ],
    [
      'listing="${listing%"$sentinel"}"',
      ": # completion sentinel retained",
      "sentinel removal",
    ],
  ]) {
    const unsafeWorktreeEnumeration = launcher.replace(needle, replacement);
    assert(
      validateIsolatedTestDiskAuthority(
        workflow,
        unsafeWorktreeEnumeration,
      ).some((issue) => issue.includes("worktree enumeration")),
      `Jobs CI guard must reject loss of ${label}`,
    );
  }

  const gateBeforeSetsid = launcher.replace(
    "exit 125 if setsid() == -1;",
    ": # private process-group setup removed",
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, gateBeforeSetsid).some((issue) =>
      issue.includes("private wrapper handshake"),
    ),
    "Jobs CI guard must reject a wrapper that can wait before private group setup",
  );

  const undeferredWrapperSpawn = launcher.replace(
    'ready="$run_root/.command-ready"\n  install_deferred_signal_traps\n  /usr/bin/perl',
    'ready="$run_root/.command-ready"\n  /usr/bin/perl',
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, undeferredWrapperSpawn).some((issue) =>
      issue.includes("private wrapper handshake"),
    ),
    "Jobs CI guard must reject an undeferred wrapper-spawn window",
  );

  const unboundWrapperGroup = launcher.replace(
    '[[ "$observed_group_id" == "$command_pid" ]] && break',
    '[[ -n "$observed_group_id" ]] && break',
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, unboundWrapperGroup).some((issue) =>
      issue.includes("observed_group_id"),
    ),
    "Jobs CI guard must require exact wrapper PID/process-group equality",
  );

  const gateBeforeDeferredDispatch = launcher.replace(
    "dispatch_deferred_signal\n  printf 'start\\n' > \"$gate\"",
    "printf 'start\\n' > \"$gate\"\n  dispatch_deferred_signal",
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, gateBeforeDeferredDispatch).some((issue) =>
      issue.includes("private wrapper handshake"),
    ),
    "Jobs CI guard must reject opening the user-command gate before deferred delivery",
  );

  const unboundedPidStop = launcher.replace(
    'pid_can_execute "$command_pid" && return 1',
    ": # wait even while the child can still execute",
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, unboundedPidStop).some((issue) =>
      issue.includes("pid_can_execute"),
    ),
    "Jobs CI guard must reject an unbounded PID fallback wait",
  );

  const callerCwdCommand = launcher.replace(
    "exit 125 unless chdir $repo_root;",
    ": # caller working directory retained",
  );
  assert(
    validateIsolatedTestDiskAuthority(workflow, callerCwdCommand).some((issue) =>
      issue.includes("chdir"),
    ),
    "Jobs CI guard must reject commands that retain the caller working directory",
  );

  const sharedMicDatabase = micDeviceTest
    .replace(
      ".tempdir_in(std::env::temp_dir())",
      '.tempdir_in("/tmp")',
    )
    .replace(
      'let db_path = temp_dir.path().join("mic-device.sqlite");',
      'let db_path = "/tmp/cue_test_mic_device_shared.db";',
    );
  assert(
    validateMicDeviceTempAuthority(sharedMicDatabase).some((issue) =>
      issue.includes("shared /tmp database"),
    ),
    "Jobs CI guard must reject a shared hardcoded microphone-test database",
  );
}

function testIntegrationTestSupportContainmentGuard() {
  const cargo = fs.readFileSync(path.join(repoRoot, "server/Cargo.toml"), "utf8");
  assert.match(cargo, /\[features\]\s+default = \[\]\s+integration-test-support = \["dep:serial_test"\]/);
  assert.match(
    cargo,
    /\[\[test\]\]\s+name = "integration_e2e"\s+path = "tests\/integration_e2e\.rs"\s+required-features = \["integration-test-support"\]/,
  );
  assert.match(cargo, /^bluey-server = \{ path = "\." \}$/m);
  assert.doesNotMatch(
    cargo,
    /^bluey-server = \{ path = "\.", features = \["integration-test-support"\] \}$/m,
  );

  const normalize = (value) => value.replace(/\s+/g, " ").trim();
  const occurrences = (value, needle) => value.split(needle).length - 1;
  const library = normalize(
    fs.readFileSync(path.join(repoRoot, "server/src/lib.rs"), "utf8"),
  );
  assert.match(
    library,
    /#\[cfg\(all\(feature = "integration-test-support", not\(debug_assertions\)\)\)\] compile_error!\("integration-test-support must never be enabled in release builds"\);/,
  );

  const supportPrefix =
    "cargo test --manifest-path server/Cargo.toml --no-default-features " +
    "--features integration-test-support --test integration_e2e";
  const supportClippy =
    "cargo clippy --manifest-path server/Cargo.toml --no-default-features " +
    "--features integration-test-support --test integration_e2e -- -D warnings";
  const jobsCi = normalize(
    fs.readFileSync(path.join(repoRoot, ".github/workflows/jobs-ci.yml"), "utf8"),
  );
  assert.equal(occurrences(jobsCi, supportClippy), 1);
  assert.equal(occurrences(jobsCi, supportPrefix), 1);
  assert.equal(occurrences(jobsCi, `${supportPrefix} jobs_`), 0);

  const release = normalize(
    fs.readFileSync(path.join(repoRoot, ".github/workflows/release.yml"), "utf8"),
  );
  assert.equal(occurrences(release, supportClippy), 1);
  assert.equal(occurrences(release, supportPrefix), 1);
  assert.equal(
    occurrences(
      release,
      "cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings",
    ),
    1,
  );
  assert.equal(
    occurrences(release, "cargo test --manifest-path server/Cargo.toml --all-targets"),
    1,
  );

  for (const productionPath of [
    "server/Dockerfile.jobs",
    ".github/workflows/jobs-managed-cloud-release.yml",
  ]) {
    const productionBuild = fs.readFileSync(path.join(repoRoot, productionPath), "utf8");
    assert.doesNotMatch(
      productionBuild,
      /integration-test-support/,
      `${productionPath} must never enable integration test support`,
    );
  }
}

function testPublicBetaAdminMutationAuditBoundary() {
  const source = fs.readFileSync(
    path.join(repoRoot, "server/src/db/jobs_beta_access.rs"),
    "utf8",
  );
  const rawMutations = [
    "update_public_beta_cohort",
    "grant_public_beta_access",
    "set_public_beta_override",
  ];
  for (const symbol of rawMutations) {
    assert.match(
      source,
      new RegExp(`#\\[cfg\\(test\\)\\]\\s+fn ${symbol}\\s*\\(`),
      `${symbol} must exist only as a private test helper`,
    );
    assert.doesNotMatch(
      source,
      new RegExp(`pub(?:\\([^)]*\\))?\\s+fn ${symbol}\\s*\\(`),
      `${symbol} must not be callable from production code`,
    );
    assert.match(
      source,
      new RegExp(`pub fn ${symbol}_audited\\s*\\(`),
      `${symbol}_audited must remain the production mutation API`,
    );
  }
  assert.doesNotMatch(
    source,
    /audit_actor:\s*Option<&str>/,
    "production mutation internals must require a typed audit context",
  );
  for (const symbol of [
    "update_cohort_sqlite",
    "update_cohort_postgres",
    "grant_access_sqlite",
    "grant_access_postgres",
    "set_override_sqlite",
    "set_override_postgres",
  ]) {
    assert.match(
      source,
      new RegExp(`fn ${symbol}\\s*\\([\\s\\S]*?audit:\\s*AdminAuditContext<'_>[\\s\\S]*?\\)\\s*->`),
      `${symbol} must require the typed administration audit context`,
    );
  }
}

function jobsParitySchema(integerType) {
  return `
    CREATE TABLE IF NOT EXISTS account_deletion_intents (
      account_id TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
      requested_at_ms ${integerType} NOT NULL CHECK(requested_at_ms >= 0),
      last_checked_at_ms ${integerType} NOT NULL CHECK(last_checked_at_ms >= 0),
      fresh_upload_cutoff_ms ${integerType} NOT NULL,
      fresh_in_flight_puts ${integerType} NOT NULL DEFAULT 0 CHECK(fresh_in_flight_puts >= 0)
    );
    CREATE TABLE IF NOT EXISTS jobs_discovery_sources (
      id TEXT PRIMARY KEY,
      status TEXT NOT NULL,
      health TEXT NOT NULL,
      next_run_at_ms ${integerType} NOT NULL,
      lease_expires_at_ms ${integerType}
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_discovery_sources_due
      ON jobs_discovery_sources(status, health, next_run_at_ms, lease_expires_at_ms);
    CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_discovery_sources_board_owner
      ON jobs_discovery_sources(account_id, provider, source_key);
    CREATE TABLE IF NOT EXISTS jobs_discovery_runs (
      id TEXT PRIMARY KEY,
      source_id TEXT NOT NULL REFERENCES jobs_discovery_sources(id) ON DELETE CASCADE,
      started_at_ms ${integerType} NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_discovery_runs_source
      ON jobs_discovery_runs(source_id, started_at_ms DESC);
    CREATE TABLE IF NOT EXISTS jobs_discovery_memberships (
      source_id TEXT NOT NULL REFERENCES jobs_discovery_sources(id) ON DELETE CASCADE,
      account_id TEXT NOT NULL,
      external_id TEXT NOT NULL,
      job_id TEXT NOT NULL,
      PRIMARY KEY(source_id, external_id)
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_discovery_memberships_job
      ON jobs_discovery_memberships(account_id, job_id);
    CREATE TABLE IF NOT EXISTS jobs_execution_leases (
      run_id TEXT PRIMARY KEY,
      account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
      application_id TEXT NOT NULL REFERENCES jobs_applications(id) ON DELETE CASCADE,
      browser_profile_id TEXT NOT NULL,
      owner_id TEXT NOT NULL,
      lease_token_sha256 TEXT NOT NULL,
      fence ${integerType} NOT NULL CHECK(fence > 0),
      phase TEXT NOT NULL CHECK(phase IN (
        'prepared', 'click_started', 'submitted', 'failed',
        'side_effect_unknown', 'released'
      )),
      lease_expires_at_ms ${integerType} NOT NULL,
      created_at_ms ${integerType} NOT NULL,
      updated_at_ms ${integerType} NOT NULL,
      finished_at_ms ${integerType}
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_execution_leases_binding
      ON jobs_execution_leases(account_id, application_id, run_id);
    CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_execution_leases_active_application
      ON jobs_execution_leases(application_id)
      WHERE phase IN ('prepared', 'click_started');
    CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_execution_leases_active_profile
      ON jobs_execution_leases(browser_profile_id)
      WHERE phase IN ('prepared', 'click_started');
    CREATE TABLE IF NOT EXISTS jobs_local_run_resume_actions (
      id TEXT PRIMARY KEY,
      run_id TEXT NOT NULL REFERENCES jobs_local_run_tickets(id) ON DELETE CASCADE,
      account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
      application_id TEXT NOT NULL REFERENCES jobs_applications(id) ON DELETE CASCADE,
      intervention_id TEXT NOT NULL UNIQUE REFERENCES jobs_interventions(id) ON DELETE CASCADE,
      action TEXT NOT NULL CHECK(action = 'approve_submission'),
      status TEXT NOT NULL CHECK(status IN ('approved', 'consumed')),
      expires_at_ms ${integerType} NOT NULL,
      created_at_ms ${integerType} NOT NULL,
      consumed_at_ms ${integerType}
    );
    CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_local_resume_actions_active_run
      ON jobs_local_run_resume_actions(run_id)
      WHERE status = 'approved';
    CREATE INDEX IF NOT EXISTS idx_jobs_local_resume_actions_application
      ON jobs_local_run_resume_actions(account_id, application_id, created_at_ms DESC);
    CREATE TABLE IF NOT EXISTS jobs_public_beta_cohorts (
      id TEXT PRIMARY KEY CHECK(id = 'public-v1'),
      state TEXT NOT NULL CHECK(state IN ('draft', 'open', 'closed_to_new', 'suspended')),
      opens_at_ms ${integerType},
      closes_at_ms ${integerType},
      hard_cap ${integerType} NOT NULL CHECK(hard_cap >= 0 AND hard_cap <= 10000),
      assigned_count ${integerType} NOT NULL CHECK(assigned_count >= 0 AND assigned_count <= hard_cap),
      revision ${integerType} NOT NULL CHECK(revision >= 1),
      created_at_ms ${integerType} NOT NULL,
      updated_at_ms ${integerType} NOT NULL,
      CHECK((opens_at_ms IS NULL) = (closes_at_ms IS NULL)),
      CHECK(opens_at_ms IS NULL OR (opens_at_ms >= 0 AND closes_at_ms > opens_at_ms)),
      CHECK(opens_at_ms IS NULL OR closes_at_ms - opens_at_ms <= 7776000000)
    );
    CREATE TABLE IF NOT EXISTS jobs_public_beta_enrollments (
      cohort_id TEXT NOT NULL REFERENCES jobs_public_beta_cohorts(id) ON DELETE RESTRICT,
      account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
      source TEXT NOT NULL CHECK(source IN ('public_window', 'admin')),
      admitted_at_ms ${integerType} NOT NULL,
      PRIMARY KEY(cohort_id, account_id)
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_public_beta_enrollments_account
      ON jobs_public_beta_enrollments(account_id, cohort_id);
    CREATE INDEX IF NOT EXISTS idx_jobs_public_beta_enrollments_source
      ON jobs_public_beta_enrollments(cohort_id, source, admitted_at_ms DESC);
    CREATE TABLE IF NOT EXISTS jobs_public_beta_overrides (
      cohort_id TEXT NOT NULL REFERENCES jobs_public_beta_cohorts(id) ON DELETE RESTRICT,
      account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
      denied INTEGER NOT NULL CHECK(denied IN (0, 1)),
      revision ${integerType} NOT NULL CHECK(revision >= 1),
      created_at_ms ${integerType} NOT NULL,
      updated_at_ms ${integerType} NOT NULL,
      PRIMARY KEY(cohort_id, account_id)
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_public_beta_overrides_account
      ON jobs_public_beta_overrides(account_id, cohort_id);
    CREATE INDEX IF NOT EXISTS idx_jobs_public_beta_overrides_active
      ON jobs_public_beta_overrides(cohort_id, denied, updated_at_ms DESC)
      WHERE denied = 1;
    CREATE TABLE IF NOT EXISTS jobs_submission_evidence_capacity (
      account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
      application_id TEXT NOT NULL REFERENCES jobs_applications(id) ON DELETE CASCADE,
      run_id TEXT NOT NULL,
      runner TEXT NOT NULL CHECK(runner IN ('cloud', 'local')),
      reserved_bytes ${integerType} NOT NULL CHECK(reserved_bytes > 0),
      reserved_objects ${integerType} NOT NULL CHECK(reserved_objects > 0),
      consumed_bytes ${integerType} NOT NULL DEFAULT 0 CHECK(consumed_bytes >= 0),
      consumed_objects ${integerType} NOT NULL DEFAULT 0 CHECK(consumed_objects >= 0),
      expires_at_ms ${integerType} NOT NULL CHECK(expires_at_ms > 0),
      state TEXT NOT NULL CHECK(state IN ('active', 'committed', 'released', 'expired')),
      created_at_ms ${integerType} NOT NULL CHECK(created_at_ms >= 0),
      updated_at_ms ${integerType} NOT NULL CHECK(updated_at_ms >= 0),
      completed_at_ms ${integerType},
      PRIMARY KEY(account_id, application_id, run_id),
      CHECK(consumed_bytes <= reserved_bytes),
      CHECK(consumed_objects <= reserved_objects)
    );
    CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_submission_evidence_capacity_active_application
      ON jobs_submission_evidence_capacity(account_id, application_id)
      WHERE state = 'active';
    CREATE INDEX IF NOT EXISTS idx_jobs_submission_evidence_capacity_account
      ON jobs_submission_evidence_capacity(account_id, state, expires_at_ms);
    CREATE TABLE IF NOT EXISTS jobs_communication_actions (
      id TEXT PRIMARY KEY,
      account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
      application_id TEXT NOT NULL REFERENCES jobs_applications(id) ON DELETE CASCADE,
      connection_id TEXT NOT NULL REFERENCES jobs_mailbox_connections(id) ON DELETE CASCADE,
      source_message_id TEXT REFERENCES jobs_provider_messages(id) ON DELETE SET NULL,
      kind TEXT NOT NULL CHECK (kind IN ('reply', 'calendar')),
      provider TEXT NOT NULL CHECK (
        provider IN ('gmail', 'outlook_email', 'google_calendar', 'outlook_calendar')
      ),
      idempotency_key TEXT NOT NULL,
      payload_sha256 TEXT NOT NULL,
      status TEXT NOT NULL CHECK (
        status IN (
          'awaiting_approval', 'approved', 'dispatching', 'sent',
          'calendar_created', 'needs_input', 'failed',
          'side_effect_unknown', 'cancelled'
        )
      ),
      provider_object_id TEXT,
      action_json TEXT NOT NULL,
      lease_owner TEXT,
      lease_token_sha256 TEXT,
      fence ${integerType} NOT NULL DEFAULT 0,
      lease_expires_at_ms ${integerType},
      next_attempt_at_ms ${integerType} NOT NULL,
      attempt_count ${integerType} NOT NULL DEFAULT 0,
      approved_at_ms ${integerType},
      dispatched_at_ms ${integerType},
      created_at_ms ${integerType} NOT NULL,
      updated_at_ms ${integerType} NOT NULL,
      UNIQUE(account_id, idempotency_key)
    );
    CREATE INDEX IF NOT EXISTS idx_jobs_communication_actions_account
      ON jobs_communication_actions(account_id, application_id, created_at_ms DESC);
    CREATE INDEX IF NOT EXISTS idx_jobs_communication_actions_due
      ON jobs_communication_actions(status, next_attempt_at_ms, lease_expires_at_ms);
    ${runnerVolumeParitySchema(integerType)}
    ${browserReleaseAuthorityParitySchema(integerType)}
    ${atsCertificationParitySchema(integerType)}
    ${communicationExecutionParitySchema(integerType)}
    ${operationalHoldsParitySchema(integerType)}
    ${phase613ParitySchema(integerType)}
  `;
}

function replaceFirstForGuardTest(source, original, replacement, invariant) {
  assert(
    source.includes(original),
    `guard self-test fixture must contain ${invariant}`,
  );
  return source.replace(original, replacement);
}

function assertOperationalHoldDriftRejected({
  dialect,
  invariant,
  original,
  replacement,
  sqlite,
  postgres,
}) {
  const operationalMigration = operationalHoldsParitySchema(
    dialect === "SQLite" ? "INTEGER" : "BIGINT",
  );
  const mutatedMigration = replaceFirstForGuardTest(
    operationalMigration,
    original,
    replacement,
    invariant,
  );
  const source = dialect === "SQLite" ? sqlite : postgres;
  assert(
    source.includes(operationalMigration),
    `${dialect} guard fixture must contain the operational-hold migration`,
  );
  const mutated = source.replace(operationalMigration, () => mutatedMigration);
  const issues =
    dialect === "SQLite"
      ? compareJobsSchemas(mutated, postgres)
      : compareJobsSchemas(sqlite, mutated);
  assert(
    issues.some((issue) =>
      issue.includes(`${dialect} operational-hold invariant ${invariant}`),
    ),
    `${dialect} ${invariant} drift must fail the operational-hold semantic guard; ` +
      `issues=${JSON.stringify(issues)}`,
  );
}

function assertPhase613DriftRejected({
  dialect,
  invariant,
  original,
  replacement,
  sqlite,
  postgres,
}) {
  const phase613Migration = phase613ParitySchema(
    dialect === "SQLite" ? "INTEGER" : "BIGINT",
  );
  const mutatedMigration = replaceFirstForGuardTest(
    phase613Migration,
    original,
    replacement,
    invariant,
  );
  const source = dialect === "SQLite" ? sqlite : postgres;
  assert(
    source.includes(phase613Migration),
    `${dialect} guard fixture must contain the Phase 613 migration`,
  );
  const mutated = source.replace(phase613Migration, () => mutatedMigration);
  const issues =
    dialect === "SQLite"
      ? compareJobsSchemas(mutated, postgres)
      : compareJobsSchemas(sqlite, mutated);
  assert(
    issues.some((issue) =>
      issue.includes(`${dialect} Phase 613 invariant ${invariant}`),
    ),
    `${dialect} ${invariant} drift must fail the Phase 613 semantic guard; ` +
      `issues=${JSON.stringify(issues)}`,
  );
}

function assertPhase614BDriftRejected({
  dialect,
  invariant,
  original,
  replacement,
  sqlite,
  postgres,
}) {
  const source = dialect === "SQLite" ? sqlite : postgres;
  const mutated = replaceFirstForGuardTest(
    source,
    original,
    replacement,
    invariant,
  );
  const issues =
    dialect === "SQLite"
      ? compareJobsSchemas(mutated, postgres)
      : compareJobsSchemas(sqlite, mutated);
  assert(
    issues.some((issue) =>
      issue.includes(`${dialect} Phase 614B invariant`),
    ),
    `${dialect} ${invariant} drift must fail the Phase 614B semantic guard; ` +
      `issues=${JSON.stringify(issues)}`,
  );
}

function assertPhase628DriftRejected({
  dialect,
  invariant,
  migrationFile,
  original,
  replacement,
  sqlite,
  postgres,
}) {
  const migrationPath = path.join(
    repoRoot,
    migrationFile ??
      (dialect === "SQLite"
        ? "infra/sqlite/server-runtime/065_jobs_source_verifier_heartbeat_boot_bindings.sql"
        : "infra/postgres/server-runtime/043_jobs_source_verifier_heartbeat_boot_bindings.sql"),
  );
  const migration = fs.readFileSync(migrationPath, "utf8");
  const mutatedMigration = replaceFirstForGuardTest(
    migration,
    original,
    replacement,
    invariant,
  );
  const source = dialect === "SQLite" ? sqlite : postgres;
  assert(
    source.includes(migration),
    `${dialect} guard fixture must contain the Phase 628 migration`,
  );
  const mutated = source.replace(migration, () => mutatedMigration);
  const issues =
    dialect === "SQLite"
      ? compareJobsSchemas(mutated, postgres)
      : compareJobsSchemas(sqlite, mutated);
  assert(
    issues.some((issue) =>
      issue.includes(`${dialect} Phase 628 invariant ${invariant}`),
    ),
    `${dialect} ${invariant} drift must fail the Phase 628 semantic guard; ` +
      `issues=${JSON.stringify(issues)}`,
  );
}

function assertPhase628OperationsV3Contract(operationsSource) {
  const heading =
    "## Managed discovery and source-verifier release-v3 authority";
  const nextHeading = "### Managed-cloud artifact publication and read-back";
  const start = operationsSource.indexOf(heading);
  const end = operationsSource.indexOf(nextHeading, start + heading.length);
  assert(start >= 0 && end > start, "active release-v3 operations section must exist");
  const activeV3 = operationsSource.slice(start, end).replace(/\s+/g, " ");
  for (const [invariant, fragment] of [
    [
      "current migration heads",
      "source-verifier boot authority is SQLite head `065` and PostgreSQL head `043`",
    ],
    [
      "boot registration endpoint",
      "/source-verifier-boot-registration",
    ],
    ["boot startup endpoint", "/source-verifier-boot-startup"],
    [
      "fresh retry authentication",
      "worker-auth nonce and HMAC are fresh for each delivery",
    ],
    [
      "no speculative probe-failed recovery",
      "do not send a speculative `degraded/probe_failed` heartbeat",
    ],
  ]) {
    assert(
      activeV3.includes(fragment),
      `active release-v3 operations must preserve ${invariant}`,
    );
  }
  assert(
    !activeV3.includes("/original-source-verifier-startup"),
    "active release-v3 operations must not restore the legacy startup endpoint",
  );
}

function testPhase628OperationsContractGuard() {
  const operations = fs.readFileSync(
    path.join(repoRoot, "jobs/OPERATIONS.md"),
    "utf8",
  );
  assertPhase628OperationsV3Contract(operations);
  for (const [invariant, original, replacement] of [
    [
      "obsolete migration heads",
      "source-verifier boot authority is SQLite head `065` and PostgreSQL head `043`",
      "source-verifier boot authority is SQLite head `062` and PostgreSQL head `040`",
    ],
    [
      "legacy startup endpoint",
      "/source-verifier-boot-startup",
      "/original-source-verifier-startup",
    ],
    [
      "speculative probe-failed recovery",
      "do not\nsend a speculative `degraded/probe_failed` heartbeat",
      "send a speculative `degraded/probe_failed` heartbeat",
    ],
  ]) {
    const mutated = replaceFirstForGuardTest(
      operations,
      original,
      replacement,
      invariant,
    );
    assert.throws(
      () => assertPhase628OperationsV3Contract(mutated),
      /active release-v3 operations/,
      `${invariant} must fail the active release-v3 documentation guard`,
    );
  }
}

function assertSourceVerifierStartupMigrationRegistration(migrationRunner) {
  assert.deepEqual(
    checkPhase628MigrationRegistration(migrationRunner),
    [],
    "SQLite/Postgres source-verifier migrations must have exact declarations and array registrations",
  );
}

function testSchemaParity() {
  const sqlite = [
    jobsParitySchema("INTEGER"),
    fs.readFileSync(
      path.join(
        repoRoot,
        "infra/sqlite/server-runtime/057_jobs_original_source_verification_authority.sql",
      ),
      "utf8",
    ),
    fs.readFileSync(
      path.join(
        repoRoot,
        "infra/sqlite/server-runtime/058_jobs_signed_job_integrity_authority.sql",
      ),
      "utf8",
    ),
    fs.readFileSync(
      path.join(
        repoRoot,
        "infra/sqlite/server-runtime/062_jobs_original_source_verifier_startup_authority.sql",
      ),
      "utf8",
    ),
    fs.readFileSync(
      path.join(
        repoRoot,
        "infra/sqlite/server-runtime/063_jobs_source_verifier_boot_generation_authority.sql",
      ),
      "utf8",
    ),
    fs.readFileSync(
      path.join(
        repoRoot,
        "infra/sqlite/server-runtime/065_jobs_source_verifier_heartbeat_boot_bindings.sql",
      ),
      "utf8",
    ),
  ].join("\n");
  const postgres = [
    jobsParitySchema("BIGINT"),
    fs.readFileSync(
      path.join(
        repoRoot,
        "infra/postgres/server-runtime/035_jobs_original_source_verification_authority.sql",
      ),
      "utf8",
    ),
    fs.readFileSync(
      path.join(
        repoRoot,
        "infra/postgres/server-runtime/036_jobs_signed_job_integrity_authority.sql",
      ),
      "utf8",
    ),
    fs.readFileSync(
      path.join(
        repoRoot,
        "infra/postgres/server-runtime/040_jobs_original_source_verifier_startup_authority.sql",
      ),
      "utf8",
    ),
    fs.readFileSync(
      path.join(
        repoRoot,
        "infra/postgres/server-runtime/041_jobs_source_verifier_boot_generation_authority.sql",
      ),
      "utf8",
    ),
    fs.readFileSync(
      path.join(
        repoRoot,
        "infra/postgres/server-runtime/043_jobs_source_verifier_heartbeat_boot_bindings.sql",
      ),
      "utf8",
    ),
  ].join("\n");
  assert.deepEqual(compareJobsSchemas(sqlite, postgres), []);
  const bootColumnDrift = sqlite.replace(
    "boot_generation                 INTEGER NOT NULL",
    "boot_generation_drift           INTEGER NOT NULL",
  );
  assert(
    compareJobsSchemas(bootColumnDrift, postgres).some((issue) =>
      issue.includes("jobs_managed_cloud_source_verifier_boot_registrations"),
    ),
    "boot registration column drift must fail schema parity",
  );
  const bootForeignKeyDrift = sqlite.replaceAll(
    "REFERENCES jobs_managed_cloud_source_verifier_boot_registrations(boot_id)",
    "REFERENCES jobs_managed_cloud_source_verifier_boot_registrations_drift(boot_id)",
  );
  assert(
    compareJobsSchemas(bootForeignKeyDrift, postgres).some((issue) =>
      issue.includes("jobs_managed_cloud_source_verifier_heartbeat_boot_bindings"),
    ),
    "boot heartbeat binding foreign-key drift must fail schema parity",
  );

  for (const mutation of [
    {
      dialect: "SQLite",
      invariant: "ready generation non-regression",
      migrationFile:
        "infra/sqlite/server-runtime/063_jobs_source_verifier_boot_generation_authority.sql",
      original:
        "OR NEW.ready_boot_generation < OLD.ready_boot_generation))",
      replacement:
        "OR NEW.ready_boot_generation > OLD.ready_boot_generation))",
    },
    {
      dialect: "SQLite",
      invariant: "ready generation non-regression rejection",
      migrationFile:
        "infra/sqlite/server-runtime/063_jobs_source_verifier_boot_generation_authority.sql",
      original:
        "SELECT RAISE(ABORT, 'source-verifier boot generation head is monotonic');",
      replacement: "SELECT 1;",
    },
    {
      dialect: "SQLite",
      invariant: "heartbeat binding current generation",
      original: "AND h.current_boot_generation=NEW.boot_generation)",
      replacement: "AND h.current_boot_generation<=NEW.boot_generation)",
    },
    {
      dialect: "SQLite",
      invariant: "attempt binding ready generation",
      original: "AND head.ready_boot_generation=NEW.boot_generation",
      replacement: "AND head.ready_boot_generation<=NEW.boot_generation",
    },
    {
      dialect: "SQLite",
      invariant: "startup requires degraded",
      original: "heartbeat.health_state='degraded'",
      replacement: "heartbeat.health_state='ready'",
    },
    {
      dialect: "SQLite",
      invariant: "startup rejects probe-failed substitute",
      original: "heartbeat.reason_code='startup'",
      replacement: "heartbeat.reason_code='probe_failed'",
    },
    {
      dialect: "SQLite",
      invariant: "attempt identity runtime instance",
      original: "attempt.runtime_instance_id=NEW.runtime_instance_id",
      replacement: "attempt.runtime_instance_id<>NEW.runtime_instance_id",
    },
    {
      dialect: "SQLite",
      invariant: "attempt identity runtime epoch",
      original: "attempt.runtime_instance_epoch=NEW.runtime_instance_epoch",
      replacement: "attempt.runtime_instance_epoch<>NEW.runtime_instance_epoch",
    },
    {
      dialect: "SQLite",
      invariant: "attempt identity worker",
      original: "attempt.worker_id=registration.worker_id",
      replacement: "attempt.worker_id<>registration.worker_id",
    },
    {
      dialect: "SQLite",
      invariant: "attempt identity boot id",
      original: "registration.boot_id=NEW.boot_id",
      replacement: "registration.boot_id<>NEW.boot_id",
    },
    {
      dialect: "SQLite",
      invariant: "attempt identity boot generation",
      original: "registration.boot_generation=NEW.boot_generation",
      replacement: "registration.boot_generation<=NEW.boot_generation",
    },
    {
      dialect: "SQLite",
      invariant: "attempt binding update rejection",
      original:
        "SELECT RAISE(ABORT, 'source-verifier attempt boot binding is immutable');",
      replacement: "SELECT 1;",
    },
    {
      dialect: "SQLite",
      invariant: "attempt binding parent-aware delete",
      original: "WHERE attempt.attempt_id=OLD.attempt_id",
      replacement: "WHERE attempt.attempt_id=NEW.attempt_id",
    },
    {
      dialect: "SQLite",
      invariant: "attempt binding parent cascade",
      original:
        "FOREIGN KEY(attempt_id) REFERENCES jobs_original_source_verification_attempts(attempt_id)\n" +
        "    ON DELETE CASCADE",
      replacement:
        "FOREIGN KEY(attempt_id) REFERENCES jobs_original_source_verification_attempts(attempt_id)\n" +
        "    ON DELETE RESTRICT",
    },
    {
      dialect: "SQLite",
      invariant: "heartbeat binding update rejection",
      original:
        "SELECT RAISE(ABORT,'source-verifier heartbeat boot binding is immutable');",
      replacement: "SELECT 1;",
    },
    {
      dialect: "SQLite",
      invariant: "heartbeat binding delete rejection",
      original:
        "SELECT RAISE(ABORT,'source-verifier heartbeat boot binding is permanent');",
      replacement: "SELECT 1;",
    },
    {
      dialect: "SQLite",
      invariant: "v1 readiness excludes boot manifests",
      original:
        "AND NOT EXISTS (\n" +
        "    SELECT 1 FROM jobs_managed_cloud_manifest_source_verifier_boot_protocols boot",
      replacement:
        "AND EXISTS (\n" +
        "    SELECT 1 FROM jobs_managed_cloud_manifest_source_verifier_boot_protocols boot",
    },
    {
      dialect: "Postgres",
      invariant: "ready generation non-regression",
      migrationFile:
        "infra/postgres/server-runtime/041_jobs_source_verifier_boot_generation_authority.sql",
      original:
        "OR NEW.ready_boot_generation < OLD.ready_boot_generation)) THEN",
      replacement:
        "OR NEW.ready_boot_generation > OLD.ready_boot_generation)) THEN",
    },
    {
      dialect: "Postgres",
      invariant: "ready generation non-regression rejection",
      migrationFile:
        "infra/postgres/server-runtime/041_jobs_source_verifier_boot_generation_authority.sql",
      original:
        "RAISE EXCEPTION 'source-verifier boot generation head is monotonic';",
      replacement: "NULL;",
    },
    {
      dialect: "Postgres",
      invariant: "heartbeat binding current generation",
      original: "AND h.current_boot_generation=NEW.boot_generation)",
      replacement: "AND h.current_boot_generation<=NEW.boot_generation)",
    },
    {
      dialect: "Postgres",
      invariant: "attempt binding ready generation",
      original: "AND head.ready_boot_generation=NEW.boot_generation",
      replacement: "AND head.ready_boot_generation<=NEW.boot_generation",
    },
    {
      dialect: "Postgres",
      invariant: "startup requires degraded",
      original: "heartbeat.health_state='degraded'",
      replacement: "heartbeat.health_state='ready'",
    },
    {
      dialect: "Postgres",
      invariant: "startup rejects probe-failed substitute",
      original: "heartbeat.reason_code='startup'",
      replacement: "heartbeat.reason_code='probe_failed'",
    },
    {
      dialect: "Postgres",
      invariant: "attempt identity runtime instance",
      original: "attempt.runtime_instance_id=NEW.runtime_instance_id",
      replacement: "attempt.runtime_instance_id<>NEW.runtime_instance_id",
    },
    {
      dialect: "Postgres",
      invariant: "attempt identity runtime epoch",
      original: "attempt.runtime_instance_epoch=NEW.runtime_instance_epoch",
      replacement: "attempt.runtime_instance_epoch<>NEW.runtime_instance_epoch",
    },
    {
      dialect: "Postgres",
      invariant: "attempt identity worker",
      original: "attempt.worker_id=registration.worker_id",
      replacement: "attempt.worker_id<>registration.worker_id",
    },
    {
      dialect: "Postgres",
      invariant: "attempt identity boot id",
      original: "registration.boot_id=NEW.boot_id",
      replacement: "registration.boot_id<>NEW.boot_id",
    },
    {
      dialect: "Postgres",
      invariant: "attempt identity boot generation",
      original: "registration.boot_generation=NEW.boot_generation",
      replacement: "registration.boot_generation<=NEW.boot_generation",
    },
    {
      dialect: "Postgres",
      invariant: "attempt binding update rejection",
      original:
        "IF TG_OP <> 'INSERT' THEN RAISE EXCEPTION 'source-verifier attempt boot binding is immutable'",
      replacement:
        "IF TG_OP <> 'INSERT' THEN NULL",
    },
    {
      dialect: "Postgres",
      invariant: "attempt binding parent-aware delete",
      original: "WHERE attempt.attempt_id=OLD.attempt_id",
      replacement: "WHERE attempt.attempt_id=NEW.attempt_id",
    },
    {
      dialect: "Postgres",
      invariant: "attempt binding parent cascade",
      original:
        "FOREIGN KEY(attempt_id) REFERENCES jobs_original_source_verification_attempts(attempt_id) ON DELETE CASCADE",
      replacement:
        "FOREIGN KEY(attempt_id) REFERENCES jobs_original_source_verification_attempts(attempt_id) ON DELETE RESTRICT",
    },
    {
      dialect: "Postgres",
      invariant: "heartbeat binding immutability rejection",
      original:
        "IF TG_OP <> 'INSERT' THEN RAISE EXCEPTION 'source-verifier heartbeat boot binding is immutable'",
      replacement: "IF TG_OP <> 'INSERT' THEN NULL",
    },
    {
      dialect: "Postgres",
      invariant: "v1 readiness excludes boot manifests exactly twice",
      original:
        "IF TG_OP = 'UPDATE' AND NOT EXISTS (\n" +
        "    SELECT 1 FROM jobs_managed_cloud_manifest_source_verifier_boot_protocols boot",
      replacement:
        "IF TG_OP = 'UPDATE' AND EXISTS (\n" +
        "    SELECT 1 FROM jobs_managed_cloud_manifest_source_verifier_boot_protocols boot",
    },
  ]) {
    assertPhase628DriftRejected({ ...mutation, sqlite, postgres });
  }

  for (const mutation of [
    {
      dialect: "SQLite",
      invariant: "trust-policy root-anchor continuity",
      original: "predecessor.root_anchor_sha256=NEW.root_anchor_sha256",
      replacement: "predecessor.root_anchor_sha256<>NEW.root_anchor_sha256",
    },
    {
      dialect: "Postgres",
      invariant: "signed-authority immutability",
      original: "RAISE EXCEPTION 'job-integrity signed authority is immutable'",
      replacement: "RETURN OLD",
    },
  ]) {
    assertPhase614BDriftRejected({ ...mutation, sqlite, postgres });
  }

  for (const mutation of [
    {
      dialect: "Postgres",
      invariant: "predecessor time ordering",
      original: "predecessor.recorded_at_ms <= NEW.recorded_at_ms",
      replacement: "predecessor.recorded_at_ms > NEW.recorded_at_ms",
    },
    {
      dialect: "SQLite",
      invariant: "released-to-released rejection",
      original:
        "NOT (NEW.transition = 'released' AND predecessor.transition = 'released')",
      replacement:
        "NOT (NEW.transition = 'held' AND predecessor.transition = 'held')",
    },
    {
      dialect: "Postgres",
      invariant: "first event is held",
      original: "predecessor_event_id IS NULL AND transition = 'held'",
      replacement: "predecessor_event_id IS NULL AND transition = 'released'",
    },
    {
      dialect: "SQLite",
      invariant: "previous revision is exactly n-1",
      original: "previous_revision_no = revision_no - 1",
      replacement: "previous_revision_no < revision_no",
    },
    {
      dialect: "Postgres",
      invariant: "ancestry revision/event uniqueness",
      original:
        "UNIQUE(capability, scope_kind, scope_id, revision_no, event_id)",
      replacement: "UNIQUE(capability, scope_kind, revision_no, event_id)",
    },
    {
      dialect: "SQLite",
      invariant: "head revision/event/ref uniqueness",
      original:
        "UNIQUE(capability, scope_kind, scope_id, revision_no, event_id, event_ref)",
      replacement:
        "UNIQUE(capability, scope_kind, scope_id, revision_no, event_id)",
    },
    {
      dialect: "Postgres",
      invariant: "ancestry foreign key",
      original:
        "capability, scope_kind, scope_id, previous_revision_no, predecessor_event_id",
      replacement:
        "capability, scope_kind, previous_revision_no, predecessor_event_id",
    },
    {
      dialect: "SQLite",
      invariant: "head foreign key",
      original:
        "capability, scope_kind, scope_id, head_revision, current_event_id, current_event_ref",
      replacement:
        "capability, scope_kind, scope_id, head_revision, current_event_id",
    },
    {
      dialect: "Postgres",
      invariant: "head insert revision is one",
      original: "NEW.head_revision <> 1",
      replacement: "NEW.head_revision < 1",
    },
    {
      dialect: "SQLite",
      invariant: "head insert capability link",
      original: "event.capability = NEW.capability",
      replacement: "event.capability <> NEW.capability",
    },
    {
      dialect: "Postgres",
      invariant: "head insert revision link",
      original: "event.revision_no = NEW.head_revision",
      replacement: "event.revision_no <= NEW.head_revision",
    },
    {
      dialect: "SQLite",
      invariant: "head insert event-ref link",
      original: "event.event_ref = NEW.current_event_ref",
      replacement: "event.event_ref <> NEW.current_event_ref",
    },
    {
      dialect: "Postgres",
      invariant: "head insert transition link",
      original: "event.transition = NEW.state",
      replacement: "event.transition <> NEW.state",
    },
    {
      dialect: "SQLite",
      invariant: "head insert actor link",
      original: "event.recorded_by = NEW.updated_by",
      replacement: "event.recorded_by <> NEW.updated_by",
    },
    {
      dialect: "Postgres",
      invariant: "head insert time link",
      original: "event.recorded_at_ms = NEW.updated_at_ms",
      replacement: "event.recorded_at_ms <= NEW.updated_at_ms",
    },
    {
      dialect: "SQLite",
      invariant: "head update advances exactly one revision",
      original: "NEW.head_revision <> OLD.head_revision + 1",
      replacement: "NEW.head_revision <= OLD.head_revision",
    },
    {
      dialect: "Postgres",
      invariant: "head scope-ref is immutable",
      original: "NEW.scope_ref <> OLD.scope_ref",
      replacement: "NEW.scope_ref = OLD.scope_ref",
    },
    {
      dialect: "SQLite",
      invariant: "head update predecessor-event link",
      original: "event.predecessor_event_id = OLD.current_event_id",
      replacement: "event.predecessor_event_id <> OLD.current_event_id",
    },
    {
      dialect: "Postgres",
      invariant: "head update event-id link",
      original:
        "event.predecessor_event_id = OLD.current_event_id\n" +
        "          AND event.event_id = NEW.current_event_id",
      replacement:
        "event.predecessor_event_id = OLD.current_event_id\n" +
        "          AND event.event_id <> NEW.current_event_id",
    },
    {
      dialect: "Postgres",
      invariant: "event update immutability",
      original:
        "CREATE TRIGGER trg_jobs_operational_hold_events_no_update\n" +
        "BEFORE UPDATE ON jobs_operational_hold_events",
      replacement:
        "CREATE TRIGGER trg_jobs_operational_hold_events_no_update\n" +
        "BEFORE INSERT ON jobs_operational_hold_events",
    },
    {
      dialect: "SQLite",
      invariant: "event delete immutability",
      original:
        "CREATE TRIGGER IF NOT EXISTS trg_jobs_operational_hold_events_no_delete\n" +
        "BEFORE DELETE ON jobs_operational_hold_events",
      replacement:
        "CREATE TRIGGER IF NOT EXISTS trg_jobs_operational_hold_events_no_delete\n" +
        "BEFORE INSERT ON jobs_operational_hold_events",
    },
    {
      dialect: "Postgres",
      invariant: "head delete immutability",
      original:
        "CREATE TRIGGER trg_jobs_operational_hold_heads_no_delete\n" +
        "BEFORE DELETE ON jobs_operational_hold_heads",
      replacement:
        "CREATE TRIGGER trg_jobs_operational_hold_heads_no_delete\n" +
        "BEFORE UPDATE ON jobs_operational_hold_heads",
    },
    {
      dialect: "Postgres",
      invariant: "trigger row binding",
      original:
        "BEFORE INSERT ON jobs_operational_hold_events\n" +
        "FOR EACH ROW EXECUTE FUNCTION validate_jobs_operational_hold_event();",
      replacement:
        "BEFORE INSERT ON jobs_operational_hold_events\n" +
        "FOR EACH STATEMENT EXECUTE FUNCTION validate_jobs_operational_hold_event();",
    },
  ]) {
    assertOperationalHoldDriftRejected({ ...mutation, sqlite, postgres });
  }

  for (const mutation of [
    {
      dialect: "Postgres",
      invariant: "taxonomy version bounded length",
      original:
        "  taxonomy_version                       TEXT NOT NULL\n" +
        "    CHECK(length(taxonomy_version) BETWEEN 1 AND 64),",
      replacement:
        "  taxonomy_version                       TEXT NOT NULL\n" +
        "    CHECK(length(taxonomy_version) <= 64),",
    },
    {
      dialect: "Postgres",
      invariant: "taxonomy activation timestamp safe-integer range",
      original:
        "  activated_at_ms                        BIGINT NOT NULL\n" +
        "    CHECK(activated_at_ms BETWEEN 0 AND 9007199254740991),",
      replacement:
        "  activated_at_ms                        BIGINT NOT NULL\n" +
        "    CHECK(activated_at_ms >= 0),",
    },
    {
      dialect: "Postgres",
      invariant: "account input generation safe-integer range",
      original:
        "  account_id                             TEXT NOT NULL,\n" +
        "  input_generation                       BIGINT NOT NULL\n" +
        "    CHECK(input_generation BETWEEN 1 AND 9007199254740991),",
      replacement:
        "  account_id                             TEXT NOT NULL,\n" +
        "  input_generation                       BIGINT NOT NULL\n" +
        "    CHECK(input_generation >= 1),",
    },
    {
      dialect: "Postgres",
      invariant: "account previous input generation safe-integer range",
      original:
        "  input_generation                       BIGINT NOT NULL\n" +
        "    CHECK(input_generation BETWEEN 1 AND 9007199254740991),\n" +
        "  previous_input_generation              BIGINT NOT NULL\n" +
        "    CHECK(previous_input_generation BETWEEN 0 AND 9007199254740991),",
      replacement:
        "  input_generation                       BIGINT NOT NULL\n" +
        "    CHECK(input_generation BETWEEN 1 AND 9007199254740991),\n" +
        "  previous_input_generation              BIGINT NOT NULL\n" +
        "    CHECK(previous_input_generation >= 0),",
    },
    {
      dialect: "Postgres",
      invariant: "account input timestamp safe-integer range",
      original:
        "  changed_at_ms                          BIGINT NOT NULL\n" +
        "    CHECK(changed_at_ms BETWEEN 0 AND 9007199254740991),\n" +
        "  UNIQUE(account_id, input_generation),",
      replacement:
        "  changed_at_ms                          BIGINT NOT NULL\n" +
        "    CHECK(changed_at_ms >= 0),\n" +
        "  UNIQUE(account_id, input_generation),",
    },
    {
      dialect: "Postgres",
      invariant: "Track input generation safe-integer range",
      original:
        "  career_track_id                        TEXT NOT NULL,\n" +
        "  input_generation                       BIGINT NOT NULL\n" +
        "    CHECK(input_generation BETWEEN 1 AND 9007199254740991),",
      replacement:
        "  career_track_id                        TEXT NOT NULL,\n" +
        "  input_generation                       BIGINT NOT NULL\n" +
        "    CHECK(input_generation >= 1),",
    },
    {
      dialect: "Postgres",
      invariant: "Track previous input generation safe-integer range",
      original:
        "  career_track_id                        TEXT NOT NULL,\n" +
        "  input_generation                       BIGINT NOT NULL\n" +
        "    CHECK(input_generation BETWEEN 1 AND 9007199254740991),\n" +
        "  previous_input_generation              BIGINT NOT NULL\n" +
        "    CHECK(previous_input_generation BETWEEN 0 AND 9007199254740991),",
      replacement:
        "  career_track_id                        TEXT NOT NULL,\n" +
        "  input_generation                       BIGINT NOT NULL\n" +
        "    CHECK(input_generation BETWEEN 1 AND 9007199254740991),\n" +
        "  previous_input_generation              BIGINT NOT NULL\n" +
        "    CHECK(previous_input_generation >= 0),",
    },
    {
      dialect: "Postgres",
      invariant: "Track input timestamp safe-integer range",
      original:
        "  changed_at_ms                          BIGINT NOT NULL\n" +
        "    CHECK(changed_at_ms BETWEEN 0 AND 9007199254740991),\n" +
        "  UNIQUE(account_id, career_track_id, input_generation),",
      replacement:
        "  changed_at_ms                          BIGINT NOT NULL\n" +
        "    CHECK(changed_at_ms >= 0),\n" +
        "  UNIQUE(account_id, career_track_id, input_generation),",
    },
    {
      dialect: "Postgres",
      invariant: "policy revision taxonomy activation epoch safe-integer range",
      original:
        "  taxonomy_activation_epoch              BIGINT NOT NULL\n" +
        "    CHECK(taxonomy_activation_epoch BETWEEN 1 AND 9007199254740991),",
      replacement:
        "  taxonomy_activation_epoch              BIGINT NOT NULL\n" +
        "    CHECK(taxonomy_activation_epoch >= 1),",
    },
    {
      dialect: "Postgres",
      invariant: "policy revision canonicalizer version safe-integer range",
      original:
        "  taxonomy_activation_epoch              BIGINT NOT NULL\n" +
        "    CHECK(taxonomy_activation_epoch BETWEEN 1 AND 9007199254740991),\n" +
        "  canonicalizer_schema_version           BIGINT NOT NULL\n" +
        "    CHECK(canonicalizer_schema_version BETWEEN 1 AND 9007199254740991),",
      replacement:
        "  taxonomy_activation_epoch              BIGINT NOT NULL\n" +
        "    CHECK(taxonomy_activation_epoch BETWEEN 1 AND 9007199254740991),\n" +
        "  canonicalizer_schema_version           BIGINT NOT NULL\n" +
        "    CHECK(canonicalizer_schema_version >= 1),",
    },
    {
      dialect: "Postgres",
      invariant: "policy revision account input generation safe-integer range",
      original:
        "  account_input_generation               BIGINT NOT NULL\n" +
        "    CHECK(account_input_generation BETWEEN 1 AND 9007199254740991),",
      replacement:
        "  account_input_generation               BIGINT NOT NULL\n" +
        "    CHECK(account_input_generation >= 1),",
    },
    {
      dialect: "Postgres",
      invariant: "policy revision Track input generation safe-integer range",
      original:
        "  track_input_generation                 BIGINT NOT NULL\n" +
        "    CHECK(track_input_generation BETWEEN 1 AND 9007199254740991),",
      replacement:
        "  track_input_generation                 BIGINT NOT NULL\n" +
        "    CHECK(track_input_generation >= 1),",
    },
    {
      dialect: "SQLite",
      invariant: "policy head advances exactly one generation",
      original: "NEW.head_generation <> OLD.head_generation + 1",
      replacement: "NEW.head_generation <= OLD.head_generation",
    },
    {
      dialect: "Postgres",
      invariant: "policy head predecessor transition binds old head",
      original:
        "NEW.predecessor_head_transition_sha256 <> OLD.head_transition_sha256",
      replacement:
        "NEW.predecessor_head_transition_sha256 = OLD.head_transition_sha256",
    },
    {
      dialect: "Postgres",
      invariant: "policy head binds immutable event review receipt",
      original: "AND event.review_receipt_id = NEW.review_receipt_id",
      replacement: "AND event.review_receipt_id <> NEW.review_receipt_id",
    },
    {
      dialect: "Postgres",
      invariant: "taxonomy activation changed-tuple requirement",
      original:
        "AND NOT (\n" +
        "             predecessor.taxonomy_version = NEW.taxonomy_version",
      replacement:
        "AND (\n" +
        "             predecessor.taxonomy_version = NEW.taxonomy_version",
    },
  ]) {
    assertPhase613DriftRejected({ ...mutation, sqlite, postgres });
  }

  const missingTrackPolicyTable = sqlite.replace(
    /CREATE TABLE IF NOT EXISTS jobs_track_policy_taxonomy_activation_events \([\s\S]*?\n\);/,
    "",
  );
  assert(
    compareJobsSchemas(missingTrackPolicyTable, postgres).some((issue) =>
      issue.includes("SQLite parity tables"),
    ),
  );

  const missingTrackPolicyIndex = postgres.replace(
    /CREATE INDEX IF NOT EXISTS idx_jobs_track_policy_account_input_transitions_history[\s\S]*?input_generation DESC\s*\);/,
    "",
  );
  assert(
    compareJobsSchemas(sqlite, missingTrackPolicyIndex).some((issue) =>
      issue.includes(
        "Postgres jobs_track_policy_account_input_transitions required index",
      ),
    ),
  );

  const migrationRunner = fs.readFileSync(
    path.join(repoRoot, "server/src/db/mod.rs"),
    "utf8",
  );
  assertSourceVerifierStartupMigrationRegistration(migrationRunner);
  for (const [index, migrationName] of [
    "062_jobs_original_source_verifier_startup_authority.sql",
    "040_jobs_original_source_verifier_startup_authority.sql",
    "063_jobs_source_verifier_boot_generation_authority.sql",
    "041_jobs_source_verifier_boot_generation_authority.sql",
    "065_jobs_source_verifier_heartbeat_boot_bindings.sql",
    "043_jobs_source_verifier_heartbeat_boot_bindings.sql",
  ].entries()) {
    const missingStartupMigration = migrationRunner.replace(
      migrationName,
      `removed-source-verifier-startup-${index}.sql`,
    );
    assert.throws(
      () => assertSourceVerifierStartupMigrationRegistration(missingStartupMigration),
      /migration runner must/i,
      `Removing either ${migrationName} binding must fail its structural guard`,
    );
  }
  assert.deepEqual(checkPhase613MigrationRegistration(migrationRunner), []);
  assert.deepEqual(checkPhase614MigrationRegistration(migrationRunner), []);
  assert.deepEqual(checkPhase614BMigrationRegistration(migrationRunner), []);
  assert.deepEqual(checkPhase621MigrationRegistration(migrationRunner), []);
  const missingSqliteMigrationRegistration = replaceFirstForGuardTest(
    migrationRunner,
    "    SQLITE_JOBS_CANONICAL_TAXONOMY_AUTHORITY,\n",
    "",
    "SQLite 056 migration registration",
  );
  assert(
    checkPhase613MigrationRegistration(missingSqliteMigrationRegistration).some(
      (issue) => issue.includes("SQLite migration runner must register"),
    ),
  );
  const missingPostgresMigrationRegistration = replaceFirstForGuardTest(
    migrationRunner,
    "    (\n" +
      "        JOBS_CANONICAL_TAXONOMY_AUTHORITY_MIGRATION_ID,\n" +
      "        POSTGRES_JOBS_CANONICAL_TAXONOMY_AUTHORITY,\n" +
      "    ),\n",
    "",
    "Postgres 034 migration registration",
  );
  assert(
    checkPhase613MigrationRegistration(
      missingPostgresMigrationRegistration,
    ).some((issue) =>
      issue.includes("Postgres migration runner must register"),
    ),
  );
  const missingSqlitePhase614Registration = replaceFirstForGuardTest(
    migrationRunner,
    "    SQLITE_JOBS_ORIGINAL_SOURCE_VERIFICATION_AUTHORITY,\n",
    "",
    "SQLite 057 migration registration",
  );
  assert(
    checkPhase614MigrationRegistration(missingSqlitePhase614Registration).some(
      (issue) => issue.includes("SQLite migration runner must register"),
    ),
  );
  const missingPostgresPhase614Registration = replaceFirstForGuardTest(
    migrationRunner,
    "    (\n" +
      "        JOBS_ORIGINAL_SOURCE_VERIFICATION_AUTHORITY_MIGRATION_ID,\n" +
      "        POSTGRES_JOBS_ORIGINAL_SOURCE_VERIFICATION_AUTHORITY,\n" +
      "    ),\n",
    "",
    "Postgres 035 migration registration",
  );
  assert(
    checkPhase614MigrationRegistration(
      missingPostgresPhase614Registration,
    ).some((issue) =>
      issue.includes("Postgres migration runner must register"),
    ),
  );
  const missingSqlitePhase614BRegistration = replaceFirstForGuardTest(
    migrationRunner,
    "    SQLITE_JOBS_SIGNED_JOB_INTEGRITY_AUTHORITY,\n",
    "",
    "SQLite 058 migration registration",
  );
  assert(
    checkPhase614BMigrationRegistration(
      missingSqlitePhase614BRegistration,
    ).some((issue) =>
      issue.includes("SQLite migration runner must register"),
    ),
  );
  const missingPostgresPhase614BRegistration = replaceFirstForGuardTest(
    migrationRunner,
    "    (\n" +
      "        JOBS_SIGNED_JOB_INTEGRITY_AUTHORITY_MIGRATION_ID,\n" +
      "        POSTGRES_JOBS_SIGNED_JOB_INTEGRITY_AUTHORITY,\n" +
      "    ),\n",
    "",
    "Postgres 036 migration registration",
  );
  assert(
    checkPhase614BMigrationRegistration(
      missingPostgresPhase614BRegistration,
    ).some((issue) =>
      issue.includes("Postgres migration runner must register"),
    ),
  );
  const missingSqlitePhase621Registration = replaceFirstForGuardTest(
    migrationRunner,
    "    SQLITE_JOBS_PUBLIC_BETA_ACCESS,\n",
    "",
    "SQLite 060 migration registration",
  );
  assert(
    checkPhase621MigrationRegistration(missingSqlitePhase621Registration).some(
      (issue) => issue.includes("SQLite migration runner must register"),
    ),
  );
  const missingPostgresPhase621Registration = replaceFirstForGuardTest(
    migrationRunner,
    "    (\n" +
      "        JOBS_PUBLIC_BETA_ACCESS_MIGRATION_ID,\n" +
      "        POSTGRES_JOBS_PUBLIC_BETA_ACCESS,\n" +
      "    ),\n",
    "",
    "Postgres 038 migration registration",
  );
  assert(
    checkPhase621MigrationRegistration(
      missingPostgresPhase621Registration,
    ).some((issue) =>
      issue.includes("Postgres migration runner must register"),
    ),
  );

  const missingAtsBindingTable = sqlite.replace(
    /CREATE TABLE IF NOT EXISTS jobs_application_ats_certification_bindings \([\s\S]*?\n\);/,
    "",
  );
  assert(
    compareJobsSchemas(missingAtsBindingTable, postgres).some((issue) =>
      issue.includes("SQLite parity tables"),
    ),
  );

  const missingProcessRuntimeIndex = postgres.replace(
    /CREATE INDEX IF NOT EXISTS idx_jobs_runner_process_runtime_bindings_process[\s\S]*?\n\s*\);/,
    "",
  );
  assert(
    compareJobsSchemas(sqlite, missingProcessRuntimeIndex).some((issue) =>
      issue.includes("jobs_runner_process_runtime_bindings index"),
    ),
  );

  const missingRunnerVolumeTable = sqlite.replace(
    /CREATE TABLE IF NOT EXISTS jobs_runner_volumes \([\s\S]*?\n\);/,
    "",
  );
  assert(
    compareJobsSchemas(missingRunnerVolumeTable, postgres).some((issue) =>
      issue.includes("SQLite parity tables"),
    ),
  );

  const missingRunnerVolumeIndex = postgres.replace(
    /CREATE INDEX IF NOT EXISTS idx_jobs_runner_volumes_worker_status[\s\S]*?updated_at_ms\);/,
    "",
  );

  const missingBrowserReleaseBinding = sqlite.replace(
    /CREATE TABLE IF NOT EXISTS jobs_local_run_release_bindings \([\s\S]*?\n\);/,
    "",
  );
  assert(
    compareJobsSchemas(missingBrowserReleaseBinding, postgres).some((issue) =>
      issue.includes("SQLite parity tables"),
    ),
  );

  const missingBrowserActivationIndex = postgres.replace(
    /CREATE INDEX IF NOT EXISTS idx_jobs_browser_release_activations_channel_history[\s\S]*?expires_at_ms\s*\);/,
    "",
  );
  assert(
    compareJobsSchemas(sqlite, missingBrowserActivationIndex).some((issue) =>
      issue.includes(
        "Postgres jobs_browser_release_activations required index",
      ),
    ),
  );

  const weakenedBrowserRevocationSubject = postgres.replace(
    "UNIQUE(subject_kind, subject_id, subject_sha256),",
    "UNIQUE(subject_kind, subject_sha256),",
  );
  assert(
    compareJobsSchemas(sqlite, weakenedBrowserRevocationSubject).some((issue) =>
      issue.includes("jobs_browser_release_revocations definition"),
    ),
  );

  const weakenedBrowserRevocationIndex = postgres.replace(
    /(CREATE INDEX IF NOT EXISTS idx_jobs_browser_release_revocations_subject[\s\S]*?subject_kind, )subject_id, /,
    "$1",
  );
  assert(
    compareJobsSchemas(sqlite, weakenedBrowserRevocationIndex).some((issue) =>
      issue.includes(
        "Postgres jobs_browser_release_revocations required index",
      ),
    ),
  );

  const missingBrowserTrustPolicy = sqlite.replace(
    /CREATE TABLE IF NOT EXISTS jobs_browser_release_trust_policies \([\s\S]*?\n\);/,
    "",
  );
  assert(
    compareJobsSchemas(missingBrowserTrustPolicy, postgres).some((issue) =>
      issue.includes("SQLite parity tables"),
    ),
  );

  const missingBrowserChannelHead = postgres.replace(
    /CREATE TABLE IF NOT EXISTS jobs_browser_release_channel_heads \([\s\S]*?\n\);/,
    "",
  );
  assert(
    compareJobsSchemas(sqlite, missingBrowserChannelHead).some((issue) =>
      issue.includes("Postgres parity tables"),
    ),
  );

  const missingBrowserSignatureSetIndex = sqlite.replace(
    /CREATE INDEX IF NOT EXISTS idx_jobs_browser_release_signature_sets_target[\s\S]*?role\s*\);/,
    "",
  );
  assert(
    compareJobsSchemas(missingBrowserSignatureSetIndex, postgres).some(
      (issue) =>
        issue.includes(
          "SQLite jobs_browser_release_signature_sets required index",
        ),
    ),
  );

  const missingBrowserAuditActor = postgres.replace(
    /recorded_by\s+TEXT NOT NULL CHECK\(length\(recorded_by\) > 0\),/,
    "",
  );
  assert(
    compareJobsSchemas(sqlite, missingBrowserAuditActor).some((issue) =>
      issue.includes("jobs_browser_release_signature_sets definition"),
    ),
  );
  assert(
    compareJobsSchemas(sqlite, missingRunnerVolumeIndex).some((issue) =>
      issue.includes("Postgres jobs_runner_volumes required index"),
    ),
  );

  const missingDeletionIntentTable = sqlite.replace(
    /CREATE TABLE IF NOT EXISTS account_deletion_intents \([\s\S]*?\n    \);/,
    "",
  );
  assert(
    compareJobsSchemas(missingDeletionIntentTable, postgres).some((issue) =>
      issue.includes("SQLite parity tables"),
    ),
  );

  const missingEvidenceCapacityTable = sqlite.replace(
    /CREATE TABLE IF NOT EXISTS jobs_submission_evidence_capacity \([\s\S]*?\n    \);/,
    "",
  );
  assert(
    compareJobsSchemas(missingEvidenceCapacityTable, postgres).some((issue) =>
      issue.includes("SQLite parity tables"),
    ),
  );

  const missingLeaseTable = sqlite.replace(
    /CREATE TABLE IF NOT EXISTS jobs_execution_leases \([\s\S]*?\n    \);/,
    "",
  );
  assert(
    compareJobsSchemas(missingLeaseTable, postgres).some((issue) =>
      issue.includes("SQLite parity tables"),
    ),
  );

  const missingLocalResumeTable = sqlite.replace(
    /CREATE TABLE IF NOT EXISTS jobs_local_run_resume_actions \([\s\S]*?\n    \);/,
    "",
  );
  assert(
    compareJobsSchemas(missingLocalResumeTable, postgres).some((issue) =>
      issue.includes("SQLite parity tables"),
    ),
  );

  const missingCommunicationTable = sqlite.replace(
    /CREATE TABLE IF NOT EXISTS jobs_communication_actions \([\s\S]*?\n    \);/,
    "",
  );

  const missingCommunicationEvidenceTable = sqlite.replace(
    /CREATE TABLE IF NOT EXISTS jobs_communication_action_attempt_evidence \([\s\S]*?\n\);/,
    "",
  );
  assert(
    compareJobsSchemas(missingCommunicationEvidenceTable, postgres).some(
      (issue) => issue.includes("SQLite parity tables"),
    ),
  );

  const missingCommunicationReconciliationIndex = postgres.replace(
    /CREATE INDEX IF NOT EXISTS idx_jobs_communication_reconciliations_action[\s\S]*?\);/,
    "",
  );
  assert(
    compareJobsSchemas(sqlite, missingCommunicationReconciliationIndex).some(
      (issue) =>
        issue.includes("jobs_communication_action_reconciliations index"),
    ),
  );
  assert(
    compareJobsSchemas(missingCommunicationTable, postgres).some((issue) =>
      issue.includes("SQLite parity tables"),
    ),
  );

  const missingColumn = postgres.replace(
    "      started_at_ms BIGINT NOT NULL\n",
    "",
  );
  assert(
    compareJobsSchemas(sqlite, missingColumn).some((issue) =>
      issue.includes("jobs_discovery_runs definition"),
    ),
  );

  const changedResumeActionConstraint = postgres.replace(
    "      action TEXT NOT NULL CHECK(action = 'approve_submission'),",
    "      action TEXT NOT NULL,",
  );
  assert(
    compareJobsSchemas(sqlite, changedResumeActionConstraint).some((issue) =>
      issue.includes("jobs_local_run_resume_actions definition"),
    ),
  );

  const missingDiscoveryIndex = postgres.replace(
    /CREATE INDEX IF NOT EXISTS idx_jobs_discovery_memberships_job[\s\S]*?job_id\);/,
    "",
  );
  assert(
    compareJobsSchemas(sqlite, missingDiscoveryIndex).some((issue) =>
      issue.includes("jobs_discovery_memberships index"),
    ),
  );

  const missingBindingIndex = postgres.replace(
    /CREATE INDEX IF NOT EXISTS idx_jobs_execution_leases_binding[\s\S]*?run_id\);/,
    "",
  );
  assert(
    compareJobsSchemas(sqlite, missingBindingIndex).some((issue) =>
      issue.includes("Postgres jobs_execution_leases required index"),
    ),
  );

  const sqliteWithoutBinding = sqlite.replace(
    /CREATE INDEX IF NOT EXISTS idx_jobs_execution_leases_binding[\s\S]*?run_id\);/,
    "",
  );
  assert(
    compareJobsSchemas(sqliteWithoutBinding, missingBindingIndex).some(
      (issue) => issue.includes("required index"),
    ),
  );

  const weakenedActivePredicate = postgres.replace(
    "WHERE phase IN ('prepared', 'click_started');",
    "WHERE phase IN ('prepared');",
  );
  assert(
    compareJobsSchemas(sqlite, weakenedActivePredicate).some((issue) =>
      issue.includes("Postgres jobs_execution_leases required index"),
    ),
  );

  const nonUniqueActiveProfile = postgres.replace(
    "CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_execution_leases_active_profile",
    "CREATE INDEX IF NOT EXISTS idx_jobs_execution_leases_active_profile",
  );
  assert(
    compareJobsSchemas(sqlite, nonUniqueActiveProfile).some((issue) =>
      issue.includes("Postgres jobs_execution_leases required index"),
    ),
  );

  const missingResumeApplicationIndex = postgres.replace(
    /CREATE INDEX IF NOT EXISTS idx_jobs_local_resume_actions_application[\s\S]*?created_at_ms DESC\);/,
    "",
  );
  assert(
    compareJobsSchemas(sqlite, missingResumeApplicationIndex).some((issue) =>
      issue.includes("Postgres jobs_local_run_resume_actions required index"),
    ),
  );

  const weakenedResumePredicate = postgres.replace(
    "WHERE status = 'approved';",
    "WHERE status IN ('approved', 'consumed');",
  );
  assert(
    compareJobsSchemas(sqlite, weakenedResumePredicate).some((issue) =>
      issue.includes("Postgres jobs_local_run_resume_actions required index"),
    ),
  );

  const nonUniqueActiveResume = postgres.replace(
    "CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_local_resume_actions_active_run",
    "CREATE INDEX IF NOT EXISTS idx_jobs_local_resume_actions_active_run",
  );
  assert(
    compareJobsSchemas(sqlite, nonUniqueActiveResume).some((issue) =>
      issue.includes("Postgres jobs_local_run_resume_actions required index"),
    ),
  );

  const missingCommunicationDueIndex = postgres.replace(
    /CREATE INDEX IF NOT EXISTS idx_jobs_communication_actions_due[\s\S]*?lease_expires_at_ms\);/,
    "",
  );
  assert(
    compareJobsSchemas(sqlite, missingCommunicationDueIndex).some((issue) =>
      issue.includes("Postgres jobs_communication_actions required index"),
    ),
  );

  const missingEvidenceCapacityAccountIndex = postgres.replace(
    /CREATE INDEX IF NOT EXISTS idx_jobs_submission_evidence_capacity_account[\s\S]*?expires_at_ms\);/,
    "",
  );
  assert(
    compareJobsSchemas(sqlite, missingEvidenceCapacityAccountIndex).some(
      (issue) =>
        issue.includes(
          "Postgres jobs_submission_evidence_capacity required index",
        ),
    ),
  );

  const weakenedEvidenceCapacityPredicate = postgres.replace(
    "WHERE state = 'active';",
    "WHERE state IN ('active', 'committed');",
  );
  assert(
    compareJobsSchemas(sqlite, weakenedEvidenceCapacityPredicate).some(
      (issue) =>
        issue.includes(
          "Postgres jobs_submission_evidence_capacity required index",
        ),
    ),
  );

  const weakenedPublicBetaCap = postgres.replace(
    "      hard_cap BIGINT NOT NULL CHECK(hard_cap >= 0 AND hard_cap <= 10000),",
    "      hard_cap BIGINT NOT NULL CHECK(hard_cap >= 0),",
  );
  assert(
    compareJobsSchemas(sqlite, weakenedPublicBetaCap).some((issue) =>
      issue.includes("jobs_public_beta_cohorts definition"),
    ),
  );

  const missingPublicBetaActiveIndex = postgres.replace(
    /CREATE INDEX IF NOT EXISTS idx_jobs_public_beta_overrides_active[\s\S]*?WHERE denied = 1;/,
    "",
  );
  assert(
    compareJobsSchemas(sqlite, missingPublicBetaActiveIndex).some((issue) =>
      issue.includes("Postgres jobs_public_beta_overrides required index"),
    ),
  );

  const uncoveredDiscoveryTable = `${postgres}\nCREATE TABLE IF NOT EXISTS jobs_discovery_unchecked (id TEXT PRIMARY KEY);`;
  assert(
    compareJobsSchemas(sqlite, uncoveredDiscoveryTable).some((issue) =>
      issue.includes("Postgres parity tables"),
    ),
  );
}

function testLicenseInventory() {
  const lockfile = {
    lockfileVersion: 3,
    packages: {
      "": { name: "example" },
      "node_modules/good-package": {
        version: "1.2.3",
        resolved:
          "https://registry.npmjs.org/good-package/-/good-package-1.2.3.tgz",
        integrity: "sha512-example",
        license: "MIT",
      },
      "node_modules/unionfs": {
        version: "4.6.0",
        resolved: "https://registry.npmjs.org/unionfs/-/unionfs-4.6.0.tgz",
        integrity: "sha512-example",
      },
    },
  };
  const inventory = inventoryPackageLock(lockfile);
  assert.deepEqual(inventory.issues, []);
  assert.equal(inventory.packages, 2);
  assert.equal(inventory.overrideCount, 1);

  const missingLicense = structuredClone(lockfile);
  missingLicense.packages["node_modules/no-license"] = {
    version: "1.0.0",
    resolved: "https://registry.npmjs.org/no-license/-/no-license-1.0.0.tgz",
    integrity: "sha512-example",
  };
  assert(
    inventoryPackageLock(missingLicense).issues.some((issue) =>
      issue.includes("missing audited license metadata"),
    ),
  );

  const rootManifest = { workspaces: ["worker"] };
  const workspaces = new Map([
    [
      "worker",
      { name: "worker", version: "1.0.0", dependencies: { dep: "^1.0.0" } },
    ],
  ]);
  const workspaceLock = {
    packages: {
      worker: {
        name: "worker",
        version: "1.0.0",
        dependencies: { dep: "^1.0.0" },
      },
    },
  };
  assert.deepEqual(
    validateWorkspaceLock(rootManifest, workspaces, workspaceLock),
    [],
  );
  workspaceLock.packages.worker.dependencies.dep = "^2.0.0";
  assert(
    validateWorkspaceLock(rootManifest, workspaces, workspaceLock).some(
      (issue) => issue.includes("dependencies"),
    ),
  );
}

function testProvenance() {
  const provenance = `
## Supplied repositories
| Repository | Reviewed commit | License observed | Bluey decision |
| --- | --- | --- | --- |
| [\`example-source\`](https://github.com/example/example-source) | \`abcdef123456\` | MIT | Adapted parser behavior into \`src/parser.ts\`. |

## Shipped adaptation map
| Bluey file | Source lineage | Bluey changes |
| --- | --- | --- |
| \`src/parser.ts\` | example-source | Reimplemented. |
`;
  const notices = "- example-source, Copyright (c) Example\n\n## MIT License\n";
  assert.equal(parseProvenanceRows(provenance).length, 1);
  assert.deepEqual(
    validateProvenance(
      provenance,
      notices,
      (filePath) => filePath === "src/parser.ts",
    ).issues,
    [],
  );

  const badCommit = provenance.replace("abcdef123456", "main");
  assert(
    validateProvenance(badCommit, notices, () => true).issues.some((issue) =>
      issue.includes("12 lowercase hex"),
    ),
  );
  assert(
    validateProvenance(provenance, "## MIT License\n", () => true).issues.some(
      (issue) => issue.includes("missing from"),
    ),
  );
}

testPrivacyPaths();
testSecretScanning();
testPortalBundleFreshnessWorkflowGuard();
testDependencySecurityWorkflowGuard();
testBuiltPortalPublicBetaTruth();
testJobsCiTimeBudgetGuard();
testIsolatedTestDiskAuthorityGuard();
testIntegrationTestSupportContainmentGuard();
testPublicBetaAdminMutationAuditBoundary();
checkBusinessMessagingSimulatorContainment();
testPhase628OperationsContractGuard();
testSchemaParity();
testLicenseInventory();
testProvenance();

console.log(
  "Jobs CI guard self-tests passed (privacy, portal bundle freshness, dependency security, " +
  "managed artifact publication, time budget, isolated test-disk authority, schema parity, " +
  "integration and " +
  "business-messaging simulator containment, lock inventory, and provenance).",
);
