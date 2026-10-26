// Bounded, set-based ATS authority reads for interactive workspace pages.
//
// The scalar resolver remains the effect-boundary authority. This module
// mirrors that resolver for `runner = None`, but reads every mutable authority
// relation once per page and performs the same fail-closed checks in memory.

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct AtsCertificationPageTargetKey {
    provider: String,
    target_key: String,
}

#[derive(Debug, Clone)]
struct AtsCertificationPageTarget {
    key: AtsCertificationPageTargetKey,
    target_key_sha256: String,
    evidence: AtsCertificationFreshTargetEvidence,
}

#[derive(Debug, Clone)]
struct AtsCertificationPageTrust {
    policy_sha256: String,
    valid_from_ms: i64,
    expires_at_ms: i64,
}

type AtsCertificationPageRevocationKey = (String, String, String);

#[derive(Debug, Default)]
struct AtsCertificationPageData {
    trust: Option<AtsCertificationPageTrust>,
    bindings: BTreeMap<(String, String, String), Vec<StoredAtsBindingRow>>,
    manifest_policies: BTreeMap<String, String>,
    account_canary_allowlists: BTreeSet<String>,
    evidence: BTreeMap<String, Vec<(i64, String)>>,
    layouts: BTreeMap<String, Vec<(i64, String, String)>>,
    runtimes: BTreeMap<String, Vec<(i64, AtsCertificationRuntimeTarget)>>,
    global_revoked: bool,
    revocations: BTreeSet<AtsCertificationPageRevocationKey>,
    quarantines: BTreeSet<(String, String, String, String)>,
    circuits: BTreeSet<(String, String)>,
}

// One interactive scan can carry one look-ahead row and at most two current
// channels per target. Signed manifests admit at most 64 evidence objects,
// 64 layout observations, and 32 runtimes; each runtime contributes at most
// four revocable identities. The five remaining identities are activation,
// manifest, target, scope, and adapter bundle. Corrupt state beyond this
// closed envelope fails rather than allocating or returning unbounded rows.
const ATS_CERTIFICATION_PAGE_MAX_EVIDENCE_PER_MANIFEST: usize = 64;
const ATS_CERTIFICATION_PAGE_MAX_LAYOUTS_PER_MANIFEST: usize = 64;
const ATS_CERTIFICATION_PAGE_MAX_RUNTIMES_PER_MANIFEST: usize = 32;
const ATS_CERTIFICATION_PAGE_MAX_REVOCATION_SUBJECTS: usize =
    (JOBS_WORKSPACE_MAX_PAGE_SIZE + 1)
        * 2
        * (5
            + ATS_CERTIFICATION_PAGE_MAX_EVIDENCE_PER_MANIFEST
            + ATS_CERTIFICATION_PAGE_MAX_LAYOUTS_PER_MANIFEST
            + (ATS_CERTIFICATION_PAGE_MAX_RUNTIMES_PER_MANIFEST * 4));

type AtsCertificationPostingPageResult =
    std::result::Result<AtsCertificationPostingResolution, AtsCertificationAuthorityError>;

pub(crate) fn resolve_ats_certification_for_postings_page_sqlite_tx(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    postings: &[JobPosting],
    now_ms: i64,
) -> Result<Vec<AtsCertificationPostingPageResult>, AtsCertificationAuthorityError> {
    if !ats_certification_text(account_id, 1, 240) {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    if postings.len() > JOBS_WORKSPACE_MAX_PAGE_SIZE + 1 {
        return Err(AtsCertificationAuthorityError::IdentityConflict);
    }
    let (targets, mut results) = ats_certification_page_targets(postings, now_ms);
    let data = load_sqlite_ats_certification_page_data(
        tx,
        account_id,
        &targets.iter().map(|(_, target)| target.clone()).collect::<Vec<_>>(),
        now_ms,
    )?;
    for (index, target) in targets {
        results[index] = Some(resolve_ats_certification_page_target(
            &data, target, now_ms,
        ));
    }
    results
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(AtsCertificationAuthorityError::IdentityConflict)
}

pub(crate) fn resolve_ats_certification_for_postings_page_postgres_tx(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    postings: &[JobPosting],
    now_ms: i64,
) -> Result<Vec<AtsCertificationPostingPageResult>, AtsCertificationAuthorityError> {
    if !ats_certification_text(account_id, 1, 240) {
        return Err(AtsCertificationAuthorityError::InvalidAuthority);
    }
    if postings.len() > JOBS_WORKSPACE_MAX_PAGE_SIZE + 1 {
        return Err(AtsCertificationAuthorityError::IdentityConflict);
    }
    let (targets, mut results) = ats_certification_page_targets(postings, now_ms);
    let data = load_postgres_ats_certification_page_data(
        tx,
        account_id,
        &targets.iter().map(|(_, target)| target.clone()).collect::<Vec<_>>(),
        now_ms,
    )?;
    for (index, target) in targets {
        results[index] = Some(resolve_ats_certification_page_target(
            &data, target, now_ms,
        ));
    }
    results
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(AtsCertificationAuthorityError::IdentityConflict)
}

fn ats_certification_page_targets(
    postings: &[JobPosting],
    now_ms: i64,
) -> (
    Vec<(usize, AtsCertificationPageTarget)>,
    Vec<Option<AtsCertificationPostingPageResult>>,
) {
    let mut targets = Vec::new();
    let mut results = (0..postings.len()).map(|_| None).collect::<Vec<_>>();
    for (index, posting) in postings.iter().enumerate() {
        let target = (|| {
            let evidence = ats_certification_fresh_target_evidence_from_posting(posting, now_ms)?;
            let (provider, target_key) =
                validate_ats_certification_fresh_target_evidence(&evidence, now_ms)?;
            let target_key_sha256 = ats_certification_target_key_sha256(&target_key)?;
            Ok(AtsCertificationPageTarget {
                key: AtsCertificationPageTargetKey {
                    provider,
                    target_key,
                },
                target_key_sha256,
                evidence,
            })
        })();
        match target {
            Ok(target) => targets.push((index, target)),
            Err(error) => results[index] = Some(Err(error)),
        }
    }
    (targets, results)
}

fn ats_certification_page_key(
    row: &StoredAtsBindingRow,
) -> (String, String, String) {
    (
        row.provider.clone(),
        row.target_key.clone(),
        row.activation.channel.clone(),
    )
}

fn ats_certification_page_unique_targets(
    targets: &[AtsCertificationPageTarget],
) -> Vec<AtsCertificationPageTargetKey> {
    targets
        .iter()
        .map(|target| target.key.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn ats_certification_page_manifest_ids(
    bindings: &BTreeMap<(String, String, String), Vec<StoredAtsBindingRow>>,
) -> Vec<String> {
    bindings
        .values()
        .flatten()
        .map(|row| row.activation.manifest_sha256.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn ats_certification_page_allowlist_ids(
    bindings: &BTreeMap<(String, String, String), Vec<StoredAtsBindingRow>>,
) -> Result<Vec<String>, AtsCertificationAuthorityError> {
    let values = bindings
        .values()
        .flatten()
        .filter_map(|row| row.activation.account_allowlist_sha256.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if values.len() > (JOBS_WORKSPACE_MAX_PAGE_SIZE + 1) * 2 {
        return Err(AtsCertificationAuthorityError::IdentityConflict);
    }
    Ok(values)
}

fn ats_certification_page_circuit_keys(
    bindings: &BTreeMap<(String, String, String), Vec<StoredAtsBindingRow>>,
    runtimes: &BTreeMap<String, Vec<(i64, AtsCertificationRuntimeTarget)>>,
) -> Result<BTreeSet<(String, String)>, AtsCertificationAuthorityError> {
    let mut values = BTreeSet::new();
    for row in bindings.values().flatten() {
        values.extend([
            ("provider".to_string(), row.provider.clone()),
            ("target".to_string(), row.target_key.clone()),
            ("adapter".to_string(), row.adapter_bundle_sha256.clone()),
            (
                "activation".to_string(),
                row.activation.activation_sha256.clone(),
            ),
        ]);
        if let Some(manifest_runtimes) = runtimes.get(&row.activation.manifest_sha256) {
            for (_, runtime) in manifest_runtimes {
                values.insert(("runtime".to_string(), runtime.runtime_id.clone()));
                values.insert(("runtime".to_string(), runtime.runtime_sha256.clone()));
            }
        }
    }
    if values.len() > ATS_CERTIFICATION_PAGE_MAX_REVOCATION_SUBJECTS {
        return Err(AtsCertificationAuthorityError::IdentityConflict);
    }
    Ok(values)
}

fn ats_certification_page_quarantine_keys(
    bindings: &BTreeMap<(String, String, String), Vec<StoredAtsBindingRow>>,
    runtimes: &BTreeMap<String, Vec<(i64, AtsCertificationRuntimeTarget)>>,
) -> Result<BTreeSet<(String, String, String)>, AtsCertificationAuthorityError> {
    let mut values = BTreeSet::new();
    for row in bindings.values().flatten() {
        let manifest = (
            row.provider.clone(),
            row.target_key.clone(),
            row.variant_key.clone(),
            row.activation.scope_sha256.clone(),
            row.adapter_version.clone(),
            row.adapter_bundle_sha256.clone(),
        );
        values.extend(ats_base_quarantine_targets(&manifest, &row.activation)?);
        if let Some(manifest_runtimes) = runtimes.get(&row.activation.manifest_sha256) {
            for (_, runtime) in manifest_runtimes {
                values.insert((
                    "runtime".to_string(),
                    runtime.runtime_id.clone(),
                    runtime.runtime_sha256.clone(),
                ));
            }
        }
    }
    if values.len() > ATS_CERTIFICATION_PAGE_MAX_REVOCATION_SUBJECTS {
        return Err(AtsCertificationAuthorityError::IdentityConflict);
    }
    Ok(values)
}

fn ats_certification_page_revocation_subjects(
    bindings: &BTreeMap<(String, String, String), Vec<StoredAtsBindingRow>>,
    evidence: &BTreeMap<String, Vec<(i64, String)>>,
    layouts: &BTreeMap<String, Vec<(i64, String, String)>>,
    runtimes: &BTreeMap<String, Vec<(i64, AtsCertificationRuntimeTarget)>>,
) -> Result<BTreeSet<AtsCertificationPageRevocationKey>, AtsCertificationAuthorityError> {
    let mut subjects = BTreeSet::<(String, String, String)>::new();
    for row in bindings.values().flatten() {
        subjects.extend([
            (
                "activation".to_string(),
                row.activation.activation_id.clone(),
                row.activation.activation_sha256.clone(),
            ),
            (
                "manifest".to_string(),
                row.certification_id.clone(),
                row.activation.manifest_sha256.clone(),
            ),
            ("target".to_string(), row.target_key.clone(), String::new()),
            (
                "scope".to_string(),
                row.activation.scope_sha256.clone(),
                row.activation.scope_sha256.clone(),
            ),
            (
                "adapter_bundle".to_string(),
                row.adapter_version.clone(),
                row.adapter_bundle_sha256.clone(),
            ),
        ]);
        if let Some(values) = evidence.get(&row.activation.manifest_sha256) {
            subjects.extend(values.iter().map(|(_, sha256)| {
                ("evidence".to_string(), String::new(), sha256.clone())
            }));
        }
        if let Some(values) = layouts.get(&row.activation.manifest_sha256) {
            subjects.extend(values.iter().map(|(_, id, sha256)| {
                ("layout_observation".to_string(), id.clone(), sha256.clone())
            }));
        }
        if let Some(values) = runtimes.get(&row.activation.manifest_sha256) {
            for (_, runtime) in values {
                subjects.insert((
                    "runtime".to_string(),
                    runtime.runtime_id.clone(),
                    runtime.runtime_sha256.clone(),
                ));
                if let Some(value) = runtime.browser_release_manifest_sha256.as_ref() {
                    subjects.insert((
                        "browser_release_manifest".to_string(),
                        value.clone(),
                        value.clone(),
                    ));
                }
                if let Some(value) = runtime.runner_image_sha256.as_ref() {
                    subjects.insert((
                        "runner_image".to_string(),
                        value.clone(),
                        value.clone(),
                    ));
                }
                if let Some(value) = runtime.runner_build_id.as_ref() {
                    subjects.insert((
                        "runner_build".to_string(),
                        value.clone(),
                        ats_certification_sha256(value.as_bytes()),
                    ));
                }
            }
        }
    }
    if subjects.len() > ATS_CERTIFICATION_PAGE_MAX_REVOCATION_SUBJECTS {
        return Err(AtsCertificationAuthorityError::IdentityConflict);
    }
    Ok(subjects)
}

fn validate_ats_certification_page_child_cardinality(
    evidence: &BTreeMap<String, Vec<(i64, String)>>,
    layouts: &BTreeMap<String, Vec<(i64, String, String)>>,
    runtimes: &BTreeMap<String, Vec<(i64, AtsCertificationRuntimeTarget)>>,
) -> Result<(), AtsCertificationAuthorityError> {
    if evidence
        .values()
        .any(|values| values.len() > ATS_CERTIFICATION_PAGE_MAX_EVIDENCE_PER_MANIFEST)
        || layouts
            .values()
            .any(|values| values.len() > ATS_CERTIFICATION_PAGE_MAX_LAYOUTS_PER_MANIFEST)
        || runtimes
            .values()
            .any(|values| values.len() > ATS_CERTIFICATION_PAGE_MAX_RUNTIMES_PER_MANIFEST)
    {
        return Err(AtsCertificationAuthorityError::IdentityConflict);
    }
    Ok(())
}

fn validate_ats_certification_page_signed_child_counts(
    bindings: &BTreeMap<(String, String, String), Vec<StoredAtsBindingRow>>,
    evidence: &BTreeMap<String, Vec<(i64, String)>>,
    layouts: &BTreeMap<String, Vec<(i64, String, String)>>,
    runtimes: &BTreeMap<String, Vec<(i64, AtsCertificationRuntimeTarget)>>,
) -> Result<(), AtsCertificationAuthorityError> {
    for row in bindings.values().flatten() {
        validate_ats_binding_child_counts(
            row,
            evidence
                .get(&row.activation.manifest_sha256)
                .map_or(0, Vec::len),
            layouts
                .get(&row.activation.manifest_sha256)
                .map_or(0, Vec::len),
            runtimes
                .get(&row.activation.manifest_sha256)
                .map_or(0, Vec::len),
        )?;
    }
    Ok(())
}

fn validate_ats_certification_page_child_ordinal(
    current_len: usize,
    ordinal: i64,
    maximum: usize,
) -> Result<(), AtsCertificationAuthorityError> {
    if current_len >= maximum || ordinal != current_len as i64 {
        return Err(AtsCertificationAuthorityError::IdentityConflict);
    }
    Ok(())
}

fn sqlite_page_placeholders(start: usize, count: usize) -> String {
    (0..count)
        .map(|offset| format!("?{}", start + offset))
        .collect::<Vec<_>>()
        .join(",")
}

const ATS_CERTIFICATION_PAGE_BINDING_COLUMNS: &str =
    "activation.activation_sha256, activation.activation_id,
     activation.activation_generation, activation.manifest_sha256,
     activation.scope_sha256, activation.channel,
     activation.channel_sequence, activation.capability,
     activation.account_allowlist_sha256,
     activation.canary_max_submissions, activation.canary_account_cap,
     activation.canary_concurrency_cap,
     activation.canary_daily_side_effect_cap,
     activation.not_before_ms, activation.expires_at_ms,
     manifest.provider, manifest.target_key, manifest.variant_key,
     manifest.surface_sha256, manifest.certification_id,
     manifest.manifest_generation, manifest.adapter_version,
     manifest.adapter_bundle_sha256, manifest.source_commit,
     manifest.layout_contract_version, manifest.layout_contract_sha256,
     manifest.not_before_ms, manifest.expires_at_ms, head.head_revision,
     head.current_transition_sha256, head.current_activation_sha256,
     head.current_channel_sequence, head.updated_by,
     activation.trust_policy_sha256, manifest.layout_set_sha256,
     manifest.allowed_provider_hosts_json,
     manifest.final_submit_control_id, manifest.tested_at_ms,
     manifest.layout_observation_count, manifest.evidence_count,
     manifest.runtime_target_count";

fn load_sqlite_ats_certification_page_data(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    targets: &[AtsCertificationPageTarget],
    now_ms: i64,
) -> Result<AtsCertificationPageData, AtsCertificationAuthorityError> {
    let unique_targets = ats_certification_page_unique_targets(targets);
    if unique_targets.len() > JOBS_WORKSPACE_MAX_PAGE_SIZE + 1 {
        return Err(AtsCertificationAuthorityError::IdentityConflict);
    }
    if unique_targets.is_empty() {
        return Ok(AtsCertificationPageData::default());
    }
    let trust = tx
        .query_row(
            "SELECT head.current_policy_sha256, policy.valid_from_ms, policy.expires_at_ms
               FROM jobs_ats_certification_trust_head head
               JOIN jobs_ats_certification_trust_policies policy
                 ON policy.policy_sha256=head.current_policy_sha256
              WHERE head.singleton_id=1",
            [],
            |row| {
                Ok(AtsCertificationPageTrust {
                    policy_sha256: row.get(0)?,
                    valid_from_ms: row.get(1)?,
                    expires_at_ms: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(ats_certification_storage)?;

    let pair_predicate = unique_targets
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let first = index * 2 + 1;
            format!(
                "(manifest.provider=?{first} AND manifest.target_key=?{})",
                first + 1
            )
        })
        .collect::<Vec<_>>()
        .join(" OR ");
    let binding_limit = unique_targets.len().saturating_mul(2).saturating_add(1);
    let sql = format!(
        "SELECT {ATS_CERTIFICATION_PAGE_BINDING_COLUMNS}, manifest.trust_policy_sha256
           FROM jobs_ats_certification_heads head
           JOIN jobs_ats_certification_activations activation
             ON activation.activation_sha256=head.current_activation_sha256
            AND activation.channel=head.channel
            AND activation.scope_sha256=head.scope_sha256
           JOIN jobs_ats_certification_manifests manifest
             ON manifest.manifest_sha256=activation.manifest_sha256
          WHERE head.channel IN ('general','canary') AND ({pair_predicate})
          ORDER BY manifest.provider, manifest.target_key, head.channel,
                   manifest.manifest_generation DESC
          LIMIT {binding_limit}"
    );
    let pair_values = unique_targets.iter().flat_map(|target| {
        [target.provider.as_str(), target.target_key.as_str()]
    });
    let mut statement = tx.prepare(&sql).map_err(ats_certification_storage)?;
    let rows = statement
        .query_map(rusqlite::params_from_iter(pair_values), |row| {
            Ok((sqlite_ats_binding_row(row)?, row.get::<_, String>(41)?))
        })
        .map_err(ats_certification_storage)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(ats_certification_storage)?;
    if rows.len() > unique_targets.len().saturating_mul(2) {
        return Err(AtsCertificationAuthorityError::IdentityConflict);
    }
    let mut bindings = BTreeMap::<_, Vec<_>>::new();
    let mut manifest_policies = BTreeMap::new();
    for (row, manifest_policy) in rows {
        match manifest_policies.entry(row.activation.manifest_sha256.clone()) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(manifest_policy);
            }
            std::collections::btree_map::Entry::Occupied(entry)
                if entry.get() != &manifest_policy =>
            {
                return Err(AtsCertificationAuthorityError::IdentityConflict);
            }
            std::collections::btree_map::Entry::Occupied(_) => {}
        }
        bindings
            .entry(ats_certification_page_key(&row))
            .or_default()
            .push(row);
    }

    let account_canary_allowlists = load_sqlite_ats_certification_page_allowlists(
        tx,
        account_id,
        &ats_certification_page_allowlist_ids(&bindings)?,
        now_ms,
    )?;

    let manifests = ats_certification_page_manifest_ids(&bindings);
    let (evidence, layouts, runtimes) =
        load_sqlite_ats_certification_page_children(tx, &manifests)?;
    validate_ats_certification_page_child_cardinality(&evidence, &layouts, &runtimes)?;
    validate_ats_certification_page_signed_child_counts(
        &bindings, &evidence, &layouts, &runtimes,
    )?;
    let policy_sha256 = trust.as_ref().map(|value| value.policy_sha256.as_str());
    let revocation_subjects =
        ats_certification_page_revocation_subjects(&bindings, &evidence, &layouts, &runtimes)?;
    let (global_revoked, revocations) = load_sqlite_ats_certification_page_revocations(
        tx,
        policy_sha256,
        &revocation_subjects,
        now_ms,
    )?;
    let quarantines = load_sqlite_ats_certification_page_quarantines(
        tx,
        policy_sha256,
        &ats_certification_page_quarantine_keys(&bindings, &runtimes)?,
    )?;
    let circuits = load_sqlite_ats_certification_page_circuits(
        tx,
        &ats_certification_page_circuit_keys(&bindings, &runtimes)?,
    )?;
    Ok(AtsCertificationPageData {
        trust,
        bindings,
        manifest_policies,
        account_canary_allowlists,
        evidence,
        layouts,
        runtimes,
        global_revoked,
        revocations,
        quarantines,
        circuits,
    })
}

type AtsCertificationPageChildren = (
    BTreeMap<String, Vec<(i64, String)>>,
    BTreeMap<String, Vec<(i64, String, String)>>,
    BTreeMap<String, Vec<(i64, AtsCertificationRuntimeTarget)>>,
);

fn load_sqlite_ats_certification_page_children(
    tx: &rusqlite::Transaction<'_>,
    manifests: &[String],
) -> Result<AtsCertificationPageChildren, AtsCertificationAuthorityError> {
    if manifests.is_empty() {
        return Ok((BTreeMap::new(), BTreeMap::new(), BTreeMap::new()));
    }
    let placeholders = sqlite_page_placeholders(1, manifests.len());
    let values = manifests.iter().map(String::as_str).collect::<Vec<_>>();
    let evidence_limit = manifests
        .len()
        .saturating_mul(ATS_CERTIFICATION_PAGE_MAX_EVIDENCE_PER_MANIFEST)
        .saturating_add(1);
    let layout_limit = manifests
        .len()
        .saturating_mul(ATS_CERTIFICATION_PAGE_MAX_LAYOUTS_PER_MANIFEST)
        .saturating_add(1);
    let runtime_limit = manifests
        .len()
        .saturating_mul(ATS_CERTIFICATION_PAGE_MAX_RUNTIMES_PER_MANIFEST)
        .saturating_add(1);

    let mut evidence = BTreeMap::<String, Vec<(i64, String)>>::new();
    let sql = format!(
        "SELECT manifest_sha256, ordinal, evidence_sha256
           FROM jobs_ats_certification_manifest_evidence
          WHERE manifest_sha256 IN ({placeholders})
          ORDER BY manifest_sha256, ordinal
          LIMIT {evidence_limit}"
    );
    let rows = tx
        .prepare(&sql)
        .map_err(ats_certification_storage)?
        .query_map(rusqlite::params_from_iter(values.iter().copied()), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(ats_certification_storage)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(ats_certification_storage)?;
    for (manifest, ordinal, sha256) in rows {
        let values = evidence.entry(manifest).or_default();
        validate_ats_certification_page_child_ordinal(
            values.len(),
            ordinal,
            ATS_CERTIFICATION_PAGE_MAX_EVIDENCE_PER_MANIFEST,
        )?;
        values.push((ordinal, sha256));
    }

    let mut layouts = BTreeMap::<String, Vec<(i64, String, String)>>::new();
    let sql = format!(
        "SELECT binding.manifest_sha256, binding.ordinal,
                observation.observation_id, binding.observation_sha256
           FROM jobs_ats_certification_manifest_layouts binding
           JOIN jobs_ats_certification_layout_observations observation
             ON observation.observation_sha256=binding.observation_sha256
          WHERE binding.manifest_sha256 IN ({placeholders})
          ORDER BY binding.manifest_sha256, binding.ordinal
          LIMIT {layout_limit}"
    );
    let rows = tx
        .prepare(&sql)
        .map_err(ats_certification_storage)?
        .query_map(rusqlite::params_from_iter(values.iter().copied()), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(ats_certification_storage)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(ats_certification_storage)?;
    for (manifest, ordinal, observation_id, sha256) in rows {
        let values = layouts.entry(manifest).or_default();
        validate_ats_certification_page_child_ordinal(
            values.len(),
            ordinal,
            ATS_CERTIFICATION_PAGE_MAX_LAYOUTS_PER_MANIFEST,
        )?;
        values.push((ordinal, observation_id, sha256));
    }

    let mut runtimes = BTreeMap::<String, Vec<(i64, AtsCertificationRuntimeTarget)>>::new();
    let sql = format!(
        "SELECT manifest_sha256, ordinal, runtime_kind, runtime_id, runtime_sha256,
                platform, architecture, automation_bundle_sha256,
                browser_release_manifest_sha256, browser_artifact_sha256,
                browser_build_descriptor_sha256, runner_build_id, runner_image_sha256,
                playwright_version, chromium_revision, chromium_executable_sha256
           FROM jobs_ats_certification_runtime_targets
          WHERE manifest_sha256 IN ({placeholders})
          ORDER BY manifest_sha256, ordinal
          LIMIT {runtime_limit}"
    );
    let rows = tx
        .prepare(&sql)
        .map_err(ats_certification_storage)?
        .query_map(rusqlite::params_from_iter(values), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                AtsCertificationRuntimeTarget {
                    runtime_kind: row.get(2)?,
                    runtime_id: row.get(3)?,
                    runtime_sha256: row.get(4)?,
                    platform: row.get(5)?,
                    architecture: row.get(6)?,
                    automation_bundle_sha256: row.get(7)?,
                    browser_release_manifest_sha256: row.get(8)?,
                    browser_artifact_sha256: row.get(9)?,
                    browser_build_descriptor_sha256: row.get(10)?,
                    runner_build_id: row.get(11)?,
                    runner_image_sha256: row.get(12)?,
                    playwright_version: row.get(13)?,
                    chromium_revision: row.get(14)?,
                    chromium_executable_sha256: row.get(15)?,
                },
            ))
        })
        .map_err(ats_certification_storage)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(ats_certification_storage)?;
    for (manifest, ordinal, runtime) in rows {
        let values = runtimes.entry(manifest).or_default();
        validate_ats_certification_page_child_ordinal(
            values.len(),
            ordinal,
            ATS_CERTIFICATION_PAGE_MAX_RUNTIMES_PER_MANIFEST,
        )?;
        values.push((ordinal, runtime));
    }
    Ok((evidence, layouts, runtimes))
}

fn load_sqlite_ats_certification_page_revocations(
    tx: &rusqlite::Transaction<'_>,
    policy_sha256: Option<&str>,
    subjects: &BTreeSet<AtsCertificationPageRevocationKey>,
    now_ms: i64,
) -> Result<(bool, BTreeSet<AtsCertificationPageRevocationKey>), AtsCertificationAuthorityError> {
    let Some(policy_sha256) = policy_sha256 else {
        return Ok((false, BTreeSet::new()));
    };
    let global_revoked = tx
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM jobs_ats_certification_revocations revocation
                WHERE revocation.trust_policy_sha256=?1
                  AND revocation.effective_at_ms<=?2 AND (
                    (revocation.subject_kind='policy'
                      AND revocation.subject_sha256=?1)
                    OR (revocation.subject_kind='trust_key' AND EXISTS (
                      SELECT 1 FROM jobs_ats_certification_trust_keys trust_key
                       WHERE trust_key.policy_sha256=?1
                         AND trust_key.key_id=revocation.subject_id
                    ))
                  )
             )",
            params![policy_sha256, now_ms],
            |row| row.get::<_, bool>(0),
        )
        .map_err(ats_certification_storage)?;
    if subjects.is_empty() {
        return Ok((global_revoked, BTreeSet::new()));
    }
    let requested = serde_json::to_string(
        &subjects
            .iter()
            .map(|(subject_kind, subject_id, subject_sha256)| {
                [
                    subject_kind.as_str(),
                    subject_id.as_str(),
                    subject_sha256.as_str(),
                ]
            })
            .collect::<Vec<_>>(),
    )
    .map_err(ats_certification_storage)?;
    let revocations = tx
        .prepare(
            "SELECT requested.subject_kind,requested.subject_id,
                    requested.subject_sha256
               FROM (
                 SELECT json_extract(value,'$[0]') AS subject_kind,
                        json_extract(value,'$[1]') AS subject_id,
                        json_extract(value,'$[2]') AS subject_sha256
                   FROM json_each(?3)
               ) requested
              WHERE EXISTS (
                SELECT 1 FROM jobs_ats_certification_revocations revocation
                 WHERE revocation.trust_policy_sha256=?1
                   AND revocation.effective_at_ms<=?2
                   AND revocation.subject_kind=requested.subject_kind
                   AND (
                     (requested.subject_kind='target'
                       AND revocation.subject_id=requested.subject_id)
                     OR (requested.subject_kind='evidence'
                       AND revocation.subject_sha256=requested.subject_sha256)
                     OR (requested.subject_kind NOT IN ('target','evidence')
                       AND revocation.subject_id=requested.subject_id
                       AND revocation.subject_sha256=requested.subject_sha256)
                   )
              )
              ORDER BY requested.subject_kind,requested.subject_id,
                       requested.subject_sha256"
        )
        .map_err(ats_certification_storage)?
        .query_map(params![policy_sha256, now_ms, requested], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map_err(ats_certification_storage)?
        .collect::<std::result::Result<BTreeSet<_>, _>>()
        .map_err(ats_certification_storage)?;
    if revocations.len() > subjects.len() {
        return Err(AtsCertificationAuthorityError::IdentityConflict);
    }
    Ok((global_revoked, revocations))
}

fn load_sqlite_ats_certification_page_allowlists(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    allowlist_ids: &[String],
    now_ms: i64,
) -> Result<BTreeSet<String>, AtsCertificationAuthorityError> {
    if allowlist_ids.is_empty() {
        return Ok(BTreeSet::new());
    }
    let requested = serde_json::to_string(allowlist_ids).map_err(ats_certification_storage)?;
    tx.prepare(
        "SELECT requested.allowlist_sha256
           FROM (
             SELECT value AS allowlist_sha256 FROM json_each(?3)
           ) requested
           JOIN jobs_ats_certification_canary_allowlists allowlist
             ON allowlist.allowlist_sha256=requested.allowlist_sha256
           JOIN jobs_ats_certification_canary_allowlist_members member
             ON member.allowlist_sha256=requested.allowlist_sha256
            AND member.account_id=?1
          WHERE allowlist.not_before_ms<=?2 AND allowlist.expires_at_ms>?2
            AND NOT EXISTS (
              SELECT 1 FROM jobs_ats_certification_canary_allowlist_revocations revocation
               WHERE revocation.allowlist_sha256=requested.allowlist_sha256
                 AND revocation.revoked_at_ms<=?2
            )
          ORDER BY requested.allowlist_sha256",
    )
    .map_err(ats_certification_storage)?
    .query_map(params![account_id, now_ms, requested], |row| row.get(0))
    .map_err(ats_certification_storage)?
    .collect::<std::result::Result<BTreeSet<_>, _>>()
    .map_err(ats_certification_storage)
}

fn load_sqlite_ats_certification_page_quarantines(
    tx: &rusqlite::Transaction<'_>,
    policy_sha256: Option<&str>,
    keys: &BTreeSet<(String, String, String)>,
) -> Result<BTreeSet<(String, String, String, String)>, AtsCertificationAuthorityError> {
    let Some(policy_sha256) = policy_sha256 else {
        return Ok(BTreeSet::new());
    };
    if keys.is_empty() {
        return Ok(BTreeSet::new());
    }
    let requested = serde_json::to_string(
        &keys
            .iter()
            .map(|(kind, id, sha256)| [kind.as_str(), id.as_str(), sha256.as_str()])
            .collect::<Vec<_>>(),
    )
    .map_err(ats_certification_storage)?;
    tx.prepare(
        "SELECT head.scope_kind, head.scope_id, head.scope_sha256,
                command.trust_policy_sha256
           FROM (
             SELECT json_extract(value,'$[0]') AS scope_kind,
                    json_extract(value,'$[1]') AS scope_id,
                    json_extract(value,'$[2]') AS scope_sha256
               FROM json_each(?2)
           ) requested
           JOIN jobs_ats_certification_quarantine_heads head
             ON head.scope_kind=requested.scope_kind
            AND head.scope_id=requested.scope_id
            AND head.scope_sha256=requested.scope_sha256
           JOIN jobs_ats_certification_quarantine_commands command
             ON command.command_sha256=head.current_command_sha256
          WHERE head.state='quarantined' AND command.trust_policy_sha256=?1
          ORDER BY head.scope_kind,head.scope_id,head.scope_sha256"
    )
    .map_err(ats_certification_storage)?
    .query_map(params![policy_sha256, requested], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
    })
    .map_err(ats_certification_storage)?
    .collect::<std::result::Result<BTreeSet<_>, _>>()
    .map_err(ats_certification_storage)
}

fn load_sqlite_ats_certification_page_circuits(
    tx: &rusqlite::Transaction<'_>,
    keys: &BTreeSet<(String, String)>,
) -> Result<BTreeSet<(String, String)>, AtsCertificationAuthorityError> {
    if keys.is_empty() {
        return Ok(BTreeSet::new());
    }
    let requested = serde_json::to_string(
        &keys
            .iter()
            .map(|(kind, key)| [kind.as_str(), key.as_str()])
            .collect::<Vec<_>>(),
    )
    .map_err(ats_certification_storage)?;
    tx.prepare(
        "SELECT head.scope_kind, head.subject_key
           FROM (
             SELECT json_extract(value,'$[0]') AS scope_kind,
                    json_extract(value,'$[1]') AS subject_key
               FROM json_each(?1)
           ) requested
           JOIN jobs_ats_certification_circuit_heads head
             ON head.scope_kind=requested.scope_kind
            AND head.subject_key=requested.subject_key
          WHERE head.state<>'closed'
          ORDER BY head.scope_kind,head.subject_key"
    )
    .map_err(ats_certification_storage)?
    .query_map(params![requested], |row| Ok((row.get(0)?, row.get(1)?)))
    .map_err(ats_certification_storage)?
    .collect::<std::result::Result<BTreeSet<_>, _>>()
    .map_err(ats_certification_storage)
}

fn load_postgres_ats_certification_page_data(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    targets: &[AtsCertificationPageTarget],
    now_ms: i64,
) -> Result<AtsCertificationPageData, AtsCertificationAuthorityError> {
    let unique_targets = ats_certification_page_unique_targets(targets);
    if unique_targets.len() > JOBS_WORKSPACE_MAX_PAGE_SIZE + 1 {
        return Err(AtsCertificationAuthorityError::IdentityConflict);
    }
    if unique_targets.is_empty() {
        return Ok(AtsCertificationPageData::default());
    }
    let trust = tx
        .query_opt(
            "SELECT head.current_policy_sha256, policy.valid_from_ms, policy.expires_at_ms
               FROM jobs_ats_certification_trust_head head
               JOIN jobs_ats_certification_trust_policies policy
                 ON policy.policy_sha256=head.current_policy_sha256
              WHERE head.singleton_id=1
              FOR SHARE OF head,policy",
            &[],
        )
        .map_err(ats_certification_storage)?
        .map(|row| AtsCertificationPageTrust {
            policy_sha256: row.get(0),
            valid_from_ms: row.get(1),
            expires_at_ms: row.get(2),
        });
    let providers = unique_targets
        .iter()
        .map(|target| target.provider.clone())
        .collect::<Vec<_>>();
    let target_keys = unique_targets
        .iter()
        .map(|target| target.target_key.clone())
        .collect::<Vec<_>>();
    let binding_limit = unique_targets.len().saturating_mul(2).saturating_add(1) as i64;
    let sql = format!(
        "SELECT {ATS_CERTIFICATION_PAGE_BINDING_COLUMNS}, manifest.trust_policy_sha256
           FROM jobs_ats_certification_heads head
           JOIN jobs_ats_certification_activations activation
             ON activation.activation_sha256=head.current_activation_sha256
            AND activation.channel=head.channel
            AND activation.scope_sha256=head.scope_sha256
           JOIN jobs_ats_certification_manifests manifest
             ON manifest.manifest_sha256=activation.manifest_sha256
           JOIN unnest($1::TEXT[], $2::TEXT[]) requested(provider,target_key)
             ON requested.provider=manifest.provider
            AND requested.target_key=manifest.target_key
          WHERE head.channel IN ('general','canary')
          ORDER BY manifest.provider, manifest.target_key, head.channel,
                   manifest.manifest_generation DESC
          LIMIT $3 FOR SHARE OF head,activation,manifest"
    );
    let mut bindings = BTreeMap::<_, Vec<_>>::new();
    let mut manifest_policies = BTreeMap::new();
    let rows = tx
        .query(&sql, &[&providers, &target_keys, &binding_limit])
        .map_err(ats_certification_storage)?;
    if rows.len() > unique_targets.len().saturating_mul(2) {
        return Err(AtsCertificationAuthorityError::IdentityConflict);
    }
    for row in rows {
        let manifest_policy: String = row.get(41);
        let row = postgres_ats_binding_row(row);
        match manifest_policies.entry(row.activation.manifest_sha256.clone()) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(manifest_policy);
            }
            std::collections::btree_map::Entry::Occupied(entry)
                if entry.get() != &manifest_policy =>
            {
                return Err(AtsCertificationAuthorityError::IdentityConflict);
            }
            std::collections::btree_map::Entry::Occupied(_) => {}
        }
        bindings
            .entry(ats_certification_page_key(&row))
            .or_default()
            .push(row);
    }
    let account_canary_allowlists = load_postgres_ats_certification_page_allowlists(
        tx,
        account_id,
        &ats_certification_page_allowlist_ids(&bindings)?,
        now_ms,
    )?;
    let manifests = ats_certification_page_manifest_ids(&bindings);
    let (evidence, layouts, runtimes) =
        load_postgres_ats_certification_page_children(tx, &manifests)?;
    validate_ats_certification_page_child_cardinality(&evidence, &layouts, &runtimes)?;
    validate_ats_certification_page_signed_child_counts(
        &bindings, &evidence, &layouts, &runtimes,
    )?;
    let policy_sha256 = trust.as_ref().map(|value| value.policy_sha256.as_str());
    let revocation_subjects =
        ats_certification_page_revocation_subjects(&bindings, &evidence, &layouts, &runtimes)?;
    let (global_revoked, revocations) = load_postgres_ats_certification_page_revocations(
        tx,
        policy_sha256,
        &revocation_subjects,
        now_ms,
    )?;
    let quarantines = load_postgres_ats_certification_page_quarantines(
        tx,
        policy_sha256,
        &ats_certification_page_quarantine_keys(&bindings, &runtimes)?,
    )?;
    let circuits = load_postgres_ats_certification_page_circuits(
        tx,
        &ats_certification_page_circuit_keys(&bindings, &runtimes)?,
    )?;
    Ok(AtsCertificationPageData {
        trust,
        bindings,
        manifest_policies,
        account_canary_allowlists,
        evidence,
        layouts,
        runtimes,
        global_revoked,
        revocations,
        quarantines,
        circuits,
    })
}

fn load_postgres_ats_certification_page_children(
    tx: &mut postgres::Transaction<'_>,
    manifests: &[String],
) -> Result<AtsCertificationPageChildren, AtsCertificationAuthorityError> {
    if manifests.is_empty() {
        return Ok((BTreeMap::new(), BTreeMap::new(), BTreeMap::new()));
    }
    let evidence_limit = manifests
        .len()
        .saturating_mul(ATS_CERTIFICATION_PAGE_MAX_EVIDENCE_PER_MANIFEST)
        .saturating_add(1) as i64;
    let layout_limit = manifests
        .len()
        .saturating_mul(ATS_CERTIFICATION_PAGE_MAX_LAYOUTS_PER_MANIFEST)
        .saturating_add(1) as i64;
    let runtime_limit = manifests
        .len()
        .saturating_mul(ATS_CERTIFICATION_PAGE_MAX_RUNTIMES_PER_MANIFEST)
        .saturating_add(1) as i64;
    let mut evidence = BTreeMap::<String, Vec<(i64, String)>>::new();
    for row in tx
        .query(
            "SELECT manifest_sha256, ordinal, evidence_sha256
               FROM jobs_ats_certification_manifest_evidence
              WHERE manifest_sha256=ANY($1)
              ORDER BY manifest_sha256, ordinal
              LIMIT $2 FOR SHARE",
            &[&manifests, &evidence_limit],
        )
        .map_err(ats_certification_storage)?
    {
        let manifest: String = row.get(0);
        let ordinal: i64 = row.get(1);
        let values = evidence.entry(manifest).or_default();
        validate_ats_certification_page_child_ordinal(
            values.len(),
            ordinal,
            ATS_CERTIFICATION_PAGE_MAX_EVIDENCE_PER_MANIFEST,
        )?;
        values.push((ordinal, row.get(2)));
    }
    let mut layouts = BTreeMap::<String, Vec<(i64, String, String)>>::new();
    for row in tx
        .query(
            "SELECT binding.manifest_sha256, binding.ordinal,
                    observation.observation_id, binding.observation_sha256
               FROM jobs_ats_certification_manifest_layouts binding
               JOIN jobs_ats_certification_layout_observations observation
                 ON observation.observation_sha256=binding.observation_sha256
              WHERE binding.manifest_sha256=ANY($1)
              ORDER BY binding.manifest_sha256, binding.ordinal
              LIMIT $2 FOR SHARE OF binding,observation",
            &[&manifests, &layout_limit],
        )
        .map_err(ats_certification_storage)?
    {
        let manifest: String = row.get(0);
        let ordinal: i64 = row.get(1);
        let values = layouts.entry(manifest).or_default();
        validate_ats_certification_page_child_ordinal(
            values.len(),
            ordinal,
            ATS_CERTIFICATION_PAGE_MAX_LAYOUTS_PER_MANIFEST,
        )?;
        values.push((ordinal, row.get(2), row.get(3)));
    }
    let mut runtimes = BTreeMap::<String, Vec<(i64, AtsCertificationRuntimeTarget)>>::new();
    for row in tx
        .query(
            "SELECT manifest_sha256, ordinal, runtime_kind, runtime_id, runtime_sha256,
                    platform, architecture, automation_bundle_sha256,
                    browser_release_manifest_sha256, browser_artifact_sha256,
                    browser_build_descriptor_sha256, runner_build_id, runner_image_sha256,
                    playwright_version, chromium_revision, chromium_executable_sha256
               FROM jobs_ats_certification_runtime_targets
              WHERE manifest_sha256=ANY($1)
              ORDER BY manifest_sha256, ordinal
              LIMIT $2 FOR SHARE",
            &[&manifests, &runtime_limit],
        )
        .map_err(ats_certification_storage)?
    {
        let manifest: String = row.get(0);
        let ordinal: i64 = row.get(1);
        let runtime = AtsCertificationRuntimeTarget {
            runtime_kind: row.get(2),
            runtime_id: row.get(3),
            runtime_sha256: row.get(4),
            platform: row.get(5),
            architecture: row.get(6),
            automation_bundle_sha256: row.get(7),
            browser_release_manifest_sha256: row.get(8),
            browser_artifact_sha256: row.get(9),
            browser_build_descriptor_sha256: row.get(10),
            runner_build_id: row.get(11),
            runner_image_sha256: row.get(12),
            playwright_version: row.get(13),
            chromium_revision: row.get(14),
            chromium_executable_sha256: row.get(15),
        };
        let values = runtimes.entry(manifest).or_default();
        validate_ats_certification_page_child_ordinal(
            values.len(),
            ordinal,
            ATS_CERTIFICATION_PAGE_MAX_RUNTIMES_PER_MANIFEST,
        )?;
        values.push((ordinal, runtime));
    }
    Ok((evidence, layouts, runtimes))
}

fn load_postgres_ats_certification_page_revocations(
    tx: &mut postgres::Transaction<'_>,
    policy_sha256: Option<&str>,
    subjects: &BTreeSet<AtsCertificationPageRevocationKey>,
    now_ms: i64,
) -> Result<(bool, BTreeSet<AtsCertificationPageRevocationKey>), AtsCertificationAuthorityError> {
    let Some(policy_sha256) = policy_sha256 else {
        return Ok((false, BTreeSet::new()));
    };
    let global_revoked = tx
        .query_opt(
            "SELECT 1 FROM jobs_ats_certification_revocations revocation
              WHERE revocation.trust_policy_sha256=$1
                AND revocation.effective_at_ms<=$2 AND (
                  (revocation.subject_kind='policy'
                    AND revocation.subject_sha256=$1)
                  OR (revocation.subject_kind='trust_key' AND EXISTS (
                    SELECT 1 FROM jobs_ats_certification_trust_keys trust_key
                     WHERE trust_key.policy_sha256=$1
                       AND trust_key.key_id=revocation.subject_id
                  ))
                )
              ORDER BY revocation.revocation_generation
              LIMIT 1 FOR SHARE OF revocation",
            &[&policy_sha256, &now_ms],
        )
        .map_err(ats_certification_storage)?
        .is_some();
    if subjects.is_empty() {
        return Ok((global_revoked, BTreeSet::new()));
    }
    let subject_kinds = subjects
        .iter()
        .map(|(subject_kind, _, _)| subject_kind.clone())
        .collect::<Vec<_>>();
    let subject_ids = subjects
        .iter()
        .map(|(_, subject_id, _)| subject_id.clone())
        .collect::<Vec<_>>();
    let subject_sha256s = subjects
        .iter()
        .map(|(_, _, subject_sha256)| subject_sha256.clone())
        .collect::<Vec<_>>();
    let revocations = tx
        .query(
            "SELECT requested.subject_kind,requested.subject_id,
                    requested.subject_sha256
               FROM unnest($3::TEXT[],$4::TEXT[],$5::TEXT[])
                    requested(subject_kind,subject_id,subject_sha256)
               JOIN LATERAL (
                SELECT 1 FROM jobs_ats_certification_revocations revocation
                 WHERE revocation.trust_policy_sha256=$1
                   AND revocation.effective_at_ms<=$2
                   AND revocation.subject_kind=requested.subject_kind
                   AND (
                     (requested.subject_kind='target'
                       AND revocation.subject_id=requested.subject_id)
                     OR (requested.subject_kind='evidence'
                       AND revocation.subject_sha256=requested.subject_sha256)
                     OR (requested.subject_kind NOT IN ('target','evidence')
                       AND revocation.subject_id=requested.subject_id
                       AND revocation.subject_sha256=requested.subject_sha256)
                   )
                 LIMIT 1 FOR SHARE OF revocation
               ) matched ON TRUE
              ORDER BY requested.subject_kind,requested.subject_id,
                       requested.subject_sha256",
            &[
                &policy_sha256,
                &now_ms,
                &subject_kinds,
                &subject_ids,
                &subject_sha256s,
            ],
        )
        .map_err(ats_certification_storage)?
        .into_iter()
        .map(|row| (row.get(0), row.get(1), row.get(2)))
        .collect::<BTreeSet<_>>();
    if revocations.len() > subjects.len() {
        return Err(AtsCertificationAuthorityError::IdentityConflict);
    }
    Ok((global_revoked, revocations))
}

fn load_postgres_ats_certification_page_allowlists(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    allowlist_ids: &[String],
    now_ms: i64,
) -> Result<BTreeSet<String>, AtsCertificationAuthorityError> {
    if allowlist_ids.is_empty() {
        return Ok(BTreeSet::new());
    }
    Ok(tx
        .query(
            "SELECT requested.allowlist_sha256
               FROM unnest($3::TEXT[]) requested(allowlist_sha256)
               JOIN jobs_ats_certification_canary_allowlists allowlist
                 ON allowlist.allowlist_sha256=requested.allowlist_sha256
               JOIN jobs_ats_certification_canary_allowlist_members member
                 ON member.allowlist_sha256=requested.allowlist_sha256
                AND member.account_id=$1
              WHERE allowlist.not_before_ms<=$2 AND allowlist.expires_at_ms>$2
                AND NOT EXISTS (
                  SELECT 1 FROM jobs_ats_certification_canary_allowlist_revocations revocation
                   WHERE revocation.allowlist_sha256=requested.allowlist_sha256
                     AND revocation.revoked_at_ms<=$2
                )
              ORDER BY requested.allowlist_sha256
              FOR SHARE OF allowlist,member",
            &[&account_id, &now_ms, &allowlist_ids],
        )
        .map_err(ats_certification_storage)?
        .into_iter()
        .map(|row| row.get(0))
        .collect())
}

fn load_postgres_ats_certification_page_quarantines(
    tx: &mut postgres::Transaction<'_>,
    policy_sha256: Option<&str>,
    keys: &BTreeSet<(String, String, String)>,
) -> Result<BTreeSet<(String, String, String, String)>, AtsCertificationAuthorityError> {
    let Some(policy_sha256) = policy_sha256 else {
        return Ok(BTreeSet::new());
    };
    if keys.is_empty() {
        return Ok(BTreeSet::new());
    }
    let scope_kinds = keys.iter().map(|(kind, _, _)| kind.clone()).collect::<Vec<_>>();
    let scope_ids = keys.iter().map(|(_, id, _)| id.clone()).collect::<Vec<_>>();
    let scope_sha256s = keys
        .iter()
        .map(|(_, _, sha256)| sha256.clone())
        .collect::<Vec<_>>();
    Ok(tx
        .query(
            "SELECT head.scope_kind, head.scope_id, head.scope_sha256,
                    command.trust_policy_sha256
               FROM unnest($2::TEXT[],$3::TEXT[],$4::TEXT[])
                    requested(scope_kind,scope_id,scope_sha256)
               JOIN jobs_ats_certification_quarantine_heads head
                 ON head.scope_kind=requested.scope_kind
                AND head.scope_id=requested.scope_id
                AND head.scope_sha256=requested.scope_sha256
               JOIN jobs_ats_certification_quarantine_commands command
                 ON command.command_sha256=head.current_command_sha256
              WHERE head.state='quarantined' AND command.trust_policy_sha256=$1
              ORDER BY head.scope_kind,head.scope_id,head.scope_sha256
              FOR SHARE OF head,command",
            &[&policy_sha256, &scope_kinds, &scope_ids, &scope_sha256s],
        )
        .map_err(ats_certification_storage)?
        .into_iter()
        .map(|row| (row.get(0), row.get(1), row.get(2), row.get(3)))
        .collect())
}

fn load_postgres_ats_certification_page_circuits(
    tx: &mut postgres::Transaction<'_>,
    keys: &BTreeSet<(String, String)>,
) -> Result<BTreeSet<(String, String)>, AtsCertificationAuthorityError> {
    if keys.is_empty() {
        return Ok(BTreeSet::new());
    }
    let scope_kinds = keys.iter().map(|(kind, _)| kind.clone()).collect::<Vec<_>>();
    let subject_keys = keys.iter().map(|(_, key)| key.clone()).collect::<Vec<_>>();
    Ok(tx
        .query(
            "SELECT head.scope_kind, head.subject_key
               FROM unnest($1::TEXT[],$2::TEXT[]) requested(scope_kind,subject_key)
               JOIN jobs_ats_certification_circuit_heads head
                 ON head.scope_kind=requested.scope_kind
                AND head.subject_key=requested.subject_key
              WHERE head.state<>'closed'
              ORDER BY head.scope_kind,head.subject_key
              FOR SHARE OF head",
            &[&scope_kinds, &subject_keys],
        )
        .map_err(ats_certification_storage)?
        .into_iter()
        .map(|row| (row.get(0), row.get(1)))
        .collect())
}

fn resolve_ats_certification_page_target(
    data: &AtsCertificationPageData,
    target: AtsCertificationPageTarget,
    now_ms: i64,
) -> AtsCertificationPostingPageResult {
    if let Some(binding) = ats_certification_page_active_binding(
        data,
        &target,
        "general",
        None,
        now_ms,
    )? {
        return Ok(AtsCertificationPostingResolution {
            status: ats_target_status_projection_for_binding(
                target.key.provider,
                target.target_key_sha256,
                &binding,
            ),
            active_binding: Some(binding),
        });
    }

    let allowlist_sha256 = ats_certification_page_canary_allowlist(data, &target, now_ms)?;
    if let Some(allowlist_sha256) = allowlist_sha256.as_deref() {
        if let Some(binding) = ats_certification_page_active_binding(
            data,
            &target,
            "canary",
            Some(allowlist_sha256),
            now_ms,
        )? {
            return Ok(AtsCertificationPostingResolution {
                status: ats_target_status_projection_for_binding(
                    target.key.provider,
                    target.target_key_sha256,
                    &binding,
                ),
                active_binding: Some(binding),
            });
        }
    }

    let general = ats_certification_page_inactive_status(
        data,
        &target.key,
        "general",
        None,
        now_ms,
    )?;
    let inactive = if general.status != "review_only" {
        general
    } else if let Some(allowlist_sha256) = allowlist_sha256.as_deref() {
        ats_certification_page_inactive_status(
            data,
            &target.key,
            "canary",
            Some(allowlist_sha256),
            now_ms,
        )?
    } else {
        general
    };
    Ok(AtsCertificationPostingResolution {
        status: ats_target_status_projection_for_inactive(
            target.key.provider,
            target.target_key_sha256,
            inactive,
        ),
        active_binding: None,
    })
}

fn ats_certification_page_rows<'a>(
    data: &'a AtsCertificationPageData,
    key: &AtsCertificationPageTargetKey,
    channel: &str,
) -> &'a [StoredAtsBindingRow] {
    data.bindings
        .get(&(
            key.provider.clone(),
            key.target_key.clone(),
            channel.to_string(),
        ))
        .map(Vec::as_slice)
        .unwrap_or_default()
}

fn ats_certification_page_current_policy_matches(
    data: &AtsCertificationPageData,
    row: &StoredAtsBindingRow,
) -> bool {
    data.trust.as_ref().is_some_and(|trust| {
        row.activation.trust_policy_sha256 == trust.policy_sha256
            && data
                .manifest_policies
                .get(&row.activation.manifest_sha256)
                == Some(&trust.policy_sha256)
    })
}

fn ats_certification_page_active_binding(
    data: &AtsCertificationPageData,
    target: &AtsCertificationPageTarget,
    channel: &str,
    account_allowlist_sha256: Option<&str>,
    now_ms: i64,
) -> Result<Option<AtsCertificationBinding>, AtsCertificationAuthorityError> {
    let Some(trust) = data.trust.as_ref() else {
        return Ok(None);
    };
    if now_ms < trust.valid_from_ms || now_ms >= trust.expires_at_ms {
        return Ok(None);
    }
    let candidates = ats_certification_page_rows(data, &target.key, channel)
        .iter()
        .filter(|row| ats_certification_page_current_policy_matches(data, row))
        .take(2)
        .collect::<Vec<_>>();
    let [row] = candidates.as_slice() else {
        return Ok(None);
    };
    if !ats_binding_row_channel_allowed(row, account_allowlist_sha256, now_ms)
        || ats_certification_page_activation_revoked(data, row)
        || ats_certification_page_base_quarantined(data, row)?
        || ats_certification_page_binding_circuit_open(data, row)
    {
        return Ok(None);
    }
    let evidence_sha256s = data
        .evidence
        .get(&row.activation.manifest_sha256)
        .map(|values| values.iter().map(|(_, value)| value.clone()).collect())
        .unwrap_or_default();
    let layout_observation_sha256s = data
        .layouts
        .get(&row.activation.manifest_sha256)
        .map(|values| {
            values
                .iter()
                .map(|(_, _, value)| value.clone())
                .collect()
        })
        .unwrap_or_default();
    let runtime_targets = data
        .runtimes
        .get(&row.activation.manifest_sha256)
        .map(|values| {
            values
                .iter()
                .filter(|(_, runtime)| {
                    !ats_certification_page_runtime_unavailable(
                        data,
                        runtime,
                        &row.activation.trust_policy_sha256,
                    )
                })
                .map(|(_, runtime)| runtime.clone())
                .collect()
        })
        .unwrap_or_default();
    let binding = ats_binding_from_row(
        (*row).clone(),
        evidence_sha256s,
        layout_observation_sha256s,
        runtime_targets,
        None,
    )?;
    Ok(binding.filter(|binding| {
        ats_certification_binding_matches_url(binding, &target.evidence.canonical_url)
    }))
}

fn ats_certification_page_canary_allowlist(
    data: &AtsCertificationPageData,
    target: &AtsCertificationPageTarget,
    now_ms: i64,
) -> Result<Option<String>, AtsCertificationAuthorityError> {
    let values = ats_certification_page_rows(data, &target.key, "canary")
        .iter()
        .filter(|row| {
            ats_certification_page_current_policy_matches(data, row)
                && row.activation.not_before_ms <= now_ms
                && row.activation.expires_at_ms > now_ms
                && row.manifest_not_before_ms <= now_ms
                && row.manifest_expires_at_ms > now_ms
        })
        .filter_map(|row| row.activation.account_allowlist_sha256.as_ref())
        .filter(|sha256| data.account_canary_allowlists.contains(*sha256))
        .cloned()
        .collect::<BTreeSet<_>>();
    match values.len() {
        0 => Ok(None),
        1 => Ok(values.into_iter().next()),
        _ => Err(AtsCertificationAuthorityError::IdentityConflict),
    }
}

fn ats_certification_page_inactive_row(
    data: &AtsCertificationPageData,
    row: &StoredAtsBindingRow,
) -> Result<AtsCertificationInactiveStatusRow, AtsCertificationAuthorityError> {
    let manifest_policy_sha256 = data
        .manifest_policies
        .get(&row.activation.manifest_sha256)
        .cloned()
        .ok_or(AtsCertificationAuthorityError::NotFound)?;
    Ok(AtsCertificationInactiveStatusRow {
        activation_sha256: row.activation.activation_sha256.clone(),
        activation_id: row.activation.activation_id.clone(),
        manifest_sha256: row.activation.manifest_sha256.clone(),
        certification_id: row.certification_id.clone(),
        scope_sha256: row.activation.scope_sha256.clone(),
        adapter_version: row.adapter_version.clone(),
        adapter_bundle_sha256: row.adapter_bundle_sha256.clone(),
        activation_policy_sha256: row.activation.trust_policy_sha256.clone(),
        manifest_policy_sha256,
        account_allowlist_sha256: row.activation.account_allowlist_sha256.clone(),
        activation_not_before_ms: row.activation.not_before_ms,
        activation_expires_at_ms: row.activation.expires_at_ms,
        manifest_not_before_ms: row.manifest_not_before_ms,
        manifest_expires_at_ms: row.manifest_expires_at_ms,
        manifest_tested_at_ms: row.last_verified_at_ms,
    })
}

fn ats_certification_page_inactive_status(
    data: &AtsCertificationPageData,
    key: &AtsCertificationPageTargetKey,
    channel: &str,
    account_allowlist_sha256: Option<&str>,
    now_ms: i64,
) -> Result<AtsCertificationInactiveStatus, AtsCertificationAuthorityError> {
    let rows = ats_certification_page_rows(data, key, channel);
    if rows.is_empty() {
        return Ok(AtsCertificationInactiveStatus::without_authority(
            "review_only",
        ));
    }
    if rows.len() != 1 {
        return Ok(AtsCertificationInactiveStatus::without_authority("drifted"));
    }
    let binding = &rows[0];
    let row = ats_certification_page_inactive_row(data, binding)?;
    if match channel {
        "canary" => row.account_allowlist_sha256.as_deref() != account_allowlist_sha256,
        "general" | "shadow" => account_allowlist_sha256.is_some(),
        _ => true,
    } {
        return Ok(AtsCertificationInactiveStatus::from_row(
            "review_only",
            &row,
        ));
    }
    let Some(trust) = data.trust.as_ref() else {
        return Ok(AtsCertificationInactiveStatus::from_row(
            "review_only",
            &row,
        ));
    };
    if ats_certification_page_inactive_revoked(data, binding) {
        return Ok(AtsCertificationInactiveStatus::from_row("revoked", &row));
    }
    if row.activation_policy_sha256 != trust.policy_sha256
        || row.manifest_policy_sha256 != trust.policy_sha256
    {
        return Ok(AtsCertificationInactiveStatus::from_row("drifted", &row));
    }
    if now_ms < trust.valid_from_ms
        || now_ms >= trust.expires_at_ms
        || now_ms < row.activation_not_before_ms
        || now_ms >= row.activation_expires_at_ms
        || now_ms < row.manifest_not_before_ms
        || now_ms >= row.manifest_expires_at_ms
    {
        return Ok(AtsCertificationInactiveStatus::from_row("expired", &row));
    }
    if ats_certification_page_binding_circuit_open(data, binding) {
        return Ok(AtsCertificationInactiveStatus::from_row(
            "suspended",
            &row,
        ));
    }
    Ok(AtsCertificationInactiveStatus::from_row("drifted", &row))
}

fn ats_certification_page_revocation_matches_base(
    data: &AtsCertificationPageData,
    row: &StoredAtsBindingRow,
) -> bool {
    ats_certification_page_revoked(
        data,
        "activation",
        &row.activation.activation_id,
        &row.activation.activation_sha256,
    ) || ats_certification_page_revoked(
        data,
        "manifest",
        &row.certification_id,
        &row.activation.manifest_sha256,
    ) || ats_certification_page_revoked(data, "target", &row.target_key, "")
        || ats_certification_page_revoked(
            data,
            "scope",
            &row.activation.scope_sha256,
            &row.activation.scope_sha256,
        )
        || ats_certification_page_revoked(
            data,
            "adapter_bundle",
            &row.adapter_version,
            &row.adapter_bundle_sha256,
        )
        || data
            .layouts
            .get(&row.activation.manifest_sha256)
            .is_some_and(|values| {
                values.iter().any(|(_, id, sha256)| {
                    ats_certification_page_revoked(data, "layout_observation", id, sha256)
                })
            })
}

fn ats_certification_page_revoked(
    data: &AtsCertificationPageData,
    subject_kind: &str,
    subject_id: &str,
    subject_sha256: &str,
) -> bool {
    data.revocations.contains(&(
        subject_kind.to_string(),
        subject_id.to_string(),
        subject_sha256.to_string(),
    ))
}

fn ats_certification_page_activation_revoked(
    data: &AtsCertificationPageData,
    row: &StoredAtsBindingRow,
) -> bool {
    data.global_revoked
        || ats_certification_page_revocation_matches_base(data, row)
        || data
            .evidence
            .get(&row.activation.manifest_sha256)
            .is_some_and(|values| {
                values.iter().any(|(_, sha256)| {
                    ats_certification_page_revoked(data, "evidence", "", sha256)
                })
            })
}

fn ats_certification_page_inactive_revoked(
    data: &AtsCertificationPageData,
    row: &StoredAtsBindingRow,
) -> bool {
    data.global_revoked || ats_certification_page_revocation_matches_base(data, row)
}

fn ats_certification_page_base_quarantined(
    data: &AtsCertificationPageData,
    row: &StoredAtsBindingRow,
) -> Result<bool, AtsCertificationAuthorityError> {
    let manifest = (
        row.provider.clone(),
        row.target_key.clone(),
        row.variant_key.clone(),
        row.activation.scope_sha256.clone(),
        row.adapter_version.clone(),
        row.adapter_bundle_sha256.clone(),
    );
    Ok(ats_base_quarantine_targets(&manifest, &row.activation)?
        .into_iter()
        .any(|(kind, id, sha256)| {
            data.quarantines.contains(&(
                kind,
                id,
                sha256,
                row.activation.trust_policy_sha256.clone(),
            ))
        }))
}

fn ats_certification_page_binding_circuit_open(
    data: &AtsCertificationPageData,
    row: &StoredAtsBindingRow,
) -> bool {
    [
        ("provider", row.provider.as_str()),
        ("target", row.target_key.as_str()),
        ("adapter", row.adapter_bundle_sha256.as_str()),
        ("activation", row.activation.activation_sha256.as_str()),
    ]
    .iter()
    .any(|(kind, key)| {
        data.circuits
            .contains(&(kind.to_string(), key.to_string()))
    })
}

fn ats_certification_page_runtime_unavailable(
    data: &AtsCertificationPageData,
    runtime: &AtsCertificationRuntimeTarget,
    policy_sha256: &str,
) -> bool {
    let runner_build_sha256 = runtime
        .runner_build_id
        .as_deref()
        .map(|value| ats_certification_sha256(value.as_bytes()));
    let revoked = ats_certification_page_revoked(
        data,
        "runtime",
        &runtime.runtime_id,
        &runtime.runtime_sha256,
    ) || runtime
        .browser_release_manifest_sha256
        .as_ref()
        .is_some_and(|value| {
            ats_certification_page_revoked(data, "browser_release_manifest", value, value)
        })
        || runtime.runner_image_sha256.as_ref().is_some_and(|value| {
            ats_certification_page_revoked(data, "runner_image", value, value)
        })
        || runtime.runner_build_id.as_ref().is_some_and(|value| {
            runner_build_sha256.as_ref().is_some_and(|sha256| {
                ats_certification_page_revoked(data, "runner_build", value, sha256)
            })
        });
    revoked
        || data.quarantines.contains(&(
            "runtime".to_string(),
            runtime.runtime_id.clone(),
            runtime.runtime_sha256.clone(),
            policy_sha256.to_string(),
        ))
        || data
            .circuits
            .contains(&("runtime".to_string(), runtime.runtime_id.clone()))
        || data
            .circuits
            .contains(&("runtime".to_string(), runtime.runtime_sha256.clone()))
}

#[cfg(test)]
mod ats_certification_page_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const PAGE_NOW_MS: i64 = 2_000_000_000_000;
    static PAGE_SELECTS: AtomicUsize = AtomicUsize::new(0);
    static COMPOSED_PAGE_SELECTS: AtomicUsize = AtomicUsize::new(0);

    fn count_page_selects(sql: &str) {
        if sql.trim_start().to_ascii_uppercase().starts_with("SELECT") {
            PAGE_SELECTS.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn count_composed_page_selects(sql: &str) {
        if sql.trim_start().to_ascii_uppercase().starts_with("SELECT") {
            COMPOSED_PAGE_SELECTS.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn page_pool() -> DbPool {
        let path = std::env::temp_dir().join(format!(
            "bluey-ats-page-{}-{}.sqlite3",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let pool = crate::db::open_pool(&path).expect("open ATS page pool");
        crate::db::run_migrations(&pool).expect("migrate ATS page pool");
        pool
    }

    fn page_posting(index: usize) -> JobPosting {
        let external_id = format!("phase626-page-{index:02}");
        let canonical_key = format!("phase626-canonical-{index:02}");
        JobPosting {
            id: uuid::Uuid::new_v4().to_string(),
            canonical_key: canonical_key.clone(),
            source: "greenhouse".to_string(),
            external_id: external_id.clone(),
            company: "Acme".to_string(),
            title: format!("Engineer {index}"),
            location: "Remote".to_string(),
            workplace: "remote".to_string(),
            canonical_url: format!(
                "https://boards.greenhouse.io/acme/jobs/{external_id}"
            ),
            description: "Build reliable systems.".to_string(),
            compensation: String::new(),
            employment_type: "full_time".to_string(),
            track_id: "track-page".to_string(),
            match_score: 100 - index as i64,
            matched_reasons: Vec::new(),
            missing_requirements: Vec::new(),
            posted_at_ms: Some(PAGE_NOW_MS - 10_000),
            last_verified_at_ms: Some(PAGE_NOW_MS - 100),
            availability_status: "active".to_string(),
            status: "matched".to_string(),
            created_at_ms: PAGE_NOW_MS - 20_000,
            updated_at_ms: PAGE_NOW_MS - 100,
            discovery_evidence: JobDiscoveryEvidence {
                provenance: "original_source".to_string(),
                canonical_status: "canonical".to_string(),
                canonical_job_id: Some(canonical_key),
                employer_verification_status: "ats_tenant_verified".to_string(),
                employer_id: None,
                canonical_employer_domain: None,
                application_domain: Some("boards.greenhouse.io".to_string()),
                scam_risk_status: "source_screened".to_string(),
                scam_signals: Vec::new(),
                original_source_status: "verified_open".to_string(),
                original_source_checked_at_ms: Some(PAGE_NOW_MS - 50),
                original_source_snapshot_expires_at_ms: Some(PAGE_NOW_MS + 60_000),
                original_source_evidence_hash: Some("a".repeat(64)),
                original_source_mismatched_fields: Vec::new(),
                requires_original_revalidation: false,
            },
            eligibility: None,
        }
    }

    fn page_runtime() -> AtsCertificationRuntimeTarget {
        AtsCertificationRuntimeTarget {
            runtime_kind: "local".to_string(),
            runtime_id: "local:phase626-page".to_string(),
            runtime_sha256: ats_certification_sha256(b"phase626-page-runtime"),
            platform: "macos".to_string(),
            architecture: "arm64".to_string(),
            automation_bundle_sha256: "4".repeat(64),
            browser_release_manifest_sha256: Some("1".repeat(64)),
            browser_artifact_sha256: Some("2".repeat(64)),
            browser_build_descriptor_sha256: Some("3".repeat(64)),
            runner_build_id: None,
            runner_image_sha256: None,
            playwright_version: "1.61.1".to_string(),
            chromium_revision: "1228".to_string(),
            chromium_executable_sha256: "7".repeat(64),
        }
    }

    fn install_one_page_authority() -> (
        DbPool,
        JobPosting,
        ats_certification_authority_tests::InstalledAtsAuthorityFixture,
    ) {
        let pool = page_pool();
        let posting = page_posting(0);
        let target =
            ats_certification_fresh_target_evidence_from_posting(&posting, PAGE_NOW_MS).unwrap();
        let surface = AtsObservedSurface {
            variant_key: "greenhouse_public".to_string(),
            layout_contract_version: 1,
            surface_sha256: ats_certification_sha256(b"phase626-differential-surface"),
        };
        let installed =
            ats_certification_authority_tests::install_signed_ats_authority_fixture(
                &pool,
                target,
                surface,
                page_runtime(),
                PAGE_NOW_MS,
            )
            .expect("install differential ATS authority");
        (pool, posting, installed)
    }

    fn resolution_signature(
        result: AtsCertificationPostingPageResult,
    ) -> (String, Option<String>, Option<String>) {
        match result {
            Ok(value) => (
                value.status.status,
                value.status.manifest_sha256,
                value
                    .active_binding
                    .map(|binding| binding.activation_sha256),
            ),
            Err(error) => (format!("error:{error:?}"), None, None),
        }
    }

    fn full_resolution_signature(result: AtsCertificationPostingPageResult) -> String {
        match result {
            Ok(value) => serde_json::to_string(&value).expect("serialize ATS resolution"),
            Err(error) => format!("error:{error:?}"),
        }
    }

    fn page_signature(pool: &DbPool, posting: &JobPosting, now_ms: i64) -> (String, Option<String>, Option<String>) {
        page_signature_for_account(pool, "acct-phase626-page", posting, now_ms)
    }

    fn page_signature_for_account(
        pool: &DbPool,
        account_id: &str,
        posting: &JobPosting,
        now_ms: i64,
    ) -> (String, Option<String>, Option<String>) {
        resolution_signature(page_resolution_for_account(
            pool,
            account_id,
            posting,
            now_ms,
        ))
    }

    fn page_resolution_for_account(
        pool: &DbPool,
        account_id: &str,
        posting: &JobPosting,
        now_ms: i64,
    ) -> AtsCertificationPostingPageResult {
        let mut connection = pool.get().expect("get differential page connection");
        let transaction = connection.transaction().expect("begin differential page snapshot");
        let result = resolve_ats_certification_for_postings_page_sqlite_tx(
            &transaction,
            account_id,
            std::slice::from_ref(posting),
            now_ms,
        )
        .expect("load differential ATS page")
        .into_iter()
        .next()
        .expect("one differential ATS resolution");
        transaction.rollback().expect("roll back differential page snapshot");
        result
    }

    fn assert_scalar_page_equivalent(pool: &DbPool, posting: &JobPosting, now_ms: i64) {
        assert_eq!(
            full_resolution_signature(page_resolution_for_account(
                pool,
                "acct-phase626-page",
                posting,
                now_ms,
            )),
            full_resolution_signature(resolve_ats_certification_for_posting(
                pool,
                "acct-phase626-page",
                posting,
                None,
                now_ms,
            ))
        );
    }

    fn assert_scalar_page_equivalent_for_account(
        pool: &DbPool,
        account_id: &str,
        posting: &JobPosting,
        now_ms: i64,
    ) {
        let scalar = full_resolution_signature(resolve_ats_certification_for_posting(
            pool,
            account_id,
            posting,
            None,
            now_ms,
        ));
        assert_eq!(
            full_resolution_signature(page_resolution_for_account(
                pool,
                account_id,
                posting,
                now_ms,
            )),
            scalar
        );
    }

    fn fixture_policy_sha256(pool: &DbPool) -> String {
        pool.get()
            .unwrap()
            .query_row(
                "SELECT current_policy_sha256
                   FROM jobs_ats_certification_trust_head WHERE singleton_id=1",
                [],
                |row| row.get(0),
            )
            .unwrap()
    }

    fn delete_fixture_manifest_child(pool: &DbPool, trigger: &str, table: &str, manifest: &str) {
        let connection = pool.get().unwrap();
        connection
            .execute_batch(&format!("DROP TRIGGER {trigger}"))
            .expect("drop fixture-only immutable child trigger");
        connection
            .execute(
                &format!("DELETE FROM {table} WHERE manifest_sha256=?1"),
                params![manifest],
            )
            .expect("delete fixture-only manifest child");
    }

    #[test]
    fn page_child_envelopes_reject_overflow_duplicate_and_gapped_ordinals() {
        assert!(validate_ats_certification_page_child_ordinal(63, 63, 64).is_ok());
        assert!(validate_ats_certification_page_child_ordinal(
            ATS_CERTIFICATION_PAGE_MAX_EVIDENCE_PER_MANIFEST,
            64,
            ATS_CERTIFICATION_PAGE_MAX_EVIDENCE_PER_MANIFEST,
        )
        .is_err());
        assert!(validate_ats_certification_page_child_ordinal(
            ATS_CERTIFICATION_PAGE_MAX_LAYOUTS_PER_MANIFEST,
            64,
            ATS_CERTIFICATION_PAGE_MAX_LAYOUTS_PER_MANIFEST,
        )
        .is_err());
        assert!(validate_ats_certification_page_child_ordinal(
            ATS_CERTIFICATION_PAGE_MAX_RUNTIMES_PER_MANIFEST,
            32,
            ATS_CERTIFICATION_PAGE_MAX_RUNTIMES_PER_MANIFEST,
        )
        .is_err());
        assert!(validate_ats_certification_page_child_ordinal(1, 0, 64).is_err());
        assert!(validate_ats_certification_page_child_ordinal(1, 2, 64).is_err());
    }

    #[test]
    fn sqlite_page_rejects_more_than_one_bounded_scan_before_query_construction() {
        let pool = page_pool();
        let postings = (0..(JOBS_WORKSPACE_MAX_PAGE_SIZE + 2))
            .map(page_posting)
            .collect::<Vec<_>>();
        let mut connection = pool.get().unwrap();
        let transaction = connection.transaction().unwrap();
        let error = resolve_ats_certification_for_postings_page_sqlite_tx(
            &transaction,
            "acct-phase626-page",
            &postings,
            PAGE_NOW_MS,
        )
        .expect_err("an oversized page must fail before placeholder construction");
        assert!(matches!(
            error,
            AtsCertificationAuthorityError::IdentityConflict
        ));
        transaction.rollback().unwrap();
    }

    #[test]
    fn sqlite_page_matches_scalar_for_positive_expiry_malformed_and_mutable_denials() {
        let (pool, posting, _) = install_one_page_authority();
        assert_scalar_page_equivalent(&pool, &posting, PAGE_NOW_MS);

        let mut expired = posting.clone();
        let expired_now_ms = PAGE_NOW_MS + 850_000;
        expired.last_verified_at_ms = Some(expired_now_ms - 100);
        expired.discovery_evidence.original_source_checked_at_ms = Some(expired_now_ms - 50);
        expired
            .discovery_evidence
            .original_source_snapshot_expires_at_ms = Some(expired_now_ms + 60_000);
        assert_scalar_page_equivalent(&pool, &expired, expired_now_ms);

        let mut malformed = posting;
        malformed.canonical_url = "https://example.test/not-an-ats-target".to_string();
        assert_scalar_page_equivalent(&pool, &malformed, PAGE_NOW_MS);

        for (trigger, table) in [
            (
                "trg_jobs_ats_certification_manifest_evidence_no_delete",
                "jobs_ats_certification_manifest_evidence",
            ),
            (
                "trg_jobs_ats_certification_manifest_layouts_no_delete",
                "jobs_ats_certification_manifest_layouts",
            ),
            (
                "trg_jobs_ats_certification_runtime_targets_no_delete",
                "jobs_ats_certification_runtime_targets",
            ),
        ] {
            let (pool, posting, installed) = install_one_page_authority();
            delete_fixture_manifest_child(&pool, trigger, table, &installed.manifest_sha256);
            assert!(resolve_ats_certification_for_posting(
                &pool,
                "acct-phase626-page",
                &posting,
                None,
                PAGE_NOW_MS,
            )
            .is_err());
            let mut connection = pool.get().unwrap();
            let transaction = connection.transaction().unwrap();
            assert!(resolve_ats_certification_for_postings_page_sqlite_tx(
                &transaction,
                "acct-phase626-page",
                std::slice::from_ref(&posting),
                PAGE_NOW_MS,
            )
            .is_err());
            transaction.rollback().unwrap();
        }

        let (pool, posting, _) = install_one_page_authority();
        let policy_sha256 = fixture_policy_sha256(&pool);
        let connection = pool.get().unwrap();
        connection
            .execute_batch("DROP TRIGGER trg_jobs_ats_certification_trust_policies_no_update")
            .unwrap();
        connection
            .execute(
                "UPDATE jobs_ats_certification_trust_policies
                    SET expires_at_ms=?1 WHERE policy_sha256=?2",
                params![PAGE_NOW_MS, policy_sha256],
            )
            .unwrap();
        drop(connection);
        assert_scalar_page_equivalent(&pool, &posting, PAGE_NOW_MS);

        let (pool, posting, installed) = install_one_page_authority();
        let connection = pool.get().unwrap();
        connection
            .execute_batch("DROP TRIGGER trg_jobs_ats_certification_activations_no_update")
            .unwrap();
        connection
            .execute(
                "UPDATE jobs_ats_certification_activations SET scope_sha256=?1
                  WHERE activation_sha256=?2",
                params!["f".repeat(64), installed.activation_sha256],
            )
            .unwrap();
        drop(connection);
        assert_scalar_page_equivalent(&pool, &posting, PAGE_NOW_MS);
        assert!(
            page_signature(&pool, &posting, PAGE_NOW_MS).2.is_none(),
            "corrupted head/activation identity must fail closed"
        );
    }

    #[test]
    fn sqlite_scalar_and_page_reject_signed_manifest_child_count_drift() {
        for count_column in [
            "evidence_count",
            "layout_observation_count",
            "runtime_target_count",
        ] {
            let (pool, posting, installed) = install_one_page_authority();
            let connection = pool.get().unwrap();
            connection
                .execute_batch("DROP TRIGGER trg_jobs_ats_certification_manifests_no_update")
                .unwrap();
            connection
                .execute(
                    &format!(
                        "UPDATE jobs_ats_certification_manifests SET {count_column}={count_column}+1
                          WHERE manifest_sha256=?1"
                    ),
                    params![installed.manifest_sha256],
                )
                .unwrap();
            drop(connection);

            assert!(resolve_ats_certification_for_posting(
                &pool,
                "acct-phase626-page",
                &posting,
                None,
                PAGE_NOW_MS,
            )
            .is_err());
            let mut connection = pool.get().unwrap();
            let transaction = connection.transaction().unwrap();
            assert!(resolve_ats_certification_for_postings_page_sqlite_tx(
                &transaction,
                "acct-phase626-page",
                std::slice::from_ref(&posting),
                PAGE_NOW_MS,
            )
            .is_err());
            transaction.rollback().unwrap();
        }
    }

    #[test]
    fn sqlite_page_matches_scalar_for_revocation_quarantine_and_circuit() {
        let (pool, posting, installed) = install_one_page_authority();
        let policy_sha256 = fixture_policy_sha256(&pool);
        let connection = pool.get().unwrap();
        let activation_id: String = connection
            .query_row(
                "SELECT activation_id FROM jobs_ats_certification_activations
                  WHERE activation_sha256=?1",
                params![installed.activation_sha256],
                |row| row.get(0),
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO jobs_ats_certification_revocations(
                    revocation_sha256,revocation_id,revocation_generation,
                    predecessor_revocation_sha256,subject_kind,subject_id,subject_sha256,
                    reason_ref,canonical_revocation_base64url,authorization_sha256,
                    trust_policy_sha256,canonical_authorization_base64url,
                    issued_at_ms,effective_at_ms,recorded_by,recorded_at_ms)
                 VALUES(?1,'phase626-revocation',1,NULL,'activation',?2,?3,
                    'phase626-test','fixture',?4,?5,'fixture',?6,?6,
                    'phase626-test',?6)",
                params![
                    ats_certification_sha256(b"phase626-revocation"),
                    activation_id,
                    installed.activation_sha256,
                    ats_certification_sha256(b"phase626-revocation-authorization"),
                    policy_sha256,
                    PAGE_NOW_MS,
                ],
            )
            .unwrap();
        drop(connection);
        assert_scalar_page_equivalent(&pool, &posting, PAGE_NOW_MS);

        let (pool, posting, installed) = install_one_page_authority();
        let policy_sha256 = fixture_policy_sha256(&pool);
        let connection = pool.get().unwrap();
        let activation_id: String = connection
            .query_row(
                "SELECT activation_id FROM jobs_ats_certification_activations
                  WHERE activation_sha256=?1",
                params![installed.activation_sha256],
                |row| row.get(0),
            )
            .unwrap();
        let command_sha256 = ats_certification_sha256(b"phase626-quarantine-command");
        connection
            .execute(
                "INSERT INTO jobs_ats_certification_quarantine_commands(
                    command_sha256,command_id,command_generation,scope_kind,scope_id,
                    scope_sha256,command_sequence,predecessor_command_sha256,action,
                    reason_ref,canonical_command_base64url,authorization_sha256,
                    trust_policy_sha256,canonical_authorization_base64url,
                    issued_at_ms,recorded_by,recorded_at_ms)
                 VALUES(?1,'phase626-quarantine',1,'activation',?2,?3,1,NULL,
                    'quarantine','phase626-test','fixture',?4,?5,'fixture',?6,
                    'phase626-test',?6)",
                params![
                    command_sha256,
                    activation_id,
                    installed.activation_sha256,
                    ats_certification_sha256(b"phase626-quarantine-authorization"),
                    policy_sha256,
                    PAGE_NOW_MS,
                ],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO jobs_ats_certification_quarantine_heads(
                    scope_kind,scope_id,scope_sha256,head_revision,current_command_sha256,
                    current_command_sequence,state,updated_by,updated_at_ms)
                 VALUES('activation',?1,?2,1,?3,1,'quarantined','phase626-test',?4)",
                params![activation_id, installed.activation_sha256, command_sha256, PAGE_NOW_MS],
            )
            .unwrap();
        drop(connection);
        assert_scalar_page_equivalent(&pool, &posting, PAGE_NOW_MS);

        let (pool, posting, _) = install_one_page_authority();
        let target = format!("greenhouse:acme:{}", posting.external_id);
        let event_id = "phase626-circuit-event";
        let connection = pool.get().unwrap();
        connection
            .execute(
                "INSERT INTO jobs_ats_certification_circuit_events(
                    event_id,event_sha256,canonical_event_base64url,scope_kind,subject_key,
                    transition,trigger_kind,window_started_at_ms,window_ended_at_ms,
                    failure_count,sample_count,threshold_count,authority_ref,event_at_ms,
                    recorded_by,recorded_at_ms)
                 VALUES(?1,?2,'fixture','target',?3,'opened','evidence_failure',?4,?4,
                    1,1,1,'phase626-test',?4,'phase626-test',?4)",
                params![
                    event_id,
                    ats_certification_sha256(b"phase626-circuit-event"),
                    target,
                    PAGE_NOW_MS,
                ],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO jobs_ats_certification_circuit_heads(
                    scope_kind,subject_key,head_revision,current_event_id,state,updated_at_ms)
                 VALUES('target',?1,1,?2,'opened',?3)",
                params![target, event_id, PAGE_NOW_MS],
            )
            .unwrap();
        drop(connection);
        assert_scalar_page_equivalent(&pool, &posting, PAGE_NOW_MS);
    }

    #[test]
    fn sqlite_page_matches_scalar_for_canary_enrollment_and_wrong_account() {
        let pool = page_pool();
        let posting = page_posting(0);
        let target =
            ats_certification_fresh_target_evidence_from_posting(&posting, PAGE_NOW_MS).unwrap();
        let surface = AtsObservedSurface {
            variant_key: "greenhouse_public".to_string(),
            layout_contract_version: 1,
            surface_sha256: ats_certification_sha256(b"phase626-canary-surface"),
        };
        ats_certification_authority_tests::install_signed_ats_canary_authority_fixture(
            &pool,
            target,
            surface,
            page_runtime(),
            PAGE_NOW_MS,
            "acct-phase626-canary",
        )
        .expect("install signed canary ATS authority");
        assert_scalar_page_equivalent_for_account(
            &pool,
            "acct-phase626-canary",
            &posting,
            PAGE_NOW_MS,
        );
        assert_scalar_page_equivalent_for_account(
            &pool,
            "acct-not-in-canary",
            &posting,
            PAGE_NOW_MS,
        );
    }

    #[test]
    fn sqlite_page_matches_scalar_when_general_and_canary_share_one_manifest() {
        let (pool, posting, installed) = install_one_page_authority();
        let general_activation_sha256 = installed.activation_sha256.clone();
        let canary_activation_sha256 =
            ats_certification_authority_tests::
                install_signed_ats_canary_activation_for_existing_manifest(
                    &pool,
                    &installed,
                    PAGE_NOW_MS,
                    "acct-phase626-page",
                )
                .expect("install signed shared-manifest canary activation");

        assert_scalar_page_equivalent(&pool, &posting, PAGE_NOW_MS);
        let general = page_resolution_for_account(
            &pool,
            "acct-phase626-page",
            &posting,
            PAGE_NOW_MS,
        )
        .expect("resolve general-first shared-manifest authority");
        let general_binding = general
            .active_binding
            .expect("general must win while both channels are active");
        assert_eq!(general_binding.channel, "general");
        assert_eq!(
            general_binding.activation_sha256,
            general_activation_sha256
        );

        let canary_now_ms = PAGE_NOW_MS + 825_000;
        let mut fresh_posting = posting;
        fresh_posting.last_verified_at_ms = Some(canary_now_ms - 100);
        fresh_posting.discovery_evidence.original_source_checked_at_ms =
            Some(canary_now_ms - 50);
        fresh_posting
            .discovery_evidence
            .original_source_snapshot_expires_at_ms = Some(canary_now_ms + 60_000);
        assert_scalar_page_equivalent(&pool, &fresh_posting, canary_now_ms);
        let canary = page_resolution_for_account(
            &pool,
            "acct-phase626-page",
            &fresh_posting,
            canary_now_ms,
        )
        .expect("resolve canary fallback for shared manifest");
        let canary_binding = canary
            .active_binding
            .expect("enrolled account must fall back to the active canary");
        assert_eq!(canary_binding.channel, "canary");
        assert_eq!(canary_binding.activation_sha256, canary_activation_sha256);

        assert_scalar_page_equivalent_for_account(
            &pool,
            "acct-not-in-canary",
            &fresh_posting,
            canary_now_ms,
        );
        assert!(
            page_resolution_for_account(
                &pool,
                "acct-not-in-canary",
                &fresh_posting,
                canary_now_ms,
            )
            .expect("resolve non-enrolled shared-manifest authority")
            .active_binding
            .is_none(),
            "a non-enrolled account must not receive the canary fallback"
        );
    }

    #[test]
    fn sqlite_page_revocation_snapshot_ignores_irrelevant_history_and_bounds_global_flags() {
        let (pool, posting, _) = install_one_page_authority();
        let policy_sha256 = fixture_policy_sha256(&pool);
        let mut connection = pool.get().unwrap();
        let transaction = connection.transaction().unwrap();
        let (trust_generation, valid_from_ms, expires_at_ms): (i64, i64, i64) = transaction
            .query_row(
                "SELECT trust_generation,valid_from_ms,expires_at_ms
                   FROM jobs_ats_certification_trust_policies
                  WHERE policy_sha256=?1",
                params![policy_sha256],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        for index in 0..512 {
            transaction
                .execute(
                    "INSERT INTO jobs_ats_certification_trust_keys(
                        policy_sha256,trust_generation,role,threshold,key_id,
                        public_key_base64url,key_state,valid_from_ms,expires_at_ms)
                     VALUES(?1,?2,'revocation',1,?3,?4,'active',?5,?6)",
                    params![
                        policy_sha256,
                        trust_generation,
                        format!("phase626-unused-trust-{index:04}"),
                        format!("phase626-unused-public-{index:04}"),
                        valid_from_ms,
                        expires_at_ms,
                    ],
                )
                .unwrap();
        }
        let mut predecessor = None::<String>;
        for generation in 1..=512_i64 {
            let revocation_sha256 = ats_certification_sha256(
                format!("phase626-irrelevant-revocation-{generation:04}").as_bytes(),
            );
            transaction
                .execute(
                    "INSERT INTO jobs_ats_certification_revocations(
                        revocation_sha256,revocation_id,revocation_generation,
                        predecessor_revocation_sha256,subject_kind,subject_id,
                        subject_sha256,reason_ref,canonical_revocation_base64url,
                        authorization_sha256,trust_policy_sha256,
                        canonical_authorization_base64url,issued_at_ms,effective_at_ms,
                        recorded_by,recorded_at_ms)
                     VALUES(?1,?2,?3,?4,'target',?5,?6,'phase626-test','fixture',
                        ?7,?8,'fixture',?9,?9,'phase626-test',?9)",
                    params![
                        revocation_sha256,
                        format!("phase626-irrelevant-revocation-{generation:04}"),
                        generation,
                        predecessor,
                        format!("phase626-irrelevant-target-{generation:04}"),
                        ats_certification_sha256(
                            format!("phase626-irrelevant-subject-{generation:04}").as_bytes(),
                        ),
                        ats_certification_sha256(
                            format!("phase626-irrelevant-authorization-{generation:04}")
                                .as_bytes(),
                        ),
                        policy_sha256,
                        PAGE_NOW_MS,
                    ],
                )
                .unwrap();
            predecessor = Some(revocation_sha256);
        }
        transaction.commit().unwrap();
        drop(connection);

        let (targets, _) = ats_certification_page_targets(
            std::slice::from_ref(&posting),
            PAGE_NOW_MS,
        );
        let targets = targets
            .into_iter()
            .map(|(_, target)| target)
            .collect::<Vec<_>>();
        let mut connection = pool.get().unwrap();
        let transaction = connection.transaction().unwrap();
        let data = load_sqlite_ats_certification_page_data(
            &transaction,
            "acct-phase626-page",
            &targets,
            PAGE_NOW_MS,
        )
        .unwrap();
        assert!(!data.global_revoked);
        assert!(data.revocations.is_empty());
        transaction.rollback().unwrap();
        drop(connection);
        assert_scalar_page_equivalent(&pool, &posting, PAGE_NOW_MS);

        let global_revocation_sha256 =
            ats_certification_sha256(b"phase626-global-trust-key-revocation");
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO jobs_ats_certification_revocations(
                    revocation_sha256,revocation_id,revocation_generation,
                    predecessor_revocation_sha256,subject_kind,subject_id,
                    subject_sha256,reason_ref,canonical_revocation_base64url,
                    authorization_sha256,trust_policy_sha256,
                    canonical_authorization_base64url,issued_at_ms,effective_at_ms,
                    recorded_by,recorded_at_ms)
                 VALUES(?1,'phase626-global-trust-key-revocation',513,?2,'trust_key',
                    'phase626-unused-trust-0000',?3,'phase626-test','fixture',?4,?5,
                    'fixture',?6,?6,'phase626-test',?6)",
                params![
                    global_revocation_sha256,
                    predecessor.expect("irrelevant revocation chain tail"),
                    ats_certification_sha256(b"phase626-global-trust-key-subject"),
                    ats_certification_sha256(b"phase626-global-trust-key-authorization"),
                    policy_sha256,
                    PAGE_NOW_MS,
                ],
            )
            .unwrap();
        let mut connection = pool.get().unwrap();
        let transaction = connection.transaction().unwrap();
        let data = load_sqlite_ats_certification_page_data(
            &transaction,
            "acct-phase626-page",
            &targets,
            PAGE_NOW_MS,
        )
        .unwrap();
        assert!(data.global_revoked);
        assert!(data.revocations.is_empty());
        transaction.rollback().unwrap();
        drop(connection);
        assert_scalar_page_equivalent(&pool, &posting, PAGE_NOW_MS);
        assert_eq!(page_signature(&pool, &posting, PAGE_NOW_MS).0, "revoked");
    }

    #[test]
    fn sqlite_page_policy_revocation_is_one_global_flag() {
        let (pool, posting, _) = install_one_page_authority();
        let policy_sha256 = fixture_policy_sha256(&pool);
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO jobs_ats_certification_revocations(
                    revocation_sha256,revocation_id,revocation_generation,
                    predecessor_revocation_sha256,subject_kind,subject_id,
                    subject_sha256,reason_ref,canonical_revocation_base64url,
                    authorization_sha256,trust_policy_sha256,
                    canonical_authorization_base64url,issued_at_ms,effective_at_ms,
                    recorded_by,recorded_at_ms)
                 VALUES(?1,'phase626-global-policy-revocation',1,NULL,'policy',
                    'phase626-current-policy',?2,'phase626-test','fixture',?3,?2,
                    'fixture',?4,?4,'phase626-test',?4)",
                params![
                    ats_certification_sha256(b"phase626-global-policy-revocation"),
                    policy_sha256,
                    ats_certification_sha256(b"phase626-global-policy-authorization"),
                    PAGE_NOW_MS,
                ],
            )
            .unwrap();
        let (targets, _) = ats_certification_page_targets(
            std::slice::from_ref(&posting),
            PAGE_NOW_MS,
        );
        let targets = targets
            .into_iter()
            .map(|(_, target)| target)
            .collect::<Vec<_>>();
        let mut connection = pool.get().unwrap();
        let transaction = connection.transaction().unwrap();
        let data = load_sqlite_ats_certification_page_data(
            &transaction,
            "acct-phase626-page",
            &targets,
            PAGE_NOW_MS,
        )
        .unwrap();
        assert!(data.global_revoked);
        assert!(data.revocations.is_empty());
        transaction.rollback().unwrap();
        drop(connection);
        assert_scalar_page_equivalent(&pool, &posting, PAGE_NOW_MS);
        assert_eq!(page_signature(&pool, &posting, PAGE_NOW_MS).0, "revoked");
    }

    #[test]
    fn postgres_page_revocation_source_uses_bounded_flags_and_exact_subject_arrays() {
        let source = include_str!("ats_certification_page.rs");
        let postgres = source
            .split_once("fn load_postgres_ats_certification_page_revocations(")
            .expect("PostgreSQL page revocation loader")
            .1
            .split_once("fn load_postgres_ats_certification_page_quarantines(")
            .expect("PostgreSQL page revocation loader boundary")
            .0;
        assert!(postgres.contains("LIMIT 1 FOR SHARE OF revocation"));
        assert!(postgres.contains("revocation.subject_kind='policy'"));
        assert!(postgres.contains("revocation.subject_kind='trust_key'"));
        assert!(postgres.contains("unnest($3::TEXT[],$4::TEXT[],$5::TEXT[])"));
        assert!(postgres.contains("JOIN LATERAL ("));
        assert!(!postgres.contains("SELECT key_id FROM jobs_ats_certification_trust_keys"));
        assert!(!postgres.contains(
            "SELECT subject_kind, subject_id, subject_sha256\n               FROM"
        ));
    }

    #[test]
    fn page_mutable_authority_queries_are_exact_and_primary_key_aligned() {
        let source = include_str!("ats_certification_page.rs");
        let sqlite = source
            .split_once("fn load_sqlite_ats_certification_page_allowlists(")
            .expect("SQLite exact allowlist loader")
            .1
            .split_once("fn load_postgres_ats_certification_page_allowlists(")
            .expect("SQLite mutable-authority loader boundary")
            .0;
        assert!(sqlite.contains("allowlist.allowlist_sha256=requested.allowlist_sha256"));
        assert!(sqlite.contains("member.account_id=?1"));
        assert!(sqlite.contains("head.scope_kind=requested.scope_kind"));
        assert!(sqlite.contains("head.scope_id=requested.scope_id"));
        assert!(sqlite.contains("head.scope_sha256=requested.scope_sha256"));
        assert!(sqlite.contains("head.subject_key=requested.subject_key"));

        let postgres = source
            .split_once("fn load_postgres_ats_certification_page_allowlists(")
            .expect("PostgreSQL exact allowlist loader")
            .1
            .split_once("fn resolve_ats_certification_page_target(")
            .expect("PostgreSQL mutable-authority loader boundary")
            .0;
        assert!(postgres.contains("unnest($3::TEXT[]) requested(allowlist_sha256)"));
        assert!(postgres.contains("member.account_id=$1"));
        assert!(postgres.contains(
            "unnest($2::TEXT[],$3::TEXT[],$4::TEXT[])\n                    requested(scope_kind,scope_id,scope_sha256)"
        ));
        assert!(postgres.contains("head.scope_sha256=requested.scope_sha256"));
        assert!(postgres.contains(
            "unnest($1::TEXT[],$2::TEXT[]) requested(scope_kind,subject_key)"
        ));
        assert!(!postgres.contains("WHERE member.account_id=$1\n              ORDER BY member"));
        assert!(!postgres.contains("WHERE head.scope_id = ANY"));
        assert!(!postgres.contains("WHERE head.subject_key = ANY"));
    }

    #[test]
    fn sqlite_page_mutable_authority_ignores_high_cardinality_irrelevant_history() {
        let (pool, _, _) = install_one_page_authority();
        let policy_sha256 = fixture_policy_sha256(&pool);
        let mut connection = pool.get().unwrap();
        let transaction = connection.transaction().unwrap();
        for index in 0..512_i64 {
            let allowlist = ats_certification_sha256(
                format!("phase626-irrelevant-allowlist-{index}").as_bytes(),
            );
            transaction.execute(
                "INSERT INTO jobs_ats_certification_canary_allowlists(
                    allowlist_sha256,allowlist_id,canonical_allowlist_base64url,
                    member_count,approval_ref,not_before_ms,expires_at_ms,
                    approved_by,approved_at_ms)
                 VALUES(?1,?2,?3,1,'phase626-test',?4,?5,'phase626-test',?4)",
                params![
                    allowlist,
                    format!("phase626-unused-allowlist-{index}"),
                    format!("phase626-unused-allowlist-payload-{index}"),
                    PAGE_NOW_MS - 1,
                    PAGE_NOW_MS + 1,
                ],
            ).unwrap();
            transaction.execute(
                "INSERT INTO jobs_ats_certification_canary_allowlist_members(
                    allowlist_sha256,ordinal,account_id) VALUES(?1,0,?2)",
                params![allowlist, "acct-phase626-page"],
            ).unwrap();

            let scope_id = format!("phase626-unused-quarantine-{index}");
            let scope_sha256 = ats_certification_sha256(scope_id.as_bytes());
            let command_sha256 = ats_certification_sha256(
                format!("phase626-unused-command-{index}").as_bytes(),
            );
            transaction.execute(
                "INSERT INTO jobs_ats_certification_quarantine_commands(
                    command_sha256,command_id,command_generation,scope_kind,scope_id,
                    scope_sha256,command_sequence,predecessor_command_sha256,action,
                    reason_ref,canonical_command_base64url,authorization_sha256,
                    trust_policy_sha256,canonical_authorization_base64url,
                    issued_at_ms,recorded_by,recorded_at_ms)
                 VALUES(?1,?2,1,'target',?3,?4,1,NULL,'quarantine','phase626-test',
                    'fixture',?5,?6,'fixture',?7,'phase626-test',?7)",
                params![
                    command_sha256,
                    format!("phase626-unused-command-{index}"),
                    scope_id,
                    scope_sha256,
                    ats_certification_sha256(
                        format!("phase626-unused-command-auth-{index}").as_bytes(),
                    ),
                    policy_sha256,
                    PAGE_NOW_MS,
                ],
            ).unwrap();
            transaction.execute(
                "INSERT INTO jobs_ats_certification_quarantine_heads(
                    scope_kind,scope_id,scope_sha256,head_revision,current_command_sha256,
                    current_command_sequence,state,updated_by,updated_at_ms)
                 VALUES('target',?1,?2,1,?3,1,'quarantined','phase626-test',?4)",
                params![scope_id, scope_sha256, command_sha256, PAGE_NOW_MS],
            ).unwrap();

            let event_id = format!("phase626-unused-event-{index}");
            let subject_key = format!("phase626-unused-circuit-{index}");
            transaction.execute(
                "INSERT INTO jobs_ats_certification_circuit_events(
                    event_id,event_sha256,canonical_event_base64url,scope_kind,subject_key,
                    transition,trigger_kind,window_started_at_ms,window_ended_at_ms,
                    failure_count,sample_count,threshold_count,authority_ref,event_at_ms,
                    recorded_by,recorded_at_ms)
                 VALUES(?1,?2,'fixture','target',?3,'opened','evidence_failure',?4,?4,
                    1,1,1,'phase626-test',?4,'phase626-test',?4)",
                params![
                    event_id,
                    ats_certification_sha256(
                        format!("phase626-unused-event-sha-{index}").as_bytes(),
                    ),
                    subject_key,
                    PAGE_NOW_MS,
                ],
            ).unwrap();
            transaction.execute(
                "INSERT INTO jobs_ats_certification_circuit_heads(
                    scope_kind,subject_key,head_revision,current_event_id,state,updated_at_ms)
                 VALUES('target',?1,1,?2,'opened',?3)",
                params![subject_key, event_id, PAGE_NOW_MS],
            ).unwrap();
        }

        assert!(load_sqlite_ats_certification_page_allowlists(
            &transaction,
            "acct-phase626-page",
            &["f".repeat(64)],
            PAGE_NOW_MS,
        ).unwrap().is_empty());
        assert!(load_sqlite_ats_certification_page_quarantines(
            &transaction,
            Some(&policy_sha256),
            &BTreeSet::from([("target".to_string(), "wanted".to_string(), "e".repeat(64))]),
        ).unwrap().is_empty());
        assert!(load_sqlite_ats_certification_page_circuits(
            &transaction,
            &BTreeSet::from([("target".to_string(), "wanted".to_string())]),
        ).unwrap().is_empty());
        let quarantine_plan = transaction
            .prepare(
                "EXPLAIN QUERY PLAN SELECT current_command_sha256
                   FROM jobs_ats_certification_quarantine_heads
                  WHERE scope_kind=?1 AND scope_id=?2 AND scope_sha256=?3",
            )
            .unwrap()
            .query_map(params!["target", "wanted", "e".repeat(64)], |row| {
                row.get::<_, String>(3)
            })
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap()
            .join(" ");
        assert!(quarantine_plan.contains(
            "sqlite_autoindex_jobs_ats_certification_quarantine_heads_"
        ));
        let circuit_plan = transaction
            .prepare(
                "EXPLAIN QUERY PLAN SELECT current_event_id
                   FROM jobs_ats_certification_circuit_heads
                  WHERE scope_kind=?1 AND subject_key=?2",
            )
            .unwrap()
            .query_map(params!["target", "wanted"], |row| row.get::<_, String>(3))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap()
            .join(" ");
        assert!(circuit_plan.contains(
            "sqlite_autoindex_jobs_ats_certification_circuit_heads_"
        ));
        transaction.rollback().unwrap();
    }

    #[test]
    fn sqlite_fifty_positive_targets_use_one_bounded_ats_page_snapshot() {
        let pool = page_pool();
        let postings = (0..50).map(page_posting).collect::<Vec<_>>();
        for (index, posting) in postings.iter().enumerate() {
            let target = ats_certification_fresh_target_evidence_from_posting(
                posting,
                PAGE_NOW_MS,
            )
            .expect("derive positive page target");
            let surface = AtsObservedSurface {
                variant_key: "greenhouse_public".to_string(),
                layout_contract_version: 1,
                surface_sha256: ats_certification_sha256(
                    format!("phase626-page-surface-{index}").as_bytes(),
                ),
            };
            ats_certification_authority_tests::install_signed_ats_authority_fixture(
                &pool,
                target,
                surface,
                page_runtime(),
                PAGE_NOW_MS,
            )
            .expect("install positive page ATS authority");
        }

        let mut connection = pool.get().expect("get ATS page connection");
        PAGE_SELECTS.store(0, Ordering::Relaxed);
        connection.trace(Some(count_page_selects));
        let transaction = connection.transaction().expect("begin ATS page snapshot");
        let resolutions = resolve_ats_certification_for_postings_page_sqlite_tx(
            &transaction,
            "acct-phase626-page",
            &postings,
            PAGE_NOW_MS,
        )
        .expect("resolve positive ATS page");
        transaction.rollback().expect("roll back ATS page snapshot");
        connection.trace(None);

        assert_eq!(resolutions.len(), 50);
        assert!(resolutions.iter().all(|resolution| {
            resolution.as_ref().is_ok_and(|value| {
                value.status.status == "active" && value.active_binding.is_some()
            })
        }));
        // Trust, heads, evidence, layouts, runtimes, one global revocation
        // flag, exact page revocations, quarantine, and circuits are nine
        // bounded reads. General-channel fixtures have no canary membership
        // query. The former scalar loop performs hundreds here.
        assert_eq!(PAGE_SELECTS.load(Ordering::Relaxed), 9);

        for posting in [&postings[0], &postings[49]] {
            let scalar = resolve_ats_certification_for_posting(
                &pool,
                "acct-phase626-page",
                posting,
                None,
                PAGE_NOW_MS,
            )
            .expect("resolve scalar comparison authority");
            assert_eq!(scalar.status.status, "active");
            assert!(scalar.active_binding.is_some());
        }
    }

    #[test]
    fn sqlite_fifty_positive_compositions_have_a_bounded_statement_budget() {
        let pool = page_pool();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO accounts (id,email,password_hash,trial_seconds_remaining)
                 VALUES ('acct-phase626-composed','phase626@example.com','hash',0)",
                [],
            )
            .unwrap();
        let identity = ensure_primary_application_identity(
            &pool,
            "acct-phase626-composed",
            "phase626@example.com",
        )
        .unwrap();
        upsert_track(
            &pool,
            "acct-phase626-composed",
            &CareerTrack {
                id: "track-phase626-composed".to_string(),
                name: "Software engineering".to_string(),
                role: "Software Engineer".to_string(),
                locations: vec!["New York, NY".to_string()],
                remote_preference: "hybrid_ok".to_string(),
                application_identity_id: Some(identity.id),
                policy: CareerTrackPolicy {
                    role_family: "software_engineering".to_string(),
                    ..CareerTrackPolicy::default()
                },
                active: true,
                match_count: 0,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        )
        .unwrap();
        let mut imported = page_posting(0);
        imported.id.clear();
        imported.canonical_key.clear();
        imported.source = "greenhouse_import".to_string();
        imported.external_id = "phase626-composed".to_string();
        imported.company = "Acme".to_string();
        imported.title = "Software Engineer".to_string();
        imported.location = "New York, NY".to_string();
        imported.workplace = "hybrid".to_string();
        imported.canonical_url =
            "https://boards.greenhouse.io/acme/jobs/phase626-composed".to_string();
        imported.description =
            "Build reliable products with Rust and TypeScript.".to_string();
        imported.compensation = "$170k-$200k".to_string();
        imported.employment_type = "full_time".to_string();
        imported.track_id = "track-phase626-composed".to_string();
        imported.match_score = 0;
        imported.matched_reasons.clear();
        imported.missing_requirements.clear();
        imported.availability_status = "active".to_string();
        imported.status = "matched".to_string();
        imported.discovery_evidence = JobDiscoveryEvidence::default();
        let fixture_now_ms = now_ms();
        imported.posted_at_ms = Some(fixture_now_ms - 60_000);
        imported.last_verified_at_ms = Some(fixture_now_ms);
        imported.created_at_ms = 0;
        imported.updated_at_ms = 0;
        let profile = default_profile("phase626@example.com");
        let preferences = JobPreferences::default();
        let (posting, managed) =
            production_positive_authority_fixture::save_production_positive_verified_import(
                &pool,
                "acct-phase626-composed",
                &imported,
                &profile,
                &preferences,
            );
        production_positive_authority_fixture::install_production_positive_job_authorities(
            &pool,
            "acct-phase626-composed",
            &posting,
            &managed,
            "acme.com",
            "phase626-composed",
        );

        let postings = vec![posting; 50];
        let mut connection = pool.get().expect("get composed page connection");
        let db_time_ms: i64 = connection
            .query_row("SELECT CAST(unixepoch('subsec') * 1000 AS INTEGER)", [], |row| {
                row.get(0)
            })
            .expect("sample composed page database time");
        COMPOSED_PAGE_SELECTS.store(0, Ordering::Relaxed);
        connection.trace(Some(count_composed_page_selects));
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .expect("begin composed page snapshot");
        let projections = resolve_composed_job_integrity_projection_page_sqlite_tx_at_ms(
            &transaction,
            "acct-phase626-composed",
            &postings,
            db_time_ms,
        )
        .expect("resolve fully positive composed page");
        transaction.rollback().expect("roll back composed page snapshot");
        connection.trace(None);
        assert_eq!(projections.len(), 50);
        assert!(projections.iter().all(|projection| {
            projection
                .ats_certification
                .as_ref()
                .is_some_and(|ats| ats.status.status == "active")
                && projection.job_integrity.status == JobIntegrityResolutionStatus::Verified
        }));
        let select_count = COMPOSED_PAGE_SELECTS.load(Ordering::Relaxed);
        eprintln!("fully positive composed page performed {select_count} SELECTs");
        assert!(
            select_count <= 24,
            "fully positive composed page performed {select_count} SELECTs"
        );
    }
}
