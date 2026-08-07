import fs from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

export const JOBS_PARITY_TABLES = [
  "account_deletion_intents",
  "jobs_browser_account_channel_assignments",
  "jobs_browser_release_activations",
  "jobs_browser_release_artifact_runtime_components",
  "jobs_browser_release_artifacts",
  "jobs_browser_release_channel_heads",
  "jobs_browser_release_channel_transitions",
  "jobs_browser_release_manifests",
  "jobs_browser_release_revocations",
  "jobs_browser_release_rollbacks",
  "jobs_browser_release_signature_sets",
  "jobs_browser_release_signatures",
  "jobs_browser_release_trust_keys",
  "jobs_browser_release_trust_policies",
  "jobs_application_ats_certification_bindings",
  "jobs_ats_certification_activations",
  "jobs_ats_certification_canary_allowlist_members",
  "jobs_ats_certification_canary_allowlist_revocations",
  "jobs_ats_certification_canary_allowlists",
  "jobs_ats_certification_canary_reservations",
  "jobs_ats_certification_circuit_events",
  "jobs_ats_certification_circuit_heads",
  "jobs_ats_certification_evidence",
  "jobs_ats_certification_head_transitions",
  "jobs_ats_certification_heads",
  "jobs_ats_certification_layout_observations",
  "jobs_ats_certification_manifest_check_results",
  "jobs_ats_certification_manifest_evidence",
  "jobs_ats_certification_manifest_layouts",
  "jobs_ats_certification_manifests",
  "jobs_ats_certification_quarantine_commands",
  "jobs_ats_certification_quarantine_heads",
  "jobs_ats_certification_revocations",
  "jobs_ats_certification_runtime_targets",
  "jobs_ats_certification_trust_head",
  "jobs_ats_certification_trust_keys",
  "jobs_ats_certification_trust_policies",
  "jobs_communication_actions",
  "jobs_communication_action_attempts",
  "jobs_communication_action_attempt_evidence",
  "jobs_communication_action_reconciliations",
  "jobs_communication_write_fences",
  "jobs_discovery_memberships",
  "jobs_discovery_runs",
  "jobs_discovery_sources",
  "jobs_execution_leases",
  "jobs_execution_lease_process_runtime_bindings",
  "jobs_execution_lease_volume_bindings",
  "jobs_local_run_claim_replays",
  "jobs_local_run_release_bindings",
  "jobs_local_run_resume_actions",
  "jobs_runner_legacy_inventory_authorities",
  "jobs_runner_account_subjects",
  "jobs_runner_purge_enforcements",
  "jobs_runner_purge_requests",
  "jobs_runner_purge_targets",
  "jobs_runner_purge_tombstones",
  "jobs_runner_process_runtime_bindings",
  "jobs_runner_process_runtime_grant_revocations",
  "jobs_runner_process_runtime_grants",
  "jobs_runner_volume_admission_grants",
  "jobs_runner_volume_authority_uses",
  "jobs_runner_volume_destructions",
  "jobs_runner_volume_fleet_state",
  "jobs_runner_volume_keys",
  "jobs_runner_volume_residencies",
  "jobs_runner_volume_storage_attestations",
  "jobs_runner_volumes",
  "jobs_submission_evidence_capacity",
];

const REQUIRED_INDEX_SIGNATURES = new Map([
  [
    "jobs_browser_account_channel_assignments",
    [
      "idx_jobs_browser_account_channel_assignments_current on jobs_browser_account_channel_assignments (account_id, assignment_generation desc)",
    ],
  ],
  [
    "jobs_browser_release_activations",
    [
      "idx_jobs_browser_release_activations_channel_history on jobs_browser_release_activations (channel, trust_generation desc, channel_sequence desc, expires_at_ms)",
    ],
  ],
  [
    "jobs_browser_release_artifact_runtime_components",
    [
      "idx_jobs_browser_release_runtime_components_target on jobs_browser_release_artifact_runtime_components (manifest_sha256, platform, architecture, package_kind, build_descriptor_sha256)",
    ],
  ],
  [
    "jobs_browser_release_artifacts",
    [
      "idx_jobs_browser_release_artifacts_descriptor_target on jobs_browser_release_artifacts (build_descriptor_sha256, platform, architecture, package_kind)",
    ],
  ],
  [
    "jobs_browser_release_channel_transitions",
    [
      "idx_jobs_browser_release_channel_transitions_history on jobs_browser_release_channel_transitions (channel, head_revision desc, recorded_at_ms desc)",
    ],
  ],
  [
    "jobs_browser_release_manifests",
    [
      "idx_jobs_browser_release_manifests_release on jobs_browser_release_manifests (release_sequence desc, recorded_at_ms desc)",
    ],
  ],
  [
    "jobs_browser_release_revocations",
    [
      "idx_jobs_browser_release_revocations_subject on jobs_browser_release_revocations (subject_kind, subject_id, subject_sha256, trust_generation desc, revocation_generation desc)",
    ],
  ],
  [
    "jobs_browser_release_rollbacks",
    [
      "idx_jobs_browser_release_rollbacks_channel on jobs_browser_release_rollbacks (channel, trust_generation desc, rollback_generation desc, recorded_at_ms desc)",
    ],
  ],
  [
    "jobs_browser_release_signature_sets",
    [
      "idx_jobs_browser_release_signature_sets_target on jobs_browser_release_signature_sets (target_audience, target_sha256, trust_generation, role)",
    ],
  ],
  [
    "jobs_browser_release_signatures",
    [
      "idx_jobs_browser_release_signatures_key on jobs_browser_release_signatures (key_id, signature_set_sha256)",
    ],
  ],
  [
    "jobs_browser_release_trust_keys",
    [
      "idx_jobs_browser_release_trust_keys_history on jobs_browser_release_trust_keys (key_id, trust_generation desc, state)",
    ],
  ],
  [
    "jobs_browser_release_trust_policies",
    [
      "idx_jobs_browser_release_trust_policies_generation on jobs_browser_release_trust_policies (trust_generation desc, expires_at_ms)",
    ],
  ],
  [
    "jobs_ats_certification_activations",
    [
      "idx_jobs_ats_certification_activations_scope on jobs_ats_certification_activations (scope_sha256, channel, channel_sequence desc, expires_at_ms)",
    ],
  ],
  [
    "jobs_ats_certification_canary_allowlist_members",
    [
      "idx_jobs_ats_certification_canary_allowlist_members_account on jobs_ats_certification_canary_allowlist_members (account_id, allowlist_sha256)",
    ],
  ],
  [
    "jobs_ats_certification_canary_reservations",
    [
      "idx_jobs_ats_certification_canary_capacity on jobs_ats_certification_canary_reservations (activation_sha256, period_key, status, reserved_at_ms)",
    ],
  ],
  [
    "jobs_ats_certification_evidence",
    [
      "idx_jobs_ats_certification_evidence_scope on jobs_ats_certification_evidence (provider, target_key, variant_key, surface_sha256, expires_at_ms)",
    ],
  ],
  [
    "jobs_ats_certification_heads",
    [
      "idx_jobs_ats_certification_heads_activation on jobs_ats_certification_heads (current_activation_sha256, channel)",
    ],
  ],
  [
    "jobs_ats_certification_layout_observations",
    [
      "idx_jobs_ats_certification_layout_observations_target on jobs_ats_certification_layout_observations (provider, target_fingerprint_sha256, page_variant, adapter_version, runner_target_sha256, expires_at_ms)",
    ],
  ],
  [
    "jobs_ats_certification_manifest_evidence",
    [
      "idx_jobs_ats_certification_manifest_evidence_evidence on jobs_ats_certification_manifest_evidence (evidence_sha256, manifest_sha256)",
    ],
  ],
  [
    "jobs_ats_certification_manifests",
    [
      "idx_jobs_ats_certification_manifests_scope on jobs_ats_certification_manifests (scope_sha256, manifest_generation desc, expires_at_ms)",
    ],
  ],
  [
    "jobs_ats_certification_quarantine_commands",
    [
      "idx_jobs_ats_certification_quarantine_commands_scope on jobs_ats_certification_quarantine_commands (scope_kind, scope_id, scope_sha256, command_sequence desc)",
    ],
  ],
  [
    "jobs_ats_certification_revocations",
    [
      "idx_jobs_ats_certification_revocations_generation on jobs_ats_certification_revocations (trust_policy_sha256, revocation_generation desc)",
      "idx_jobs_ats_certification_revocations_subject on jobs_ats_certification_revocations (subject_kind, subject_id, subject_sha256, effective_at_ms)",
    ].sort(),
  ],
  [
    "jobs_ats_certification_runtime_targets",
    [
      "idx_jobs_ats_certification_runtime_targets_runtime on jobs_ats_certification_runtime_targets (runtime_kind, runtime_id, runtime_sha256)",
    ],
  ],
  [
    "jobs_communication_actions",
    [
      "idx_jobs_communication_actions_account on jobs_communication_actions (account_id, application_id, created_at_ms desc)",
      "idx_jobs_communication_actions_due on jobs_communication_actions (status, next_attempt_at_ms, lease_expires_at_ms)",
      "unique idx_jobs_communication_action_authority on jobs_communication_actions (id, account_id, connection_id)",
      "unique idx_jobs_communication_action_attempt_authority on jobs_communication_actions (id, account_id, connection_id, provider)",
      "unique idx_jobs_communication_provider_object on jobs_communication_actions (account_id, connection_id, provider, provider_object_id) where provider_object_id is not null and provider_object_id <> ''",
    ].sort(),
  ],
  [
    "jobs_communication_action_attempts",
    [
      "idx_jobs_communication_attempts_action on jobs_communication_action_attempts (account_id, action_id, dispatch_no desc)",
    ],
  ],
  [
    "jobs_communication_action_attempt_evidence",
    [
      "idx_jobs_communication_attempt_evidence_action on jobs_communication_action_attempt_evidence (account_id, action_id, recorded_at_ms desc)",
    ],
  ],
  [
    "jobs_communication_action_reconciliations",
    [
      "idx_jobs_communication_reconciliations_action on jobs_communication_action_reconciliations (account_id, action_id, recorded_at_ms desc)",
    ],
  ],
  [
    "jobs_discovery_memberships",
    [
      "idx_jobs_discovery_memberships_job on jobs_discovery_memberships (account_id, job_id)",
    ],
  ],
  [
    "jobs_discovery_runs",
    [
      "idx_jobs_discovery_runs_source on jobs_discovery_runs (source_id, started_at_ms desc)",
    ],
  ],
  [
    "jobs_discovery_sources",
    [
      "idx_jobs_discovery_sources_due on jobs_discovery_sources (status, health, next_run_at_ms, lease_expires_at_ms)",
      "unique idx_jobs_discovery_sources_board_owner on jobs_discovery_sources (account_id, provider, source_key)",
    ].sort(),
  ],
  [
    "jobs_execution_leases",
    [
      "idx_jobs_execution_leases_binding on jobs_execution_leases (account_id, application_id, run_id)",
      "unique idx_jobs_execution_leases_active_application on jobs_execution_leases (application_id) where phase in ('prepared', 'click_started')",
      "unique idx_jobs_execution_leases_active_profile on jobs_execution_leases (browser_profile_id) where phase in ('prepared', 'click_started')",
    ].sort(),
  ],
  [
    "jobs_execution_lease_process_runtime_bindings",
    [
      "idx_jobs_execution_lease_process_runtime_grant on jobs_execution_lease_process_runtime_bindings (runtime_grant_id, run_id, fence)",
    ],
  ],
  [
    "jobs_execution_lease_volume_bindings",
    [
      "idx_jobs_execution_lease_volume_bindings_volume on jobs_execution_lease_volume_bindings (volume_id, volume_epoch, bound_at_ms)",
    ],
  ],
  [
    "jobs_local_run_resume_actions",
    [
      "idx_jobs_local_resume_actions_application on jobs_local_run_resume_actions (account_id, application_id, created_at_ms desc)",
      "unique idx_jobs_local_resume_actions_active_run on jobs_local_run_resume_actions (run_id) where status = 'approved'",
    ].sort(),
  ],
  [
    "jobs_local_run_claim_replays",
    [
      "idx_jobs_local_run_claim_replays_created on jobs_local_run_claim_replays (account_id, created_at_ms desc)",
    ],
  ],
  [
    "jobs_local_run_release_bindings",
    [
      "idx_jobs_local_run_release_bindings_account on jobs_local_run_release_bindings (account_id, application_id, bound_at_ms desc)",
    ],
  ],
  [
    "jobs_runner_legacy_inventory_authorities",
    [
      "idx_jobs_runner_legacy_inventory_authorities_reconciliation on jobs_runner_legacy_inventory_authorities (reconciliation_id, authority_generation, authority_state)",
    ],
  ],
  [
    "jobs_runner_purge_enforcements",
    [
      "idx_jobs_runner_purge_enforcements_volume_state on jobs_runner_purge_enforcements (volume_id, volume_epoch, state, updated_at_ms)",
    ],
  ],
  [
    "jobs_runner_purge_requests",
    [
      "idx_jobs_runner_purge_requests_account_state on jobs_runner_purge_requests (account_id, state, updated_at_ms)",
      "idx_jobs_runner_purge_requests_deletion_attempt on jobs_runner_purge_requests (deletion_request_id, purge_generation, updated_at_ms)",
    ].sort(),
  ],
  [
    "jobs_runner_purge_targets",
    [
      "idx_jobs_runner_purge_targets_request_state on jobs_runner_purge_targets (request_id, state, updated_at_ms)",
      "idx_jobs_runner_purge_targets_volume_state on jobs_runner_purge_targets (volume_id, volume_epoch, state, updated_at_ms)",
    ].sort(),
  ],
  [
    "jobs_runner_purge_tombstones",
    [
      "idx_jobs_runner_purge_tombstones_request on jobs_runner_purge_tombstones (request_id, completed_at_ms)",
    ],
  ],
  [
    "jobs_runner_process_runtime_bindings",
    [
      "idx_jobs_runner_process_runtime_bindings_process on jobs_runner_process_runtime_bindings (worker_id, volume_id, enrollment_epoch, process_instance_id)",
    ],
  ],
  [
    "jobs_runner_process_runtime_grants",
    [
      "idx_jobs_runner_process_runtime_grants_expiry on jobs_runner_process_runtime_grants (expires_at_ms, expected_worker_id)",
    ],
  ],
  [
    "jobs_runner_volume_admission_grants",
    [
      "idx_jobs_runner_volume_grants_expiry on jobs_runner_volume_admission_grants (expires_at_ms, consumed_at_ms)",
    ],
  ],
  [
    "jobs_runner_volume_authority_uses",
    [
      "idx_jobs_runner_volume_authority_uses_consumed on jobs_runner_volume_authority_uses (consumed_at_ms)",
    ],
  ],
  [
    "jobs_runner_volume_destructions",
    [
      "idx_jobs_runner_volume_destructions_volume on jobs_runner_volume_destructions (volume_id, volume_epoch, recorded_at_ms)",
    ],
  ],
  [
    "jobs_runner_volume_residencies",
    [
      "idx_jobs_runner_residencies_subject_state on jobs_runner_volume_residencies (purge_subject, state, volume_id, volume_epoch)",
      "idx_jobs_runner_residencies_volume_state on jobs_runner_volume_residencies (volume_id, volume_epoch, state, last_recorded_at_ms)",
    ].sort(),
  ],
  [
    "jobs_runner_volume_storage_attestations",
    [
      "idx_jobs_runner_volume_storage_attestations_latest on jobs_runner_volume_storage_attestations (volume_id, enrollment_epoch, attestation_generation desc)",
    ],
  ],
  [
    "jobs_runner_volumes",
    [
      "idx_jobs_runner_volumes_worker_status on jobs_runner_volumes (worker_id, status, updated_at_ms)",
    ],
  ],
  [
    "jobs_submission_evidence_capacity",
    [
      "idx_jobs_submission_evidence_capacity_account on jobs_submission_evidence_capacity (account_id, state, expires_at_ms)",
      "unique idx_jobs_submission_evidence_capacity_active_application on jobs_submission_evidence_capacity (account_id, application_id) where state = 'active'",
    ].sort(),
  ],
]);

function balancedBody(sql, openingParen) {
  let depth = 0;
  let quote = null;
  for (let index = openingParen; index < sql.length; index += 1) {
    const character = sql[index];
    if (quote) {
      if (character === quote && sql[index + 1] === quote) {
        index += 1;
      } else if (character === quote) {
        quote = null;
      }
      continue;
    }
    if (character === "'" || character === '"') {
      quote = character;
    } else if (character === "(") {
      depth += 1;
    } else if (character === ")") {
      depth -= 1;
      if (depth === 0) return sql.slice(openingParen + 1, index);
    }
  }
  throw new Error("Unbalanced CREATE TABLE statement");
}

function splitTopLevel(value) {
  const parts = [];
  let start = 0;
  let depth = 0;
  let quote = null;
  for (let index = 0; index < value.length; index += 1) {
    const character = value[index];
    if (quote) {
      if (character === quote && value[index + 1] === quote) {
        index += 1;
      } else if (character === quote) {
        quote = null;
      }
      continue;
    }
    if (character === "'" || character === '"') quote = character;
    else if (character === "(") depth += 1;
    else if (character === ")") depth -= 1;
    else if (character === "," && depth === 0) {
      parts.push(value.slice(start, index));
      start = index + 1;
    }
  }
  parts.push(value.slice(start));
  return parts.map((part) => part.trim()).filter(Boolean);
}

function normalizeSql(value) {
  return value
    .replace(/--[^\n]*/g, " ")
    .replace(
      /\bglob\s+'([^']*)'/gi,
      (_, pattern) => `LIKE '${pattern.replaceAll("*", "%")}'`,
    )
    .replace(/\bsmallint\b/gi, "INTEGER")
    .replace(/\bbigint\b/gi, "INTEGER")
    .replace(/\s+/g, " ")
    .replace(/\(\s+/g, "(")
    .replace(/\s+\)/g, ")")
    .trim()
    .toLowerCase();
}

function extractTable(sql, tableName) {
  const expression = new RegExp(
    `CREATE\\s+TABLE\\s+IF\\s+NOT\\s+EXISTS\\s+${tableName}\\s*\\(`,
    "i",
  );
  const match = expression.exec(sql);
  if (!match) return null;
  const openingParen = sql.indexOf("(", match.index);
  return splitTopLevel(balancedBody(sql, openingParen)).map(normalizeSql);
}

function extractIndexes(sql, tableName) {
  const indexes = [];
  const expression =
    /CREATE\s+(UNIQUE\s+)?INDEX\s+IF\s+NOT\s+EXISTS\s+([A-Za-z_][A-Za-z0-9_]*)\s+ON\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(([^;]+?)\)\s*(WHERE\s+[^;]+)?;/gi;
  for (const match of sql.matchAll(expression)) {
    if (match[3].toLowerCase() !== tableName.toLowerCase()) continue;
    indexes.push(
      normalizeSql(
        `${match[1] ? "unique " : ""}${match[2]} on ${match[3]} (${match[4]}) ${match[5] ?? ""}`,
      ),
    );
  }
  return [...new Set(indexes)].sort();
}

function parityTableNames(sql) {
  const expectedNames = new Set(JOBS_PARITY_TABLES);
  const names = [
    ...sql.matchAll(
      /CREATE\s+TABLE\s+IF\s+NOT\s+EXISTS\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(/gi,
    ),
  ]
    .map((match) => match[1].toLowerCase())
    .filter(
      (tableName) =>
        expectedNames.has(tableName) || tableName.startsWith("jobs_discovery_"),
    );
  return [...new Set(names)].sort();
}

function firstDifference(left, right) {
  const length = Math.max(left.length, right.length);
  for (let index = 0; index < length; index += 1) {
    if (left[index] !== right[index])
      return { index, left: left[index], right: right[index] };
  }
  return null;
}

export function compareJobsSchemas(sqliteSource, postgresSource) {
  const issues = [];
  const expectedNames = [...JOBS_PARITY_TABLES].sort();
  const sqliteNames = parityTableNames(sqliteSource);
  const postgresNames = parityTableNames(postgresSource);

  if (JSON.stringify(sqliteNames) !== JSON.stringify(expectedNames)) {
    issues.push(
      `SQLite parity tables: expected ${expectedNames.join(", ")}; found ${sqliteNames.join(", ") || "none"}`,
    );
  }
  if (JSON.stringify(postgresNames) !== JSON.stringify(expectedNames)) {
    issues.push(
      `Postgres parity tables: expected ${expectedNames.join(", ")}; found ${postgresNames.join(", ") || "none"}`,
    );
  }

  for (const tableName of JOBS_PARITY_TABLES) {
    const sqliteTable = extractTable(sqliteSource, tableName);
    const postgresTable = extractTable(postgresSource, tableName);
    if (!sqliteTable || !postgresTable) continue;

    const tableDifference = firstDifference(sqliteTable, postgresTable);
    if (tableDifference) {
      issues.push(
        `${tableName} definition ${tableDifference.index + 1} differs: SQLite=${JSON.stringify(tableDifference.left ?? "<missing>")} Postgres=${JSON.stringify(tableDifference.right ?? "<missing>")}`,
      );
    }

    const sqliteIndexes = extractIndexes(sqliteSource, tableName);
    const postgresIndexes = extractIndexes(postgresSource, tableName);
    const requiredIndexes = REQUIRED_INDEX_SIGNATURES.get(tableName) ?? [];
    for (const [dialect, indexes] of [
      ["SQLite", sqliteIndexes],
      ["Postgres", postgresIndexes],
    ]) {
      const requiredDifference = firstDifference(requiredIndexes, indexes);
      if (requiredDifference) {
        issues.push(
          `${dialect} ${tableName} required index ${requiredDifference.index + 1} differs: expected=${JSON.stringify(requiredDifference.left ?? "<missing>")} found=${JSON.stringify(requiredDifference.right ?? "<missing>")}`,
        );
      }
    }
    const indexDifference = firstDifference(sqliteIndexes, postgresIndexes);
    if (indexDifference) {
      issues.push(
        `${tableName} index ${indexDifference.index + 1} differs: SQLite=${JSON.stringify(indexDifference.left ?? "<missing>")} Postgres=${JSON.stringify(indexDifference.right ?? "<missing>")}`,
      );
    }
  }

  return issues;
}

function main() {
  const repoRoot = path.resolve(
    path.dirname(fileURLToPath(import.meta.url)),
    "../..",
  );
  const sqlitePath = path.join(repoRoot, "server/src/db/mod.rs");
  const sqliteCommunicationPath = path.join(
    repoRoot,
    "infra/sqlite/server-runtime/041_jobs_communication_actions.sql",
  );
  const sqliteDeletionIntentPath = path.join(
    repoRoot,
    "infra/sqlite/server-runtime/043_account_deletion_intents.sql",
  );
  const sqliteEvidenceCapacityPath = path.join(
    repoRoot,
    "infra/sqlite/server-runtime/044_jobs_submission_evidence_reservations.sql",
  );
  const postgresPath = path.join(
    repoRoot,
    "infra/postgres/server-runtime/002_jobs.sql",
  );
  const postgresCommunicationPath = path.join(
    repoRoot,
    "infra/postgres/server-runtime/019_jobs_communication_actions.sql",
  );
  const postgresDeletionIntentPath = path.join(
    repoRoot,
    "infra/postgres/server-runtime/021_account_deletion_intents.sql",
  );
  const postgresEvidenceCapacityPath = path.join(
    repoRoot,
    "infra/postgres/server-runtime/022_jobs_submission_evidence_reservations.sql",
  );
  const sqliteRunnerPurgePath = path.join(
    repoRoot,
    "infra/sqlite/server-runtime/046_jobs_runner_volume_purge.sql",
  );
  const postgresRunnerPurgePath = path.join(
    repoRoot,
    "infra/postgres/server-runtime/024_jobs_runner_volume_purge.sql",
  );
  const sqliteBrowserReleaseAuthorityPath = path.join(
    repoRoot,
    "infra/sqlite/server-runtime/047_jobs_browser_release_authority.sql",
  );
  const postgresBrowserReleaseAuthorityPath = path.join(
    repoRoot,
    "infra/postgres/server-runtime/025_jobs_browser_release_authority.sql",
  );
  const sqliteAtsCertificationAuthorityPath = path.join(
    repoRoot,
    "infra/sqlite/server-runtime/048_jobs_ats_certification_authority.sql",
  );
  const postgresAtsCertificationAuthorityPath = path.join(
    repoRoot,
    "infra/postgres/server-runtime/026_jobs_ats_certification_authority.sql",
  );
  const sqliteBrowserRuntimeComponentsPath = path.join(
    repoRoot,
    "infra/sqlite/server-runtime/049_jobs_browser_release_runtime_components.sql",
  );
  const postgresBrowserRuntimeComponentsPath = path.join(
    repoRoot,
    "infra/postgres/server-runtime/027_jobs_browser_release_runtime_components.sql",
  );
  const sqliteRunnerProcessRuntimePath = path.join(
    repoRoot,
    "infra/sqlite/server-runtime/050_jobs_runner_process_runtime_authority.sql",
  );
  const postgresRunnerProcessRuntimePath = path.join(
    repoRoot,
    "infra/postgres/server-runtime/028_jobs_runner_process_runtime_authority.sql",
  );
  const sqliteCommunicationExecutionPath = path.join(
    repoRoot,
    "infra/sqlite/server-runtime/051_jobs_communication_execution.sql",
  );
  const postgresCommunicationExecutionPath = path.join(
    repoRoot,
    "infra/postgres/server-runtime/029_jobs_communication_execution.sql",
  );
  const sqliteSource = [
    sqlitePath,
    sqliteCommunicationPath,
    sqliteDeletionIntentPath,
    sqliteEvidenceCapacityPath,
    sqliteRunnerPurgePath,
    sqliteBrowserReleaseAuthorityPath,
    sqliteAtsCertificationAuthorityPath,
    sqliteBrowserRuntimeComponentsPath,
    sqliteRunnerProcessRuntimePath,
    sqliteCommunicationExecutionPath,
  ]
    .map((sourcePath) => fs.readFileSync(sourcePath, "utf8"))
    .join("\n");
  const postgresSource = [
    postgresPath,
    postgresCommunicationPath,
    postgresDeletionIntentPath,
    postgresEvidenceCapacityPath,
    postgresRunnerPurgePath,
    postgresBrowserReleaseAuthorityPath,
    postgresAtsCertificationAuthorityPath,
    postgresBrowserRuntimeComponentsPath,
    postgresRunnerProcessRuntimePath,
    postgresCommunicationExecutionPath,
  ]
    .map((sourcePath) => fs.readFileSync(sourcePath, "utf8"))
    .join("\n");
  const issues = compareJobsSchemas(sqliteSource, postgresSource);

  const includePath =
    'include_str!("../../../infra/postgres/server-runtime/002_jobs.sql")';
  if (!sqliteSource.includes(includePath))
    issues.push(
      `server migration runner does not include 002_jobs.sql via ${includePath}`,
    );
  if (!sqliteSource.includes('&[&"002_jobs.sql"]'))
    issues.push("server migration runner does not record 002_jobs.sql");
  const communicationInclude =
    'include_str!("../../../infra/postgres/server-runtime/019_jobs_communication_actions.sql")';
  if (!sqliteSource.includes(communicationInclude)) {
    issues.push(
      `server migration runner does not include 019_jobs_communication_actions.sql via ${communicationInclude}`,
    );
  }
  if (!sqliteSource.includes('"019_jobs_communication_actions.sql"')) {
    issues.push(
      "server migration runner does not record 019_jobs_communication_actions.sql",
    );
  }
  const runnerPurgeInclude =
    'include_str!("../../../infra/postgres/server-runtime/024_jobs_runner_volume_purge.sql")';
  if (!sqliteSource.includes(runnerPurgeInclude)) {
    issues.push(
      `server migration runner does not include 024_jobs_runner_volume_purge.sql via ${runnerPurgeInclude}`,
    );
  }
  if (!sqliteSource.includes('"024_jobs_runner_volume_purge.sql"')) {
    issues.push(
      "server migration runner does not record 024_jobs_runner_volume_purge.sql",
    );
  }
  const sqliteBrowserReleaseAuthorityInclude =
    'include_str!("../../../infra/sqlite/server-runtime/047_jobs_browser_release_authority.sql")';
  if (!sqliteSource.includes(sqliteBrowserReleaseAuthorityInclude)) {
    issues.push(
      `server migration runner does not include 047_jobs_browser_release_authority.sql via ${sqliteBrowserReleaseAuthorityInclude}`,
    );
  }
  if (
    (sqliteSource.match(/SQLITE_JOBS_BROWSER_RELEASE_AUTHORITY/g) ?? [])
      .length < 2
  ) {
    issues.push(
      "server SQLite migration runner does not register 047_jobs_browser_release_authority.sql",
    );
  }
  const browserReleaseAuthorityInclude =
    'include_str!("../../../infra/postgres/server-runtime/025_jobs_browser_release_authority.sql")';
  if (!sqliteSource.includes(browserReleaseAuthorityInclude)) {
    issues.push(
      `server migration runner does not include 025_jobs_browser_release_authority.sql via ${browserReleaseAuthorityInclude}`,
    );
  }
  if (!sqliteSource.includes('"025_jobs_browser_release_authority.sql"')) {
    issues.push(
      "server migration runner does not record 025_jobs_browser_release_authority.sql",
    );
  }
  for (const [dialect, migration, symbol] of [
    [
      "SQLite",
      "048_jobs_ats_certification_authority.sql",
      "SQLITE_JOBS_ATS_CERTIFICATION_AUTHORITY",
    ],
    [
      "SQLite",
      "049_jobs_browser_release_runtime_components.sql",
      "SQLITE_JOBS_BROWSER_RELEASE_RUNTIME_COMPONENTS",
    ],
    [
      "SQLite",
      "050_jobs_runner_process_runtime_authority.sql",
      "SQLITE_JOBS_RUNNER_PROCESS_RUNTIME_AUTHORITY",
    ],
    [
      "SQLite",
      "051_jobs_communication_execution.sql",
      "SQLITE_JOBS_COMMUNICATION_EXECUTION",
    ],
  ]) {
    const migrationPath = `infra/${dialect.toLowerCase()}/server-runtime/${migration}`;
    if (!sqliteSource.includes(migrationPath)) {
      issues.push(
        `server ${dialect} migration runner does not include ${migration} via ${migrationPath}`,
      );
    }
    if ((sqliteSource.match(new RegExp(symbol, "g")) ?? []).length < 2) {
      issues.push(
        `server ${dialect} migration runner does not register ${migration}`,
      );
    }
  }
  for (const migration of [
    "026_jobs_ats_certification_authority.sql",
    "027_jobs_browser_release_runtime_components.sql",
    "028_jobs_runner_process_runtime_authority.sql",
    "029_jobs_communication_execution.sql",
  ]) {
    const migrationPath = `infra/postgres/server-runtime/${migration}`;
    if (!sqliteSource.includes(migrationPath)) {
      issues.push(
        `server Postgres migration runner does not include ${migration} via ${migrationPath}`,
      );
    }
    if (!sqliteSource.includes(`"${migration}"`)) {
      issues.push(
        `server Postgres migration runner does not record ${migration}`,
      );
    }
  }

  if (issues.length > 0) {
    console.error("Jobs SQLite/Postgres schema parity failed:");
    for (const issue of issues.sort()) console.error(`- ${issue}`);
    process.exitCode = 1;
    return;
  }
  const indexCount = JOBS_PARITY_TABLES.reduce(
    (count, tableName) =>
      count + extractIndexes(sqliteSource, tableName).length,
    0,
  );
  console.log(
    `Jobs SQLite/Postgres schema parity passed (${JOBS_PARITY_TABLES.length} tables, ${indexCount} indexes).`,
  );
}

if (
  process.argv[1] &&
  path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)
)
  main();
