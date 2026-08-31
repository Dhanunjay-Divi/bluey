import assert from "node:assert/strict";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

import {
  checkPhase613MigrationRegistration,
  checkPhase614MigrationRegistration,
  checkPhase614BMigrationRegistration,
  checkPhase621MigrationRegistration,
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
  const managedAuditIndex = managedRelease.indexOf("npm audit --audit-level=moderate");
  const managedBuildIndex = managedRelease.indexOf("docker buildx build --platform");
  assert(
    managedAuditIndex >= 0 &&
      managedBuildIndex > managedAuditIndex &&
      managedRelease.includes("major===22&&minor<13"),
    "Managed-cloud candidate must prove Node compatibility and audit before building artifacts",
  );
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
  ].join("\n");
  assert.deepEqual(compareJobsSchemas(sqlite, postgres), []);

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
testIntegrationTestSupportContainmentGuard();
testPublicBetaAdminMutationAuditBoundary();
checkBusinessMessagingSimulatorContainment();
testSchemaParity();
testLicenseInventory();
testProvenance();

console.log(
  "Jobs CI guard self-tests passed (privacy, portal bundle freshness, dependency security, " +
  "time budget, schema parity, integration and business-messaging simulator containment, " +
  "lock inventory, and provenance).",
);
