import assert from "node:assert/strict";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

import { compareJobsSchemas } from "./check-jobs-schema-parity.mjs";
import {
  inventoryPackageLock,
  parseProvenanceRows,
  validateProvenance,
  validateWorkspaceLock,
} from "./check-provenance-licenses.mjs";
import { classifyTrackedPath, scanTextForSecrets } from "./privacy-gate.mjs";

const repoRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../..",
);

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
  `;
}

function testSchemaParity() {
  const sqlite = jobsParitySchema("INTEGER");
  const postgres = jobsParitySchema("BIGINT");
  assert.deepEqual(compareJobsSchemas(sqlite, postgres), []);

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
testSchemaParity();
testLicenseInventory();
testProvenance();

console.log(
  "Jobs CI guard self-tests passed (privacy, schema parity, lock inventory, and provenance).",
);
