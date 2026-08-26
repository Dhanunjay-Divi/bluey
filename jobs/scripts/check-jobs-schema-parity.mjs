import fs from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

export const PHASE_613_PARITY_TABLES = [
  "jobs_track_policy_account_input_heads",
  "jobs_track_policy_account_input_transitions",
  "jobs_track_policy_head_transitions",
  "jobs_track_policy_heads",
  "jobs_track_policy_revisions",
  "jobs_track_policy_review_receipts",
  "jobs_track_policy_taxonomy_activation_events",
  "jobs_track_policy_taxonomy_activation_head",
  "jobs_track_policy_track_input_heads",
  "jobs_track_policy_track_input_transitions",
];

const PHASE_613_SEMANTIC_TABLES = new Set(PHASE_613_PARITY_TABLES);

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
  "jobs_operational_hold_events",
  "jobs_operational_hold_heads",
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
  ...PHASE_613_PARITY_TABLES,
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
    "jobs_operational_hold_events",
    [
      "idx_jobs_operational_hold_events_history on jobs_operational_hold_events (capability, scope_kind, scope_id, revision_no desc)",
    ],
  ],
  [
    "jobs_operational_hold_heads",
    [
      "idx_jobs_operational_hold_heads_lookup on jobs_operational_hold_heads (state, capability, scope_kind, scope_id)",
      "idx_jobs_operational_hold_heads_refs on jobs_operational_hold_heads (capability, scope_kind, scope_ref, head_revision)",
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
  [
    "jobs_track_policy_revisions",
    [
      "idx_jobs_track_policy_revisions_authority on jobs_track_policy_revisions (account_id, verified_application_identity_id, source_resume_asset_id, review_state)",
      "idx_jobs_track_policy_revisions_history on jobs_track_policy_revisions (account_id, career_track_id, revision_no desc)",
    ].sort(),
  ],
  [
    "jobs_track_policy_review_receipts",
    [
      "idx_jobs_track_policy_review_receipts_revision on jobs_track_policy_review_receipts (account_id, career_track_id, policy_revision_no, decision, decided_at_ms desc)",
    ],
  ],
  [
    "jobs_track_policy_heads",
    [
      "idx_jobs_track_policy_heads_revision on jobs_track_policy_heads (account_id, policy_revision_id, policy_revision_no, head_generation)",
    ],
  ],
  [
    "jobs_track_policy_account_input_transitions",
    [
      "idx_jobs_track_policy_account_input_transitions_history on jobs_track_policy_account_input_transitions (account_id, input_generation desc)",
    ],
  ],
  [
    "jobs_track_policy_track_input_transitions",
    [
      "idx_jobs_track_policy_track_input_transitions_history on jobs_track_policy_track_input_transitions (account_id, career_track_id, input_generation desc)",
    ],
  ],
  [
    "jobs_track_policy_head_transitions",
    [
      "idx_jobs_track_policy_head_transitions_history on jobs_track_policy_head_transitions (account_id, career_track_id, head_generation desc)",
    ],
  ],
]);

const PHASE_613_INDEX_NAMES = [
  "idx_jobs_tracks_account_id_unique",
  "idx_jobs_track_policy_account_input_transitions_history",
  "idx_jobs_track_policy_head_transitions_history",
  "idx_jobs_track_policy_heads_revision",
  "idx_jobs_track_policy_revisions_authority",
  "idx_jobs_track_policy_revisions_history",
  "idx_jobs_track_policy_review_receipts_revision",
  "idx_jobs_track_policy_track_input_transitions_history",
];

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

function extractColumnShapes(tableDefinition) {
  if (!tableDefinition) return [];
  return tableDefinition
    .filter(
      (part) =>
        !/^(?:constraint|primary\s+key|unique|foreign\s+key|check)\b/i.test(
          part,
        ),
    )
    .map((part) => {
      const match = part.match(/^([a-z_][a-z0-9_]*)\s+([a-z]+)\b/i);
      if (!match) return "";
      const flags = [];
      if (/\bnot null\b/i.test(part)) flags.push("not null");
      if (/\bprimary key\b/i.test(part)) flags.push("primary key");
      if (/\bunique\b/i.test(part)) flags.push("unique");
      return [match[1], match[2], ...flags].join(" ");
    })
    .filter(Boolean);
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

function requireExactIndexes(issues, dialect, sql, tableName, requiredIndexes) {
  const indexes = extractIndexes(sql, tableName);
  const requiredDifference = firstDifference(requiredIndexes, indexes);
  if (requiredDifference) {
    issues.push(
      `${dialect} ${tableName} required index ${requiredDifference.index + 1} differs: expected=${JSON.stringify(requiredDifference.left ?? "<missing>")} found=${JSON.stringify(requiredDifference.right ?? "<missing>")}`,
    );
  }
  return indexes;
}

function requireIndexesPresent(
  issues,
  dialect,
  sql,
  tableName,
  requiredIndexes,
) {
  const indexes = extractIndexes(sql, tableName);
  for (const requiredIndex of requiredIndexes) {
    if (!indexes.includes(requiredIndex)) {
      issues.push(
        `${dialect} ${tableName} required index is missing: expected=${JSON.stringify(requiredIndex)} found=${JSON.stringify(indexes)}`,
      );
    }
  }
}

function requireSinglePhase613IndexDeclarations(issues, dialect, sql) {
  for (const indexName of PHASE_613_INDEX_NAMES) {
    const expression = new RegExp(
      `CREATE\\s+(?:UNIQUE\\s+)?INDEX\\s+IF\\s+NOT\\s+EXISTS\\s+${indexName}\\b`,
      "gi",
    );
    const count = [...sql.matchAll(expression)].length;
    if (count !== 1) {
      issues.push(
        `${dialect} Phase 613 index ${indexName} must be declared exactly once; found ${count}`,
      );
    }
  }
}

function requireSinglePhase613TableDeclarations(issues, dialect, sql) {
  for (const tableName of PHASE_613_PARITY_TABLES) {
    const expression = new RegExp(
      `CREATE\\s+TABLE\\s+IF\\s+NOT\\s+EXISTS\\s+${tableName}\\s*\\(`,
      "gi",
    );
    const count = [...sql.matchAll(expression)].length;
    if (count !== 1) {
      issues.push(
        `${dialect} Phase 613 table ${tableName} must be declared exactly once; found ${count}`,
      );
    }
  }
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

function extractSingleSqlObject(sql, expression) {
  const matches = [...sql.matchAll(expression)];
  if (matches.length !== 1) return null;
  return normalizeSql(matches[0][0]);
}

function extractSqliteTrigger(sql, triggerName) {
  return extractSingleSqlObject(
    sql,
    new RegExp(
      `CREATE\\s+TRIGGER\\s+(?:IF\\s+NOT\\s+EXISTS\\s+)?${triggerName}\\b[\\s\\S]*?\\bEND\\s*;`,
      "gi",
    ),
  );
}

function extractPostgresFunction(sql, functionName) {
  return extractSingleSqlObject(
    sql,
    new RegExp(
      `CREATE\\s+OR\\s+REPLACE\\s+FUNCTION\\s+${functionName}\\s*\\([^)]*\\)` +
        `\\s*RETURNS\\s+trigger[\\s\\S]*?\\$\\$\\s*;`,
      "gi",
    ),
  );
}

function extractPostgresTrigger(sql, triggerName) {
  return extractSingleSqlObject(
    sql,
    new RegExp(`CREATE\\s+TRIGGER\\s+${triggerName}\\b[\\s\\S]*?;`, "gi"),
  );
}

function requireOperationalHoldFragments(
  issues,
  dialect,
  objectName,
  objectSource,
  requirements,
) {
  if (!objectSource) {
    issues.push(
      `${dialect} operational-hold object ${objectName} must exist exactly once`,
    );
    return;
  }
  for (const [invariant, fragment] of requirements) {
    if (!objectSource.includes(normalizeSql(fragment))) {
      issues.push(
        `${dialect} operational-hold invariant ${invariant} is missing from ${objectName}`,
      );
    }
  }
}

function operationalHoldTableRequirements(issues, dialect, sql) {
  const eventTable = extractTable(sql, "jobs_operational_hold_events");
  const headTable = extractTable(sql, "jobs_operational_hold_heads");
  requireOperationalHoldFragments(
    issues,
    dialect,
    "jobs_operational_hold_events",
    eventTable ? normalizeSql(eventTable.join(", ")) : null,
    [
      [
        "first event is held",
        "revision_no = 1 AND previous_revision_no IS NULL " +
          "AND predecessor_event_id IS NULL AND transition = 'held'",
      ],
      [
        "previous revision is exactly n-1",
        "revision_no > 1 AND previous_revision_no = revision_no - 1 " +
          "AND predecessor_event_id IS NOT NULL",
      ],
      [
        "ancestry revision/event uniqueness",
        "UNIQUE(capability, scope_kind, scope_id, revision_no, event_id)",
      ],
      [
        "head revision/event/ref uniqueness",
        "UNIQUE(capability, scope_kind, scope_id, revision_no, event_id, event_ref)",
      ],
      [
        "ancestry foreign key",
        "FOREIGN KEY(capability, scope_kind, scope_id, previous_revision_no, " +
          "predecessor_event_id) REFERENCES jobs_operational_hold_events(" +
          "capability, scope_kind, scope_id, revision_no, event_id) ON DELETE RESTRICT",
      ],
    ],
  );
  requireOperationalHoldFragments(
    issues,
    dialect,
    "jobs_operational_hold_heads",
    headTable ? normalizeSql(headTable.join(", ")) : null,
    [
      ["head scope authority", "PRIMARY KEY(capability, scope_kind, scope_id)"],
      [
        "head foreign key",
        "FOREIGN KEY(capability, scope_kind, scope_id, head_revision, " +
          "current_event_id, current_event_ref) REFERENCES jobs_operational_hold_events(" +
          "capability, scope_kind, scope_id, revision_no, event_id, event_ref) " +
          "ON DELETE RESTRICT",
      ],
    ],
  );
}

const OPERATIONAL_HOLD_EVENT_VALIDATION_REQUIREMENTS = [
  ["predecessor capability link", "predecessor.capability = NEW.capability"],
  ["predecessor scope-kind link", "predecessor.scope_kind = NEW.scope_kind"],
  ["predecessor scope-id link", "predecessor.scope_id = NEW.scope_id"],
  [
    "predecessor revision link",
    "predecessor.revision_no = NEW.previous_revision_no",
  ],
  ["predecessor event link", "predecessor.event_id = NEW.predecessor_event_id"],
  [
    "predecessor time ordering",
    "predecessor.recorded_at_ms <= NEW.recorded_at_ms",
  ],
  [
    "released-to-released rejection",
    "NOT (NEW.transition = 'released' AND predecessor.transition = 'released')",
  ],
];

const OPERATIONAL_HOLD_HEAD_INSERT_REQUIREMENTS = [
  ["head insert revision is one", "NEW.head_revision <> 1"],
  ["head insert capability link", "event.capability = NEW.capability"],
  ["head insert scope-kind link", "event.scope_kind = NEW.scope_kind"],
  ["head insert scope-id link", "event.scope_id = NEW.scope_id"],
  ["head insert revision link", "event.revision_no = NEW.head_revision"],
  ["head insert event-id link", "event.event_id = NEW.current_event_id"],
  ["head insert event-ref link", "event.event_ref = NEW.current_event_ref"],
  ["head insert transition link", "event.transition = NEW.state"],
  ["head insert actor link", "event.recorded_by = NEW.updated_by"],
  ["head insert time link", "event.recorded_at_ms = NEW.updated_at_ms"],
];

const OPERATIONAL_HOLD_HEAD_UPDATE_REQUIREMENTS = [
  ["head capability is immutable", "NEW.capability <> OLD.capability"],
  ["head scope kind is immutable", "NEW.scope_kind <> OLD.scope_kind"],
  ["head scope id is immutable", "NEW.scope_id <> OLD.scope_id"],
  ["head scope-ref is immutable", "NEW.scope_ref <> OLD.scope_ref"],
  [
    "head update advances exactly one revision",
    "NEW.head_revision <> OLD.head_revision + 1",
  ],
  ["head event-id advances", "NEW.current_event_id = OLD.current_event_id"],
  ["head event-ref advances", "NEW.current_event_ref = OLD.current_event_ref"],
  ["head update time is monotonic", "NEW.updated_at_ms < OLD.updated_at_ms"],
  ["head update capability link", "event.capability = NEW.capability"],
  ["head update scope-kind link", "event.scope_kind = NEW.scope_kind"],
  ["head update scope-id link", "event.scope_id = NEW.scope_id"],
  ["head update revision link", "event.revision_no = NEW.head_revision"],
  [
    "head update previous-revision link",
    "event.previous_revision_no = OLD.head_revision",
  ],
  [
    "head update predecessor-event link",
    "event.predecessor_event_id = OLD.current_event_id",
  ],
  ["head update event-id link", "event.event_id = NEW.current_event_id"],
  ["head update event-ref link", "event.event_ref = NEW.current_event_ref"],
  ["head update transition link", "event.transition = NEW.state"],
  ["head update actor link", "event.recorded_by = NEW.updated_by"],
  ["head update time link", "event.recorded_at_ms = NEW.updated_at_ms"],
];

function operationalHoldSqliteRequirements(issues, sql) {
  const dialect = "SQLite";
  operationalHoldTableRequirements(issues, dialect, sql);
  requireOperationalHoldFragments(
    issues,
    dialect,
    "trg_jobs_operational_hold_events_validate_insert",
    extractSqliteTrigger(
      sql,
      "trg_jobs_operational_hold_events_validate_insert",
    ),
    [
      [
        "event validation trigger binding",
        "BEFORE INSERT ON jobs_operational_hold_events",
      ],
      ...OPERATIONAL_HOLD_EVENT_VALIDATION_REQUIREMENTS,
    ],
  );
  for (const [triggerName, operation] of [
    ["trg_jobs_operational_hold_events_no_update", "UPDATE"],
    ["trg_jobs_operational_hold_events_no_delete", "DELETE"],
  ]) {
    requireOperationalHoldFragments(
      issues,
      dialect,
      triggerName,
      extractSqliteTrigger(sql, triggerName),
      [
        [
          `event ${operation.toLowerCase()} immutability`,
          `BEFORE ${operation} ON jobs_operational_hold_events`,
        ],
        [
          `event ${operation.toLowerCase()} rejection`,
          "RAISE(ABORT, 'operational hold event is immutable')",
        ],
      ],
    );
  }
  requireOperationalHoldFragments(
    issues,
    dialect,
    "trg_jobs_operational_hold_heads_validate_insert",
    extractSqliteTrigger(
      sql,
      "trg_jobs_operational_hold_heads_validate_insert",
    ),
    [
      [
        "head insert trigger binding",
        "BEFORE INSERT ON jobs_operational_hold_heads",
      ],
      ...OPERATIONAL_HOLD_HEAD_INSERT_REQUIREMENTS,
    ],
  );
  requireOperationalHoldFragments(
    issues,
    dialect,
    "trg_jobs_operational_hold_heads_monotonic",
    extractSqliteTrigger(sql, "trg_jobs_operational_hold_heads_monotonic"),
    [
      [
        "head update trigger binding",
        "BEFORE UPDATE ON jobs_operational_hold_heads",
      ],
      ...OPERATIONAL_HOLD_HEAD_UPDATE_REQUIREMENTS,
    ],
  );
  requireOperationalHoldFragments(
    issues,
    dialect,
    "trg_jobs_operational_hold_heads_no_delete",
    extractSqliteTrigger(sql, "trg_jobs_operational_hold_heads_no_delete"),
    [
      [
        "head delete immutability",
        "BEFORE DELETE ON jobs_operational_hold_heads",
      ],
      [
        "head delete rejection",
        "RAISE(ABORT, 'operational hold head cannot be deleted')",
      ],
    ],
  );
}

function operationalHoldPostgresRequirements(issues, sql) {
  const dialect = "Postgres";
  operationalHoldTableRequirements(issues, dialect, sql);
  requireOperationalHoldFragments(
    issues,
    dialect,
    "validate_jobs_operational_hold_event",
    extractPostgresFunction(sql, "validate_jobs_operational_hold_event"),
    OPERATIONAL_HOLD_EVENT_VALIDATION_REQUIREMENTS,
  );
  requireOperationalHoldFragments(
    issues,
    dialect,
    "validate_jobs_operational_hold_head_insert",
    extractPostgresFunction(sql, "validate_jobs_operational_hold_head_insert"),
    OPERATIONAL_HOLD_HEAD_INSERT_REQUIREMENTS,
  );
  requireOperationalHoldFragments(
    issues,
    dialect,
    "enforce_jobs_operational_hold_head_monotonic",
    extractPostgresFunction(
      sql,
      "enforce_jobs_operational_hold_head_monotonic",
    ),
    OPERATIONAL_HOLD_HEAD_UPDATE_REQUIREMENTS,
  );
  requireOperationalHoldFragments(
    issues,
    dialect,
    "reject_jobs_operational_hold_event_mutation",
    extractPostgresFunction(sql, "reject_jobs_operational_hold_event_mutation"),
    [
      [
        "event mutation rejection",
        "RAISE EXCEPTION 'operational hold event is immutable'",
      ],
    ],
  );
  requireOperationalHoldFragments(
    issues,
    dialect,
    "reject_jobs_operational_hold_head_delete",
    extractPostgresFunction(sql, "reject_jobs_operational_hold_head_delete"),
    [
      [
        "head delete rejection",
        "RAISE EXCEPTION 'operational hold head cannot be deleted'",
      ],
    ],
  );

  for (const [triggerName, requirements] of [
    [
      "trg_jobs_operational_hold_events_validate_insert",
      [
        [
          "event validation trigger binding",
          "BEFORE INSERT ON jobs_operational_hold_events",
        ],
        [
          "event validation function binding",
          "EXECUTE FUNCTION validate_jobs_operational_hold_event()",
        ],
      ],
    ],
    [
      "trg_jobs_operational_hold_events_no_update",
      [
        [
          "event update immutability",
          "BEFORE UPDATE ON jobs_operational_hold_events",
        ],
        [
          "event update rejection binding",
          "EXECUTE FUNCTION reject_jobs_operational_hold_event_mutation()",
        ],
      ],
    ],
    [
      "trg_jobs_operational_hold_events_no_delete",
      [
        [
          "event delete immutability",
          "BEFORE DELETE ON jobs_operational_hold_events",
        ],
        [
          "event delete rejection binding",
          "EXECUTE FUNCTION reject_jobs_operational_hold_event_mutation()",
        ],
      ],
    ],
    [
      "trg_jobs_operational_hold_heads_validate_insert",
      [
        [
          "head insert trigger binding",
          "BEFORE INSERT ON jobs_operational_hold_heads",
        ],
        [
          "head insert function binding",
          "EXECUTE FUNCTION validate_jobs_operational_hold_head_insert()",
        ],
      ],
    ],
    [
      "trg_jobs_operational_hold_heads_monotonic",
      [
        [
          "head update trigger binding",
          "BEFORE UPDATE ON jobs_operational_hold_heads",
        ],
        [
          "head update function binding",
          "EXECUTE FUNCTION enforce_jobs_operational_hold_head_monotonic()",
        ],
      ],
    ],
    [
      "trg_jobs_operational_hold_heads_no_delete",
      [
        [
          "head delete immutability",
          "BEFORE DELETE ON jobs_operational_hold_heads",
        ],
        [
          "head delete rejection binding",
          "EXECUTE FUNCTION reject_jobs_operational_hold_head_delete()",
        ],
      ],
    ],
  ]) {
    requireOperationalHoldFragments(
      issues,
      dialect,
      triggerName,
      extractPostgresTrigger(sql, triggerName),
      [["trigger row binding", "FOR EACH ROW"], ...requirements],
    );
  }
}

function requirePhase613Fragments(
  issues,
  dialect,
  objectName,
  objectSource,
  requirements,
) {
  if (!objectSource) {
    issues.push(
      `${dialect} Phase 613 object ${objectName} must exist exactly once`,
    );
    return;
  }
  for (const [invariant, fragment] of requirements) {
    if (!objectSource.includes(normalizeSql(fragment))) {
      issues.push(
        `${dialect} Phase 613 invariant ${invariant} is missing from ${objectName}`,
      );
    }
  }
}

function phase613TableRequirements(issues, dialect, sql) {
  const requireTable = (tableName, requirements) => {
    const table = extractTable(sql, tableName);
    requirePhase613Fragments(
      issues,
      dialect,
      tableName,
      table ? normalizeSql(table.join(", ")) : null,
      requirements,
    );
  };

  requireTable("jobs_track_policy_taxonomy_activation_events", [
    [
      "taxonomy activation epoch identity",
      "activation_epoch INTEGER PRIMARY KEY",
    ],
    [
      "taxonomy activation epoch safe-integer range",
      "CHECK(activation_epoch BETWEEN 1 AND 9007199254740991)",
    ],
    [
      "taxonomy previous activation epoch safe-integer range",
      "CHECK(previous_activation_epoch BETWEEN 0 AND 9007199254740991)",
    ],
    [
      "taxonomy version bounded length",
      "CHECK(length(taxonomy_version) BETWEEN 1 AND 64)",
    ],
    [
      "taxonomy canonicalizer version safe-integer range",
      "CHECK(canonicalizer_schema_version BETWEEN 1 AND 9007199254740991)",
    ],
    [
      "taxonomy activation timestamp safe-integer range",
      "CHECK(activated_at_ms BETWEEN 0 AND 9007199254740991)",
    ],
    [
      "taxonomy activation transition identity",
      "UNIQUE(activation_epoch, activation_transition_sha256)",
    ],
    [
      "taxonomy activation predecessor binding",
      "FOREIGN KEY(previous_activation_epoch, " +
        "predecessor_activation_transition_sha256) REFERENCES " +
        "jobs_track_policy_taxonomy_activation_events(activation_epoch, " +
        "activation_transition_sha256)",
    ],
    [
      "initial taxonomy activation has no predecessor",
      "activation_epoch = 1 AND previous_activation_epoch = 0 " +
        "AND predecessor_activation_transition_sha256 IS NULL",
    ],
    [
      "taxonomy activation advances exactly one epoch",
      "activation_epoch > 1 " +
        "AND previous_activation_epoch = activation_epoch - 1 " +
        "AND predecessor_activation_transition_sha256 IS NOT NULL",
    ],
  ]);
  requireTable("jobs_track_policy_taxonomy_activation_head", [
    ["taxonomy activation singleton", "PRIMARY KEY CHECK(singleton_id = 1)"],
    [
      "taxonomy activation head exact event identity",
      "UNIQUE(activation_epoch, activation_transition_sha256)",
    ],
    [
      "taxonomy activation head event binding",
      "FOREIGN KEY(activation_epoch, activation_transition_sha256) " +
        "REFERENCES jobs_track_policy_taxonomy_activation_events(" +
        "activation_epoch, activation_transition_sha256)",
    ],
  ]);
  requireTable("jobs_track_policy_account_input_transitions", [
    [
      "account input generation safe-integer range",
      "CHECK(input_generation BETWEEN 1 AND 9007199254740991)",
    ],
    [
      "account previous input generation safe-integer range",
      "CHECK(previous_input_generation BETWEEN 0 AND 9007199254740991)",
    ],
    [
      "account input timestamp safe-integer range",
      "CHECK(changed_at_ms BETWEEN 0 AND 9007199254740991)",
    ],
    [
      "account input generation uniqueness",
      "UNIQUE(account_id, input_generation)",
    ],
    [
      "account input transition exact identity",
      "UNIQUE(account_id, input_generation, input_transition_id, " +
        "input_transition_sha256)",
    ],
    [
      "account input tenant cascade binding",
      "FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE",
    ],
    [
      "account input predecessor tenant cascade binding",
      "FOREIGN KEY(account_id, previous_input_generation, " +
        "predecessor_input_transition_sha256) REFERENCES " +
        "jobs_track_policy_account_input_transitions(account_id, " +
        "input_generation, input_transition_sha256) ON DELETE CASCADE",
    ],
    [
      "initial account input has no predecessor",
      "input_generation = 1 AND previous_input_generation = 0 " +
        "AND predecessor_input_transition_sha256 IS NULL",
    ],
    [
      "account input advances exactly one generation",
      "input_generation > 1 " +
        "AND previous_input_generation = input_generation - 1 " +
        "AND predecessor_input_transition_sha256 IS NOT NULL",
    ],
  ]);
  requireTable("jobs_track_policy_account_input_heads", [
    ["account input head tenant identity", "account_id TEXT PRIMARY KEY"],
    [
      "account input head account cascade binding",
      "FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE",
    ],
    [
      "account input head exact event cascade binding",
      "FOREIGN KEY(account_id, input_generation, input_transition_id, " +
        "input_transition_sha256) REFERENCES " +
        "jobs_track_policy_account_input_transitions(account_id, " +
        "input_generation, input_transition_id, input_transition_sha256) " +
        "ON DELETE CASCADE",
    ],
  ]);
  requireTable("jobs_track_policy_track_input_transitions", [
    [
      "Track input generation safe-integer range",
      "CHECK(input_generation BETWEEN 1 AND 9007199254740991)",
    ],
    [
      "Track previous input generation safe-integer range",
      "CHECK(previous_input_generation BETWEEN 0 AND 9007199254740991)",
    ],
    [
      "Track input timestamp safe-integer range",
      "CHECK(changed_at_ms BETWEEN 0 AND 9007199254740991)",
    ],
    [
      "Track input generation uniqueness",
      "UNIQUE(account_id, career_track_id, input_generation)",
    ],
    [
      "Track input transition exact identity",
      "UNIQUE(account_id, career_track_id, input_generation, " +
        "input_transition_id, input_transition_sha256)",
    ],
    [
      "Track input tenant cascade binding",
      "FOREIGN KEY(account_id, career_track_id) " +
        "REFERENCES jobs_tracks(account_id, id) ON DELETE CASCADE",
    ],
    [
      "Track input predecessor tenant cascade binding",
      "FOREIGN KEY(account_id, career_track_id, previous_input_generation, " +
        "predecessor_input_transition_sha256) REFERENCES " +
        "jobs_track_policy_track_input_transitions(account_id, " +
        "career_track_id, input_generation, input_transition_sha256) " +
        "ON DELETE CASCADE",
    ],
    [
      "initial Track input has no predecessor",
      "input_generation = 1 AND previous_input_generation = 0 " +
        "AND predecessor_input_transition_sha256 IS NULL",
    ],
    [
      "Track input advances exactly one generation",
      "input_generation > 1 " +
        "AND previous_input_generation = input_generation - 1 " +
        "AND predecessor_input_transition_sha256 IS NOT NULL",
    ],
  ]);
  requireTable("jobs_track_policy_track_input_heads", [
    [
      "Track input head tenant identity",
      "PRIMARY KEY(account_id, career_track_id)",
    ],
    [
      "Track input head Track cascade binding",
      "FOREIGN KEY(account_id, career_track_id) " +
        "REFERENCES jobs_tracks(account_id, id) ON DELETE CASCADE",
    ],
    [
      "Track input head exact event cascade binding",
      "FOREIGN KEY(account_id, career_track_id, input_generation, " +
        "input_transition_id, input_transition_sha256) REFERENCES " +
        "jobs_track_policy_track_input_transitions(account_id, career_track_id, " +
        "input_generation, input_transition_id, input_transition_sha256) " +
        "ON DELETE CASCADE",
    ],
  ]);
  requireTable("jobs_track_policy_revisions", [
    [
      "policy revision taxonomy activation epoch safe-integer range",
      "CHECK(taxonomy_activation_epoch BETWEEN 1 AND 9007199254740991)",
    ],
    [
      "policy revision canonicalizer version safe-integer range",
      "CHECK(canonicalizer_schema_version BETWEEN 1 AND 9007199254740991)",
    ],
    [
      "policy revision account input generation safe-integer range",
      "CHECK(account_input_generation BETWEEN 1 AND 9007199254740991)",
    ],
    [
      "policy revision Track input generation safe-integer range",
      "CHECK(track_input_generation BETWEEN 1 AND 9007199254740991)",
    ],
    [
      "policy revision tenant generation uniqueness",
      "UNIQUE(account_id, career_track_id, revision_no)",
    ],
    [
      "policy revision exact identity uniqueness",
      "UNIQUE(account_id, career_track_id, revision_no, revision_id, " +
        "canonical_policy_sha256)",
    ],
    [
      "policy revision Track tenant and cascade binding",
      "FOREIGN KEY(account_id, career_track_id) " +
        "REFERENCES jobs_tracks(account_id, id) ON DELETE CASCADE",
    ],
    [
      "policy revision predecessor tenant and cascade binding",
      "FOREIGN KEY(account_id, career_track_id, predecessor_revision_no, " +
        "predecessor_revision_id, predecessor_policy_sha256) " +
        "REFERENCES jobs_track_policy_revisions(account_id, career_track_id, " +
        "revision_no, revision_id, canonical_policy_sha256) ON DELETE CASCADE",
    ],
    [
      "first policy revision has no predecessor",
      "revision_no = 1 AND predecessor_revision_id IS NULL " +
        "AND predecessor_revision_no IS NULL " +
        "AND predecessor_policy_sha256 IS NULL " +
        "AND compatibility_classification = 'initial'",
    ],
    [
      "successor policy revision advances exactly one generation",
      "revision_no > 1 AND predecessor_revision_id IS NOT NULL " +
        "AND predecessor_revision_no = revision_no - 1 " +
        "AND predecessor_policy_sha256 IS NOT NULL " +
        "AND compatibility_classification <> 'initial'",
    ],
  ]);
  requireTable("jobs_track_policy_review_receipts", [
    [
      "review receipt reviewer idempotency",
      "UNIQUE(account_id, career_track_id, policy_revision_id, reviewer_id)",
    ],
    [
      "review receipt exact policy identity",
      "UNIQUE(account_id, career_track_id, policy_revision_no, " +
        "policy_revision_id, canonical_policy_sha256, review_receipt_id, " +
        "review_receipt_sha256)",
    ],
    [
      "review receipt tenant and cascade revision binding",
      "FOREIGN KEY(account_id, career_track_id, policy_revision_no, " +
        "policy_revision_id, canonical_policy_sha256) REFERENCES " +
        "jobs_track_policy_revisions(account_id, career_track_id, revision_no, " +
        "revision_id, canonical_policy_sha256) ON DELETE CASCADE",
    ],
  ]);
  requireTable("jobs_track_policy_head_transitions", [
    [
      "policy head transition tenant generation identity",
      "PRIMARY KEY(account_id, career_track_id, head_generation)",
    ],
    [
      "policy head transition exact identity",
      "UNIQUE(account_id, career_track_id, head_generation, " +
        "head_transition_sha256)",
    ],
    [
      "policy head transition tenant revision cascade binding",
      "FOREIGN KEY(account_id, career_track_id, policy_revision_no, " +
        "policy_revision_id, canonical_policy_sha256) REFERENCES " +
        "jobs_track_policy_revisions(account_id, career_track_id, revision_no, " +
        "revision_id, canonical_policy_sha256) ON DELETE CASCADE",
    ],
    [
      "policy head transition tenant receipt cascade binding",
      "FOREIGN KEY(account_id, career_track_id, policy_revision_no, " +
        "policy_revision_id, canonical_policy_sha256, review_receipt_id, " +
        "review_receipt_sha256) REFERENCES jobs_track_policy_review_receipts(" +
        "account_id, career_track_id, policy_revision_no, policy_revision_id, " +
        "canonical_policy_sha256, review_receipt_id, review_receipt_sha256) " +
        "ON DELETE CASCADE",
    ],
    [
      "policy head transition predecessor tenant cascade binding",
      "FOREIGN KEY(account_id, career_track_id, previous_head_generation, " +
        "predecessor_head_transition_sha256) REFERENCES " +
        "jobs_track_policy_head_transitions(account_id, career_track_id, " +
        "head_generation, head_transition_sha256) ON DELETE CASCADE",
    ],
    [
      "initial policy head transition has no predecessor",
      "head_generation = 1 AND previous_head_generation = 0 " +
        "AND predecessor_head_transition_sha256 IS NULL",
    ],
    [
      "policy head transition advances exactly one generation",
      "head_generation > 1 " +
        "AND previous_head_generation = head_generation - 1 " +
        "AND predecessor_head_transition_sha256 IS NOT NULL",
    ],
  ]);
  requireTable("jobs_track_policy_heads", [
    ["policy head tenant identity", "PRIMARY KEY(account_id, career_track_id)"],
    [
      "policy head exact revision identity",
      "UNIQUE(account_id, career_track_id, policy_revision_no, " +
        "policy_revision_id, canonical_policy_sha256)",
    ],
    [
      "policy head tenant and cascade revision binding",
      "FOREIGN KEY(account_id, career_track_id, policy_revision_no, " +
        "policy_revision_id, canonical_policy_sha256) REFERENCES " +
        "jobs_track_policy_revisions(account_id, career_track_id, revision_no, " +
        "revision_id, canonical_policy_sha256) ON DELETE CASCADE",
    ],
    [
      "policy head exact immutable transition cascade binding",
      "FOREIGN KEY(account_id, career_track_id, head_generation, " +
        "head_transition_sha256) REFERENCES jobs_track_policy_head_transitions(" +
        "account_id, career_track_id, head_generation, " +
        "head_transition_sha256) ON DELETE CASCADE",
    ],
    [
      "policy head tenant and cascade receipt binding",
      "FOREIGN KEY(account_id, career_track_id, policy_revision_no, " +
        "policy_revision_id, canonical_policy_sha256, review_receipt_id, " +
        "review_receipt_sha256) REFERENCES jobs_track_policy_review_receipts(" +
        "account_id, career_track_id, policy_revision_no, policy_revision_id, " +
        "canonical_policy_sha256, review_receipt_id, review_receipt_sha256) " +
        "ON DELETE CASCADE",
    ],
    [
      "policy head generation equals revision",
      "CHECK(head_generation = policy_revision_no)",
    ],
    [
      "initial policy head has no predecessor transition",
      "head_generation = 1 AND previous_head_generation = 0 " +
        "AND predecessor_head_transition_sha256 IS NULL",
    ],
    [
      "successor policy head advances exactly one generation",
      "head_generation > 1 " +
        "AND previous_head_generation = head_generation - 1 " +
        "AND predecessor_head_transition_sha256 IS NOT NULL",
    ],
  ]);
  for (const tableName of [
    "jobs_track_policy_revisions",
    "jobs_track_policy_review_receipts",
    "jobs_track_policy_head_transitions",
    "jobs_track_policy_heads",
  ]) {
    requireTable(tableName, [
      ["taxonomy activation epoch column", "taxonomy_activation_epoch"],
      ["canonicalizer version column", "canonicalizer_schema_version"],
      ["canonicalizer digest column", "canonicalizer_digest_sha256"],
      ["account input generation column", "account_input_generation"],
      ["account input transition column", "account_input_transition_sha256"],
      ["account semantic digest column", "account_semantic_sha256"],
      ["Track input generation column", "track_input_generation"],
      ["Track input transition column", "track_input_transition_sha256"],
      ["Track semantic digest column", "track_semantic_sha256"],
    ]);
  }
}

const PHASE_613_REVISION_INSERT_REQUIREMENTS = [
  [
    "policy revision verified identity tenant binding",
    "identity.account_id = NEW.account_id " +
      "AND identity.id = NEW.verified_application_identity_id " +
      "AND identity.verification_status = 'verified'",
  ],
  [
    "policy revision source resume tenant binding",
    "asset.account_id = NEW.account_id AND asset.id = NEW.source_resume_asset_id " +
      "AND asset.sha256 = NEW.source_resume_sha256",
  ],
  [
    "policy revision predecessor exact tenant binding",
    "predecessor.account_id = NEW.account_id " +
      "AND predecessor.career_track_id = NEW.career_track_id " +
      "AND predecessor.revision_no = NEW.predecessor_revision_no " +
      "AND predecessor.revision_id = NEW.predecessor_revision_id " +
      "AND predecessor.canonical_policy_sha256 = NEW.predecessor_policy_sha256",
  ],
  [
    "policy revision predecessor time ordering",
    "predecessor.created_at_ms <= NEW.created_at_ms",
  ],
];

const PHASE_613_HEAD_INSERT_REQUIREMENTS = [
  [
    "initial policy head generation",
    "NEW.head_generation <> 1 OR NEW.previous_head_generation <> 0",
  ],
  [
    "initial policy head has no predecessor transition",
    "NEW.predecessor_head_transition_sha256 IS NOT NULL",
  ],
  [
    "initial policy head exact revision tenant binding",
    "revision.account_id = NEW.account_id " +
      "AND revision.career_track_id = NEW.career_track_id " +
      "AND revision.revision_id = NEW.policy_revision_id " +
      "AND revision.revision_no = NEW.policy_revision_no " +
      "AND revision.canonical_policy_sha256 = NEW.canonical_policy_sha256",
  ],
  [
    "initial policy head approved receipt tenant binding",
    "receipt.account_id = NEW.account_id " +
      "AND receipt.career_track_id = NEW.career_track_id " +
      "AND receipt.policy_revision_id = NEW.policy_revision_id " +
      "AND receipt.policy_revision_no = NEW.policy_revision_no " +
      "AND receipt.canonical_policy_sha256 = NEW.canonical_policy_sha256",
  ],
];

const PHASE_613_HEAD_UPDATE_REQUIREMENTS = [
  [
    "policy head account cannot be reparented",
    "NEW.account_id <> OLD.account_id",
  ],
  [
    "policy head Track cannot be reparented",
    "NEW.career_track_id <> OLD.career_track_id",
  ],
  [
    "policy head advances exactly one generation",
    "NEW.head_generation <> OLD.head_generation + 1",
  ],
  [
    "policy head previous generation binds old head",
    "NEW.previous_head_generation <> OLD.head_generation",
  ],
  [
    "policy head predecessor transition binds old head",
    "NEW.predecessor_head_transition_sha256 <> OLD.head_transition_sha256",
  ],
  [
    "policy head revision advances exactly one generation",
    "NEW.policy_revision_no <> OLD.policy_revision_no + 1",
  ],
  [
    "policy head revision identity advances",
    "NEW.policy_revision_id = OLD.policy_revision_id",
  ],
  [
    "policy head transition identity advances",
    "NEW.head_transition_sha256 = OLD.head_transition_sha256",
  ],
  [
    "policy head update time is monotonic",
    "NEW.updated_at_ms < OLD.updated_at_ms",
  ],
  [
    "policy head revision predecessor binds old revision",
    "revision.predecessor_revision_id = OLD.policy_revision_id " +
      "AND revision.predecessor_revision_no = OLD.policy_revision_no " +
      "AND revision.predecessor_policy_sha256 = OLD.canonical_policy_sha256",
  ],
];

const PHASE_613_TAXONOMY_ACTIVATION_EVENT_REQUIREMENTS = [
  [
    "taxonomy activation predecessor epoch binding",
    "predecessor.activation_epoch = NEW.previous_activation_epoch",
  ],
  [
    "taxonomy activation predecessor transition binding",
    "predecessor.activation_transition_sha256 = " +
      "NEW.predecessor_activation_transition_sha256",
  ],
  [
    "taxonomy activation time ordering",
    "predecessor.activated_at_ms <= NEW.activated_at_ms",
  ],
  [
    "taxonomy activation changed-tuple requirement",
    "NOT (predecessor.taxonomy_version = NEW.taxonomy_version " +
      "AND predecessor.taxonomy_digest_sha256 = NEW.taxonomy_digest_sha256 " +
      "AND predecessor.canonicalizer_schema_version = " +
      "NEW.canonicalizer_schema_version " +
      "AND predecessor.canonicalizer_digest_sha256 = " +
      "NEW.canonicalizer_digest_sha256)",
  ],
];

const PHASE_613_TAXONOMY_ACTIVATION_HEAD_UPDATE_REQUIREMENTS = [
  [
    "taxonomy activation singleton cannot be reparented",
    "NEW.singleton_id <> OLD.singleton_id",
  ],
  [
    "taxonomy activation advances exactly one epoch",
    "NEW.activation_epoch <> OLD.activation_epoch + 1",
  ],
  [
    "taxonomy activation previous epoch binds old head",
    "NEW.previous_activation_epoch <> OLD.activation_epoch",
  ],
  [
    "taxonomy activation predecessor transition binds old head",
    "NEW.predecessor_activation_transition_sha256 <> " +
      "OLD.activation_transition_sha256",
  ],
  [
    "taxonomy activation update time is monotonic",
    "NEW.activated_at_ms < OLD.activated_at_ms",
  ],
  [
    "taxonomy activation head binds exact event tuple",
    "event.taxonomy_version = NEW.taxonomy_version " +
      "AND event.taxonomy_digest_sha256 = NEW.taxonomy_digest_sha256 " +
      "AND event.canonicalizer_schema_version = " +
      "NEW.canonicalizer_schema_version " +
      "AND event.canonicalizer_digest_sha256 = " +
      "NEW.canonicalizer_digest_sha256",
  ],
];

const PHASE_613_ACCOUNT_INPUT_TRANSITION_REQUIREMENTS = [
  [
    "account input predecessor tenant binding",
    "predecessor.account_id = NEW.account_id",
  ],
  [
    "account input predecessor generation binding",
    "predecessor.input_generation = NEW.previous_input_generation",
  ],
  [
    "account input predecessor transition binding",
    "predecessor.input_transition_sha256 = " +
      "NEW.predecessor_input_transition_sha256",
  ],
  [
    "account input time ordering",
    "predecessor.changed_at_ms <= NEW.changed_at_ms",
  ],
];

const PHASE_613_ACCOUNT_INPUT_HEAD_UPDATE_REQUIREMENTS = [
  [
    "account input head cannot be reparented",
    "NEW.account_id <> OLD.account_id",
  ],
  [
    "account input head advances exactly one generation",
    "NEW.input_generation <> OLD.input_generation + 1",
  ],
  [
    "account input head previous generation binds old head",
    "NEW.previous_input_generation <> OLD.input_generation",
  ],
  [
    "account input head predecessor transition binds old head",
    "NEW.predecessor_input_transition_sha256 <> OLD.input_transition_sha256",
  ],
  [
    "account input head update time is monotonic",
    "NEW.updated_at_ms < OLD.updated_at_ms",
  ],
  [
    "account input head binds exact event semantics",
    "event.input_kind = NEW.input_kind " +
      "AND event.input_subject_sha256 = NEW.input_subject_sha256 " +
      "AND event.account_semantic_sha256 = NEW.account_semantic_sha256 " +
      "AND event.input_transition_sha256 = NEW.input_transition_sha256",
  ],
];

const PHASE_613_TRACK_INPUT_TRANSITION_REQUIREMENTS = [
  [
    "Track input predecessor tenant binding",
    "predecessor.account_id = NEW.account_id " +
      "AND predecessor.career_track_id = NEW.career_track_id",
  ],
  [
    "Track input predecessor generation binding",
    "predecessor.input_generation = NEW.previous_input_generation",
  ],
  [
    "Track input predecessor transition binding",
    "predecessor.input_transition_sha256 = " +
      "NEW.predecessor_input_transition_sha256",
  ],
  [
    "Track input time ordering",
    "predecessor.changed_at_ms <= NEW.changed_at_ms",
  ],
];

const PHASE_613_TRACK_INPUT_HEAD_UPDATE_REQUIREMENTS = [
  [
    "Track input head account cannot be reparented",
    "NEW.account_id <> OLD.account_id",
  ],
  [
    "Track input head Track cannot be reparented",
    "NEW.career_track_id <> OLD.career_track_id",
  ],
  [
    "Track input head advances exactly one generation",
    "NEW.input_generation <> OLD.input_generation + 1",
  ],
  [
    "Track input head previous generation binds old head",
    "NEW.previous_input_generation <> OLD.input_generation",
  ],
  [
    "Track input head predecessor transition binds old head",
    "NEW.predecessor_input_transition_sha256 <> OLD.input_transition_sha256",
  ],
  [
    "Track input head update time is monotonic",
    "NEW.updated_at_ms < OLD.updated_at_ms",
  ],
  [
    "Track input head binds exact event semantic digest",
    "event.track_semantic_sha256 = NEW.track_semantic_sha256 " +
      "AND event.input_transition_sha256 = NEW.input_transition_sha256",
  ],
];

const PHASE_613_POLICY_INPUT_BINDING_REQUIREMENTS = [
  [
    "policy binds active taxonomy epoch and tuple",
    "activation.activation_epoch = NEW.taxonomy_activation_epoch " +
      "AND activation.taxonomy_version = NEW.taxonomy_version " +
      "AND activation.taxonomy_digest_sha256 = NEW.taxonomy_digest_sha256 " +
      "AND activation.canonicalizer_schema_version = " +
      "NEW.canonicalizer_schema_version " +
      "AND activation.canonicalizer_digest_sha256 = " +
      "NEW.canonicalizer_digest_sha256",
  ],
  [
    "policy binds current account input generation and semantic digest",
    "input.account_id = NEW.account_id " +
      "AND input.input_generation = NEW.account_input_generation " +
      "AND input.input_transition_sha256 = NEW.account_input_transition_sha256 " +
      "AND input.account_semantic_sha256 = NEW.account_semantic_sha256",
  ],
  [
    "policy binds current Track input generation and semantic digest",
    "input.account_id = NEW.account_id " +
      "AND input.career_track_id = NEW.career_track_id " +
      "AND input.input_generation = NEW.track_input_generation " +
      "AND input.input_transition_sha256 = NEW.track_input_transition_sha256 " +
      "AND input.track_semantic_sha256 = NEW.track_semantic_sha256",
  ],
];

const PHASE_613_REVISION_GENERATION_BINDING_REQUIREMENTS = [
  [
    "exact taxonomy activation epoch binding",
    "revision.taxonomy_activation_epoch = NEW.taxonomy_activation_epoch",
  ],
  [
    "exact canonicalizer version and digest binding",
    "revision.canonicalizer_schema_version = " +
      "NEW.canonicalizer_schema_version " +
      "AND revision.canonicalizer_digest_sha256 = " +
      "NEW.canonicalizer_digest_sha256",
  ],
  [
    "exact account input generation and transition binding",
    "revision.account_input_generation = NEW.account_input_generation " +
      "AND revision.account_input_transition_sha256 = " +
      "NEW.account_input_transition_sha256",
  ],
  [
    "exact account semantic digest binding",
    "revision.account_semantic_sha256 = NEW.account_semantic_sha256",
  ],
  [
    "exact Track input generation and transition binding",
    "revision.track_input_generation = NEW.track_input_generation " +
      "AND revision.track_input_transition_sha256 = " +
      "NEW.track_input_transition_sha256",
  ],
  [
    "exact Track semantic digest binding",
    "revision.track_semantic_sha256 = NEW.track_semantic_sha256",
  ],
];

const PHASE_613_HEAD_EVENT_GENERATION_BINDING_REQUIREMENTS = [
  [
    "policy head binds immutable event revision identity",
    "event.policy_revision_id = NEW.policy_revision_id " +
      "AND event.policy_revision_no = NEW.policy_revision_no " +
      "AND event.canonical_policy_sha256 = NEW.canonical_policy_sha256",
  ],
  [
    "policy head binds immutable event taxonomy digest",
    "event.taxonomy_digest_sha256 = NEW.taxonomy_digest_sha256",
  ],
  [
    "policy head binds immutable event taxonomy activation epoch",
    "event.taxonomy_activation_epoch = NEW.taxonomy_activation_epoch",
  ],
  [
    "policy head binds immutable event canonicalizer",
    "event.canonicalizer_schema_version = NEW.canonicalizer_schema_version " +
      "AND event.canonicalizer_digest_sha256 = " +
      "NEW.canonicalizer_digest_sha256",
  ],
  [
    "policy head binds immutable event account input generation",
    "event.account_input_generation = NEW.account_input_generation " +
      "AND event.account_input_transition_sha256 = " +
      "NEW.account_input_transition_sha256",
  ],
  [
    "policy head binds immutable event account semantic digest",
    "event.account_semantic_sha256 = NEW.account_semantic_sha256",
  ],
  [
    "policy head binds immutable event Track input generation",
    "event.track_input_generation = NEW.track_input_generation " +
      "AND event.track_input_transition_sha256 = " +
      "NEW.track_input_transition_sha256",
  ],
  [
    "policy head binds immutable event Track semantic digest",
    "event.track_semantic_sha256 = NEW.track_semantic_sha256",
  ],
  [
    "policy head binds immutable event verified identity digest",
    "event.verified_application_identity_sha256 = " +
      "NEW.verified_application_identity_sha256",
  ],
  [
    "policy head binds immutable event resume and preferences digests",
    "event.source_resume_sha256 = NEW.source_resume_sha256 " +
      "AND event.job_preferences_sha256 = NEW.job_preferences_sha256",
  ],
  [
    "policy head binds immutable event review receipt",
    "event.review_receipt_id = NEW.review_receipt_id " +
      "AND event.review_receipt_sha256 = NEW.review_receipt_sha256",
  ],
  [
    "policy head binds immutable event transition identity",
    "event.head_transition_sha256 = NEW.head_transition_sha256",
  ],
  [
    "policy head binds immutable event updater and time",
    "event.updated_by = NEW.updated_by " +
      "AND event.updated_at_ms = NEW.updated_at_ms",
  ],
];

function phase613SqliteRequirements(issues, sql) {
  const dialect = "SQLite";
  phase613TableRequirements(issues, dialect, sql);
  for (const [triggerName, tableName, requirements] of [
    [
      "trg_jobs_track_policy_taxonomy_activation_events_validate_insert",
      "jobs_track_policy_taxonomy_activation_events",
      PHASE_613_TAXONOMY_ACTIVATION_EVENT_REQUIREMENTS,
    ],
    [
      "trg_jobs_track_policy_account_input_transitions_validate_insert",
      "jobs_track_policy_account_input_transitions",
      PHASE_613_ACCOUNT_INPUT_TRANSITION_REQUIREMENTS,
    ],
    [
      "trg_jobs_track_policy_track_input_transitions_validate_insert",
      "jobs_track_policy_track_input_transitions",
      PHASE_613_TRACK_INPUT_TRANSITION_REQUIREMENTS,
    ],
  ]) {
    requirePhase613Fragments(
      issues,
      dialect,
      triggerName,
      extractSqliteTrigger(sql, triggerName),
      [
        [
          `${tableName} validation trigger binding`,
          `BEFORE INSERT ON ${tableName}`,
        ],
        ...requirements,
      ],
    );
  }
  for (const [triggerName, tableName, requirements] of [
    [
      "trg_jobs_track_policy_taxonomy_activation_head_validate_insert",
      "jobs_track_policy_taxonomy_activation_head",
      [
        ["initial taxonomy activation epoch", "NEW.activation_epoch <> 1"],
        [
          "initial taxonomy activation previous epoch",
          "NEW.previous_activation_epoch <> 0",
        ],
        [
          "initial taxonomy activation has no predecessor transition",
          "NEW.predecessor_activation_transition_sha256 IS NOT NULL",
        ],
        [
          "initial taxonomy activation binds exact event tuple",
          "event.taxonomy_version = NEW.taxonomy_version " +
            "AND event.taxonomy_digest_sha256 = NEW.taxonomy_digest_sha256 " +
            "AND event.canonicalizer_schema_version = " +
            "NEW.canonicalizer_schema_version " +
            "AND event.canonicalizer_digest_sha256 = " +
            "NEW.canonicalizer_digest_sha256",
        ],
      ],
    ],
    [
      "trg_jobs_track_policy_account_input_heads_validate_insert",
      "jobs_track_policy_account_input_heads",
      [
        ["initial account input generation", "NEW.input_generation <> 1"],
        [
          "initial account input previous generation",
          "NEW.previous_input_generation <> 0",
        ],
        [
          "initial account input has no predecessor transition",
          "NEW.predecessor_input_transition_sha256 IS NOT NULL",
        ],
        [
          "initial account input binds event tenant",
          "event.account_id = NEW.account_id",
        ],
        [
          "initial account input binds exact event semantics",
          "event.input_kind = NEW.input_kind " +
            "AND event.input_subject_sha256 = NEW.input_subject_sha256 " +
            "AND event.account_semantic_sha256 = NEW.account_semantic_sha256 " +
            "AND event.input_transition_sha256 = NEW.input_transition_sha256",
        ],
      ],
    ],
    [
      "trg_jobs_track_policy_track_input_heads_validate_insert",
      "jobs_track_policy_track_input_heads",
      [
        ["initial Track input generation", "NEW.input_generation <> 1"],
        [
          "initial Track input previous generation",
          "NEW.previous_input_generation <> 0",
        ],
        [
          "initial Track input has no predecessor transition",
          "NEW.predecessor_input_transition_sha256 IS NOT NULL",
        ],
        [
          "initial Track input binds event tenant",
          "event.account_id = NEW.account_id " +
            "AND event.career_track_id = NEW.career_track_id",
        ],
        [
          "initial Track input binds exact event semantics",
          "event.track_semantic_sha256 = NEW.track_semantic_sha256 " +
            "AND event.input_transition_sha256 = NEW.input_transition_sha256",
        ],
      ],
    ],
  ]) {
    requirePhase613Fragments(
      issues,
      dialect,
      triggerName,
      extractSqliteTrigger(sql, triggerName),
      [
        [
          `${tableName} initial validation trigger binding`,
          `BEFORE INSERT ON ${tableName}`,
        ],
        ...requirements,
      ],
    );
  }
  for (const [triggerName, tableName, requirements] of [
    [
      "trg_jobs_track_policy_taxonomy_activation_head_monotonic",
      "jobs_track_policy_taxonomy_activation_head",
      PHASE_613_TAXONOMY_ACTIVATION_HEAD_UPDATE_REQUIREMENTS,
    ],
    [
      "trg_jobs_track_policy_account_input_heads_monotonic",
      "jobs_track_policy_account_input_heads",
      PHASE_613_ACCOUNT_INPUT_HEAD_UPDATE_REQUIREMENTS,
    ],
    [
      "trg_jobs_track_policy_track_input_heads_monotonic",
      "jobs_track_policy_track_input_heads",
      PHASE_613_TRACK_INPUT_HEAD_UPDATE_REQUIREMENTS,
    ],
  ]) {
    requirePhase613Fragments(
      issues,
      dialect,
      triggerName,
      extractSqliteTrigger(sql, triggerName),
      [
        [
          `${tableName} monotonic trigger binding`,
          `BEFORE UPDATE ON ${tableName}`,
        ],
        ...requirements,
      ],
    );
  }
  for (const tableName of [
    "jobs_track_policy_taxonomy_activation_events",
    "jobs_track_policy_account_input_transitions",
    "jobs_track_policy_track_input_transitions",
    "jobs_track_policy_head_transitions",
  ]) {
    for (const operation of ["update", "delete"]) {
      const triggerName = `trg_${tableName}_no_${operation}`;
      requirePhase613Fragments(
        issues,
        dialect,
        triggerName,
        extractSqliteTrigger(sql, triggerName),
        [
          [
            `${tableName} ${operation} immutability`,
            `BEFORE ${operation.toUpperCase()} ON ${tableName}`,
          ],
          [`${tableName} ${operation} rejection`, "RAISE(ABORT"],
        ],
      );
    }
  }
  for (const tableName of [
    "jobs_track_policy_taxonomy_activation_head",
    "jobs_track_policy_account_input_heads",
    "jobs_track_policy_track_input_heads",
  ]) {
    const triggerName = `trg_${tableName}_no_delete`;
    requirePhase613Fragments(
      issues,
      dialect,
      triggerName,
      extractSqliteTrigger(sql, triggerName),
      [
        [`${tableName} delete immutability`, `BEFORE DELETE ON ${tableName}`],
        [`${tableName} delete rejection`, "RAISE(ABORT"],
      ],
    );
  }
  requirePhase613Fragments(
    issues,
    dialect,
    "trg_jobs_track_policy_revisions_validate_insert",
    extractSqliteTrigger(
      sql,
      "trg_jobs_track_policy_revisions_validate_insert",
    ),
    [
      [
        "policy revision validation trigger binding",
        "BEFORE INSERT ON jobs_track_policy_revisions",
      ],
      ...PHASE_613_POLICY_INPUT_BINDING_REQUIREMENTS,
      ...PHASE_613_REVISION_INSERT_REQUIREMENTS,
    ],
  );
  requirePhase613Fragments(
    issues,
    dialect,
    "trg_jobs_track_policy_review_receipts_validate_insert",
    extractSqliteTrigger(
      sql,
      "trg_jobs_track_policy_review_receipts_validate_insert",
    ),
    [
      [
        "policy review receipt validation trigger binding",
        "BEFORE INSERT ON jobs_track_policy_review_receipts",
      ],
      ...PHASE_613_REVISION_GENERATION_BINDING_REQUIREMENTS,
    ],
  );
  requirePhase613Fragments(
    issues,
    dialect,
    "trg_jobs_track_policy_head_transitions_validate_insert",
    extractSqliteTrigger(
      sql,
      "trg_jobs_track_policy_head_transitions_validate_insert",
    ),
    [
      [
        "policy head transition validation trigger binding",
        "BEFORE INSERT ON jobs_track_policy_head_transitions",
      ],
      ...PHASE_613_REVISION_GENERATION_BINDING_REQUIREMENTS,
      [
        "policy head transition predecessor tenant binding",
        "predecessor.account_id = NEW.account_id " +
          "AND predecessor.career_track_id = NEW.career_track_id " +
          "AND predecessor.head_generation = NEW.previous_head_generation " +
          "AND predecessor.head_transition_sha256 = " +
          "NEW.predecessor_head_transition_sha256",
      ],
      [
        "policy head transition time ordering",
        "predecessor.updated_at_ms <= NEW.updated_at_ms",
      ],
    ],
  );
  for (const [tableName, errorMessage] of [
    [
      "jobs_track_policy_revisions",
      "Career Track policy evidence is immutable",
    ],
    [
      "jobs_track_policy_review_receipts",
      "Career Track policy evidence is immutable",
    ],
  ]) {
    for (const operation of ["update", "delete"]) {
      const triggerName = `trg_${tableName}_no_${operation}`;
      const requirements = [
        [
          `${tableName} ${operation} immutability`,
          `BEFORE ${operation.toUpperCase()} ON ${tableName}`,
        ],
        [
          `${tableName} ${operation} rejection`,
          `RAISE(ABORT, '${errorMessage}')`,
        ],
      ];
      if (operation === "delete") {
        requirements.push(
          [
            `${tableName} account cascade escape is tenant-bound`,
            "EXISTS (SELECT 1 FROM accounts WHERE id = OLD.account_id)",
          ],
          [
            `${tableName} Track cascade escape is tenant-bound`,
            "WHERE account_id = OLD.account_id AND id = OLD.career_track_id",
          ],
        );
      }
      requirePhase613Fragments(
        issues,
        dialect,
        triggerName,
        extractSqliteTrigger(sql, triggerName),
        requirements,
      );
    }
  }
  requirePhase613Fragments(
    issues,
    dialect,
    "trg_jobs_track_policy_heads_validate_insert",
    extractSqliteTrigger(sql, "trg_jobs_track_policy_heads_validate_insert"),
    [
      [
        "initial policy head validation trigger binding",
        "BEFORE INSERT ON jobs_track_policy_heads",
      ],
      ...PHASE_613_HEAD_INSERT_REQUIREMENTS,
      ...PHASE_613_HEAD_EVENT_GENERATION_BINDING_REQUIREMENTS,
    ],
  );
  requirePhase613Fragments(
    issues,
    dialect,
    "trg_jobs_track_policy_heads_monotonic",
    extractSqliteTrigger(sql, "trg_jobs_track_policy_heads_monotonic"),
    [
      [
        "policy head monotonic trigger binding",
        "BEFORE UPDATE ON jobs_track_policy_heads",
      ],
      ...PHASE_613_HEAD_UPDATE_REQUIREMENTS,
      ...PHASE_613_HEAD_EVENT_GENERATION_BINDING_REQUIREMENTS,
    ],
  );
  requirePhase613Fragments(
    issues,
    dialect,
    "trg_jobs_track_policy_heads_no_delete",
    extractSqliteTrigger(sql, "trg_jobs_track_policy_heads_no_delete"),
    [
      [
        "policy head delete immutability",
        "BEFORE DELETE ON jobs_track_policy_heads",
      ],
      [
        "policy head delete rejection",
        "RAISE(ABORT, 'Career Track policy evidence is immutable')",
      ],
      [
        "policy head account cascade escape is tenant-bound",
        "EXISTS (SELECT 1 FROM accounts WHERE id = OLD.account_id)",
      ],
      [
        "policy head Track cascade escape is tenant-bound",
        "WHERE account_id = OLD.account_id AND id = OLD.career_track_id",
      ],
    ],
  );
}

function phase613PostgresRequirements(issues, sql) {
  const dialect = "Postgres";
  phase613TableRequirements(issues, dialect, sql);
  for (const [functionName, requirements] of [
    [
      "validate_jobs_track_policy_activation_event_insert",
      PHASE_613_TAXONOMY_ACTIVATION_EVENT_REQUIREMENTS,
    ],
    [
      "enforce_jobs_track_policy_activation_head",
      [
        ["taxonomy activation insert branch", "TG_OP = 'INSERT'"],
        ["initial taxonomy activation epoch", "NEW.activation_epoch <> 1"],
        [
          "initial taxonomy activation previous epoch",
          "NEW.previous_activation_epoch <> 0",
        ],
        ["taxonomy activation update branch", "TG_OP = 'UPDATE'"],
        ...PHASE_613_TAXONOMY_ACTIVATION_HEAD_UPDATE_REQUIREMENTS,
      ],
    ],
    [
      "validate_jobs_track_policy_account_input_insert",
      PHASE_613_ACCOUNT_INPUT_TRANSITION_REQUIREMENTS,
    ],
    [
      "enforce_jobs_track_policy_account_input_head",
      [
        ["account input head insert branch", "TG_OP = 'INSERT'"],
        ["initial account input generation", "NEW.input_generation <> 1"],
        ["account input head update branch", "TG_OP = 'UPDATE'"],
        ...PHASE_613_ACCOUNT_INPUT_HEAD_UPDATE_REQUIREMENTS,
      ],
    ],
    [
      "validate_jobs_track_policy_track_input_insert",
      PHASE_613_TRACK_INPUT_TRANSITION_REQUIREMENTS,
    ],
    [
      "enforce_jobs_track_policy_track_input_head",
      [
        ["Track input head insert branch", "TG_OP = 'INSERT'"],
        ["initial Track input generation", "NEW.input_generation <> 1"],
        ["Track input head update branch", "TG_OP = 'UPDATE'"],
        ...PHASE_613_TRACK_INPUT_HEAD_UPDATE_REQUIREMENTS,
      ],
    ],
    [
      "validate_jobs_track_policy_revision_insert",
      [
        ...PHASE_613_POLICY_INPUT_BINDING_REQUIREMENTS,
        ...PHASE_613_REVISION_INSERT_REQUIREMENTS,
      ],
    ],
    [
      "validate_jobs_track_policy_review_receipt_insert",
      PHASE_613_REVISION_GENERATION_BINDING_REQUIREMENTS,
    ],
    [
      "validate_jobs_track_policy_head_transition_insert",
      [
        ...PHASE_613_REVISION_GENERATION_BINDING_REQUIREMENTS,
        [
          "policy head transition predecessor tenant binding",
          "predecessor.account_id = NEW.account_id " +
            "AND predecessor.career_track_id = NEW.career_track_id " +
            "AND predecessor.head_generation = NEW.previous_head_generation " +
            "AND predecessor.head_transition_sha256 = " +
            "NEW.predecessor_head_transition_sha256",
        ],
        [
          "policy head transition time ordering",
          "predecessor.updated_at_ms <= NEW.updated_at_ms",
        ],
      ],
    ],
    [
      "validate_jobs_track_policy_head_insert",
      [
        ...PHASE_613_HEAD_INSERT_REQUIREMENTS,
        ...PHASE_613_HEAD_EVENT_GENERATION_BINDING_REQUIREMENTS,
      ],
    ],
    [
      "enforce_jobs_track_policy_head_monotonic",
      [
        ...PHASE_613_HEAD_UPDATE_REQUIREMENTS,
        ...PHASE_613_HEAD_EVENT_GENERATION_BINDING_REQUIREMENTS,
      ],
    ],
    [
      "reject_jobs_track_policy_global_immutable_mutation",
      [
        [
          "global taxonomy activation mutation rejection",
          "RAISE EXCEPTION 'global taxonomy activation evidence is immutable'",
        ],
      ],
    ],
    [
      "reject_jobs_track_policy_account_input_mutation",
      [
        [
          "account input mutation rejection",
          "RAISE EXCEPTION 'account semantic-input evidence is immutable'",
        ],
        [
          "account input cascade escape",
          "NOT EXISTS (SELECT 1 FROM accounts WHERE id = OLD.account_id)",
        ],
      ],
    ],
    [
      "reject_jobs_track_policy_immutable_mutation",
      [
        [
          "policy evidence mutation rejection",
          "RAISE EXCEPTION 'Career Track policy evidence is immutable'",
        ],
        [
          "policy evidence account cascade escape",
          "NOT EXISTS (SELECT 1 FROM accounts WHERE id = OLD.account_id)",
        ],
        [
          "policy evidence Track cascade escape is tenant-bound",
          "WHERE account_id = OLD.account_id AND id = OLD.career_track_id",
        ],
      ],
    ],
  ]) {
    requirePhase613Fragments(
      issues,
      dialect,
      functionName,
      extractPostgresFunction(sql, functionName),
      requirements,
    );
  }
  for (const [triggerName, tableName, operation, functionName] of [
    [
      "trg_jobs_track_policy_taxonomy_activation_events_validate_insert",
      "jobs_track_policy_taxonomy_activation_events",
      "INSERT",
      "validate_jobs_track_policy_activation_event_insert",
    ],
    [
      "trg_jobs_track_policy_taxonomy_activation_events_no_update",
      "jobs_track_policy_taxonomy_activation_events",
      "UPDATE",
      "reject_jobs_track_policy_global_immutable_mutation",
    ],
    [
      "trg_jobs_track_policy_taxonomy_activation_events_no_delete",
      "jobs_track_policy_taxonomy_activation_events",
      "DELETE",
      "reject_jobs_track_policy_global_immutable_mutation",
    ],
    [
      "trg_jobs_track_policy_taxonomy_activation_head_validate_insert",
      "jobs_track_policy_taxonomy_activation_head",
      "INSERT",
      "enforce_jobs_track_policy_activation_head",
    ],
    [
      "trg_jobs_track_policy_taxonomy_activation_head_monotonic",
      "jobs_track_policy_taxonomy_activation_head",
      "UPDATE",
      "enforce_jobs_track_policy_activation_head",
    ],
    [
      "trg_jobs_track_policy_taxonomy_activation_head_no_delete",
      "jobs_track_policy_taxonomy_activation_head",
      "DELETE",
      "reject_jobs_track_policy_global_immutable_mutation",
    ],
    [
      "trg_jobs_track_policy_account_input_transitions_validate_insert",
      "jobs_track_policy_account_input_transitions",
      "INSERT",
      "validate_jobs_track_policy_account_input_insert",
    ],
    [
      "trg_jobs_track_policy_account_input_transitions_no_update",
      "jobs_track_policy_account_input_transitions",
      "UPDATE",
      "reject_jobs_track_policy_account_input_mutation",
    ],
    [
      "trg_jobs_track_policy_account_input_transitions_no_delete",
      "jobs_track_policy_account_input_transitions",
      "DELETE",
      "reject_jobs_track_policy_account_input_mutation",
    ],
    [
      "trg_jobs_track_policy_account_input_heads_validate_insert",
      "jobs_track_policy_account_input_heads",
      "INSERT",
      "enforce_jobs_track_policy_account_input_head",
    ],
    [
      "trg_jobs_track_policy_account_input_heads_monotonic",
      "jobs_track_policy_account_input_heads",
      "UPDATE",
      "enforce_jobs_track_policy_account_input_head",
    ],
    [
      "trg_jobs_track_policy_account_input_heads_no_delete",
      "jobs_track_policy_account_input_heads",
      "DELETE",
      "reject_jobs_track_policy_account_input_mutation",
    ],
    [
      "trg_jobs_track_policy_track_input_transitions_validate_insert",
      "jobs_track_policy_track_input_transitions",
      "INSERT",
      "validate_jobs_track_policy_track_input_insert",
    ],
    [
      "trg_jobs_track_policy_track_input_transitions_no_update",
      "jobs_track_policy_track_input_transitions",
      "UPDATE",
      "reject_jobs_track_policy_immutable_mutation",
    ],
    [
      "trg_jobs_track_policy_track_input_transitions_no_delete",
      "jobs_track_policy_track_input_transitions",
      "DELETE",
      "reject_jobs_track_policy_immutable_mutation",
    ],
    [
      "trg_jobs_track_policy_track_input_heads_validate_insert",
      "jobs_track_policy_track_input_heads",
      "INSERT",
      "enforce_jobs_track_policy_track_input_head",
    ],
    [
      "trg_jobs_track_policy_track_input_heads_monotonic",
      "jobs_track_policy_track_input_heads",
      "UPDATE",
      "enforce_jobs_track_policy_track_input_head",
    ],
    [
      "trg_jobs_track_policy_track_input_heads_no_delete",
      "jobs_track_policy_track_input_heads",
      "DELETE",
      "reject_jobs_track_policy_immutable_mutation",
    ],
    [
      "trg_jobs_track_policy_revisions_validate_insert",
      "jobs_track_policy_revisions",
      "INSERT",
      "validate_jobs_track_policy_revision_insert",
    ],
    [
      "trg_jobs_track_policy_revisions_no_update",
      "jobs_track_policy_revisions",
      "UPDATE",
      "reject_jobs_track_policy_immutable_mutation",
    ],
    [
      "trg_jobs_track_policy_review_receipts_validate_insert",
      "jobs_track_policy_review_receipts",
      "INSERT",
      "validate_jobs_track_policy_review_receipt_insert",
    ],
    [
      "trg_jobs_track_policy_revisions_no_delete",
      "jobs_track_policy_revisions",
      "DELETE",
      "reject_jobs_track_policy_immutable_mutation",
    ],
    [
      "trg_jobs_track_policy_head_transitions_validate_insert",
      "jobs_track_policy_head_transitions",
      "INSERT",
      "validate_jobs_track_policy_head_transition_insert",
    ],
    [
      "trg_jobs_track_policy_head_transitions_no_update",
      "jobs_track_policy_head_transitions",
      "UPDATE",
      "reject_jobs_track_policy_immutable_mutation",
    ],
    [
      "trg_jobs_track_policy_head_transitions_no_delete",
      "jobs_track_policy_head_transitions",
      "DELETE",
      "reject_jobs_track_policy_immutable_mutation",
    ],
    [
      "trg_jobs_track_policy_review_receipts_no_update",
      "jobs_track_policy_review_receipts",
      "UPDATE",
      "reject_jobs_track_policy_immutable_mutation",
    ],
    [
      "trg_jobs_track_policy_review_receipts_no_delete",
      "jobs_track_policy_review_receipts",
      "DELETE",
      "reject_jobs_track_policy_immutable_mutation",
    ],
    [
      "trg_jobs_track_policy_heads_validate_insert",
      "jobs_track_policy_heads",
      "INSERT",
      "validate_jobs_track_policy_head_insert",
    ],
    [
      "trg_jobs_track_policy_heads_monotonic",
      "jobs_track_policy_heads",
      "UPDATE",
      "enforce_jobs_track_policy_head_monotonic",
    ],
    [
      "trg_jobs_track_policy_heads_no_delete",
      "jobs_track_policy_heads",
      "DELETE",
      "reject_jobs_track_policy_immutable_mutation",
    ],
  ]) {
    requirePhase613Fragments(
      issues,
      dialect,
      triggerName,
      extractPostgresTrigger(sql, triggerName),
      [
        [
          `${triggerName} operation binding`,
          `BEFORE ${operation} ON ${tableName}`,
        ],
        [`${triggerName} row binding`, "FOR EACH ROW"],
        [
          `${triggerName} function binding`,
          `EXECUTE FUNCTION ${functionName}()`,
        ],
      ],
    );
  }
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

    const sqliteComparableTable = PHASE_613_SEMANTIC_TABLES.has(tableName)
      ? extractColumnShapes(sqliteTable)
      : sqliteTable;
    const postgresComparableTable = PHASE_613_SEMANTIC_TABLES.has(tableName)
      ? extractColumnShapes(postgresTable)
      : postgresTable;
    const tableDifference = firstDifference(
      sqliteComparableTable,
      postgresComparableTable,
    );
    if (tableDifference) {
      issues.push(
        `${tableName} ${PHASE_613_SEMANTIC_TABLES.has(tableName) ? "column" : "definition"} ${tableDifference.index + 1} differs: SQLite=${JSON.stringify(tableDifference.left ?? "<missing>")} Postgres=${JSON.stringify(tableDifference.right ?? "<missing>")}`,
      );
    }

    const requiredIndexes = REQUIRED_INDEX_SIGNATURES.get(tableName) ?? [];
    const sqliteIndexes = requireExactIndexes(
      issues,
      "SQLite",
      sqliteSource,
      tableName,
      requiredIndexes,
    );
    const postgresIndexes = requireExactIndexes(
      issues,
      "Postgres",
      postgresSource,
      tableName,
      requiredIndexes,
    );
    const indexDifference = firstDifference(sqliteIndexes, postgresIndexes);
    if (indexDifference) {
      issues.push(
        `${tableName} index ${indexDifference.index + 1} differs: SQLite=${JSON.stringify(indexDifference.left ?? "<missing>")} Postgres=${JSON.stringify(indexDifference.right ?? "<missing>")}`,
      );
    }
  }

  operationalHoldSqliteRequirements(issues, sqliteSource);
  operationalHoldPostgresRequirements(issues, postgresSource);
  requireIndexesPresent(issues, "SQLite", sqliteSource, "jobs_tracks", [
    "unique idx_jobs_tracks_account_id_unique on jobs_tracks (account_id, id)",
  ]);
  requireIndexesPresent(issues, "Postgres", postgresSource, "jobs_tracks", [
    "unique idx_jobs_tracks_account_id_unique on jobs_tracks (account_id, id)",
  ]);
  requireSinglePhase613IndexDeclarations(issues, "SQLite", sqliteSource);
  requireSinglePhase613IndexDeclarations(issues, "Postgres", postgresSource);
  requireSinglePhase613TableDeclarations(issues, "SQLite", sqliteSource);
  requireSinglePhase613TableDeclarations(issues, "Postgres", postgresSource);
  phase613SqliteRequirements(issues, sqliteSource);
  phase613PostgresRequirements(issues, postgresSource);

  return issues;
}

function extractRustArrayBody(source, constantName) {
  const expression = new RegExp(
    `const\\s+${constantName}\\s*:[^=]+?=\\s*&\\[([\\s\\S]*?)\\n\\];`,
  );
  return expression.exec(source)?.[1] ?? null;
}

export function checkPhase613MigrationRegistration(runnerSource) {
  const issues = [];
  const sqliteDeclaration =
    /const\s+SQLITE_JOBS_CANONICAL_TAXONOMY_AUTHORITY\s*:\s*&str\s*=\s*include_str!\(\s*"\.\.\/\.\.\/\.\.\/infra\/sqlite\/server-runtime\/056_jobs_canonical_taxonomy_authority\.sql"\s*\)\s*;/g;
  if ([...runnerSource.matchAll(sqliteDeclaration)].length !== 1) {
    issues.push(
      "server SQLite migration runner must include 056_jobs_canonical_taxonomy_authority.sql exactly once",
    );
  }
  const sqliteMigrations = extractRustArrayBody(runnerSource, "MIGRATIONS");
  if (
    !sqliteMigrations ||
    (
      sqliteMigrations.match(/\bSQLITE_JOBS_CANONICAL_TAXONOMY_AUTHORITY\b/g) ??
      []
    ).length !== 1
  ) {
    issues.push(
      "server SQLite migration runner must register 056_jobs_canonical_taxonomy_authority.sql exactly once",
    );
  }

  const postgresIdDeclaration =
    /pub\s+const\s+JOBS_CANONICAL_TAXONOMY_AUTHORITY_MIGRATION_ID\s*:\s*&str\s*=\s*"034_jobs_canonical_taxonomy_authority\.sql"\s*;/g;
  if ([...runnerSource.matchAll(postgresIdDeclaration)].length !== 1) {
    issues.push(
      "server Postgres migration runner must declare 034_jobs_canonical_taxonomy_authority.sql exactly once",
    );
  }
  const postgresDeclaration =
    /const\s+POSTGRES_JOBS_CANONICAL_TAXONOMY_AUTHORITY\s*:\s*&str\s*=\s*include_str!\(\s*"\.\.\/\.\.\/\.\.\/infra\/postgres\/server-runtime\/034_jobs_canonical_taxonomy_authority\.sql"\s*\)\s*;/g;
  if ([...runnerSource.matchAll(postgresDeclaration)].length !== 1) {
    issues.push(
      "server Postgres migration runner must include 034_jobs_canonical_taxonomy_authority.sql exactly once",
    );
  }
  const postgresMigrations = extractRustArrayBody(
    runnerSource,
    "POSTGRES_POST_JOBS_MIGRATIONS",
  );
  const postgresRegistration =
    /\(\s*JOBS_CANONICAL_TAXONOMY_AUTHORITY_MIGRATION_ID\s*,\s*POSTGRES_JOBS_CANONICAL_TAXONOMY_AUTHORITY\s*,?\s*\)/g;
  if (
    !postgresMigrations ||
    [...postgresMigrations.matchAll(postgresRegistration)].length !== 1
  ) {
    issues.push(
      "server Postgres migration runner must register 034_jobs_canonical_taxonomy_authority.sql exactly once",
    );
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
  const sqliteOperationalHoldsPath = path.join(
    repoRoot,
    "infra/sqlite/server-runtime/052_jobs_operational_holds.sql",
  );
  const postgresOperationalHoldsPath = path.join(
    repoRoot,
    "infra/postgres/server-runtime/030_jobs_operational_holds.sql",
  );
  const sqliteCanonicalTaxonomyAuthorityPath = path.join(
    repoRoot,
    "infra/sqlite/server-runtime/056_jobs_canonical_taxonomy_authority.sql",
  );
  const postgresCanonicalTaxonomyAuthorityPath = path.join(
    repoRoot,
    "infra/postgres/server-runtime/034_jobs_canonical_taxonomy_authority.sql",
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
    sqliteOperationalHoldsPath,
    sqliteCanonicalTaxonomyAuthorityPath,
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
    postgresOperationalHoldsPath,
    postgresCanonicalTaxonomyAuthorityPath,
  ]
    .map((sourcePath) => fs.readFileSync(sourcePath, "utf8"))
    .join("\n");
  const issues = compareJobsSchemas(sqliteSource, postgresSource);
  issues.push(
    ...checkPhase613MigrationRegistration(fs.readFileSync(sqlitePath, "utf8")),
  );

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
    [
      "SQLite",
      "052_jobs_operational_holds.sql",
      "SQLITE_JOBS_OPERATIONAL_HOLDS",
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
    "030_jobs_operational_holds.sql",
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
  const indexCount =
    JOBS_PARITY_TABLES.reduce(
      (count, tableName) =>
        count + extractIndexes(sqliteSource, tableName).length,
      0,
    ) + 1;
  console.log(
    `Jobs SQLite/Postgres schema parity passed (${JOBS_PARITY_TABLES.length} tables, ${indexCount} indexes).`,
  );
}

if (
  process.argv[1] &&
  path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)
)
  main();
