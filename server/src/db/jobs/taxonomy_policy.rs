const TRACK_POLICY_CANONICALIZER_SCHEMA_VERSION: i64 = 2;
const TRACK_POLICY_CANONICALIZER_CONTRACT: &str =
    "bluey-jobs-career-track-policy/canonical-json-v2/generation-bound";
const TRACK_POLICY_ACTIVATION_LOCK_SQL: &str = "SELECT pg_advisory_xact_lock(hashtextextended(\
        'jobs-track-policy-taxonomy-activation-v1', 0))";

#[derive(Debug, Clone)]
struct TaxonomyActivationHead {
    activation_epoch: i64,
    taxonomy_version: String,
    taxonomy_sha256: String,
    canonicalizer_schema_version: i64,
    canonicalizer_sha256: String,
    transition_sha256: String,
}

#[derive(Debug, Clone)]
struct SemanticInputHead {
    generation: i64,
    transition_sha256: String,
    semantic_sha256: Option<String>,
}

#[derive(Debug, Clone)]
struct CanonicalPolicyGenerations {
    activation: TaxonomyActivationHead,
    account: SemanticInputHead,
    track: SemanticInputHead,
}

#[derive(Debug, Clone)]
struct CanonicalTrackPolicyRecord {
    taxonomy_version: String,
    taxonomy_sha256: String,
    taxonomy_activation_epoch: i64,
    canonicalizer_schema_version: i64,
    canonicalizer_sha256: String,
    account_input_generation: i64,
    account_input_transition_sha256: String,
    account_semantic_sha256: String,
    track_input_generation: i64,
    track_input_transition_sha256: String,
    track_semantic_sha256: String,
    canonical_policy_json: String,
    canonical_policy_sha256: String,
    canonical_role_id: String,
    canonical_role_family_id: String,
    application_identity_id: String,
    application_identity_sha256: String,
    source_resume_asset_id: String,
    source_resume_sha256: String,
    job_preferences_sha256: String,
}

#[derive(Debug, Clone)]
struct PersistedTrackPolicyHead {
    revision_id: String,
    revision_no: i64,
    canonical_policy_sha256: String,
    head_generation: i64,
    head_transition_sha256: String,
    review_receipt_id: String,
    review_receipt_sha256: String,
}

#[derive(Debug, Clone)]
struct StoredTrackPolicyHead {
    head_generation: i64,
    previous_head_generation: i64,
    revision_id: String,
    revision_no: i64,
    canonical_policy_sha256: String,
    taxonomy_sha256: String,
    taxonomy_activation_epoch: i64,
    canonicalizer_schema_version: i64,
    canonicalizer_sha256: String,
    account_input_generation: i64,
    account_input_transition_sha256: String,
    account_semantic_sha256: String,
    track_input_generation: i64,
    track_input_transition_sha256: String,
    track_semantic_sha256: String,
    application_identity_sha256: String,
    source_resume_sha256: String,
    job_preferences_sha256: String,
    review_receipt_id: String,
    review_receipt_sha256: String,
    head_transition_sha256: String,
    predecessor_head_transition_sha256: Option<String>,
    updated_by: String,
    updated_at_ms: i64,
}

#[derive(Debug, Clone)]
struct StoredTrackPolicyLedger {
    head: StoredTrackPolicyHead,
    taxonomy_version: String,
    canonical_policy_ciphertext: String,
    canonical_role_id: String,
    canonical_role_family_id: String,
    application_identity_id: String,
    source_resume_asset_id: String,
    predecessor_revision_id: Option<String>,
    predecessor_revision_no: Option<i64>,
    predecessor_policy_sha256: Option<String>,
    compatibility_classification: String,
    revision_review_state: String,
    revision_created_by: String,
    revision_created_at_ms: i64,
    canonical_review_receipt_ciphertext: String,
    reviewer_id: String,
    decision: String,
    decided_at_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalTrackPolicyRevisionExport {
    pub revision_id: String,
    pub career_track_id: String,
    pub revision_no: i64,
    pub taxonomy_version: String,
    pub taxonomy_sha256: String,
    pub taxonomy_activation_epoch: i64,
    pub canonicalizer_schema_version: i64,
    pub canonicalizer_sha256: String,
    pub account_input_generation: i64,
    pub account_input_transition_sha256: String,
    pub account_semantic_sha256: String,
    pub track_input_generation: i64,
    pub track_input_transition_sha256: String,
    pub track_semantic_sha256: String,
    pub canonical_policy_sha256: String,
    pub canonical_policy_json: String,
    pub canonical_role_id: String,
    pub canonical_role_family_id: String,
    pub application_identity_id: String,
    pub application_identity_sha256: String,
    pub source_resume_asset_id: String,
    pub source_resume_sha256: String,
    pub job_preferences_sha256: String,
    pub predecessor_revision_id: Option<String>,
    pub predecessor_revision_no: Option<i64>,
    pub predecessor_policy_sha256: Option<String>,
    pub compatibility_classification: String,
    pub review_state: String,
    pub created_by: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalTrackPolicyReviewReceiptExport {
    pub review_receipt_id: String,
    pub review_receipt_sha256: String,
    pub canonical_review_receipt_json: String,
    pub career_track_id: String,
    pub policy_revision_id: String,
    pub policy_revision_no: i64,
    pub canonical_policy_sha256: String,
    pub taxonomy_sha256: String,
    pub taxonomy_activation_epoch: i64,
    pub canonicalizer_schema_version: i64,
    pub canonicalizer_sha256: String,
    pub account_input_generation: i64,
    pub account_input_transition_sha256: String,
    pub account_semantic_sha256: String,
    pub track_input_generation: i64,
    pub track_input_transition_sha256: String,
    pub track_semantic_sha256: String,
    pub application_identity_sha256: String,
    pub source_resume_sha256: String,
    pub job_preferences_sha256: String,
    pub reviewer_id: String,
    pub decision: String,
    pub decided_at_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalTrackPolicyHeadExport {
    pub career_track_id: String,
    pub head_generation: i64,
    pub previous_head_generation: i64,
    pub policy_revision_id: String,
    pub policy_revision_no: i64,
    pub canonical_policy_sha256: String,
    pub taxonomy_sha256: String,
    pub taxonomy_activation_epoch: i64,
    pub canonicalizer_schema_version: i64,
    pub canonicalizer_sha256: String,
    pub account_input_generation: i64,
    pub account_input_transition_sha256: String,
    pub account_semantic_sha256: String,
    pub track_input_generation: i64,
    pub track_input_transition_sha256: String,
    pub track_semantic_sha256: String,
    pub application_identity_sha256: String,
    pub source_resume_sha256: String,
    pub job_preferences_sha256: String,
    pub review_receipt_id: String,
    pub review_receipt_sha256: String,
    pub head_transition_sha256: String,
    pub predecessor_head_transition_sha256: Option<String>,
    pub updated_by: String,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaxonomyActivationTransitionExport {
    pub activation_epoch: i64,
    pub previous_activation_epoch: i64,
    pub taxonomy_version: String,
    pub taxonomy_sha256: String,
    pub canonicalizer_schema_version: i64,
    pub canonicalizer_sha256: String,
    pub transition_sha256: String,
    pub predecessor_transition_sha256: Option<String>,
    pub activated_at_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInputTransitionExport {
    pub transition_id: String,
    pub generation: i64,
    pub previous_generation: i64,
    pub input_kind: String,
    pub subject_sha256: String,
    pub account_semantic_sha256: String,
    pub transition_sha256: String,
    pub predecessor_transition_sha256: Option<String>,
    pub changed_at_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackInputTransitionExport {
    pub transition_id: String,
    pub career_track_id: String,
    pub generation: i64,
    pub previous_generation: i64,
    pub track_semantic_sha256: String,
    pub transition_sha256: String,
    pub predecessor_transition_sha256: Option<String>,
    pub changed_at_ms: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalTrackPolicyLedgerExport {
    pub taxonomy_activation_transitions: Vec<TaxonomyActivationTransitionExport>,
    pub taxonomy_activation_head: Option<TaxonomyActivationTransitionExport>,
    pub account_input_transitions: Vec<AccountInputTransitionExport>,
    pub account_input_head: Option<AccountInputTransitionExport>,
    pub track_input_transitions: Vec<TrackInputTransitionExport>,
    pub track_input_heads: Vec<TrackInputTransitionExport>,
    pub revisions: Vec<CanonicalTrackPolicyRevisionExport>,
    pub review_receipts: Vec<CanonicalTrackPolicyReviewReceiptExport>,
    pub head_transitions: Vec<CanonicalTrackPolicyHeadExport>,
    pub heads: Vec<CanonicalTrackPolicyHeadExport>,
}

fn track_policy_sha256(value: impl AsRef<[u8]>) -> String {
    hex::encode(Sha256::digest(value.as_ref()))
}

fn track_policy_canonicalizer_sha256() -> String {
    track_policy_sha256(TRACK_POLICY_CANONICALIZER_CONTRACT)
}

#[allow(clippy::too_many_arguments)]
fn taxonomy_activation_transition_sha256(
    activation_epoch: i64,
    previous_activation_epoch: i64,
    taxonomy_version: &str,
    taxonomy_sha256: &str,
    canonicalizer_schema_version: i64,
    canonicalizer_sha256: &str,
    predecessor_transition_sha256: Option<&str>,
    activated_at_ms: i64,
) -> Result<String> {
    let canonical = serde_json::to_vec(&json!({
        "schema_version": 1,
        "activation_epoch": activation_epoch,
        "previous_activation_epoch": previous_activation_epoch,
        "taxonomy_version": taxonomy_version,
        "taxonomy_sha256": taxonomy_sha256,
        "canonicalizer_schema_version": canonicalizer_schema_version,
        "canonicalizer_sha256": canonicalizer_sha256,
        "predecessor_activation_transition_sha256": predecessor_transition_sha256,
        "activated_at_ms": activated_at_ms,
    }))
    .context("serialize taxonomy activation transition")?;
    Ok(track_policy_sha256(canonical))
}

fn current_taxonomy_tuple() -> (String, String, i64, String) {
    (
        crate::jobs_taxonomy::taxonomy_version().to_string(),
        crate::jobs_taxonomy::taxonomy_sha256(),
        TRACK_POLICY_CANONICALIZER_SCHEMA_VERSION,
        track_policy_canonicalizer_sha256(),
    )
}

pub(crate) fn activate_canonical_taxonomy_authority_sqlite(
    conn: &mut rusqlite::Connection,
) -> Result<()> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current = tx
        .query_row(
            "SELECT activation_epoch, taxonomy_version, taxonomy_digest_sha256,
                    canonicalizer_schema_version, canonicalizer_digest_sha256,
                    activation_transition_sha256, activated_at_ms
               FROM jobs_track_policy_taxonomy_activation_head
              WHERE singleton_id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            },
        )
        .optional()?;
    let (taxonomy_version, taxonomy_sha256, canonicalizer_version, canonicalizer_sha256) =
        current_taxonomy_tuple();
    if current.as_ref().is_some_and(|value| {
        value.1 == taxonomy_version
            && value.2 == taxonomy_sha256
            && value.3 == canonicalizer_version
            && value.4 == canonicalizer_sha256
    }) {
        load_taxonomy_activation_sqlite(&tx)
            .context("validate idempotent taxonomy activation ledger")?;
        tx.commit()?;
        return Ok(());
    }
    let epoch = current.as_ref().map_or(1, |value| value.0 + 1);
    let previous_epoch = current.as_ref().map_or(0, |value| value.0);
    let predecessor = current.as_ref().map(|value| value.5.as_str());
    let activated_at_ms = now_ms().max(current.as_ref().map_or(0, |value| value.6));
    let transition = taxonomy_activation_transition_sha256(
        epoch,
        previous_epoch,
        &taxonomy_version,
        &taxonomy_sha256,
        canonicalizer_version,
        &canonicalizer_sha256,
        predecessor,
        activated_at_ms,
    )?;
    tx.execute(
        "INSERT INTO jobs_track_policy_taxonomy_activation_events (
            activation_epoch, previous_activation_epoch, taxonomy_version,
            taxonomy_digest_sha256, canonicalizer_schema_version,
            canonicalizer_digest_sha256, activation_transition_sha256,
            predecessor_activation_transition_sha256, activated_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            epoch,
            previous_epoch,
            taxonomy_version,
            taxonomy_sha256,
            canonicalizer_version,
            canonicalizer_sha256,
            transition,
            predecessor,
            activated_at_ms,
        ],
    )?;
    if current.is_some() {
        let changed = tx.execute(
            "UPDATE jobs_track_policy_taxonomy_activation_head SET
                activation_epoch = ?1, previous_activation_epoch = ?2,
                taxonomy_version = ?3, taxonomy_digest_sha256 = ?4,
                canonicalizer_schema_version = ?5,
                canonicalizer_digest_sha256 = ?6,
                activation_transition_sha256 = ?7,
                predecessor_activation_transition_sha256 = ?8,
                activated_at_ms = ?9
              WHERE singleton_id = 1 AND activation_epoch = ?2
                AND activation_transition_sha256 = ?8",
            params![
                epoch,
                previous_epoch,
                taxonomy_version,
                taxonomy_sha256,
                canonicalizer_version,
                canonicalizer_sha256,
                transition,
                predecessor,
                activated_at_ms,
            ],
        )?;
        anyhow::ensure!(changed == 1, "taxonomy activation changed during startup");
    } else {
        tx.execute(
            "INSERT INTO jobs_track_policy_taxonomy_activation_head (
                singleton_id, activation_epoch, previous_activation_epoch,
                taxonomy_version, taxonomy_digest_sha256,
                canonicalizer_schema_version, canonicalizer_digest_sha256,
                activation_transition_sha256,
                predecessor_activation_transition_sha256, activated_at_ms
             ) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8)",
            params![
                epoch,
                previous_epoch,
                taxonomy_version,
                taxonomy_sha256,
                canonicalizer_version,
                canonicalizer_sha256,
                transition,
                activated_at_ms,
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub(crate) fn activate_canonical_taxonomy_authority_postgres(
    conn: &mut postgres::Client,
) -> Result<()> {
    let mut tx = conn.transaction()?;
    tx.query_one(TRACK_POLICY_ACTIVATION_LOCK_SQL, &[])?;
    let current = tx.query_opt(
        "SELECT activation_epoch, taxonomy_version, taxonomy_digest_sha256,
                canonicalizer_schema_version, canonicalizer_digest_sha256,
                activation_transition_sha256, activated_at_ms
           FROM jobs_track_policy_taxonomy_activation_head
          WHERE singleton_id = 1 FOR UPDATE",
        &[],
    )?;
    let (taxonomy_version, taxonomy_sha256, canonicalizer_version, canonicalizer_sha256) =
        current_taxonomy_tuple();
    if current.as_ref().is_some_and(|row| {
        row.get::<_, String>(1) == taxonomy_version
            && row.get::<_, String>(2) == taxonomy_sha256
            && row.get::<_, i64>(3) == canonicalizer_version
            && row.get::<_, String>(4) == canonicalizer_sha256
    }) {
        load_taxonomy_activation_postgres(&mut tx)
            .context("validate idempotent taxonomy activation ledger")?;
        tx.commit()?;
        return Ok(());
    }
    let epoch = current.as_ref().map_or(1, |row| row.get::<_, i64>(0) + 1);
    let previous_epoch = current.as_ref().map_or(0, |row| row.get(0));
    let predecessor = current.as_ref().map(|row| row.get::<_, String>(5));
    let activated_at_ms = now_ms().max(current.as_ref().map_or(0, |row| row.get::<_, i64>(6)));
    let transition = taxonomy_activation_transition_sha256(
        epoch,
        previous_epoch,
        &taxonomy_version,
        &taxonomy_sha256,
        canonicalizer_version,
        &canonicalizer_sha256,
        predecessor.as_deref(),
        activated_at_ms,
    )?;
    tx.execute(
        "INSERT INTO jobs_track_policy_taxonomy_activation_events (
            activation_epoch, previous_activation_epoch, taxonomy_version,
            taxonomy_digest_sha256, canonicalizer_schema_version,
            canonicalizer_digest_sha256, activation_transition_sha256,
            predecessor_activation_transition_sha256, activated_at_ms
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        &[
            &epoch,
            &previous_epoch,
            &taxonomy_version,
            &taxonomy_sha256,
            &canonicalizer_version,
            &canonicalizer_sha256,
            &transition,
            &predecessor,
            &activated_at_ms,
        ],
    )?;
    if current.is_some() {
        let changed = tx.execute(
            "UPDATE jobs_track_policy_taxonomy_activation_head SET
                activation_epoch = $1, previous_activation_epoch = $2,
                taxonomy_version = $3, taxonomy_digest_sha256 = $4,
                canonicalizer_schema_version = $5,
                canonicalizer_digest_sha256 = $6,
                activation_transition_sha256 = $7,
                predecessor_activation_transition_sha256 = $8,
                activated_at_ms = $9
              WHERE singleton_id = 1 AND activation_epoch = $2
                AND activation_transition_sha256 = $8",
            &[
                &epoch,
                &previous_epoch,
                &taxonomy_version,
                &taxonomy_sha256,
                &canonicalizer_version,
                &canonicalizer_sha256,
                &transition,
                &predecessor,
                &activated_at_ms,
            ],
        )?;
        anyhow::ensure!(changed == 1, "taxonomy activation changed during startup");
    } else {
        tx.execute(
            "INSERT INTO jobs_track_policy_taxonomy_activation_head (
                singleton_id, activation_epoch, previous_activation_epoch,
                taxonomy_version, taxonomy_digest_sha256,
                canonicalizer_schema_version, canonicalizer_digest_sha256,
                activation_transition_sha256,
                predecessor_activation_transition_sha256, activated_at_ms
             ) VALUES (1, $1, $2, $3, $4, $5, $6, $7, NULL, $8)",
            &[
                &epoch,
                &previous_epoch,
                &taxonomy_version,
                &taxonomy_sha256,
                &canonicalizer_version,
                &canonicalizer_sha256,
                &transition,
                &activated_at_ms,
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

fn load_taxonomy_activation_sqlite(conn: &rusqlite::Connection) -> Result<TaxonomyActivationHead> {
    let (activation, previous_epoch, predecessor, activated_at_ms) = conn.query_row(
        "SELECT activation_epoch, taxonomy_version, taxonomy_digest_sha256,
                canonicalizer_schema_version, canonicalizer_digest_sha256,
                activation_transition_sha256, previous_activation_epoch,
                predecessor_activation_transition_sha256, activated_at_ms
           FROM jobs_track_policy_taxonomy_activation_head AS head
          WHERE singleton_id = 1 AND EXISTS (
            SELECT 1 FROM jobs_track_policy_taxonomy_activation_events AS event
             WHERE event.activation_epoch = head.activation_epoch
               AND event.previous_activation_epoch = head.previous_activation_epoch
               AND event.taxonomy_version = head.taxonomy_version
               AND event.taxonomy_digest_sha256 = head.taxonomy_digest_sha256
               AND event.canonicalizer_schema_version =
                   head.canonicalizer_schema_version
               AND event.canonicalizer_digest_sha256 =
                   head.canonicalizer_digest_sha256
               AND event.activation_transition_sha256 =
                   head.activation_transition_sha256
               AND event.predecessor_activation_transition_sha256
                     IS head.predecessor_activation_transition_sha256
               AND event.activated_at_ms = head.activated_at_ms
          )",
        [],
        |row| {
            Ok((
                TaxonomyActivationHead {
                    activation_epoch: row.get(0)?,
                    taxonomy_version: row.get(1)?,
                    taxonomy_sha256: row.get(2)?,
                    canonicalizer_schema_version: row.get(3)?,
                    canonicalizer_sha256: row.get(4)?,
                    transition_sha256: row.get(5)?,
                },
                row.get::<_, i64>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, i64>(8)?,
            ))
        },
    )?;
    let transition = taxonomy_activation_transition_sha256(
        activation.activation_epoch,
        previous_epoch,
        &activation.taxonomy_version,
        &activation.taxonomy_sha256,
        activation.canonicalizer_schema_version,
        &activation.canonicalizer_sha256,
        predecessor.as_deref(),
        activated_at_ms,
    )?;
    anyhow::ensure!(
        transition == activation.transition_sha256,
        "taxonomy activation transition digest is invalid"
    );
    require_current_taxonomy_activation(&activation)?;
    Ok(activation)
}

fn load_taxonomy_activation_postgres<C: postgres::GenericClient>(
    client: &mut C,
) -> Result<TaxonomyActivationHead> {
    let row = client.query_one(
        "SELECT activation_epoch, taxonomy_version, taxonomy_digest_sha256,
                canonicalizer_schema_version, canonicalizer_digest_sha256,
                activation_transition_sha256, previous_activation_epoch,
                predecessor_activation_transition_sha256, activated_at_ms
           FROM jobs_track_policy_taxonomy_activation_head AS head
          WHERE singleton_id = 1 AND EXISTS (
            SELECT 1 FROM jobs_track_policy_taxonomy_activation_events AS event
             WHERE event.activation_epoch = head.activation_epoch
               AND event.previous_activation_epoch = head.previous_activation_epoch
               AND event.taxonomy_version = head.taxonomy_version
               AND event.taxonomy_digest_sha256 = head.taxonomy_digest_sha256
               AND event.canonicalizer_schema_version =
                   head.canonicalizer_schema_version
               AND event.canonicalizer_digest_sha256 =
                   head.canonicalizer_digest_sha256
               AND event.activation_transition_sha256 =
                   head.activation_transition_sha256
               AND event.predecessor_activation_transition_sha256
                     IS NOT DISTINCT FROM head.predecessor_activation_transition_sha256
               AND event.activated_at_ms = head.activated_at_ms
          ) FOR SHARE OF head",
        &[],
    )?;
    let activation = TaxonomyActivationHead {
        activation_epoch: row.get(0),
        taxonomy_version: row.get(1),
        taxonomy_sha256: row.get(2),
        canonicalizer_schema_version: row.get(3),
        canonicalizer_sha256: row.get(4),
        transition_sha256: row.get(5),
    };
    let previous_epoch: i64 = row.get(6);
    let predecessor: Option<String> = row.get(7);
    let activated_at_ms: i64 = row.get(8);
    let transition = taxonomy_activation_transition_sha256(
        activation.activation_epoch,
        previous_epoch,
        &activation.taxonomy_version,
        &activation.taxonomy_sha256,
        activation.canonicalizer_schema_version,
        &activation.canonicalizer_sha256,
        predecessor.as_deref(),
        activated_at_ms,
    )?;
    anyhow::ensure!(
        transition == activation.transition_sha256,
        "taxonomy activation transition digest is invalid"
    );
    require_current_taxonomy_activation(&activation)?;
    Ok(activation)
}

fn require_current_taxonomy_activation(activation: &TaxonomyActivationHead) -> Result<()> {
    let (version, sha256, canonicalizer_version, canonicalizer_sha256) = current_taxonomy_tuple();
    anyhow::ensure!(
        activation.taxonomy_version == version
            && activation.taxonomy_sha256 == sha256
            && activation.canonicalizer_schema_version == canonicalizer_version
            && activation.canonicalizer_sha256 == canonicalizer_sha256,
        "compiled Career Track taxonomy/canonicalizer tuple is not activated"
    );
    anyhow::ensure!(
        activation.activation_epoch >= 1 && activation.transition_sha256.len() == 64,
        "taxonomy activation head is invalid"
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn semantic_input_transition_sha256(
    scope: &str,
    account_id: &str,
    track_id: Option<&str>,
    generation: i64,
    previous_generation: i64,
    input_kind: &str,
    semantic_sha256: &str,
    predecessor_transition_sha256: Option<&str>,
    changed_at_ms: i64,
) -> Result<String> {
    let canonical = serde_json::to_vec(&json!({
        "schema_version": 1,
        "scope": scope,
        "account_id": account_id,
        "career_track_id": track_id,
        "input_generation": generation,
        "previous_input_generation": previous_generation,
        "input_kind": input_kind,
        "semantic_sha256": semantic_sha256,
        "predecessor_input_transition_sha256": predecessor_transition_sha256,
        "changed_at_ms": changed_at_ms,
    }))?;
    Ok(track_policy_sha256(canonical))
}

fn strip_account_transport_fields(value: &mut Value, kind: &str) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    object.remove("updatedAtMs");
    object.remove("updated_at_ms");
    object.remove("createdAtMs");
    object.remove("created_at_ms");
    if kind == "profile" {
        object.remove("onboardingStep");
        object.remove("onboarding_step");
        object.remove("onboardingComplete");
        object.remove("onboarding_complete");
    }
}

type AccountSemanticFact = (
    String,
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    i64,
);
type AccountSemanticResume = (String, String, String, String, i64, Option<i64>, String);

fn account_semantic_sha256_from_parts(
    profile: Option<String>,
    preferences: Option<String>,
    facts: Vec<AccountSemanticFact>,
    identities: Vec<(String, String, String, i64)>,
    resume: Option<AccountSemanticResume>,
) -> Result<String> {
    let mut profile = profile
        .map(|raw| parse_json::<Value>(raw, "account semantic profile"))
        .transpose()?;
    if let Some(value) = profile.as_mut() {
        strip_account_transport_fields(value, "profile");
    }
    let mut preferences = preferences
        .map(|raw| parse_json::<Value>(raw, "account semantic preferences"))
        .transpose()?;
    if let Some(value) = preferences.as_mut() {
        strip_account_transport_fields(value, "preferences");
    }
    let facts = facts
        .into_iter()
        .map(
            |(id, category, label, raw, source, verification_status, confirmed_by, schema)| {
                // CareerFact.value is arbitrary user-owned semantic JSON. Keys
                // that resemble transport metadata remain authoritative here;
                // the actual relational transport timestamps are excluded by
                // the query that constructs AccountSemanticFact.
                let value = parse_json::<Value>(raw, "account semantic fact")?;
                Ok(json!({
                    "id": id,
                    "category": category,
                    "label": label,
                    "value": value,
                    "source": source,
                    "verification_status": verification_status,
                    "confirmed_by": confirmed_by,
                    "schema_version": schema,
                }))
            },
        )
        .collect::<Result<Vec<_>>>()?;
    let identities = identities
        .into_iter()
        .map(|(id, raw, verification_status, is_default)| {
            let mut value = parse_json::<Value>(raw, "account semantic identity")?;
            strip_account_transport_fields(&mut value, "identity");
            Ok(json!({
                "id": id,
                "identity": value,
                "verification_status": verification_status,
                "is_default": is_default != 0,
            }))
        })
        .collect::<Result<Vec<_>>>()?;
    let resume = resume.map(
        |(id, sha256, media_type, file_type, size_bytes, page_count, template_status)| {
            json!({
                "id": id,
                "sha256": sha256,
                "media_type": media_type,
                "file_type": file_type,
                "size_bytes": size_bytes,
                "page_count": page_count,
                "template_status": template_status,
            })
        },
    );
    let canonical = serde_json::to_vec(&json!({
        "schema_version": 1,
        "profile": profile,
        "preferences": preferences,
        "facts": facts,
        "application_identities": identities,
        "source_resume": resume,
    }))?;
    Ok(track_policy_sha256(canonical))
}

fn account_semantic_sha256_sqlite(conn: &rusqlite::Connection, account_id: &str) -> Result<String> {
    let profile = conn
        .query_row(
            "SELECT profile_json FROM jobs_profiles WHERE account_id = ?1",
            params![account_id],
            |row| row.get(0),
        )
        .optional()?;
    let preferences = conn
        .query_row(
            "SELECT preferences_json FROM jobs_preferences WHERE account_id = ?1",
            params![account_id],
            |row| row.get(0),
        )
        .optional()?;
    let facts = {
        let mut stmt = conn.prepare(
            "SELECT id, category, label, value_json, source,
                    verification_status, confirmed_by, schema_version
               FROM jobs_facts WHERE account_id = ?1 ORDER BY id",
        )?;
        let values = stmt
            .query_map(params![account_id], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        values
    };
    let identities = {
        let mut stmt = conn.prepare(
            "SELECT id, identity_json, verification_status, is_default
               FROM jobs_application_identities
              WHERE account_id = ?1 ORDER BY id",
        )?;
        let values = stmt
            .query_map(params![account_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        values
    };
    let resume = conn
        .query_row(
            "SELECT id, sha256, media_type, file_type, size_bytes, page_count,
                    template_status
               FROM jobs_resume_source_assets WHERE account_id = ?1",
            params![account_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .optional()?;
    account_semantic_sha256_from_parts(profile, preferences, facts, identities, resume)
}

fn account_semantic_sha256_postgres<C: postgres::GenericClient>(
    client: &mut C,
    account_id: &str,
) -> Result<String> {
    let profile = client
        .query_opt(
            "SELECT profile_json FROM jobs_profiles WHERE account_id = $1",
            &[&account_id],
        )?
        .map(|row| row.get(0));
    let preferences = client
        .query_opt(
            "SELECT preferences_json FROM jobs_preferences WHERE account_id = $1",
            &[&account_id],
        )?
        .map(|row| row.get(0));
    let facts = client
        .query(
            "SELECT id, category, label, value_json, source,
                    verification_status, confirmed_by, schema_version
               FROM jobs_facts WHERE account_id = $1 ORDER BY id",
            &[&account_id],
        )?
        .into_iter()
        .map(|row| {
            (
                row.get(0),
                row.get(1),
                row.get(2),
                row.get(3),
                row.get(4),
                row.get(5),
                row.get(6),
                row.get(7),
            )
        })
        .collect();
    let identities = client
        .query(
            "SELECT id, identity_json, verification_status, is_default
               FROM jobs_application_identities
              WHERE account_id = $1 ORDER BY id",
            &[&account_id],
        )?
        .into_iter()
        .map(|row| {
            let is_default: i32 = row.get(3);
            (row.get(0), row.get(1), row.get(2), i64::from(is_default))
        })
        .collect();
    let resume = client
        .query_opt(
            "SELECT id, sha256, media_type, file_type, size_bytes, page_count,
                    template_status
               FROM jobs_resume_source_assets WHERE account_id = $1",
            &[&account_id],
        )?
        .map(|row| {
            (
                row.get(0),
                row.get(1),
                row.get(2),
                row.get(3),
                row.get(4),
                row.get(5),
                row.get(6),
            )
        });
    account_semantic_sha256_from_parts(profile, preferences, facts, identities, resume)
}

fn lock_account_policy_inputs_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    exclusive: bool,
) -> Result<()> {
    let lock = if exclusive { "UPDATE" } else { "SHARE" };
    anyhow::ensure!(
        tx.query_opt(
            "SELECT 1 FROM accounts WHERE id = $1 FOR KEY SHARE",
            &[&account_id],
        )?
        .is_some(),
        "account not found"
    );
    for (table, order) in [
        ("jobs_track_policy_account_input_heads", "account_id"),
        ("jobs_profiles", "account_id"),
        ("jobs_preferences", "account_id"),
        ("jobs_facts", "id"),
        ("jobs_application_identities", "id"),
        ("jobs_resume_source_assets", "id"),
        ("jobs_tracks", "id"),
    ] {
        tx.query(
            &format!(
                "SELECT {order} FROM {table}
                  WHERE account_id = $1 ORDER BY {order} FOR {lock}"
            ),
            &[&account_id],
        )?;
    }
    Ok(())
}

fn validate_account_input_head_sqlite(
    conn: &rusqlite::Connection,
    account_id: &str,
) -> Result<SemanticInputHead> {
    let (generation, previous, kind, semantic, transition, predecessor, changed_at_ms) = conn
        .query_row(
            "SELECT head.input_generation, head.previous_input_generation,
                    head.input_kind, head.account_semantic_sha256,
                    head.input_transition_sha256,
                    head.predecessor_input_transition_sha256, head.updated_at_ms
               FROM jobs_track_policy_account_input_heads AS head
              WHERE head.account_id = ?1 AND EXISTS (
                SELECT 1 FROM jobs_track_policy_account_input_transitions AS event
                 WHERE event.account_id = head.account_id
                   AND event.input_generation = head.input_generation
                   AND event.previous_input_generation = head.previous_input_generation
                   AND event.input_transition_id = head.input_transition_id
                   AND event.input_kind = head.input_kind
                   AND event.input_subject_sha256 = head.input_subject_sha256
                   AND event.account_semantic_sha256 = head.account_semantic_sha256
                   AND event.input_transition_sha256 = head.input_transition_sha256
                   AND event.predecessor_input_transition_sha256
                         IS head.predecessor_input_transition_sha256
                   AND event.changed_at_ms = head.updated_at_ms
              )",
            params![account_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            },
        )
        .context("validate account semantic-input head evidence")?;
    let expected = semantic_input_transition_sha256(
        "account",
        account_id,
        None,
        generation,
        previous,
        &kind,
        &semantic,
        predecessor.as_deref(),
        changed_at_ms,
    )?;
    anyhow::ensure!(
        expected == transition,
        "account semantic-input transition is corrupt"
    );
    Ok(SemanticInputHead {
        generation,
        transition_sha256: transition,
        semantic_sha256: Some(semantic),
    })
}

fn validate_account_input_head_postgres<C: postgres::GenericClient>(
    client: &mut C,
    account_id: &str,
) -> Result<SemanticInputHead> {
    let row = client
        .query_opt(
            "SELECT head.input_generation, head.previous_input_generation,
                    head.input_kind, head.account_semantic_sha256,
                    head.input_transition_sha256,
                    head.predecessor_input_transition_sha256, head.updated_at_ms
               FROM jobs_track_policy_account_input_heads AS head
              WHERE head.account_id = $1 AND EXISTS (
                SELECT 1 FROM jobs_track_policy_account_input_transitions AS event
                 WHERE event.account_id = head.account_id
                   AND event.input_generation = head.input_generation
                   AND event.previous_input_generation = head.previous_input_generation
                   AND event.input_transition_id = head.input_transition_id
                   AND event.input_kind = head.input_kind
                   AND event.input_subject_sha256 = head.input_subject_sha256
                   AND event.account_semantic_sha256 = head.account_semantic_sha256
                   AND event.input_transition_sha256 = head.input_transition_sha256
                   AND event.predecessor_input_transition_sha256
                         IS NOT DISTINCT FROM head.predecessor_input_transition_sha256
                   AND event.changed_at_ms = head.updated_at_ms
              ) FOR SHARE OF head",
            &[&account_id],
        )?
        .context("validate account semantic-input head evidence")?;
    let generation = row.get(0);
    let previous = row.get(1);
    let kind: String = row.get(2);
    let semantic: String = row.get(3);
    let transition: String = row.get(4);
    let predecessor: Option<String> = row.get(5);
    let changed_at_ms = row.get(6);
    let expected = semantic_input_transition_sha256(
        "account",
        account_id,
        None,
        generation,
        previous,
        &kind,
        &semantic,
        predecessor.as_deref(),
        changed_at_ms,
    )?;
    anyhow::ensure!(
        expected == transition,
        "account semantic-input transition is corrupt"
    );
    Ok(SemanticInputHead {
        generation,
        transition_sha256: transition,
        semantic_sha256: Some(semantic),
    })
}

fn validate_track_input_head_sqlite(
    conn: &rusqlite::Connection,
    account_id: &str,
    track_id: &str,
) -> Result<SemanticInputHead> {
    let (generation, previous, semantic, transition, predecessor, changed_at_ms) = conn
        .query_row(
            "SELECT head.input_generation, head.previous_input_generation,
                    head.track_semantic_sha256, head.input_transition_sha256,
                    head.predecessor_input_transition_sha256, head.updated_at_ms
               FROM jobs_track_policy_track_input_heads AS head
              WHERE head.account_id = ?1 AND head.career_track_id = ?2
                AND EXISTS (
                  SELECT 1 FROM jobs_track_policy_track_input_transitions AS event
                   WHERE event.account_id = head.account_id
                     AND event.career_track_id = head.career_track_id
                     AND event.input_generation = head.input_generation
                     AND event.previous_input_generation = head.previous_input_generation
                     AND event.input_transition_id = head.input_transition_id
                     AND event.track_semantic_sha256 = head.track_semantic_sha256
                     AND event.input_transition_sha256 = head.input_transition_sha256
                     AND event.predecessor_input_transition_sha256
                           IS head.predecessor_input_transition_sha256
                     AND event.changed_at_ms = head.updated_at_ms
                )",
            params![account_id, track_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            },
        )
        .context("validate Track semantic-input head evidence")?;
    let expected = semantic_input_transition_sha256(
        "track",
        account_id,
        Some(track_id),
        generation,
        previous,
        "track_upsert",
        &semantic,
        predecessor.as_deref(),
        changed_at_ms,
    )?;
    anyhow::ensure!(
        expected == transition,
        "Track semantic-input transition is corrupt"
    );
    Ok(SemanticInputHead {
        generation,
        transition_sha256: transition,
        semantic_sha256: Some(semantic),
    })
}

fn validate_track_input_head_postgres<C: postgres::GenericClient>(
    client: &mut C,
    account_id: &str,
    track_id: &str,
) -> Result<SemanticInputHead> {
    let row = client
        .query_opt(
            "SELECT head.input_generation, head.previous_input_generation,
                    head.track_semantic_sha256, head.input_transition_sha256,
                    head.predecessor_input_transition_sha256, head.updated_at_ms
               FROM jobs_track_policy_track_input_heads AS head
              WHERE head.account_id = $1 AND head.career_track_id = $2
                AND EXISTS (
                  SELECT 1 FROM jobs_track_policy_track_input_transitions AS event
                   WHERE event.account_id = head.account_id
                     AND event.career_track_id = head.career_track_id
                     AND event.input_generation = head.input_generation
                     AND event.previous_input_generation = head.previous_input_generation
                     AND event.input_transition_id = head.input_transition_id
                     AND event.track_semantic_sha256 = head.track_semantic_sha256
                     AND event.input_transition_sha256 = head.input_transition_sha256
                     AND event.predecessor_input_transition_sha256
                           IS NOT DISTINCT FROM head.predecessor_input_transition_sha256
                     AND event.changed_at_ms = head.updated_at_ms
                ) FOR SHARE OF head",
            &[&account_id, &track_id],
        )?
        .context("validate Track semantic-input head evidence")?;
    let generation = row.get(0);
    let previous = row.get(1);
    let semantic: String = row.get(2);
    let transition: String = row.get(3);
    let predecessor: Option<String> = row.get(4);
    let changed_at_ms = row.get(5);
    let expected = semantic_input_transition_sha256(
        "track",
        account_id,
        Some(track_id),
        generation,
        previous,
        "track_upsert",
        &semantic,
        predecessor.as_deref(),
        changed_at_ms,
    )?;
    anyhow::ensure!(
        expected == transition,
        "Track semantic-input transition is corrupt"
    );
    Ok(SemanticInputHead {
        generation,
        transition_sha256: transition,
        semantic_sha256: Some(semantic),
    })
}

fn advance_account_input_generation_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    input_kind: &str,
    subject: &str,
    now: i64,
) -> Result<SemanticInputHead> {
    let current: Option<(i64, String, String, i64)> = tx
        .query_row(
            "SELECT input_generation, input_transition_sha256,
                    account_semantic_sha256, updated_at_ms
               FROM jobs_track_policy_account_input_heads WHERE account_id = ?1",
            params![account_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let account_semantic_sha256 = account_semantic_sha256_sqlite(tx, account_id)?;
    if let Some(current) = current
        .as_ref()
        .filter(|current| current.2 == account_semantic_sha256)
    {
        let validated = validate_account_input_head_sqlite(tx, account_id)?;
        anyhow::ensure!(
            validated.generation == current.0
                && validated.transition_sha256 == current.1
                && validated.semantic_sha256.as_deref() == Some(current.2.as_str()),
            "account semantic-input fast path is mismatched"
        );
        return Ok(validated);
    }
    let generation = current.as_ref().map_or(1, |value| value.0 + 1);
    let previous_generation = current.as_ref().map_or(0, |value| value.0);
    let predecessor = current.as_ref().map(|value| value.1.as_str());
    let changed_at_ms = now.max(current.as_ref().map_or(0, |value| value.3));
    let subject_sha256 = track_policy_sha256(subject);
    let transition_sha256 = semantic_input_transition_sha256(
        "account",
        account_id,
        None,
        generation,
        previous_generation,
        input_kind,
        &account_semantic_sha256,
        predecessor,
        changed_at_ms,
    )?;
    let transition_id = format!("account-input-{}", uuid::Uuid::new_v4().simple());
    tx.execute(
        "INSERT INTO jobs_track_policy_account_input_transitions (
            input_transition_id, account_id, input_generation,
            previous_input_generation, input_kind, input_subject_sha256,
            account_semantic_sha256,
            input_transition_sha256, predecessor_input_transition_sha256,
            changed_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            transition_id,
            account_id,
            generation,
            previous_generation,
            input_kind,
            subject_sha256,
            account_semantic_sha256,
            transition_sha256,
            predecessor,
            changed_at_ms,
        ],
    )?;
    if current.is_some() {
        let changed = tx.execute(
            "UPDATE jobs_track_policy_account_input_heads SET
                input_generation = ?2, previous_input_generation = ?3,
                input_transition_id = ?4, input_kind = ?5,
                input_subject_sha256 = ?6, account_semantic_sha256 = ?7,
                input_transition_sha256 = ?8,
                predecessor_input_transition_sha256 = ?9, updated_at_ms = ?10
              WHERE account_id = ?1 AND input_generation = ?3
                AND input_transition_sha256 = ?9",
            params![
                account_id,
                generation,
                previous_generation,
                transition_id,
                input_kind,
                subject_sha256,
                account_semantic_sha256,
                transition_sha256,
                predecessor,
                changed_at_ms,
            ],
        )?;
        anyhow::ensure!(changed == 1, "account semantic inputs changed concurrently");
    } else {
        tx.execute(
            "INSERT INTO jobs_track_policy_account_input_heads (
                account_id, input_generation, previous_input_generation,
                input_transition_id, input_kind, input_subject_sha256,
                account_semantic_sha256, input_transition_sha256,
                predecessor_input_transition_sha256,
                updated_at_ms
             ) VALUES (?1, 1, 0, ?2, ?3, ?4, ?5, ?6, NULL, ?7)",
            params![
                account_id,
                transition_id,
                input_kind,
                subject_sha256,
                account_semantic_sha256,
                transition_sha256,
                changed_at_ms,
            ],
        )?;
    }
    Ok(SemanticInputHead {
        generation,
        transition_sha256,
        semantic_sha256: Some(account_semantic_sha256),
    })
}

fn ensure_account_input_generation_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    now: i64,
) -> Result<SemanticInputHead> {
    let exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM jobs_track_policy_account_input_heads
                        WHERE account_id = ?1)",
        params![account_id],
        |row| row.get(0),
    )?;
    if exists {
        validate_account_input_head_sqlite(tx, account_id)
    } else {
        advance_account_input_generation_sqlite(tx, account_id, "baseline", "baseline", now)
    }
}

fn advance_account_input_generation_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    input_kind: &str,
    subject: &str,
    now: i64,
) -> Result<SemanticInputHead> {
    let current = tx.query_opt(
        "SELECT input_generation, input_transition_sha256,
                account_semantic_sha256, updated_at_ms
           FROM jobs_track_policy_account_input_heads
          WHERE account_id = $1 FOR UPDATE",
        &[&account_id],
    )?;
    let account_semantic_sha256 = account_semantic_sha256_postgres(tx, account_id)?;
    if let Some(current) = current
        .as_ref()
        .filter(|row| row.get::<_, String>(2) == account_semantic_sha256)
    {
        let validated = validate_account_input_head_postgres(tx, account_id)?;
        anyhow::ensure!(
            validated.generation == current.get::<_, i64>(0)
                && validated.transition_sha256 == current.get::<_, String>(1)
                && validated.semantic_sha256.as_deref()
                    == Some(current.get::<_, String>(2).as_str()),
            "account semantic-input fast path is mismatched"
        );
        return Ok(validated);
    }
    let generation = current.as_ref().map_or(1, |row| row.get::<_, i64>(0) + 1);
    let previous_generation = current.as_ref().map_or(0, |row| row.get(0));
    let predecessor = current.as_ref().map(|row| row.get::<_, String>(1));
    let changed_at_ms = now.max(current.as_ref().map_or(0, |row| row.get::<_, i64>(3)));
    let subject_sha256 = track_policy_sha256(subject);
    let transition_sha256 = semantic_input_transition_sha256(
        "account",
        account_id,
        None,
        generation,
        previous_generation,
        input_kind,
        &account_semantic_sha256,
        predecessor.as_deref(),
        changed_at_ms,
    )?;
    let transition_id = format!("account-input-{}", uuid::Uuid::new_v4().simple());
    tx.execute(
        "INSERT INTO jobs_track_policy_account_input_transitions (
            input_transition_id, account_id, input_generation,
            previous_input_generation, input_kind, input_subject_sha256,
            account_semantic_sha256,
            input_transition_sha256, predecessor_input_transition_sha256,
            changed_at_ms
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        &[
            &transition_id,
            &account_id,
            &generation,
            &previous_generation,
            &input_kind,
            &subject_sha256,
            &account_semantic_sha256,
            &transition_sha256,
            &predecessor,
            &changed_at_ms,
        ],
    )?;
    if current.is_some() {
        let changed = tx.execute(
            "UPDATE jobs_track_policy_account_input_heads SET
                input_generation = $2, previous_input_generation = $3,
                input_transition_id = $4, input_kind = $5,
                input_subject_sha256 = $6, account_semantic_sha256 = $7,
                input_transition_sha256 = $8,
                predecessor_input_transition_sha256 = $9, updated_at_ms = $10
              WHERE account_id = $1 AND input_generation = $3
                AND input_transition_sha256 = $9",
            &[
                &account_id,
                &generation,
                &previous_generation,
                &transition_id,
                &input_kind,
                &subject_sha256,
                &account_semantic_sha256,
                &transition_sha256,
                &predecessor,
                &changed_at_ms,
            ],
        )?;
        anyhow::ensure!(changed == 1, "account semantic inputs changed concurrently");
    } else {
        tx.execute(
            "INSERT INTO jobs_track_policy_account_input_heads (
                account_id, input_generation, previous_input_generation,
                input_transition_id, input_kind, input_subject_sha256,
                account_semantic_sha256, input_transition_sha256,
                predecessor_input_transition_sha256,
                updated_at_ms
             ) VALUES ($1, 1, 0, $2, $3, $4, $5, $6, NULL, $7)",
            &[
                &account_id,
                &transition_id,
                &input_kind,
                &subject_sha256,
                &account_semantic_sha256,
                &transition_sha256,
                &changed_at_ms,
            ],
        )?;
    }
    Ok(SemanticInputHead {
        generation,
        transition_sha256,
        semantic_sha256: Some(account_semantic_sha256),
    })
}

fn ensure_account_input_generation_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    now: i64,
) -> Result<SemanticInputHead> {
    let exists: bool = tx
        .query_one(
            "SELECT EXISTS(SELECT 1 FROM jobs_track_policy_account_input_heads
                            WHERE account_id = $1)",
            &[&account_id],
        )?
        .get(0);
    if exists {
        return validate_account_input_head_postgres(tx, account_id);
    }
    advance_account_input_generation_postgres(tx, account_id, "baseline", "baseline", now)
}

fn load_canonical_policy_generations_sqlite(
    conn: &rusqlite::Connection,
    account_id: &str,
    track: &CareerTrack,
) -> Result<CanonicalPolicyGenerations> {
    let activation = load_taxonomy_activation_sqlite(conn)?;
    let account = validate_account_input_head_sqlite(conn, account_id)?;
    let track_head = validate_track_input_head_sqlite(conn, account_id, &track.id)?;
    anyhow::ensure!(
        account.semantic_sha256.as_deref()
            == Some(account_semantic_sha256_sqlite(conn, account_id)?.as_str()),
        "account semantic-input generation is stale"
    );
    anyhow::ensure!(
        track_head.semantic_sha256.as_deref()
            == Some(track_semantic_policy_sha256(track)?.as_str()),
        "Track semantic-input generation is stale"
    );
    Ok(CanonicalPolicyGenerations {
        activation,
        account,
        track: track_head,
    })
}

fn load_canonical_policy_generations_postgres<C: postgres::GenericClient>(
    client: &mut C,
    account_id: &str,
    track: &CareerTrack,
) -> Result<CanonicalPolicyGenerations> {
    let activation = load_taxonomy_activation_postgres(client)?;
    let account = validate_account_input_head_postgres(client, account_id)?;
    let track_head = validate_track_input_head_postgres(client, account_id, &track.id)?;
    let account_semantic_sha256 = account_semantic_sha256_postgres(client, account_id)?;
    anyhow::ensure!(
        account.semantic_sha256.as_deref() == Some(account_semantic_sha256.as_str()),
        "account semantic-input generation is stale"
    );
    let track_semantic_sha256 = track_semantic_policy_sha256(track)?;
    anyhow::ensure!(
        track_head.semantic_sha256.as_deref() == Some(track_semantic_sha256.as_str()),
        "Track semantic-input generation is stale"
    );
    Ok(CanonicalPolicyGenerations {
        activation,
        account,
        track: track_head,
    })
}

fn track_semantic_policy_sha256(track: &CareerTrack) -> Result<String> {
    let canonical = serde_json::to_vec(&json!({
        "schema_version": 1,
        "id": track.id,
        "name": track.name,
        "role": track.role,
        "locations": track.locations,
        "remote_preference": track.remote_preference,
        "application_identity_id": track.application_identity_id,
        "role_family": track.policy.role_family,
        "relevant_employment_ids": track.policy.relevant_employment_ids,
        "employment_types": track.policy.employment_types,
        "engagement_types": track.policy.engagement_types,
        "work_authorizations": track.policy.work_authorizations,
        "active": track.active,
    }))?;
    Ok(track_policy_sha256(canonical))
}

fn advance_track_input_generation_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    track: &CareerTrack,
    now: i64,
) -> Result<SemanticInputHead> {
    let current: Option<(i64, String, String, i64)> = tx
        .query_row(
            "SELECT input_generation, input_transition_sha256,
                    track_semantic_sha256, updated_at_ms
               FROM jobs_track_policy_track_input_heads
              WHERE account_id = ?1 AND career_track_id = ?2",
            params![account_id, track.id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let semantic_sha256 = track_semantic_policy_sha256(track)?;
    if let Some(current) = current
        .as_ref()
        .filter(|current| current.2 == semantic_sha256)
    {
        let validated = validate_track_input_head_sqlite(tx, account_id, &track.id)?;
        anyhow::ensure!(
            validated.generation == current.0
                && validated.transition_sha256 == current.1
                && validated.semantic_sha256.as_deref() == Some(current.2.as_str()),
            "Track semantic-input fast path is mismatched"
        );
        return Ok(validated);
    }
    let generation = current.as_ref().map_or(1, |value| value.0 + 1);
    let previous_generation = current.as_ref().map_or(0, |value| value.0);
    let predecessor = current.as_ref().map(|value| value.1.as_str());
    let changed_at_ms = now.max(current.as_ref().map_or(0, |value| value.3));
    let transition_sha256 = semantic_input_transition_sha256(
        "track",
        account_id,
        Some(&track.id),
        generation,
        previous_generation,
        "track_upsert",
        &semantic_sha256,
        predecessor,
        changed_at_ms,
    )?;
    let transition_id = format!("track-input-{}", uuid::Uuid::new_v4().simple());
    tx.execute(
        "INSERT INTO jobs_track_policy_track_input_transitions (
            input_transition_id, account_id, career_track_id, input_generation,
            previous_input_generation, track_semantic_sha256,
            input_transition_sha256, predecessor_input_transition_sha256,
            changed_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            transition_id,
            account_id,
            track.id,
            generation,
            previous_generation,
            semantic_sha256,
            transition_sha256,
            predecessor,
            changed_at_ms,
        ],
    )?;
    if current.is_some() {
        let changed = tx.execute(
            "UPDATE jobs_track_policy_track_input_heads SET
                input_generation = ?3, previous_input_generation = ?4,
                input_transition_id = ?5, track_semantic_sha256 = ?6,
                input_transition_sha256 = ?7,
                predecessor_input_transition_sha256 = ?8, updated_at_ms = ?9
              WHERE account_id = ?1 AND career_track_id = ?2
                AND input_generation = ?4 AND input_transition_sha256 = ?8",
            params![
                account_id,
                track.id,
                generation,
                previous_generation,
                transition_id,
                semantic_sha256,
                transition_sha256,
                predecessor,
                changed_at_ms,
            ],
        )?;
        anyhow::ensure!(changed == 1, "Track semantic inputs changed concurrently");
    } else {
        tx.execute(
            "INSERT INTO jobs_track_policy_track_input_heads (
                account_id, career_track_id, input_generation,
                previous_input_generation, input_transition_id,
                track_semantic_sha256, input_transition_sha256,
                predecessor_input_transition_sha256, updated_at_ms
             ) VALUES (?1, ?2, 1, 0, ?3, ?4, ?5, NULL, ?6)",
            params![
                account_id,
                track.id,
                transition_id,
                semantic_sha256,
                transition_sha256,
                changed_at_ms,
            ],
        )?;
    }
    Ok(SemanticInputHead {
        generation,
        transition_sha256,
        semantic_sha256: Some(semantic_sha256),
    })
}

fn advance_track_input_generation_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    track: &CareerTrack,
    now: i64,
) -> Result<SemanticInputHead> {
    let current = tx.query_opt(
        "SELECT input_generation, input_transition_sha256,
                track_semantic_sha256, updated_at_ms
           FROM jobs_track_policy_track_input_heads
          WHERE account_id = $1 AND career_track_id = $2 FOR UPDATE",
        &[&account_id, &track.id],
    )?;
    let semantic_sha256 = track_semantic_policy_sha256(track)?;
    if let Some(current) = current
        .as_ref()
        .filter(|row| row.get::<_, String>(2) == semantic_sha256)
    {
        let validated = validate_track_input_head_postgres(tx, account_id, &track.id)?;
        anyhow::ensure!(
            validated.generation == current.get::<_, i64>(0)
                && validated.transition_sha256 == current.get::<_, String>(1)
                && validated.semantic_sha256.as_deref()
                    == Some(current.get::<_, String>(2).as_str()),
            "Track semantic-input fast path is mismatched"
        );
        return Ok(validated);
    }
    let generation = current.as_ref().map_or(1, |row| row.get::<_, i64>(0) + 1);
    let previous_generation = current.as_ref().map_or(0, |row| row.get(0));
    let predecessor = current.as_ref().map(|row| row.get::<_, String>(1));
    let changed_at_ms = now.max(current.as_ref().map_or(0, |row| row.get::<_, i64>(3)));
    let transition_sha256 = semantic_input_transition_sha256(
        "track",
        account_id,
        Some(&track.id),
        generation,
        previous_generation,
        "track_upsert",
        &semantic_sha256,
        predecessor.as_deref(),
        changed_at_ms,
    )?;
    let transition_id = format!("track-input-{}", uuid::Uuid::new_v4().simple());
    tx.execute(
        "INSERT INTO jobs_track_policy_track_input_transitions (
            input_transition_id, account_id, career_track_id, input_generation,
            previous_input_generation, track_semantic_sha256,
            input_transition_sha256, predecessor_input_transition_sha256,
            changed_at_ms
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        &[
            &transition_id,
            &account_id,
            &track.id,
            &generation,
            &previous_generation,
            &semantic_sha256,
            &transition_sha256,
            &predecessor,
            &changed_at_ms,
        ],
    )?;
    if current.is_some() {
        let changed = tx.execute(
            "UPDATE jobs_track_policy_track_input_heads SET
                input_generation = $3, previous_input_generation = $4,
                input_transition_id = $5, track_semantic_sha256 = $6,
                input_transition_sha256 = $7,
                predecessor_input_transition_sha256 = $8, updated_at_ms = $9
              WHERE account_id = $1 AND career_track_id = $2
                AND input_generation = $4 AND input_transition_sha256 = $8",
            &[
                &account_id,
                &track.id,
                &generation,
                &previous_generation,
                &transition_id,
                &semantic_sha256,
                &transition_sha256,
                &predecessor,
                &changed_at_ms,
            ],
        )?;
        anyhow::ensure!(changed == 1, "Track semantic inputs changed concurrently");
    } else {
        tx.execute(
            "INSERT INTO jobs_track_policy_track_input_heads (
                account_id, career_track_id, input_generation,
                previous_input_generation, input_transition_id,
                track_semantic_sha256, input_transition_sha256,
                predecessor_input_transition_sha256, updated_at_ms
             ) VALUES ($1, $2, 1, 0, $3, $4, $5, NULL, $6)",
            &[
                &account_id,
                &track.id,
                &transition_id,
                &semantic_sha256,
                &transition_sha256,
                &changed_at_ms,
            ],
        )?;
    }
    Ok(SemanticInputHead {
        generation,
        transition_sha256,
        semantic_sha256: Some(semantic_sha256),
    })
}

fn track_policy_review_receipt(
    account_id: &str,
    track_id: &str,
    record: &CanonicalTrackPolicyRecord,
    revision_id: &str,
    revision_no: i64,
    decided_at_ms: i64,
) -> Result<(String, String, String)> {
    let receipt_id = format!("track-review-{}", uuid::Uuid::new_v4().simple());
    let canonical = canonical_track_policy_review_receipt(
        account_id,
        track_id,
        record,
        revision_id,
        revision_no,
        &receipt_id,
        account_id,
        "approved",
        decided_at_ms,
    )?;
    let sha256 = track_policy_sha256(canonical.as_bytes());
    Ok((receipt_id, sha256, canonical))
}

#[allow(clippy::too_many_arguments)]
fn canonical_track_policy_review_receipt(
    account_id: &str,
    track_id: &str,
    record: &CanonicalTrackPolicyRecord,
    revision_id: &str,
    revision_no: i64,
    receipt_id: &str,
    reviewer_id: &str,
    decision: &str,
    decided_at_ms: i64,
) -> Result<String> {
    serde_json::to_string(&json!({
        "schema_version": 1,
        "review_receipt_id": receipt_id,
        "account_id": account_id,
        "career_track_id": track_id,
        "policy_revision_id": revision_id,
        "policy_revision_no": revision_no,
        "canonical_policy_sha256": record.canonical_policy_sha256,
        "taxonomy_sha256": record.taxonomy_sha256,
        "taxonomy_activation_epoch": record.taxonomy_activation_epoch,
        "canonicalizer_schema_version": record.canonicalizer_schema_version,
        "canonicalizer_sha256": record.canonicalizer_sha256,
        "account_input_generation": record.account_input_generation,
        "account_input_transition_sha256": record.account_input_transition_sha256,
        "account_semantic_sha256": record.account_semantic_sha256,
        "track_input_generation": record.track_input_generation,
        "track_input_transition_sha256": record.track_input_transition_sha256,
        "track_semantic_sha256": record.track_semantic_sha256,
        "application_identity_sha256": record.application_identity_sha256,
        "source_resume_sha256": record.source_resume_sha256,
        "job_preferences_sha256": record.job_preferences_sha256,
        "reviewer_id": reviewer_id,
        "decision": decision,
        "decided_at_ms": decided_at_ms,
    }))
    .context("serialize Career Track policy review receipt")
}

#[allow(clippy::too_many_arguments)]
fn track_policy_head_transition_sha256(
    account_id: &str,
    track_id: &str,
    head_generation: i64,
    previous_head_generation: i64,
    revision_id: &str,
    revision_no: i64,
    record: &CanonicalTrackPolicyRecord,
    review_receipt_id: &str,
    review_receipt_sha256: &str,
    predecessor_head_transition_sha256: Option<&str>,
    updated_by: &str,
    updated_at_ms: i64,
) -> Result<String> {
    let canonical = serde_json::to_vec(&json!({
        "schema_version": 1,
        "account_id": account_id,
        "career_track_id": track_id,
        "head_generation": head_generation,
        "previous_head_generation": previous_head_generation,
        "policy_revision_id": revision_id,
        "policy_revision_no": revision_no,
        "canonical_policy_sha256": record.canonical_policy_sha256,
        "taxonomy_sha256": record.taxonomy_sha256,
        "taxonomy_activation_epoch": record.taxonomy_activation_epoch,
        "canonicalizer_schema_version": record.canonicalizer_schema_version,
        "canonicalizer_sha256": record.canonicalizer_sha256,
        "account_input_generation": record.account_input_generation,
        "account_input_transition_sha256": record.account_input_transition_sha256,
        "account_semantic_sha256": record.account_semantic_sha256,
        "track_input_generation": record.track_input_generation,
        "track_input_transition_sha256": record.track_input_transition_sha256,
        "track_semantic_sha256": record.track_semantic_sha256,
        "application_identity_sha256": record.application_identity_sha256,
        "source_resume_sha256": record.source_resume_sha256,
        "job_preferences_sha256": record.job_preferences_sha256,
        "review_receipt_id": review_receipt_id,
        "review_receipt_sha256": review_receipt_sha256,
        "predecessor_head_transition_sha256": predecessor_head_transition_sha256,
        "updated_by": updated_by,
        "updated_at_ms": updated_at_ms,
    }))
    .context("serialize Career Track policy head transition")?;
    Ok(track_policy_sha256(canonical))
}

fn load_track_policy_head_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    track_id: &str,
) -> Result<Option<StoredTrackPolicyHead>> {
    tx.query_row(
        "SELECT head_generation, previous_head_generation, policy_revision_id,
                policy_revision_no, canonical_policy_sha256,
                taxonomy_digest_sha256, taxonomy_activation_epoch,
                canonicalizer_schema_version, canonicalizer_digest_sha256,
                account_input_generation, account_input_transition_sha256,
                account_semantic_sha256, track_input_generation,
                track_input_transition_sha256, track_semantic_sha256,
                verified_application_identity_sha256,
                source_resume_sha256, job_preferences_sha256,
                review_receipt_id, review_receipt_sha256,
                head_transition_sha256, predecessor_head_transition_sha256,
                updated_by, updated_at_ms
           FROM jobs_track_policy_heads
          WHERE account_id = ?1 AND career_track_id = ?2",
        params![account_id, track_id],
        |row| {
            Ok(StoredTrackPolicyHead {
                head_generation: row.get(0)?,
                previous_head_generation: row.get(1)?,
                revision_id: row.get(2)?,
                revision_no: row.get(3)?,
                canonical_policy_sha256: row.get(4)?,
                taxonomy_sha256: row.get(5)?,
                taxonomy_activation_epoch: row.get(6)?,
                canonicalizer_schema_version: row.get(7)?,
                canonicalizer_sha256: row.get(8)?,
                account_input_generation: row.get(9)?,
                account_input_transition_sha256: row.get(10)?,
                account_semantic_sha256: row.get(11)?,
                track_input_generation: row.get(12)?,
                track_input_transition_sha256: row.get(13)?,
                track_semantic_sha256: row.get(14)?,
                application_identity_sha256: row.get(15)?,
                source_resume_sha256: row.get(16)?,
                job_preferences_sha256: row.get(17)?,
                review_receipt_id: row.get(18)?,
                review_receipt_sha256: row.get(19)?,
                head_transition_sha256: row.get(20)?,
                predecessor_head_transition_sha256: row.get(21)?,
                updated_by: row.get(22)?,
                updated_at_ms: row.get(23)?,
            })
        },
    )
    .optional()
    .context("load Career Track policy head")
}

fn persist_track_policy_revision_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    track_id: &str,
    record: &CanonicalTrackPolicyRecord,
    now: i64,
) -> Result<PersistedTrackPolicyHead> {
    let current = load_track_policy_head_sqlite(tx, account_id, track_id)?;
    if let Some(head) = current
        .as_ref()
        .filter(|head| head.canonical_policy_sha256 == record.canonical_policy_sha256)
    {
        let ledger = load_track_policy_ledger_sqlite(tx, account_id, track_id)?;
        validate_equal_track_policy_record(account_id, track_id, &ledger, record)?;
        return Ok(PersistedTrackPolicyHead {
            revision_id: head.revision_id.clone(),
            revision_no: head.revision_no,
            canonical_policy_sha256: head.canonical_policy_sha256.clone(),
            head_generation: head.head_generation,
            head_transition_sha256: head.head_transition_sha256.clone(),
            review_receipt_id: head.review_receipt_id.clone(),
            review_receipt_sha256: head.review_receipt_sha256.clone(),
        });
    }
    let revision_id = format!("track-policy-{}", uuid::Uuid::new_v4().simple());
    let revision_no = current.as_ref().map_or(1, |head| head.revision_no + 1);
    let compatibility = if current.is_some() {
        "review_required"
    } else {
        "initial"
    };
    let policy_ciphertext = encrypt_payload(&record.canonical_policy_json)
        .context("encrypt canonical Career Track policy evidence")?;
    tx.execute(
        "INSERT INTO jobs_track_policy_revisions (
            revision_id, account_id, career_track_id, revision_no,
            taxonomy_version, taxonomy_digest_sha256, canonical_policy_sha256,
            canonical_policy_ciphertext, canonical_role_id, canonical_role_family,
            verified_application_identity_id, verified_application_identity_sha256,
            source_resume_asset_id, source_resume_sha256, job_preferences_sha256,
            taxonomy_activation_epoch, canonicalizer_schema_version,
            canonicalizer_digest_sha256, account_input_generation,
            account_input_transition_sha256, account_semantic_sha256,
            track_input_generation, track_input_transition_sha256,
            track_semantic_sha256,
            predecessor_revision_id, predecessor_revision_no, predecessor_policy_sha256,
            compatibility_classification, review_state, created_by, created_at_ms
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
            ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24,
            ?25, ?26, ?27, ?28, 'approved', ?2, ?29
         )",
        params![
            revision_id,
            account_id,
            track_id,
            revision_no,
            record.taxonomy_version,
            record.taxonomy_sha256,
            record.canonical_policy_sha256,
            policy_ciphertext,
            record.canonical_role_id,
            record.canonical_role_family_id,
            record.application_identity_id,
            record.application_identity_sha256,
            record.source_resume_asset_id,
            record.source_resume_sha256,
            record.job_preferences_sha256,
            record.taxonomy_activation_epoch,
            record.canonicalizer_schema_version,
            record.canonicalizer_sha256,
            record.account_input_generation,
            record.account_input_transition_sha256,
            record.account_semantic_sha256,
            record.track_input_generation,
            record.track_input_transition_sha256,
            record.track_semantic_sha256,
            current.as_ref().map(|head| head.revision_id.as_str()),
            current.as_ref().map(|head| head.revision_no),
            current
                .as_ref()
                .map(|head| head.canonical_policy_sha256.as_str()),
            compatibility,
            now,
        ],
    )?;

    let (receipt_id, receipt_sha256, receipt_json) =
        track_policy_review_receipt(account_id, track_id, record, &revision_id, revision_no, now)?;
    let receipt_ciphertext =
        encrypt_payload(&receipt_json).context("encrypt canonical Career Track review receipt")?;
    tx.execute(
        "INSERT INTO jobs_track_policy_review_receipts (
            review_receipt_id, review_receipt_sha256,
            canonical_review_receipt_ciphertext,
            account_id, career_track_id, policy_revision_id, policy_revision_no,
            canonical_policy_sha256, taxonomy_digest_sha256,
            taxonomy_activation_epoch, canonicalizer_schema_version,
            canonicalizer_digest_sha256, account_input_generation,
            account_input_transition_sha256, account_semantic_sha256,
            track_input_generation, track_input_transition_sha256,
            track_semantic_sha256,
            verified_application_identity_sha256, source_resume_sha256,
            job_preferences_sha256, reviewer_id, decision, decided_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                   ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21,
                   ?4, 'approved', ?22)",
        params![
            receipt_id,
            receipt_sha256,
            receipt_ciphertext,
            account_id,
            track_id,
            revision_id,
            revision_no,
            record.canonical_policy_sha256,
            record.taxonomy_sha256,
            record.taxonomy_activation_epoch,
            record.canonicalizer_schema_version,
            record.canonicalizer_sha256,
            record.account_input_generation,
            record.account_input_transition_sha256,
            record.account_semantic_sha256,
            record.track_input_generation,
            record.track_input_transition_sha256,
            record.track_semantic_sha256,
            record.application_identity_sha256,
            record.source_resume_sha256,
            record.job_preferences_sha256,
            now,
        ],
    )?;

    let generation = current.as_ref().map_or(1, |head| head.head_generation + 1);
    let previous_generation = current.as_ref().map_or(0, |head| head.head_generation);
    let predecessor_transition = current
        .as_ref()
        .map(|head| head.head_transition_sha256.as_str());
    let transition_sha256 = track_policy_head_transition_sha256(
        account_id,
        track_id,
        generation,
        previous_generation,
        &revision_id,
        revision_no,
        record,
        &receipt_id,
        &receipt_sha256,
        predecessor_transition,
        account_id,
        now,
    )?;
    tx.execute(
        "INSERT INTO jobs_track_policy_head_transitions (
            account_id, career_track_id, head_generation,
            previous_head_generation, policy_revision_id, policy_revision_no,
            canonical_policy_sha256, taxonomy_digest_sha256,
            taxonomy_activation_epoch, canonicalizer_schema_version,
            canonicalizer_digest_sha256, account_input_generation,
            account_input_transition_sha256, account_semantic_sha256,
            track_input_generation, track_input_transition_sha256,
            track_semantic_sha256, verified_application_identity_sha256,
            source_resume_sha256, job_preferences_sha256, review_receipt_id,
            review_receipt_sha256, head_transition_sha256,
            predecessor_head_transition_sha256, updated_by, updated_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                   ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21,
                   ?22, ?23, ?24, ?1, ?25)",
        params![
            account_id,
            track_id,
            generation,
            previous_generation,
            revision_id,
            revision_no,
            record.canonical_policy_sha256,
            record.taxonomy_sha256,
            record.taxonomy_activation_epoch,
            record.canonicalizer_schema_version,
            record.canonicalizer_sha256,
            record.account_input_generation,
            record.account_input_transition_sha256,
            record.account_semantic_sha256,
            record.track_input_generation,
            record.track_input_transition_sha256,
            record.track_semantic_sha256,
            record.application_identity_sha256,
            record.source_resume_sha256,
            record.job_preferences_sha256,
            receipt_id,
            receipt_sha256,
            transition_sha256,
            predecessor_transition,
            now,
        ],
    )?;
    if let Some(head) = current {
        let changed = tx.execute(
            "UPDATE jobs_track_policy_heads SET
                head_generation = ?3, previous_head_generation = ?4,
                policy_revision_id = ?5, policy_revision_no = ?6,
                canonical_policy_sha256 = ?7, taxonomy_digest_sha256 = ?8,
                taxonomy_activation_epoch = ?9,
                canonicalizer_schema_version = ?10,
                canonicalizer_digest_sha256 = ?11,
                account_input_generation = ?12,
                account_input_transition_sha256 = ?13,
                account_semantic_sha256 = ?14,
                track_input_generation = ?15,
                track_input_transition_sha256 = ?16,
                track_semantic_sha256 = ?17,
                verified_application_identity_sha256 = ?18,
                source_resume_sha256 = ?19, job_preferences_sha256 = ?20,
                review_receipt_id = ?21, review_receipt_sha256 = ?22,
                head_transition_sha256 = ?23,
                predecessor_head_transition_sha256 = ?24, updated_by = ?1,
                updated_at_ms = ?25
              WHERE account_id = ?1 AND career_track_id = ?2
                AND head_generation = ?4 AND head_transition_sha256 = ?24",
            params![
                account_id,
                track_id,
                generation,
                previous_generation,
                revision_id,
                revision_no,
                record.canonical_policy_sha256,
                record.taxonomy_sha256,
                record.taxonomy_activation_epoch,
                record.canonicalizer_schema_version,
                record.canonicalizer_sha256,
                record.account_input_generation,
                record.account_input_transition_sha256,
                record.account_semantic_sha256,
                record.track_input_generation,
                record.track_input_transition_sha256,
                record.track_semantic_sha256,
                record.application_identity_sha256,
                record.source_resume_sha256,
                record.job_preferences_sha256,
                receipt_id,
                receipt_sha256,
                transition_sha256,
                head.head_transition_sha256,
                now,
            ],
        )?;
        if changed != 1 {
            anyhow::bail!("Career Track policy changed while it was being saved")
        }
    } else {
        tx.execute(
            "INSERT INTO jobs_track_policy_heads (
                account_id, career_track_id, head_generation, previous_head_generation,
                policy_revision_id, policy_revision_no, canonical_policy_sha256,
                taxonomy_digest_sha256, taxonomy_activation_epoch,
                canonicalizer_schema_version, canonicalizer_digest_sha256,
                account_input_generation, account_input_transition_sha256,
                account_semantic_sha256, track_input_generation,
                track_input_transition_sha256, track_semantic_sha256,
                verified_application_identity_sha256,
                source_resume_sha256, job_preferences_sha256,
                review_receipt_id, review_receipt_sha256, head_transition_sha256,
                predecessor_head_transition_sha256, updated_by, updated_at_ms
             ) VALUES (?1, ?2, 1, 0, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                       ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20,
                       ?21, NULL, ?1, ?22)",
            params![
                account_id,
                track_id,
                revision_id,
                revision_no,
                record.canonical_policy_sha256,
                record.taxonomy_sha256,
                record.taxonomy_activation_epoch,
                record.canonicalizer_schema_version,
                record.canonicalizer_sha256,
                record.account_input_generation,
                record.account_input_transition_sha256,
                record.account_semantic_sha256,
                record.track_input_generation,
                record.track_input_transition_sha256,
                record.track_semantic_sha256,
                record.application_identity_sha256,
                record.source_resume_sha256,
                record.job_preferences_sha256,
                receipt_id,
                receipt_sha256,
                transition_sha256,
                now,
            ],
        )?;
    }
    Ok(PersistedTrackPolicyHead {
        revision_id,
        revision_no,
        canonical_policy_sha256: record.canonical_policy_sha256.clone(),
        head_generation: generation,
        head_transition_sha256: transition_sha256,
        review_receipt_id: receipt_id,
        review_receipt_sha256: receipt_sha256,
    })
}

fn load_track_policy_head_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    track_id: &str,
) -> Result<Option<StoredTrackPolicyHead>> {
    Ok(tx
        .query_opt(
            "SELECT head_generation, previous_head_generation, policy_revision_id,
                    policy_revision_no, canonical_policy_sha256,
                    taxonomy_digest_sha256, taxonomy_activation_epoch,
                    canonicalizer_schema_version, canonicalizer_digest_sha256,
                    account_input_generation, account_input_transition_sha256,
                    account_semantic_sha256, track_input_generation,
                    track_input_transition_sha256, track_semantic_sha256,
                    verified_application_identity_sha256,
                    source_resume_sha256, job_preferences_sha256,
                    review_receipt_id, review_receipt_sha256,
                    head_transition_sha256, predecessor_head_transition_sha256,
                    updated_by, updated_at_ms
               FROM jobs_track_policy_heads
              WHERE account_id = $1 AND career_track_id = $2
              FOR UPDATE",
            &[&account_id, &track_id],
        )?
        .map(|row| StoredTrackPolicyHead {
            head_generation: row.get(0),
            previous_head_generation: row.get(1),
            revision_id: row.get(2),
            revision_no: row.get(3),
            canonical_policy_sha256: row.get(4),
            taxonomy_sha256: row.get(5),
            taxonomy_activation_epoch: row.get(6),
            canonicalizer_schema_version: row.get(7),
            canonicalizer_sha256: row.get(8),
            account_input_generation: row.get(9),
            account_input_transition_sha256: row.get(10),
            account_semantic_sha256: row.get(11),
            track_input_generation: row.get(12),
            track_input_transition_sha256: row.get(13),
            track_semantic_sha256: row.get(14),
            application_identity_sha256: row.get(15),
            source_resume_sha256: row.get(16),
            job_preferences_sha256: row.get(17),
            review_receipt_id: row.get(18),
            review_receipt_sha256: row.get(19),
            head_transition_sha256: row.get(20),
            predecessor_head_transition_sha256: row.get(21),
            updated_by: row.get(22),
            updated_at_ms: row.get(23),
        }))
}

fn persist_track_policy_revision_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    track_id: &str,
    record: &CanonicalTrackPolicyRecord,
    now: i64,
) -> Result<PersistedTrackPolicyHead> {
    let current = load_track_policy_head_postgres(tx, account_id, track_id)?;
    if let Some(head) = current
        .as_ref()
        .filter(|head| head.canonical_policy_sha256 == record.canonical_policy_sha256)
    {
        let ledger = load_track_policy_ledger_postgres(tx, account_id, track_id)?;
        validate_equal_track_policy_record(account_id, track_id, &ledger, record)?;
        return Ok(PersistedTrackPolicyHead {
            revision_id: head.revision_id.clone(),
            revision_no: head.revision_no,
            canonical_policy_sha256: head.canonical_policy_sha256.clone(),
            head_generation: head.head_generation,
            head_transition_sha256: head.head_transition_sha256.clone(),
            review_receipt_id: head.review_receipt_id.clone(),
            review_receipt_sha256: head.review_receipt_sha256.clone(),
        });
    }
    let revision_id = format!("track-policy-{}", uuid::Uuid::new_v4().simple());
    let revision_no = current.as_ref().map_or(1, |head| head.revision_no + 1);
    let compatibility = if current.is_some() {
        "review_required"
    } else {
        "initial"
    };
    let policy_ciphertext = encrypt_payload(&record.canonical_policy_json)
        .context("encrypt canonical Career Track policy evidence")?;
    tx.execute(
        "INSERT INTO jobs_track_policy_revisions (
            revision_id, account_id, career_track_id, revision_no,
            taxonomy_version, taxonomy_digest_sha256, canonical_policy_sha256,
            canonical_policy_ciphertext, canonical_role_id, canonical_role_family,
            verified_application_identity_id, verified_application_identity_sha256,
            source_resume_asset_id, source_resume_sha256, job_preferences_sha256,
            taxonomy_activation_epoch, canonicalizer_schema_version,
            canonicalizer_digest_sha256, account_input_generation,
            account_input_transition_sha256, account_semantic_sha256,
            track_input_generation, track_input_transition_sha256,
            track_semantic_sha256,
            predecessor_revision_id, predecessor_revision_no, predecessor_policy_sha256,
            compatibility_classification, review_state, created_by, created_at_ms
         ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14,
            $15, $16, $17, $18, $19, $20, $21, $22, $23, $24,
            $25, $26, $27, $28, 'approved', $2, $29
         )",
        &[
            &revision_id,
            &account_id,
            &track_id,
            &revision_no,
            &record.taxonomy_version,
            &record.taxonomy_sha256,
            &record.canonical_policy_sha256,
            &policy_ciphertext,
            &record.canonical_role_id,
            &record.canonical_role_family_id,
            &record.application_identity_id,
            &record.application_identity_sha256,
            &record.source_resume_asset_id,
            &record.source_resume_sha256,
            &record.job_preferences_sha256,
            &record.taxonomy_activation_epoch,
            &record.canonicalizer_schema_version,
            &record.canonicalizer_sha256,
            &record.account_input_generation,
            &record.account_input_transition_sha256,
            &record.account_semantic_sha256,
            &record.track_input_generation,
            &record.track_input_transition_sha256,
            &record.track_semantic_sha256,
            &current.as_ref().map(|head| head.revision_id.as_str()),
            &current.as_ref().map(|head| head.revision_no),
            &current
                .as_ref()
                .map(|head| head.canonical_policy_sha256.as_str()),
            &compatibility,
            &now,
        ],
    )?;

    let (receipt_id, receipt_sha256, receipt_json) =
        track_policy_review_receipt(account_id, track_id, record, &revision_id, revision_no, now)?;
    let receipt_ciphertext =
        encrypt_payload(&receipt_json).context("encrypt canonical Career Track review receipt")?;
    tx.execute(
        "INSERT INTO jobs_track_policy_review_receipts (
            review_receipt_id, review_receipt_sha256,
            canonical_review_receipt_ciphertext,
            account_id, career_track_id, policy_revision_id, policy_revision_no,
            canonical_policy_sha256, taxonomy_digest_sha256,
            taxonomy_activation_epoch, canonicalizer_schema_version,
            canonicalizer_digest_sha256, account_input_generation,
            account_input_transition_sha256, account_semantic_sha256,
            track_input_generation, track_input_transition_sha256,
            track_semantic_sha256,
            verified_application_identity_sha256, source_resume_sha256,
            job_preferences_sha256, reviewer_id, decision, decided_at_ms
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                   $13, $14, $15, $16, $17, $18, $19, $20, $21,
                   $4, 'approved', $22)",
        &[
            &receipt_id,
            &receipt_sha256,
            &receipt_ciphertext,
            &account_id,
            &track_id,
            &revision_id,
            &revision_no,
            &record.canonical_policy_sha256,
            &record.taxonomy_sha256,
            &record.taxonomy_activation_epoch,
            &record.canonicalizer_schema_version,
            &record.canonicalizer_sha256,
            &record.account_input_generation,
            &record.account_input_transition_sha256,
            &record.account_semantic_sha256,
            &record.track_input_generation,
            &record.track_input_transition_sha256,
            &record.track_semantic_sha256,
            &record.application_identity_sha256,
            &record.source_resume_sha256,
            &record.job_preferences_sha256,
            &now,
        ],
    )?;

    let generation = current.as_ref().map_or(1, |head| head.head_generation + 1);
    let previous_generation = current.as_ref().map_or(0, |head| head.head_generation);
    let predecessor_transition = current
        .as_ref()
        .map(|head| head.head_transition_sha256.as_str());
    let transition_sha256 = track_policy_head_transition_sha256(
        account_id,
        track_id,
        generation,
        previous_generation,
        &revision_id,
        revision_no,
        record,
        &receipt_id,
        &receipt_sha256,
        predecessor_transition,
        account_id,
        now,
    )?;
    tx.execute(
        "INSERT INTO jobs_track_policy_head_transitions (
            account_id, career_track_id, head_generation,
            previous_head_generation, policy_revision_id, policy_revision_no,
            canonical_policy_sha256, taxonomy_digest_sha256,
            taxonomy_activation_epoch, canonicalizer_schema_version,
            canonicalizer_digest_sha256, account_input_generation,
            account_input_transition_sha256, account_semantic_sha256,
            track_input_generation, track_input_transition_sha256,
            track_semantic_sha256, verified_application_identity_sha256,
            source_resume_sha256, job_preferences_sha256, review_receipt_id,
            review_receipt_sha256, head_transition_sha256,
            predecessor_head_transition_sha256, updated_by, updated_at_ms
         ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11,
                   $12, $13, $14, $15, $16, $17, $18, $19, $20, $21,
                   $22, $23, $24, $1, $25)",
        &[
            &account_id,
            &track_id,
            &generation,
            &previous_generation,
            &revision_id,
            &revision_no,
            &record.canonical_policy_sha256,
            &record.taxonomy_sha256,
            &record.taxonomy_activation_epoch,
            &record.canonicalizer_schema_version,
            &record.canonicalizer_sha256,
            &record.account_input_generation,
            &record.account_input_transition_sha256,
            &record.account_semantic_sha256,
            &record.track_input_generation,
            &record.track_input_transition_sha256,
            &record.track_semantic_sha256,
            &record.application_identity_sha256,
            &record.source_resume_sha256,
            &record.job_preferences_sha256,
            &receipt_id,
            &receipt_sha256,
            &transition_sha256,
            &predecessor_transition,
            &now,
        ],
    )?;
    if let Some(head) = current {
        let changed = tx.execute(
            "UPDATE jobs_track_policy_heads SET
                head_generation = $3, previous_head_generation = $4,
                policy_revision_id = $5, policy_revision_no = $6,
                canonical_policy_sha256 = $7, taxonomy_digest_sha256 = $8,
                taxonomy_activation_epoch = $9,
                canonicalizer_schema_version = $10,
                canonicalizer_digest_sha256 = $11,
                account_input_generation = $12,
                account_input_transition_sha256 = $13,
                account_semantic_sha256 = $14,
                track_input_generation = $15,
                track_input_transition_sha256 = $16,
                track_semantic_sha256 = $17,
                verified_application_identity_sha256 = $18,
                source_resume_sha256 = $19, job_preferences_sha256 = $20,
                review_receipt_id = $21, review_receipt_sha256 = $22,
                head_transition_sha256 = $23,
                predecessor_head_transition_sha256 = $24, updated_by = $1,
                updated_at_ms = $25
              WHERE account_id = $1 AND career_track_id = $2
                AND head_generation = $4 AND head_transition_sha256 = $24",
            &[
                &account_id,
                &track_id,
                &generation,
                &previous_generation,
                &revision_id,
                &revision_no,
                &record.canonical_policy_sha256,
                &record.taxonomy_sha256,
                &record.taxonomy_activation_epoch,
                &record.canonicalizer_schema_version,
                &record.canonicalizer_sha256,
                &record.account_input_generation,
                &record.account_input_transition_sha256,
                &record.account_semantic_sha256,
                &record.track_input_generation,
                &record.track_input_transition_sha256,
                &record.track_semantic_sha256,
                &record.application_identity_sha256,
                &record.source_resume_sha256,
                &record.job_preferences_sha256,
                &receipt_id,
                &receipt_sha256,
                &transition_sha256,
                &head.head_transition_sha256,
                &now,
            ],
        )?;
        if changed != 1 {
            anyhow::bail!("Career Track policy changed while it was being saved")
        }
    } else {
        tx.execute(
            "INSERT INTO jobs_track_policy_heads (
                account_id, career_track_id, head_generation, previous_head_generation,
                policy_revision_id, policy_revision_no, canonical_policy_sha256,
                taxonomy_digest_sha256, taxonomy_activation_epoch,
                canonicalizer_schema_version, canonicalizer_digest_sha256,
                account_input_generation, account_input_transition_sha256,
                account_semantic_sha256, track_input_generation,
                track_input_transition_sha256, track_semantic_sha256,
                verified_application_identity_sha256,
                source_resume_sha256, job_preferences_sha256,
                review_receipt_id, review_receipt_sha256, head_transition_sha256,
                predecessor_head_transition_sha256, updated_by, updated_at_ms
             ) VALUES ($1, $2, 1, 0, $3, $4, $5, $6, $7, $8, $9, $10,
                       $11, $12, $13, $14, $15, $16, $17, $18, $19, $20,
                       $21, NULL, $1, $22)",
            &[
                &account_id,
                &track_id,
                &revision_id,
                &revision_no,
                &record.canonical_policy_sha256,
                &record.taxonomy_sha256,
                &record.taxonomy_activation_epoch,
                &record.canonicalizer_schema_version,
                &record.canonicalizer_sha256,
                &record.account_input_generation,
                &record.account_input_transition_sha256,
                &record.account_semantic_sha256,
                &record.track_input_generation,
                &record.track_input_transition_sha256,
                &record.track_semantic_sha256,
                &record.application_identity_sha256,
                &record.source_resume_sha256,
                &record.job_preferences_sha256,
                &receipt_id,
                &receipt_sha256,
                &transition_sha256,
                &now,
            ],
        )?;
    }
    Ok(PersistedTrackPolicyHead {
        revision_id,
        revision_no,
        canonical_policy_sha256: record.canonical_policy_sha256.clone(),
        head_generation: generation,
        head_transition_sha256: transition_sha256,
        review_receipt_id: receipt_id,
        review_receipt_sha256: receipt_sha256,
    })
}
fn track_trim_dedupe(values: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .iter()
        .filter_map(|value| {
            let trimmed = value.trim();
            let key = trimmed.to_lowercase();
            (!trimmed.is_empty() && seen.insert(key)).then(|| trimmed.to_string())
        })
        .collect()
}

fn normalize_track_policy_values(values: &[String], kind: &str) -> Vec<String> {
    let normalized = track_trim_dedupe(values)
        .into_iter()
        .map(|value| match kind {
            "employment" => normalize_candidate_employment_type(&value)
                .map(str::to_string)
                .unwrap_or(value),
            "engagement" => normalize_candidate_engagement_type(&value)
                .map(str::to_string)
                .unwrap_or(value),
            _ => candidate_normalize(&value).replace(' ', "_"),
        })
        .collect::<Vec<_>>();
    track_trim_dedupe(&normalized)
}

fn normalize_canonical_track(track: &CareerTrack) -> Result<CareerTrack> {
    let mut value = track.clone();
    value.name = value.name.trim().to_string();
    value.application_identity_id = value
        .application_identity_id
        .as_deref()
        .map(str::trim)
        .filter(|identity_id| !identity_id.is_empty())
        .map(str::to_string);
    value.locations = track_trim_dedupe(&value.locations);
    value.policy.relevant_employment_ids = track_trim_dedupe(&value.policy.relevant_employment_ids);
    value.policy.employment_types =
        normalize_track_policy_values(&value.policy.employment_types, "employment");
    if value.policy.employment_types.is_empty() {
        value.policy.employment_types.push("full_time".to_string());
    }
    value.policy.engagement_types =
        normalize_track_policy_values(&value.policy.engagement_types, "engagement");
    value.policy.work_authorizations =
        normalize_track_policy_values(&value.policy.work_authorizations, "authorization");
    value.remote_preference = match candidate_normalize(&value.remote_preference).as_str() {
        "remote only" => "remote_only",
        "remote or hybrid" => "remote_or_hybrid",
        "hybrid ok" => "hybrid_ok",
        "onsite ok" | "on site ok" => "onsite_ok",
        "any" => "any",
        _ => anyhow::bail!(
            "Choose a supported remote preference: remote only, remote or hybrid, \
             hybrid ok, onsite ok, or any."
        ),
    }
    .to_string();

    let mut reasons = Vec::new();
    match crate::jobs_taxonomy::resolve_target_role(&value.role) {
        crate::jobs_taxonomy::TargetRoleResolution::Known {
            role_id,
            label,
            family_id,
            ..
        } => {
            value.role = label;
            value.policy.role_family = family_id.clone();
            value.policy.authority.canonical_role_id = role_id;
            value.policy.authority.canonical_role_family_id = family_id;
        }
        crate::jobs_taxonomy::TargetRoleResolution::Ambiguous {
            candidate_role_ids, ..
        } => anyhow::bail!(
            "Choose a full target role; this abbreviation could mean {}.",
            candidate_role_ids.join(", ")
        ),
        crate::jobs_taxonomy::TargetRoleResolution::CustomReview {
            raw, normalized, ..
        } => {
            if normalized.is_empty() {
                anyhow::bail!("Choose a target role for this Career Track.")
            }
            value.role = raw.trim().to_string();
            value.policy.role_family = "custom".to_string();
            value.policy.authority.canonical_role_id = format!(
                "custom-{}",
                &track_policy_sha256(normalized.as_bytes())[..24]
            );
            value.policy.authority.canonical_role_family_id = "custom".to_string();
            reasons.push("custom_role_review_required".to_string());
        }
    }

    let mut canonical_location_ids = BTreeSet::new();
    if value.locations.is_empty() {
        reasons.push("location_required".to_string());
    }
    for location in &value.locations {
        match crate::jobs_taxonomy::normalize_geography(location) {
            crate::jobs_taxonomy::GeographyClassification::Known { normalized, .. } => {
                canonical_location_ids.extend(normalized.canonical_ids());
            }
            crate::jobs_taxonomy::GeographyClassification::Ambiguous { .. } => {
                reasons.push("ambiguous_location".to_string());
            }
            crate::jobs_taxonomy::GeographyClassification::Unknown { .. } => {
                reasons.push("unknown_location".to_string());
            }
        }
    }
    value.policy.authority.canonical_location_ids = canonical_location_ids.into_iter().collect();
    value.policy.authority.taxonomy_version = crate::jobs_taxonomy::taxonomy_version().to_string();
    value.policy.authority.taxonomy_sha256 = crate::jobs_taxonomy::taxonomy_sha256();
    value.policy.authority.taxonomy_activation_epoch = 0;
    value.policy.authority.canonicalizer_schema_version = 0;
    value.policy.authority.canonicalizer_sha256.clear();
    value.policy.authority.account_input_generation = 0;
    value
        .policy
        .authority
        .account_input_transition_sha256
        .clear();
    value.policy.authority.account_input_semantic_sha256.clear();
    value.policy.authority.track_input_generation = 0;
    value.policy.authority.track_input_transition_sha256.clear();
    value.policy.authority.track_semantic_sha256.clear();
    value.policy.authority.policy_revision_id.clear();
    value.policy.authority.policy_revision_no = 0;
    value.policy.authority.canonical_policy_sha256.clear();
    value.policy.authority.policy_head_generation = 0;
    value.policy.authority.policy_head_transition_sha256.clear();
    value.policy.authority.policy_review_receipt_id.clear();
    value.policy.authority.policy_review_receipt_sha256.clear();
    value.policy.authority.source_resume_asset_id.clear();
    value.policy.authority.source_resume_sha256.clear();
    value.policy.authority.application_identity_id.clear();
    value.policy.authority.application_identity_sha256.clear();
    value.policy.authority.job_preferences_sha256.clear();
    reasons.sort();
    reasons.dedup();
    value.policy.authority.review_state = "needs_review".to_string();
    value.policy.authority.review_reason_codes = reasons;
    Ok(value)
}

fn application_identity_policy_sha256(identity: &ApplicationIdentity) -> Result<String> {
    let encoded = serde_json::to_vec(&json!({
        "id": identity.id,
        "email": identity.email,
        "label": identity.label,
        "verification_status": identity.verification_status,
    }))
    .context("serialize Career Track application identity")?;
    Ok(track_policy_sha256(encoded))
}

fn job_preferences_policy_sha256(preferences: &JobPreferences) -> Result<String> {
    let encoded = serde_json::to_vec(&json!({
        "desired_roles": preferences.desired_roles,
        "desired_locations": preferences.desired_locations,
        "location_policy": preferences.location_policy,
        "remote_preference": preferences.remote_preference,
        "employment_types": preferences.employment_types,
        "engagement_types": preferences.engagement_types,
        "minimum_compensation": preferences.minimum_compensation,
        "sponsorship": preferences.sponsorship,
        "excluded_companies": preferences.excluded_companies,
        "excluded_titles": preferences.excluded_titles,
        "daily_limit": preferences.daily_limit,
        "apply_once_per_company": preferences.apply_once_per_company,
        "max_posting_age_days": preferences.max_posting_age_days,
        "time_zone_offset_minutes": preferences.time_zone_offset_minutes,
    }))
    .context("serialize Career Track job preferences")?;
    Ok(track_policy_sha256(encoded))
}

fn prepare_canonical_track_policy_record(
    account_id: &str,
    track: &mut CareerTrack,
    profile: &CareerProfile,
    preferences: &JobPreferences,
    identity: Option<&ApplicationIdentity>,
    resume_asset_verified: bool,
    generations: &CanonicalPolicyGenerations,
) -> Result<Option<CanonicalTrackPolicyRecord>> {
    let mut reasons = track.policy.authority.review_reason_codes.clone();
    track.policy.authority.job_preferences_sha256 = job_preferences_policy_sha256(preferences)?;
    let identity = match identity {
        Some(identity) if identity.verification_status == "verified" => Some(identity),
        Some(_) => {
            reasons.push("application_identity_not_verified".to_string());
            None
        }
        None => {
            reasons.push("application_identity_required".to_string());
            None
        }
    };
    if profile.source_resume_asset_id.trim().is_empty()
        || profile.source_resume_sha256.len() != 64
        || !resume_asset_verified
    {
        reasons.push("source_resume_review_required".to_string());
    }
    track.policy.authority.source_resume_asset_id = profile.source_resume_asset_id.clone();
    track.policy.authority.source_resume_sha256 = profile.source_resume_sha256.clone();
    reasons.sort();
    reasons.dedup();
    track.policy.authority.review_reason_codes = reasons;
    if !track.policy.authority.review_reason_codes.is_empty() {
        track.policy.authority.review_state = "needs_review".to_string();
        return Ok(None);
    }
    let identity = identity.expect("verified identity was checked");
    let identity_sha256 = application_identity_policy_sha256(identity)?;
    track.policy.authority.application_identity_id = identity.id.clone();
    track.policy.authority.application_identity_sha256 = identity_sha256.clone();
    let canonical_policy_json = serde_json::to_string(&json!({
        "schema_version": 1,
        "account_id": account_id,
        "taxonomy": {
            "version": track.policy.authority.taxonomy_version,
            "sha256": track.policy.authority.taxonomy_sha256,
            "activation_epoch": generations.activation.activation_epoch,
            "activation_transition_sha256": generations.activation.transition_sha256,
        },
        "canonicalizer": {
            "schema_version": generations.activation.canonicalizer_schema_version,
            "sha256": generations.activation.canonicalizer_sha256,
        },
        "semantic_inputs": {
            "account_generation": generations.account.generation,
            "account_transition_sha256": generations.account.transition_sha256,
            "account_semantic_sha256": generations.account.semantic_sha256,
            "track_generation": generations.track.generation,
            "track_transition_sha256": generations.track.transition_sha256,
            "track_semantic_sha256": generations.track.semantic_sha256,
        },
        "career_track": {
            "id": track.id,
            "role": track.role,
            "canonical_role_id": track.policy.authority.canonical_role_id,
            "canonical_role_family_id": track.policy.authority.canonical_role_family_id,
            "raw_locations": track.locations,
            "canonical_location_ids": track.policy.authority.canonical_location_ids,
            "remote_preference": track.remote_preference,
            "relevant_employment_ids": track.policy.relevant_employment_ids,
            "employment_types": track.policy.employment_types,
            "engagement_types": track.policy.engagement_types,
            "work_authorizations": track.policy.work_authorizations,
            "active": track.active,
        },
        "application_identity": {
            "id": identity.id,
            "sha256": identity_sha256,
        },
        "source_resume": {
            "asset_id": profile.source_resume_asset_id,
            "sha256": profile.source_resume_sha256,
        },
        "job_preferences": {
            "desired_roles": preferences.desired_roles,
            "desired_locations": preferences.desired_locations,
            "location_policy": preferences.location_policy,
            "remote_preference": preferences.remote_preference,
            "employment_types": preferences.employment_types,
            "engagement_types": preferences.engagement_types,
            "minimum_compensation": preferences.minimum_compensation,
            "sponsorship": preferences.sponsorship,
            "excluded_companies": preferences.excluded_companies,
            "excluded_titles": preferences.excluded_titles,
            "daily_limit": preferences.daily_limit,
            "apply_once_per_company": preferences.apply_once_per_company,
            "max_posting_age_days": preferences.max_posting_age_days,
            "time_zone_offset_minutes": preferences.time_zone_offset_minutes,
        },
    }))
    .context("serialize canonical Career Track policy")?;
    let canonical_policy_sha256 = track_policy_sha256(canonical_policy_json.as_bytes());
    Ok(Some(CanonicalTrackPolicyRecord {
        taxonomy_version: track.policy.authority.taxonomy_version.clone(),
        taxonomy_sha256: track.policy.authority.taxonomy_sha256.clone(),
        taxonomy_activation_epoch: generations.activation.activation_epoch,
        canonicalizer_schema_version: generations.activation.canonicalizer_schema_version,
        canonicalizer_sha256: generations.activation.canonicalizer_sha256.clone(),
        account_input_generation: generations.account.generation,
        account_input_transition_sha256: generations.account.transition_sha256.clone(),
        account_semantic_sha256: generations
            .account
            .semantic_sha256
            .clone()
            .context("account semantic input digest is missing")?,
        track_input_generation: generations.track.generation,
        track_input_transition_sha256: generations.track.transition_sha256.clone(),
        track_semantic_sha256: generations
            .track
            .semantic_sha256
            .clone()
            .context("Track semantic input digest is missing")?,
        canonical_policy_json,
        canonical_policy_sha256,
        canonical_role_id: track.policy.authority.canonical_role_id.clone(),
        canonical_role_family_id: track.policy.authority.canonical_role_family_id.clone(),
        application_identity_id: identity.id.clone(),
        application_identity_sha256: identity_sha256,
        source_resume_asset_id: profile.source_resume_asset_id.clone(),
        source_resume_sha256: profile.source_resume_sha256.clone(),
        job_preferences_sha256: track.policy.authority.job_preferences_sha256.clone(),
    }))
}

fn canonical_track_policy_inputs_sqlite(
    conn: &rusqlite::Connection,
    account_id: &str,
    track: &CareerTrack,
) -> Result<(
    CareerProfile,
    JobPreferences,
    Option<ApplicationIdentity>,
    bool,
)> {
    let profile: CareerProfile = conn
        .query_row(
            "SELECT profile_json FROM jobs_profiles WHERE account_id = ?1",
            params![account_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .map(|raw| parse_json(raw, "Career Track policy profile"))
        .transpose()?
        .unwrap_or_default();
    let preferences: JobPreferences = conn
        .query_row(
            "SELECT preferences_json FROM jobs_preferences WHERE account_id = ?1",
            params![account_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .map(|raw| parse_json(raw, "Career Track policy preferences"))
        .transpose()?
        .unwrap_or_default();
    let identity = if let Some(identity_id) = track.application_identity_id.as_deref() {
        let row = conn
            .query_row(
                "SELECT identity_json, verification_status, is_default
                   FROM jobs_application_identities
                  WHERE account_id = ?1 AND id = ?2",
                params![account_id, identity_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| anyhow::anyhow!("application email not found"))?;
        Some(parse_application_identity_row(row.0, row.1, row.2 != 0)?)
    } else {
        None
    };
    if identity
        .as_ref()
        .is_some_and(|identity| identity.verification_status != "verified")
    {
        anyhow::bail!("verify the application email before using it on a Career Track")
    }
    let resume_asset_verified = !profile.source_resume_asset_id.trim().is_empty()
        && profile.source_resume_sha256.len() == 64
        && conn.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM jobs_resume_source_assets
                 WHERE account_id = ?1 AND id = ?2 AND sha256 = ?3
            )",
            params![
                account_id,
                profile.source_resume_asset_id,
                profile.source_resume_sha256
            ],
            |row| row.get::<_, bool>(0),
        )?;
    Ok((profile, preferences, identity, resume_asset_verified))
}

fn canonical_track_policy_inputs_postgres<C: postgres::GenericClient>(
    client: &mut C,
    account_id: &str,
    track: &CareerTrack,
) -> Result<(
    CareerProfile,
    JobPreferences,
    Option<ApplicationIdentity>,
    bool,
)> {
    let profile: CareerProfile = client
        .query_opt(
            "SELECT profile_json FROM jobs_profiles WHERE account_id = $1 FOR SHARE",
            &[&account_id],
        )?
        .map(|row| parse_json(row.get(0), "Career Track policy profile"))
        .transpose()?
        .unwrap_or_default();
    let preferences: JobPreferences = client
        .query_opt(
            "SELECT preferences_json FROM jobs_preferences WHERE account_id = $1 FOR SHARE",
            &[&account_id],
        )?
        .map(|row| parse_json(row.get(0), "Career Track policy preferences"))
        .transpose()?
        .unwrap_or_default();
    let identity = if let Some(identity_id) = track.application_identity_id.as_deref() {
        let row = client
            .query_opt(
                "SELECT identity_json, verification_status, is_default
                   FROM jobs_application_identities
                  WHERE account_id = $1 AND id = $2
                  FOR KEY SHARE",
                &[&account_id, &identity_id],
            )?
            .ok_or_else(|| anyhow::anyhow!("application email not found"))?;
        Some(parse_application_identity_row(
            row.get(0),
            row.get(1),
            row.get::<_, i32>(2) != 0,
        )?)
    } else {
        None
    };
    if identity
        .as_ref()
        .is_some_and(|identity| identity.verification_status != "verified")
    {
        anyhow::bail!("verify the application email before using it on a Career Track")
    }
    let resume_asset_verified = !profile.source_resume_asset_id.trim().is_empty()
        && profile.source_resume_sha256.len() == 64
        && client
            .query_one(
                "SELECT EXISTS(
                    SELECT 1 FROM jobs_resume_source_assets
                     WHERE account_id = $1 AND id = $2 AND sha256 = $3
                )",
                &[
                    &account_id,
                    &profile.source_resume_asset_id,
                    &profile.source_resume_sha256,
                ],
            )?
            .get::<_, bool>(0);
    Ok((profile, preferences, identity, resume_asset_verified))
}

fn load_track_policy_ledger_sqlite(
    conn: &rusqlite::Connection,
    account_id: &str,
    track_id: &str,
) -> Result<StoredTrackPolicyLedger> {
    conn.query_row(
        "SELECT h.head_generation, h.previous_head_generation,
                h.policy_revision_id, h.policy_revision_no,
                h.canonical_policy_sha256, h.taxonomy_digest_sha256,
                h.taxonomy_activation_epoch, h.canonicalizer_schema_version,
                h.canonicalizer_digest_sha256, h.account_input_generation,
                h.account_input_transition_sha256, h.account_semantic_sha256,
                h.track_input_generation, h.track_input_transition_sha256,
                h.track_semantic_sha256,
                h.verified_application_identity_sha256,
                h.source_resume_sha256, h.job_preferences_sha256,
                h.review_receipt_id, h.review_receipt_sha256,
                h.head_transition_sha256,
                h.predecessor_head_transition_sha256,
                h.updated_by, h.updated_at_ms,
                revision.taxonomy_version,
                revision.canonical_policy_ciphertext,
                revision.canonical_role_id, revision.canonical_role_family,
                revision.verified_application_identity_id,
                revision.source_resume_asset_id,
                revision.predecessor_revision_id,
                revision.predecessor_revision_no,
                revision.predecessor_policy_sha256,
                revision.compatibility_classification,
                revision.review_state, revision.created_by,
                revision.created_at_ms,
                receipt.canonical_review_receipt_ciphertext,
                receipt.reviewer_id, receipt.decision, receipt.decided_at_ms
           FROM jobs_track_policy_heads AS h
           JOIN jobs_track_policy_head_transitions AS transition
             ON transition.account_id = h.account_id
            AND transition.career_track_id = h.career_track_id
            AND transition.head_generation = h.head_generation
            AND transition.previous_head_generation = h.previous_head_generation
            AND transition.policy_revision_id = h.policy_revision_id
            AND transition.policy_revision_no = h.policy_revision_no
            AND transition.canonical_policy_sha256 = h.canonical_policy_sha256
            AND transition.taxonomy_digest_sha256 = h.taxonomy_digest_sha256
            AND transition.taxonomy_activation_epoch = h.taxonomy_activation_epoch
            AND transition.canonicalizer_schema_version = h.canonicalizer_schema_version
            AND transition.canonicalizer_digest_sha256 = h.canonicalizer_digest_sha256
            AND transition.account_input_generation = h.account_input_generation
            AND transition.account_input_transition_sha256 =
                h.account_input_transition_sha256
            AND transition.account_semantic_sha256 = h.account_semantic_sha256
            AND transition.track_input_generation = h.track_input_generation
            AND transition.track_input_transition_sha256 = h.track_input_transition_sha256
            AND transition.track_semantic_sha256 = h.track_semantic_sha256
            AND transition.verified_application_identity_sha256 =
                h.verified_application_identity_sha256
            AND transition.source_resume_sha256 = h.source_resume_sha256
            AND transition.job_preferences_sha256 = h.job_preferences_sha256
            AND transition.review_receipt_id = h.review_receipt_id
            AND transition.review_receipt_sha256 = h.review_receipt_sha256
            AND transition.head_transition_sha256 = h.head_transition_sha256
            AND transition.predecessor_head_transition_sha256
                  IS h.predecessor_head_transition_sha256
            AND transition.updated_by = h.updated_by
            AND transition.updated_at_ms = h.updated_at_ms
           JOIN jobs_track_policy_revisions AS revision
             ON revision.account_id = h.account_id
            AND revision.career_track_id = h.career_track_id
            AND revision.revision_id = h.policy_revision_id
            AND revision.revision_no = h.policy_revision_no
            AND revision.canonical_policy_sha256 = h.canonical_policy_sha256
            AND revision.taxonomy_digest_sha256 = h.taxonomy_digest_sha256
            AND revision.taxonomy_activation_epoch = h.taxonomy_activation_epoch
            AND revision.canonicalizer_schema_version = h.canonicalizer_schema_version
            AND revision.canonicalizer_digest_sha256 = h.canonicalizer_digest_sha256
            AND revision.account_input_generation = h.account_input_generation
            AND revision.account_input_transition_sha256 = h.account_input_transition_sha256
            AND revision.account_semantic_sha256 = h.account_semantic_sha256
            AND revision.track_input_generation = h.track_input_generation
            AND revision.track_input_transition_sha256 = h.track_input_transition_sha256
            AND revision.track_semantic_sha256 = h.track_semantic_sha256
            AND revision.verified_application_identity_sha256 =
                h.verified_application_identity_sha256
            AND revision.source_resume_sha256 = h.source_resume_sha256
            AND revision.job_preferences_sha256 = h.job_preferences_sha256
           JOIN jobs_track_policy_review_receipts AS receipt
             ON receipt.account_id = h.account_id
            AND receipt.career_track_id = h.career_track_id
            AND receipt.policy_revision_id = h.policy_revision_id
            AND receipt.policy_revision_no = h.policy_revision_no
            AND receipt.canonical_policy_sha256 = h.canonical_policy_sha256
            AND receipt.taxonomy_digest_sha256 = h.taxonomy_digest_sha256
            AND receipt.taxonomy_activation_epoch = h.taxonomy_activation_epoch
            AND receipt.canonicalizer_schema_version = h.canonicalizer_schema_version
            AND receipt.canonicalizer_digest_sha256 = h.canonicalizer_digest_sha256
            AND receipt.account_input_generation = h.account_input_generation
            AND receipt.account_input_transition_sha256 = h.account_input_transition_sha256
            AND receipt.account_semantic_sha256 = h.account_semantic_sha256
            AND receipt.track_input_generation = h.track_input_generation
            AND receipt.track_input_transition_sha256 = h.track_input_transition_sha256
            AND receipt.track_semantic_sha256 = h.track_semantic_sha256
            AND receipt.verified_application_identity_sha256 =
                h.verified_application_identity_sha256
            AND receipt.source_resume_sha256 = h.source_resume_sha256
            AND receipt.job_preferences_sha256 = h.job_preferences_sha256
            AND receipt.review_receipt_id = h.review_receipt_id
            AND receipt.review_receipt_sha256 = h.review_receipt_sha256
          WHERE h.account_id = ?1 AND h.career_track_id = ?2",
        params![account_id, track_id],
        |row| {
            Ok(StoredTrackPolicyLedger {
                head: StoredTrackPolicyHead {
                    head_generation: row.get(0)?,
                    previous_head_generation: row.get(1)?,
                    revision_id: row.get(2)?,
                    revision_no: row.get(3)?,
                    canonical_policy_sha256: row.get(4)?,
                    taxonomy_sha256: row.get(5)?,
                    taxonomy_activation_epoch: row.get(6)?,
                    canonicalizer_schema_version: row.get(7)?,
                    canonicalizer_sha256: row.get(8)?,
                    account_input_generation: row.get(9)?,
                    account_input_transition_sha256: row.get(10)?,
                    account_semantic_sha256: row.get(11)?,
                    track_input_generation: row.get(12)?,
                    track_input_transition_sha256: row.get(13)?,
                    track_semantic_sha256: row.get(14)?,
                    application_identity_sha256: row.get(15)?,
                    source_resume_sha256: row.get(16)?,
                    job_preferences_sha256: row.get(17)?,
                    review_receipt_id: row.get(18)?,
                    review_receipt_sha256: row.get(19)?,
                    head_transition_sha256: row.get(20)?,
                    predecessor_head_transition_sha256: row.get(21)?,
                    updated_by: row.get(22)?,
                    updated_at_ms: row.get(23)?,
                },
                taxonomy_version: row.get(24)?,
                canonical_policy_ciphertext: row.get(25)?,
                canonical_role_id: row.get(26)?,
                canonical_role_family_id: row.get(27)?,
                application_identity_id: row.get(28)?,
                source_resume_asset_id: row.get(29)?,
                predecessor_revision_id: row.get(30)?,
                predecessor_revision_no: row.get(31)?,
                predecessor_policy_sha256: row.get(32)?,
                compatibility_classification: row.get(33)?,
                revision_review_state: row.get(34)?,
                revision_created_by: row.get(35)?,
                revision_created_at_ms: row.get(36)?,
                canonical_review_receipt_ciphertext: row.get(37)?,
                reviewer_id: row.get(38)?,
                decision: row.get(39)?,
                decided_at_ms: row.get(40)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| anyhow::anyhow!("Career Track policy ledger is missing or mismatched"))
}

fn load_track_policy_ledger_postgres<C: postgres::GenericClient>(
    client: &mut C,
    account_id: &str,
    track_id: &str,
) -> Result<StoredTrackPolicyLedger> {
    let row = client
        .query_opt(
            "SELECT h.head_generation, h.previous_head_generation,
                    h.policy_revision_id, h.policy_revision_no,
                    h.canonical_policy_sha256, h.taxonomy_digest_sha256,
                    h.taxonomy_activation_epoch, h.canonicalizer_schema_version,
                    h.canonicalizer_digest_sha256, h.account_input_generation,
                    h.account_input_transition_sha256, h.account_semantic_sha256,
                    h.track_input_generation, h.track_input_transition_sha256,
                    h.track_semantic_sha256,
                    h.verified_application_identity_sha256,
                    h.source_resume_sha256, h.job_preferences_sha256,
                    h.review_receipt_id, h.review_receipt_sha256,
                    h.head_transition_sha256,
                    h.predecessor_head_transition_sha256,
                    h.updated_by, h.updated_at_ms,
                    revision.taxonomy_version,
                    revision.canonical_policy_ciphertext,
                    revision.canonical_role_id, revision.canonical_role_family,
                    revision.verified_application_identity_id,
                    revision.source_resume_asset_id,
                    revision.predecessor_revision_id,
                    revision.predecessor_revision_no,
                    revision.predecessor_policy_sha256,
                    revision.compatibility_classification,
                    revision.review_state, revision.created_by,
                    revision.created_at_ms,
                    receipt.canonical_review_receipt_ciphertext,
                    receipt.reviewer_id, receipt.decision, receipt.decided_at_ms
               FROM jobs_track_policy_heads AS h
               JOIN jobs_track_policy_head_transitions AS transition
                 ON transition.account_id = h.account_id
                AND transition.career_track_id = h.career_track_id
                AND transition.head_generation = h.head_generation
                AND transition.previous_head_generation = h.previous_head_generation
                AND transition.policy_revision_id = h.policy_revision_id
                AND transition.policy_revision_no = h.policy_revision_no
                AND transition.canonical_policy_sha256 = h.canonical_policy_sha256
                AND transition.taxonomy_digest_sha256 = h.taxonomy_digest_sha256
                AND transition.taxonomy_activation_epoch = h.taxonomy_activation_epoch
                AND transition.canonicalizer_schema_version = h.canonicalizer_schema_version
                AND transition.canonicalizer_digest_sha256 = h.canonicalizer_digest_sha256
                AND transition.account_input_generation = h.account_input_generation
                AND transition.account_input_transition_sha256 =
                    h.account_input_transition_sha256
                AND transition.account_semantic_sha256 = h.account_semantic_sha256
                AND transition.track_input_generation = h.track_input_generation
                AND transition.track_input_transition_sha256 = h.track_input_transition_sha256
                AND transition.track_semantic_sha256 = h.track_semantic_sha256
                AND transition.verified_application_identity_sha256 =
                    h.verified_application_identity_sha256
                AND transition.source_resume_sha256 = h.source_resume_sha256
                AND transition.job_preferences_sha256 = h.job_preferences_sha256
                AND transition.review_receipt_id = h.review_receipt_id
                AND transition.review_receipt_sha256 = h.review_receipt_sha256
                AND transition.head_transition_sha256 = h.head_transition_sha256
                AND transition.predecessor_head_transition_sha256
                      IS NOT DISTINCT FROM h.predecessor_head_transition_sha256
                AND transition.updated_by = h.updated_by
                AND transition.updated_at_ms = h.updated_at_ms
               JOIN jobs_track_policy_revisions AS revision
                 ON revision.account_id = h.account_id
                AND revision.career_track_id = h.career_track_id
                AND revision.revision_id = h.policy_revision_id
                AND revision.revision_no = h.policy_revision_no
                AND revision.canonical_policy_sha256 = h.canonical_policy_sha256
                AND revision.taxonomy_digest_sha256 = h.taxonomy_digest_sha256
                AND revision.taxonomy_activation_epoch = h.taxonomy_activation_epoch
                AND revision.canonicalizer_schema_version = h.canonicalizer_schema_version
                AND revision.canonicalizer_digest_sha256 = h.canonicalizer_digest_sha256
                AND revision.account_input_generation = h.account_input_generation
                AND revision.account_input_transition_sha256 = h.account_input_transition_sha256
                AND revision.account_semantic_sha256 = h.account_semantic_sha256
                AND revision.track_input_generation = h.track_input_generation
                AND revision.track_input_transition_sha256 = h.track_input_transition_sha256
                AND revision.track_semantic_sha256 = h.track_semantic_sha256
                AND revision.verified_application_identity_sha256 =
                    h.verified_application_identity_sha256
                AND revision.source_resume_sha256 = h.source_resume_sha256
                AND revision.job_preferences_sha256 = h.job_preferences_sha256
               JOIN jobs_track_policy_review_receipts AS receipt
                 ON receipt.account_id = h.account_id
                AND receipt.career_track_id = h.career_track_id
                AND receipt.policy_revision_id = h.policy_revision_id
                AND receipt.policy_revision_no = h.policy_revision_no
                AND receipt.canonical_policy_sha256 = h.canonical_policy_sha256
                AND receipt.taxonomy_digest_sha256 = h.taxonomy_digest_sha256
                AND receipt.taxonomy_activation_epoch = h.taxonomy_activation_epoch
                AND receipt.canonicalizer_schema_version = h.canonicalizer_schema_version
                AND receipt.canonicalizer_digest_sha256 = h.canonicalizer_digest_sha256
                AND receipt.account_input_generation = h.account_input_generation
                AND receipt.account_input_transition_sha256 = h.account_input_transition_sha256
                AND receipt.account_semantic_sha256 = h.account_semantic_sha256
                AND receipt.track_input_generation = h.track_input_generation
                AND receipt.track_input_transition_sha256 = h.track_input_transition_sha256
                AND receipt.track_semantic_sha256 = h.track_semantic_sha256
                AND receipt.verified_application_identity_sha256 =
                    h.verified_application_identity_sha256
                AND receipt.source_resume_sha256 = h.source_resume_sha256
                AND receipt.job_preferences_sha256 = h.job_preferences_sha256
                AND receipt.review_receipt_id = h.review_receipt_id
                AND receipt.review_receipt_sha256 = h.review_receipt_sha256
              WHERE h.account_id = $1 AND h.career_track_id = $2
              FOR SHARE OF h, transition, revision, receipt",
            &[&account_id, &track_id],
        )?
        .ok_or_else(|| anyhow::anyhow!("Career Track policy ledger is missing or mismatched"))?;
    Ok(StoredTrackPolicyLedger {
        head: StoredTrackPolicyHead {
            head_generation: row.get(0),
            previous_head_generation: row.get(1),
            revision_id: row.get(2),
            revision_no: row.get(3),
            canonical_policy_sha256: row.get(4),
            taxonomy_sha256: row.get(5),
            taxonomy_activation_epoch: row.get(6),
            canonicalizer_schema_version: row.get(7),
            canonicalizer_sha256: row.get(8),
            account_input_generation: row.get(9),
            account_input_transition_sha256: row.get(10),
            account_semantic_sha256: row.get(11),
            track_input_generation: row.get(12),
            track_input_transition_sha256: row.get(13),
            track_semantic_sha256: row.get(14),
            application_identity_sha256: row.get(15),
            source_resume_sha256: row.get(16),
            job_preferences_sha256: row.get(17),
            review_receipt_id: row.get(18),
            review_receipt_sha256: row.get(19),
            head_transition_sha256: row.get(20),
            predecessor_head_transition_sha256: row.get(21),
            updated_by: row.get(22),
            updated_at_ms: row.get(23),
        },
        taxonomy_version: row.get(24),
        canonical_policy_ciphertext: row.get(25),
        canonical_role_id: row.get(26),
        canonical_role_family_id: row.get(27),
        application_identity_id: row.get(28),
        source_resume_asset_id: row.get(29),
        predecessor_revision_id: row.get(30),
        predecessor_revision_no: row.get(31),
        predecessor_policy_sha256: row.get(32),
        compatibility_classification: row.get(33),
        revision_review_state: row.get(34),
        revision_created_by: row.get(35),
        revision_created_at_ms: row.get(36),
        canonical_review_receipt_ciphertext: row.get(37),
        reviewer_id: row.get(38),
        decision: row.get(39),
        decided_at_ms: row.get(40),
    })
}

fn decrypt_immutable_track_policy_evidence(raw: &str, label: &str) -> Result<String> {
    if !raw.starts_with(ENCRYPTED_PAYLOAD_PREFIX) {
        anyhow::bail!("{label} is not encrypted")
    }
    decrypt_payload(raw).with_context(|| format!("decrypt {label}"))
}

fn validate_equal_track_policy_record(
    account_id: &str,
    track_id: &str,
    ledger: &StoredTrackPolicyLedger,
    record: &CanonicalTrackPolicyRecord,
) -> Result<()> {
    let head = &ledger.head;
    anyhow::ensure!(
        head.canonical_policy_sha256 == record.canonical_policy_sha256
            && head.taxonomy_sha256 == record.taxonomy_sha256
            && head.taxonomy_activation_epoch == record.taxonomy_activation_epoch
            && head.canonicalizer_schema_version == record.canonicalizer_schema_version
            && head.canonicalizer_sha256 == record.canonicalizer_sha256
            && head.account_input_generation == record.account_input_generation
            && head.account_input_transition_sha256 == record.account_input_transition_sha256
            && head.account_semantic_sha256 == record.account_semantic_sha256
            && head.track_input_generation == record.track_input_generation
            && head.track_input_transition_sha256 == record.track_input_transition_sha256
            && head.track_semantic_sha256 == record.track_semantic_sha256
            && head.application_identity_sha256 == record.application_identity_sha256
            && head.source_resume_sha256 == record.source_resume_sha256
            && head.job_preferences_sha256 == record.job_preferences_sha256
            && ledger.taxonomy_version == record.taxonomy_version
            && ledger.canonical_role_id == record.canonical_role_id
            && ledger.canonical_role_family_id == record.canonical_role_family_id
            && ledger.application_identity_id == record.application_identity_id
            && ledger.source_resume_asset_id == record.source_resume_asset_id
            && ledger.revision_review_state == "approved"
            && ledger.decision == "approved",
        "equal canonical policy fast path does not bind the exact immutable head"
    );
    let canonical_policy = decrypt_immutable_track_policy_evidence(
        &ledger.canonical_policy_ciphertext,
        "canonical Career Track policy evidence",
    )?;
    anyhow::ensure!(
        canonical_policy == record.canonical_policy_json
            && track_policy_sha256(canonical_policy.as_bytes()) == head.canonical_policy_sha256,
        "equal canonical policy fast path evidence is corrupt"
    );
    let receipt = canonical_track_policy_review_receipt(
        account_id,
        track_id,
        record,
        &head.revision_id,
        head.revision_no,
        &head.review_receipt_id,
        &ledger.reviewer_id,
        &ledger.decision,
        ledger.decided_at_ms,
    )?;
    let stored_receipt = decrypt_immutable_track_policy_evidence(
        &ledger.canonical_review_receipt_ciphertext,
        "canonical Career Track policy review receipt",
    )?;
    anyhow::ensure!(
        receipt == stored_receipt
            && track_policy_sha256(stored_receipt.as_bytes()) == head.review_receipt_sha256,
        "equal canonical policy fast path receipt is corrupt"
    );
    let transition = track_policy_head_transition_sha256(
        account_id,
        track_id,
        head.head_generation,
        head.previous_head_generation,
        &head.revision_id,
        head.revision_no,
        record,
        &head.review_receipt_id,
        &head.review_receipt_sha256,
        head.predecessor_head_transition_sha256.as_deref(),
        &head.updated_by,
        head.updated_at_ms,
    )?;
    anyhow::ensure!(
        transition == head.head_transition_sha256,
        "equal canonical policy fast path head transition is corrupt"
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_track_policy_ledger_common(
    account_id: &str,
    track: &CareerTrack,
    ledger: &StoredTrackPolicyLedger,
    profile: &CareerProfile,
    preferences: &JobPreferences,
    identity: Option<&ApplicationIdentity>,
    resume_asset_verified: bool,
    generations: &CanonicalPolicyGenerations,
) -> Result<()> {
    let authority = &track.policy.authority;
    anyhow::ensure!(
        authority.review_state == "approved" && authority.review_reason_codes.is_empty(),
        "Career Track policy projection is not approved"
    );
    let mut expected_track = track.clone();
    let record = prepare_canonical_track_policy_record(
        account_id,
        &mut expected_track,
        profile,
        preferences,
        identity,
        resume_asset_verified,
        generations,
    )?
    .ok_or_else(|| anyhow::anyhow!("current Career Track policy inputs require review"))?;
    let head = &ledger.head;

    anyhow::ensure!(
        head.revision_no >= 1
            && head.head_generation == head.revision_no
            && head.updated_by == account_id
            && ledger.revision_created_by == account_id
            && ledger.reviewer_id == account_id
            && ledger.revision_review_state == "approved"
            && ledger.decision == "approved"
            && ledger.revision_created_at_ms <= ledger.decided_at_ms
            && ledger.decided_at_ms <= head.updated_at_ms,
        "Career Track policy ledger metadata is invalid"
    );
    if head.revision_no == 1 {
        anyhow::ensure!(
            head.previous_head_generation == 0
                && head.predecessor_head_transition_sha256.is_none()
                && ledger.predecessor_revision_id.is_none()
                && ledger.predecessor_revision_no.is_none()
                && ledger.predecessor_policy_sha256.is_none()
                && ledger.compatibility_classification == "initial",
            "initial Career Track policy ledger metadata is invalid"
        );
    } else {
        anyhow::ensure!(
            head.previous_head_generation == head.head_generation - 1
                && head.predecessor_head_transition_sha256.is_some()
                && ledger.predecessor_revision_id.is_some()
                && ledger.predecessor_revision_no == Some(head.revision_no - 1)
                && ledger.predecessor_policy_sha256.is_some()
                && ledger.compatibility_classification == "review_required",
            "Career Track policy CAS lineage is invalid"
        );
    }

    anyhow::ensure!(
        authority.policy_revision_id == head.revision_id
            && authority.policy_revision_no == head.revision_no
            && authority.canonical_policy_sha256 == head.canonical_policy_sha256
            && authority.policy_head_generation == head.head_generation
            && authority.policy_head_transition_sha256 == head.head_transition_sha256
            && authority.policy_review_receipt_id == head.review_receipt_id
            && authority.policy_review_receipt_sha256 == head.review_receipt_sha256
            && authority.taxonomy_version == ledger.taxonomy_version
            && authority.taxonomy_sha256 == head.taxonomy_sha256
            && authority.taxonomy_activation_epoch == head.taxonomy_activation_epoch
            && authority.canonicalizer_schema_version == head.canonicalizer_schema_version
            && authority.canonicalizer_sha256 == head.canonicalizer_sha256
            && authority.account_input_generation == head.account_input_generation
            && authority.account_input_transition_sha256 == head.account_input_transition_sha256
            && authority.account_input_semantic_sha256 == head.account_semantic_sha256
            && authority.track_input_generation == head.track_input_generation
            && authority.track_input_transition_sha256 == head.track_input_transition_sha256
            && authority.track_semantic_sha256 == head.track_semantic_sha256
            && authority.canonical_role_id == ledger.canonical_role_id
            && authority.canonical_role_family_id == ledger.canonical_role_family_id
            && authority.application_identity_id == ledger.application_identity_id
            && authority.application_identity_sha256 == head.application_identity_sha256
            && authority.source_resume_asset_id == ledger.source_resume_asset_id
            && authority.source_resume_sha256 == head.source_resume_sha256
            && authority.job_preferences_sha256 == head.job_preferences_sha256,
        "Career Track projection does not match its canonical policy head"
    );
    anyhow::ensure!(
        track.application_identity_id.as_deref() == Some(ledger.application_identity_id.as_str())
            && record.taxonomy_version == ledger.taxonomy_version
            && record.taxonomy_sha256 == head.taxonomy_sha256
            && record.taxonomy_activation_epoch == head.taxonomy_activation_epoch
            && record.canonicalizer_schema_version == head.canonicalizer_schema_version
            && record.canonicalizer_sha256 == head.canonicalizer_sha256
            && record.account_input_generation == head.account_input_generation
            && record.account_input_transition_sha256 == head.account_input_transition_sha256
            && record.account_semantic_sha256 == head.account_semantic_sha256
            && record.track_input_generation == head.track_input_generation
            && record.track_input_transition_sha256 == head.track_input_transition_sha256
            && record.track_semantic_sha256 == head.track_semantic_sha256
            && record.canonical_role_id == ledger.canonical_role_id
            && record.canonical_role_family_id == ledger.canonical_role_family_id
            && record.application_identity_id == ledger.application_identity_id
            && record.application_identity_sha256 == head.application_identity_sha256
            && record.source_resume_asset_id == ledger.source_resume_asset_id
            && record.source_resume_sha256 == head.source_resume_sha256
            && record.job_preferences_sha256 == head.job_preferences_sha256
            && record.canonical_policy_sha256 == head.canonical_policy_sha256,
        "current Career Track inputs do not match the canonical policy revision"
    );
    anyhow::ensure!(
        ledger.taxonomy_version == crate::jobs_taxonomy::taxonomy_version()
            && head.taxonomy_sha256 == crate::jobs_taxonomy::taxonomy_sha256()
            && head.taxonomy_activation_epoch == generations.activation.activation_epoch
            && head.canonicalizer_schema_version
                == generations.activation.canonicalizer_schema_version
            && head.canonicalizer_sha256 == generations.activation.canonicalizer_sha256
            && head.account_input_generation == generations.account.generation
            && head.account_input_transition_sha256 == generations.account.transition_sha256
            && head.account_semantic_sha256
                == generations
                    .account
                    .semantic_sha256
                    .as_deref()
                    .unwrap_or_default()
            && head.track_input_generation == generations.track.generation
            && head.track_input_transition_sha256 == generations.track.transition_sha256
            && head.track_semantic_sha256
                == generations
                    .track
                    .semantic_sha256
                    .as_deref()
                    .unwrap_or_default(),
        "Career Track taxonomy authority is stale"
    );

    let canonical_policy = decrypt_immutable_track_policy_evidence(
        &ledger.canonical_policy_ciphertext,
        "canonical Career Track policy evidence",
    )?;
    anyhow::ensure!(
        track_policy_sha256(canonical_policy.as_bytes()) == head.canonical_policy_sha256
            && canonical_policy == record.canonical_policy_json,
        "canonical Career Track policy evidence is corrupt or stale"
    );
    let canonical_receipt = canonical_track_policy_review_receipt(
        account_id,
        &track.id,
        &record,
        &head.revision_id,
        head.revision_no,
        &head.review_receipt_id,
        &ledger.reviewer_id,
        &ledger.decision,
        ledger.decided_at_ms,
    )?;
    let stored_receipt = decrypt_immutable_track_policy_evidence(
        &ledger.canonical_review_receipt_ciphertext,
        "canonical Career Track policy review receipt",
    )?;
    anyhow::ensure!(
        canonical_receipt == stored_receipt
            && track_policy_sha256(stored_receipt.as_bytes()) == head.review_receipt_sha256,
        "canonical Career Track review receipt is corrupt or stale"
    );
    let transition_sha256 = track_policy_head_transition_sha256(
        account_id,
        &track.id,
        head.head_generation,
        head.previous_head_generation,
        &head.revision_id,
        head.revision_no,
        &record,
        &head.review_receipt_id,
        &head.review_receipt_sha256,
        head.predecessor_head_transition_sha256.as_deref(),
        &head.updated_by,
        head.updated_at_ms,
    )?;
    anyhow::ensure!(
        transition_sha256 == head.head_transition_sha256,
        "Career Track policy head transition digest is invalid"
    );
    Ok(())
}

fn validate_track_policy_ledger_sqlite(
    conn: &rusqlite::Connection,
    account_id: &str,
    track: &CareerTrack,
) -> Result<()> {
    let ledger = load_track_policy_ledger_sqlite(conn, account_id, &track.id)?;
    let generations = load_canonical_policy_generations_sqlite(conn, account_id, track)?;
    let (profile, preferences, identity, resume_asset_verified) =
        canonical_track_policy_inputs_sqlite(conn, account_id, track)?;
    validate_track_policy_ledger_common(
        account_id,
        track,
        &ledger,
        &profile,
        &preferences,
        identity.as_ref(),
        resume_asset_verified,
        &generations,
    )?;
    if ledger.head.revision_no > 1 {
        let predecessor_exists: bool = conn.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM jobs_track_policy_revisions
                 WHERE account_id = ?1 AND career_track_id = ?2
                   AND revision_id = ?3 AND revision_no = ?4
                   AND canonical_policy_sha256 = ?5
            )",
            params![
                account_id,
                track.id,
                ledger.predecessor_revision_id,
                ledger.predecessor_revision_no,
                ledger.predecessor_policy_sha256,
            ],
            |row| row.get(0),
        )?;
        anyhow::ensure!(
            predecessor_exists,
            "Career Track policy predecessor is missing"
        );
    }
    Ok(())
}

fn validate_track_policy_ledger_postgres<C: postgres::GenericClient>(
    client: &mut C,
    account_id: &str,
    track: &CareerTrack,
) -> Result<()> {
    let ledger = load_track_policy_ledger_postgres(client, account_id, &track.id)?;
    let generations = load_canonical_policy_generations_postgres(client, account_id, track)?;
    let (profile, preferences, identity, resume_asset_verified) =
        canonical_track_policy_inputs_postgres(client, account_id, track)?;
    validate_track_policy_ledger_common(
        account_id,
        track,
        &ledger,
        &profile,
        &preferences,
        identity.as_ref(),
        resume_asset_verified,
        &generations,
    )?;
    if ledger.head.revision_no > 1 {
        let predecessor_exists: bool = client
            .query_one(
                "SELECT EXISTS(
                    SELECT 1 FROM jobs_track_policy_revisions
                     WHERE account_id = $1 AND career_track_id = $2
                       AND revision_id = $3 AND revision_no = $4
                       AND canonical_policy_sha256 = $5
                )",
                &[
                    &account_id,
                    &track.id,
                    &ledger.predecessor_revision_id,
                    &ledger.predecessor_revision_no,
                    &ledger.predecessor_policy_sha256,
                ],
            )?
            .get(0);
        anyhow::ensure!(
            predecessor_exists,
            "Career Track policy predecessor is missing"
        );
    }
    Ok(())
}

fn export_policy_revision_from_sqlite_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<(CanonicalTrackPolicyRevisionExport, String)> {
    Ok((
        CanonicalTrackPolicyRevisionExport {
            revision_id: row.get(0)?,
            career_track_id: row.get(1)?,
            revision_no: row.get(2)?,
            taxonomy_version: row.get(3)?,
            taxonomy_sha256: row.get(4)?,
            taxonomy_activation_epoch: row.get(21)?,
            canonicalizer_schema_version: row.get(22)?,
            canonicalizer_sha256: row.get(23)?,
            account_input_generation: row.get(24)?,
            account_input_transition_sha256: row.get(25)?,
            account_semantic_sha256: row.get(26)?,
            track_input_generation: row.get(27)?,
            track_input_transition_sha256: row.get(28)?,
            track_semantic_sha256: row.get(29)?,
            canonical_policy_sha256: row.get(5)?,
            canonical_policy_json: String::new(),
            canonical_role_id: row.get(7)?,
            canonical_role_family_id: row.get(8)?,
            application_identity_id: row.get(9)?,
            application_identity_sha256: row.get(10)?,
            source_resume_asset_id: row.get(11)?,
            source_resume_sha256: row.get(12)?,
            job_preferences_sha256: row.get(13)?,
            predecessor_revision_id: row.get(14)?,
            predecessor_revision_no: row.get(15)?,
            predecessor_policy_sha256: row.get(16)?,
            compatibility_classification: row.get(17)?,
            review_state: row.get(18)?,
            created_by: row.get(19)?,
            created_at_ms: row.get(20)?,
        },
        row.get(6)?,
    ))
}

fn export_policy_receipt_from_sqlite_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<(CanonicalTrackPolicyReviewReceiptExport, String)> {
    Ok((
        CanonicalTrackPolicyReviewReceiptExport {
            review_receipt_id: row.get(0)?,
            review_receipt_sha256: row.get(1)?,
            canonical_review_receipt_json: String::new(),
            career_track_id: row.get(3)?,
            policy_revision_id: row.get(4)?,
            policy_revision_no: row.get(5)?,
            canonical_policy_sha256: row.get(6)?,
            taxonomy_sha256: row.get(7)?,
            taxonomy_activation_epoch: row.get(14)?,
            canonicalizer_schema_version: row.get(15)?,
            canonicalizer_sha256: row.get(16)?,
            account_input_generation: row.get(17)?,
            account_input_transition_sha256: row.get(18)?,
            account_semantic_sha256: row.get(19)?,
            track_input_generation: row.get(20)?,
            track_input_transition_sha256: row.get(21)?,
            track_semantic_sha256: row.get(22)?,
            application_identity_sha256: row.get(8)?,
            source_resume_sha256: row.get(9)?,
            job_preferences_sha256: row.get(10)?,
            reviewer_id: row.get(11)?,
            decision: row.get(12)?,
            decided_at_ms: row.get(13)?,
        },
        row.get(2)?,
    ))
}

fn export_policy_head_from_sqlite_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<CanonicalTrackPolicyHeadExport> {
    Ok(CanonicalTrackPolicyHeadExport {
        career_track_id: row.get(0)?,
        head_generation: row.get(1)?,
        previous_head_generation: row.get(2)?,
        policy_revision_id: row.get(3)?,
        policy_revision_no: row.get(4)?,
        canonical_policy_sha256: row.get(5)?,
        taxonomy_sha256: row.get(6)?,
        taxonomy_activation_epoch: row.get(16)?,
        canonicalizer_schema_version: row.get(17)?,
        canonicalizer_sha256: row.get(18)?,
        account_input_generation: row.get(19)?,
        account_input_transition_sha256: row.get(20)?,
        account_semantic_sha256: row.get(21)?,
        track_input_generation: row.get(22)?,
        track_input_transition_sha256: row.get(23)?,
        track_semantic_sha256: row.get(24)?,
        application_identity_sha256: row.get(7)?,
        source_resume_sha256: row.get(8)?,
        job_preferences_sha256: row.get(9)?,
        review_receipt_id: row.get(10)?,
        review_receipt_sha256: row.get(11)?,
        head_transition_sha256: row.get(12)?,
        predecessor_head_transition_sha256: row.get(13)?,
        updated_by: row.get(14)?,
        updated_at_ms: row.get(15)?,
    })
}

fn export_policy_revision_from_postgres_row(
    row: &postgres::Row,
) -> (CanonicalTrackPolicyRevisionExport, String) {
    (
        CanonicalTrackPolicyRevisionExport {
            revision_id: row.get(0),
            career_track_id: row.get(1),
            revision_no: row.get(2),
            taxonomy_version: row.get(3),
            taxonomy_sha256: row.get(4),
            taxonomy_activation_epoch: row.get(21),
            canonicalizer_schema_version: row.get(22),
            canonicalizer_sha256: row.get(23),
            account_input_generation: row.get(24),
            account_input_transition_sha256: row.get(25),
            account_semantic_sha256: row.get(26),
            track_input_generation: row.get(27),
            track_input_transition_sha256: row.get(28),
            track_semantic_sha256: row.get(29),
            canonical_policy_sha256: row.get(5),
            canonical_policy_json: String::new(),
            canonical_role_id: row.get(7),
            canonical_role_family_id: row.get(8),
            application_identity_id: row.get(9),
            application_identity_sha256: row.get(10),
            source_resume_asset_id: row.get(11),
            source_resume_sha256: row.get(12),
            job_preferences_sha256: row.get(13),
            predecessor_revision_id: row.get(14),
            predecessor_revision_no: row.get(15),
            predecessor_policy_sha256: row.get(16),
            compatibility_classification: row.get(17),
            review_state: row.get(18),
            created_by: row.get(19),
            created_at_ms: row.get(20),
        },
        row.get(6),
    )
}

fn export_policy_receipt_from_postgres_row(
    row: &postgres::Row,
) -> (CanonicalTrackPolicyReviewReceiptExport, String) {
    (
        CanonicalTrackPolicyReviewReceiptExport {
            review_receipt_id: row.get(0),
            review_receipt_sha256: row.get(1),
            canonical_review_receipt_json: String::new(),
            career_track_id: row.get(3),
            policy_revision_id: row.get(4),
            policy_revision_no: row.get(5),
            canonical_policy_sha256: row.get(6),
            taxonomy_sha256: row.get(7),
            taxonomy_activation_epoch: row.get(14),
            canonicalizer_schema_version: row.get(15),
            canonicalizer_sha256: row.get(16),
            account_input_generation: row.get(17),
            account_input_transition_sha256: row.get(18),
            account_semantic_sha256: row.get(19),
            track_input_generation: row.get(20),
            track_input_transition_sha256: row.get(21),
            track_semantic_sha256: row.get(22),
            application_identity_sha256: row.get(8),
            source_resume_sha256: row.get(9),
            job_preferences_sha256: row.get(10),
            reviewer_id: row.get(11),
            decision: row.get(12),
            decided_at_ms: row.get(13),
        },
        row.get(2),
    )
}

fn export_policy_head_from_postgres_row(row: &postgres::Row) -> CanonicalTrackPolicyHeadExport {
    CanonicalTrackPolicyHeadExport {
        career_track_id: row.get(0),
        head_generation: row.get(1),
        previous_head_generation: row.get(2),
        policy_revision_id: row.get(3),
        policy_revision_no: row.get(4),
        canonical_policy_sha256: row.get(5),
        taxonomy_sha256: row.get(6),
        taxonomy_activation_epoch: row.get(16),
        canonicalizer_schema_version: row.get(17),
        canonicalizer_sha256: row.get(18),
        account_input_generation: row.get(19),
        account_input_transition_sha256: row.get(20),
        account_semantic_sha256: row.get(21),
        track_input_generation: row.get(22),
        track_input_transition_sha256: row.get(23),
        track_semantic_sha256: row.get(24),
        application_identity_sha256: row.get(7),
        source_resume_sha256: row.get(8),
        job_preferences_sha256: row.get(9),
        review_receipt_id: row.get(10),
        review_receipt_sha256: row.get(11),
        head_transition_sha256: row.get(12),
        predecessor_head_transition_sha256: row.get(13),
        updated_by: row.get(14),
        updated_at_ms: row.get(15),
    }
}

fn export_activation_from_sqlite_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<TaxonomyActivationTransitionExport> {
    Ok(TaxonomyActivationTransitionExport {
        activation_epoch: row.get(0)?,
        previous_activation_epoch: row.get(1)?,
        taxonomy_version: row.get(2)?,
        taxonomy_sha256: row.get(3)?,
        canonicalizer_schema_version: row.get(4)?,
        canonicalizer_sha256: row.get(5)?,
        transition_sha256: row.get(6)?,
        predecessor_transition_sha256: row.get(7)?,
        activated_at_ms: row.get(8)?,
    })
}

fn export_activation_from_postgres_row(row: &postgres::Row) -> TaxonomyActivationTransitionExport {
    TaxonomyActivationTransitionExport {
        activation_epoch: row.get(0),
        previous_activation_epoch: row.get(1),
        taxonomy_version: row.get(2),
        taxonomy_sha256: row.get(3),
        canonicalizer_schema_version: row.get(4),
        canonicalizer_sha256: row.get(5),
        transition_sha256: row.get(6),
        predecessor_transition_sha256: row.get(7),
        activated_at_ms: row.get(8),
    }
}

fn export_account_input_from_sqlite_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<AccountInputTransitionExport> {
    Ok(AccountInputTransitionExport {
        transition_id: row.get(0)?,
        generation: row.get(1)?,
        previous_generation: row.get(2)?,
        input_kind: row.get(3)?,
        subject_sha256: row.get(4)?,
        account_semantic_sha256: row.get(5)?,
        transition_sha256: row.get(6)?,
        predecessor_transition_sha256: row.get(7)?,
        changed_at_ms: row.get(8)?,
    })
}

fn export_account_input_from_postgres_row(row: &postgres::Row) -> AccountInputTransitionExport {
    AccountInputTransitionExport {
        transition_id: row.get(0),
        generation: row.get(1),
        previous_generation: row.get(2),
        input_kind: row.get(3),
        subject_sha256: row.get(4),
        account_semantic_sha256: row.get(5),
        transition_sha256: row.get(6),
        predecessor_transition_sha256: row.get(7),
        changed_at_ms: row.get(8),
    }
}

fn export_track_input_from_sqlite_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<TrackInputTransitionExport> {
    Ok(TrackInputTransitionExport {
        transition_id: row.get(0)?,
        career_track_id: row.get(1)?,
        generation: row.get(2)?,
        previous_generation: row.get(3)?,
        track_semantic_sha256: row.get(4)?,
        transition_sha256: row.get(5)?,
        predecessor_transition_sha256: row.get(6)?,
        changed_at_ms: row.get(7)?,
    })
}

fn export_track_input_from_postgres_row(row: &postgres::Row) -> TrackInputTransitionExport {
    TrackInputTransitionExport {
        transition_id: row.get(0),
        career_track_id: row.get(1),
        generation: row.get(2),
        previous_generation: row.get(3),
        track_semantic_sha256: row.get(4),
        transition_sha256: row.get(5),
        predecessor_transition_sha256: row.get(6),
        changed_at_ms: row.get(7),
    }
}

fn decrypt_export_policy_evidence(raw: &str, sha256: &str, label: &str) -> Result<String> {
    let canonical = decrypt_immutable_track_policy_evidence(raw, label)?;
    anyhow::ensure!(
        track_policy_sha256(canonical.as_bytes()) == sha256,
        "{label} digest does not match its immutable ledger"
    );
    Ok(canonical)
}

const TRACK_POLICY_REVISION_EXPORT_SQL: &str =
    "SELECT revision_id, career_track_id, revision_no, taxonomy_version,
            taxonomy_digest_sha256, canonical_policy_sha256,
            canonical_policy_ciphertext, canonical_role_id,
            canonical_role_family, verified_application_identity_id,
            verified_application_identity_sha256, source_resume_asset_id,
            source_resume_sha256, job_preferences_sha256,
            predecessor_revision_id, predecessor_revision_no,
            predecessor_policy_sha256, compatibility_classification,
            review_state, created_by, created_at_ms,
            taxonomy_activation_epoch, canonicalizer_schema_version,
            canonicalizer_digest_sha256, account_input_generation,
            account_input_transition_sha256, account_semantic_sha256,
            track_input_generation, track_input_transition_sha256,
            track_semantic_sha256
       FROM jobs_track_policy_revisions";
const TRACK_POLICY_RECEIPT_EXPORT_SQL: &str = "SELECT review_receipt_id, review_receipt_sha256,
            canonical_review_receipt_ciphertext, career_track_id,
            policy_revision_id, policy_revision_no, canonical_policy_sha256,
            taxonomy_digest_sha256, verified_application_identity_sha256,
            source_resume_sha256, job_preferences_sha256, reviewer_id,
            decision, decided_at_ms, taxonomy_activation_epoch,
            canonicalizer_schema_version, canonicalizer_digest_sha256,
            account_input_generation, account_input_transition_sha256,
            account_semantic_sha256, track_input_generation,
            track_input_transition_sha256, track_semantic_sha256
       FROM jobs_track_policy_review_receipts";
const TRACK_POLICY_HEAD_EXPORT_SQL: &str =
    "SELECT career_track_id, head_generation, previous_head_generation,
            policy_revision_id, policy_revision_no, canonical_policy_sha256,
            taxonomy_digest_sha256, verified_application_identity_sha256,
            source_resume_sha256, job_preferences_sha256, review_receipt_id,
            review_receipt_sha256, head_transition_sha256,
            predecessor_head_transition_sha256, updated_by, updated_at_ms,
            taxonomy_activation_epoch, canonicalizer_schema_version,
            canonicalizer_digest_sha256, account_input_generation,
            account_input_transition_sha256, account_semantic_sha256,
            track_input_generation, track_input_transition_sha256,
            track_semantic_sha256
       FROM jobs_track_policy_heads";
const TRACK_POLICY_HEAD_TRANSITION_EXPORT_SQL: &str =
    "SELECT career_track_id, head_generation, previous_head_generation,
            policy_revision_id, policy_revision_no, canonical_policy_sha256,
            taxonomy_digest_sha256, verified_application_identity_sha256,
            source_resume_sha256, job_preferences_sha256, review_receipt_id,
            review_receipt_sha256, head_transition_sha256,
            predecessor_head_transition_sha256, updated_by, updated_at_ms,
            taxonomy_activation_epoch, canonicalizer_schema_version,
            canonicalizer_digest_sha256, account_input_generation,
            account_input_transition_sha256, account_semantic_sha256,
            track_input_generation, track_input_transition_sha256,
            track_semantic_sha256
       FROM jobs_track_policy_head_transitions";
const TAXONOMY_ACTIVATION_EXPORT_SQL: &str =
    "SELECT activation_epoch, previous_activation_epoch, taxonomy_version,
            taxonomy_digest_sha256, canonicalizer_schema_version,
            canonicalizer_digest_sha256, activation_transition_sha256,
            predecessor_activation_transition_sha256, activated_at_ms";
const ACCOUNT_INPUT_EXPORT_SQL: &str =
    "SELECT input_transition_id, input_generation, previous_input_generation,
            input_kind, input_subject_sha256, account_semantic_sha256,
            input_transition_sha256, predecessor_input_transition_sha256,
            changed_at_ms";
const ACCOUNT_INPUT_HEAD_EXPORT_SQL: &str =
    "SELECT input_transition_id, input_generation, previous_input_generation,
            input_kind, input_subject_sha256, account_semantic_sha256,
            input_transition_sha256, predecessor_input_transition_sha256,
            updated_at_ms";
const TRACK_INPUT_EXPORT_SQL: &str =
    "SELECT input_transition_id, career_track_id, input_generation,
            previous_input_generation, track_semantic_sha256,
            input_transition_sha256, predecessor_input_transition_sha256,
            changed_at_ms";
const TRACK_INPUT_HEAD_EXPORT_SQL: &str =
    "SELECT input_transition_id, career_track_id, input_generation,
            previous_input_generation, track_semantic_sha256,
            input_transition_sha256, predecessor_input_transition_sha256,
            updated_at_ms";

fn export_record(revision: &CanonicalTrackPolicyRevisionExport) -> CanonicalTrackPolicyRecord {
    CanonicalTrackPolicyRecord {
        taxonomy_version: revision.taxonomy_version.clone(),
        taxonomy_sha256: revision.taxonomy_sha256.clone(),
        taxonomy_activation_epoch: revision.taxonomy_activation_epoch,
        canonicalizer_schema_version: revision.canonicalizer_schema_version,
        canonicalizer_sha256: revision.canonicalizer_sha256.clone(),
        account_input_generation: revision.account_input_generation,
        account_input_transition_sha256: revision.account_input_transition_sha256.clone(),
        account_semantic_sha256: revision.account_semantic_sha256.clone(),
        track_input_generation: revision.track_input_generation,
        track_input_transition_sha256: revision.track_input_transition_sha256.clone(),
        track_semantic_sha256: revision.track_semantic_sha256.clone(),
        canonical_policy_json: revision.canonical_policy_json.clone(),
        canonical_policy_sha256: revision.canonical_policy_sha256.clone(),
        canonical_role_id: revision.canonical_role_id.clone(),
        canonical_role_family_id: revision.canonical_role_family_id.clone(),
        application_identity_id: revision.application_identity_id.clone(),
        application_identity_sha256: revision.application_identity_sha256.clone(),
        source_resume_asset_id: revision.source_resume_asset_id.clone(),
        source_resume_sha256: revision.source_resume_sha256.clone(),
        job_preferences_sha256: revision.job_preferences_sha256.clone(),
    }
}

fn validate_track_policy_export(
    account_id: &str,
    export: &CanonicalTrackPolicyLedgerExport,
) -> Result<()> {
    let mut activation_predecessor: Option<&str> = None;
    let mut activation_time = 0_i64;
    for (index, event) in export.taxonomy_activation_transitions.iter().enumerate() {
        let epoch = i64::try_from(index)? + 1;
        anyhow::ensure!(
            event.activation_epoch == epoch
                && event.previous_activation_epoch == epoch - 1
                && event.predecessor_transition_sha256.as_deref() == activation_predecessor
                && event.activated_at_ms >= activation_time,
            "exported taxonomy activation chain is not contiguous"
        );
        let expected = taxonomy_activation_transition_sha256(
            event.activation_epoch,
            event.previous_activation_epoch,
            &event.taxonomy_version,
            &event.taxonomy_sha256,
            event.canonicalizer_schema_version,
            &event.canonicalizer_sha256,
            event.predecessor_transition_sha256.as_deref(),
            event.activated_at_ms,
        )?;
        anyhow::ensure!(
            expected == event.transition_sha256,
            "exported taxonomy activation transition is corrupt"
        );
        activation_predecessor = Some(&event.transition_sha256);
        activation_time = event.activated_at_ms;
    }
    anyhow::ensure!(
        export
            .taxonomy_activation_head
            .as_ref()
            .map(serde_json::to_value)
            .transpose()?
            == export
                .taxonomy_activation_transitions
                .last()
                .map(serde_json::to_value)
                .transpose()?,
        "exported taxonomy activation head is not the terminal transition"
    );

    let mut predecessor: Option<&str> = None;
    let mut account_time = 0_i64;
    for (index, event) in export.account_input_transitions.iter().enumerate() {
        let generation = i64::try_from(index)? + 1;
        anyhow::ensure!(
            event.generation == generation
                && event.previous_generation == generation - 1
                && event.predecessor_transition_sha256.as_deref() == predecessor
                && event.changed_at_ms >= account_time,
            "exported account semantic-input chain is not contiguous"
        );
        let expected = semantic_input_transition_sha256(
            "account",
            account_id,
            None,
            event.generation,
            event.previous_generation,
            &event.input_kind,
            &event.account_semantic_sha256,
            event.predecessor_transition_sha256.as_deref(),
            event.changed_at_ms,
        )?;
        anyhow::ensure!(
            expected == event.transition_sha256,
            "exported account semantic-input transition is corrupt"
        );
        predecessor = Some(&event.transition_sha256);
        account_time = event.changed_at_ms;
    }
    anyhow::ensure!(
        export
            .account_input_head
            .as_ref()
            .map(serde_json::to_value)
            .transpose()?
            == export
                .account_input_transitions
                .last()
                .map(serde_json::to_value)
                .transpose()?,
        "exported account semantic-input head is not terminal"
    );

    let mut track_id = "";
    let mut track_generation = 0_i64;
    let mut track_predecessor: Option<&str> = None;
    let mut track_time = 0_i64;
    for event in &export.track_input_transitions {
        if event.career_track_id != track_id {
            track_id = &event.career_track_id;
            track_generation = 0;
            track_predecessor = None;
            track_time = 0;
        }
        track_generation += 1;
        anyhow::ensure!(
            event.generation == track_generation
                && event.previous_generation == track_generation - 1
                && event.predecessor_transition_sha256.as_deref() == track_predecessor
                && event.changed_at_ms >= track_time,
            "exported Track semantic-input chain is not contiguous"
        );
        let expected = semantic_input_transition_sha256(
            "track",
            account_id,
            Some(&event.career_track_id),
            event.generation,
            event.previous_generation,
            "track_upsert",
            &event.track_semantic_sha256,
            event.predecessor_transition_sha256.as_deref(),
            event.changed_at_ms,
        )?;
        anyhow::ensure!(
            expected == event.transition_sha256,
            "exported Track semantic-input transition is corrupt"
        );
        track_predecessor = Some(&event.transition_sha256);
        track_time = event.changed_at_ms;
    }
    for head in &export.track_input_heads {
        let terminal = export
            .track_input_transitions
            .iter()
            .rev()
            .find(|event| event.career_track_id == head.career_track_id)
            .context("Track semantic-input head has no history")?;
        anyhow::ensure!(
            serde_json::to_value(head)? == serde_json::to_value(terminal)?,
            "Track semantic-input head is not terminal"
        );
    }
    let transition_track_count = export
        .track_input_transitions
        .iter()
        .map(|event| event.career_track_id.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    anyhow::ensure!(
        transition_track_count == export.track_input_heads.len(),
        "exported Track semantic-input history is missing a current head"
    );

    let mut revision_track = "";
    let mut revision_no = 0_i64;
    let mut previous_revision: Option<&CanonicalTrackPolicyRevisionExport> = None;
    for revision in &export.revisions {
        if revision.career_track_id != revision_track {
            revision_track = &revision.career_track_id;
            revision_no = 0;
            previous_revision = None;
        }
        revision_no += 1;
        anyhow::ensure!(
            revision.revision_no == revision_no
                && track_policy_sha256(revision.canonical_policy_json.as_bytes())
                    == revision.canonical_policy_sha256
                && revision.review_state == "approved"
                && revision.created_by == account_id,
            "exported canonical policy revision chain is corrupt"
        );
        if let Some(previous) = previous_revision {
            anyhow::ensure!(
                revision.predecessor_revision_id.as_deref() == Some(previous.revision_id.as_str())
                    && revision.predecessor_revision_no == Some(previous.revision_no)
                    && revision.predecessor_policy_sha256.as_deref()
                        == Some(previous.canonical_policy_sha256.as_str())
                    && revision.compatibility_classification == "review_required"
                    && revision.created_at_ms >= previous.created_at_ms,
                "exported canonical policy predecessor is invalid"
            );
        } else {
            anyhow::ensure!(
                revision.predecessor_revision_id.is_none()
                    && revision.predecessor_revision_no.is_none()
                    && revision.predecessor_policy_sha256.is_none()
                    && revision.compatibility_classification == "initial",
                "exported initial canonical policy predecessor is invalid"
            );
        }
        let canonical: Value = serde_json::from_str(&revision.canonical_policy_json)?;
        let activation = export
            .taxonomy_activation_transitions
            .iter()
            .find(|event| event.activation_epoch == revision.taxonomy_activation_epoch)
            .context("exported canonical policy revision has no taxonomy activation")?;
        let account_input = export
            .account_input_transitions
            .iter()
            .find(|event| event.generation == revision.account_input_generation)
            .context("exported canonical policy revision has no account input transition")?;
        let track_input = export
            .track_input_transitions
            .iter()
            .find(|event| {
                event.career_track_id == revision.career_track_id
                    && event.generation == revision.track_input_generation
            })
            .context("exported canonical policy revision has no Track input transition")?;
        let canonical_preferences: JobPreferences = serde_json::from_value(
            canonical
                .pointer("/job_preferences")
                .cloned()
                .context("canonical policy job preferences are missing")?,
        )?;
        anyhow::ensure!(
            canonical.pointer("/account_id").and_then(Value::as_str) == Some(account_id)
                && canonical
                    .pointer("/career_track/id")
                    .and_then(Value::as_str)
                    == Some(revision.career_track_id.as_str())
                && canonical
                    .pointer("/taxonomy/activation_epoch")
                    .and_then(Value::as_i64)
                    == Some(revision.taxonomy_activation_epoch)
                && canonical
                    .pointer("/taxonomy/version")
                    .and_then(Value::as_str)
                    == Some(revision.taxonomy_version.as_str())
                && canonical
                    .pointer("/taxonomy/sha256")
                    .and_then(Value::as_str)
                    == Some(revision.taxonomy_sha256.as_str())
                && canonical
                    .pointer("/taxonomy/activation_transition_sha256")
                    .and_then(Value::as_str)
                    == Some(activation.transition_sha256.as_str())
                && activation.taxonomy_version == revision.taxonomy_version
                && activation.taxonomy_sha256 == revision.taxonomy_sha256
                && activation.canonicalizer_schema_version == revision.canonicalizer_schema_version
                && activation.canonicalizer_sha256 == revision.canonicalizer_sha256
                && canonical
                    .pointer("/canonicalizer/schema_version")
                    .and_then(Value::as_i64)
                    == Some(revision.canonicalizer_schema_version)
                && canonical
                    .pointer("/canonicalizer/sha256")
                    .and_then(Value::as_str)
                    == Some(revision.canonicalizer_sha256.as_str())
                && canonical
                    .pointer("/semantic_inputs/account_generation")
                    .and_then(Value::as_i64)
                    == Some(revision.account_input_generation)
                && canonical
                    .pointer("/semantic_inputs/account_transition_sha256")
                    .and_then(Value::as_str)
                    == Some(revision.account_input_transition_sha256.as_str())
                && canonical
                    .pointer("/semantic_inputs/account_semantic_sha256")
                    .and_then(Value::as_str)
                    == Some(revision.account_semantic_sha256.as_str())
                && account_input.transition_sha256 == revision.account_input_transition_sha256
                && account_input.account_semantic_sha256 == revision.account_semantic_sha256
                && canonical
                    .pointer("/semantic_inputs/track_generation")
                    .and_then(Value::as_i64)
                    == Some(revision.track_input_generation)
                && canonical
                    .pointer("/semantic_inputs/track_transition_sha256")
                    .and_then(Value::as_str)
                    == Some(revision.track_input_transition_sha256.as_str())
                && canonical
                    .pointer("/semantic_inputs/track_semantic_sha256")
                    .and_then(Value::as_str)
                    == Some(revision.track_semantic_sha256.as_str())
                && track_input.transition_sha256 == revision.track_input_transition_sha256
                && track_input.track_semantic_sha256 == revision.track_semantic_sha256
                && canonical
                    .pointer("/career_track/canonical_role_id")
                    .and_then(Value::as_str)
                    == Some(revision.canonical_role_id.as_str())
                && canonical
                    .pointer("/career_track/canonical_role_family_id")
                    .and_then(Value::as_str)
                    == Some(revision.canonical_role_family_id.as_str())
                && canonical
                    .pointer("/application_identity/id")
                    .and_then(Value::as_str)
                    == Some(revision.application_identity_id.as_str())
                && canonical
                    .pointer("/application_identity/sha256")
                    .and_then(Value::as_str)
                    == Some(revision.application_identity_sha256.as_str())
                && canonical
                    .pointer("/source_resume/asset_id")
                    .and_then(Value::as_str)
                    == Some(revision.source_resume_asset_id.as_str())
                && canonical
                    .pointer("/source_resume/sha256")
                    .and_then(Value::as_str)
                    == Some(revision.source_resume_sha256.as_str())
                && job_preferences_policy_sha256(&canonical_preferences)?
                    == revision.job_preferences_sha256,
            "exported canonical policy JSON does not reconstruct its relational binding"
        );
        previous_revision = Some(revision);
    }

    for receipt in &export.review_receipts {
        let revision = export
            .revisions
            .iter()
            .find(|revision| {
                revision.career_track_id == receipt.career_track_id
                    && revision.revision_id == receipt.policy_revision_id
                    && revision.revision_no == receipt.policy_revision_no
            })
            .context("exported review receipt has no exact policy revision")?;
        let record = export_record(revision);
        anyhow::ensure!(
            receipt.canonical_policy_sha256 == revision.canonical_policy_sha256
                && receipt.taxonomy_sha256 == revision.taxonomy_sha256
                && receipt.taxonomy_activation_epoch == revision.taxonomy_activation_epoch
                && receipt.canonicalizer_schema_version == revision.canonicalizer_schema_version
                && receipt.canonicalizer_sha256 == revision.canonicalizer_sha256
                && receipt.account_input_generation == revision.account_input_generation
                && receipt.account_input_transition_sha256
                    == revision.account_input_transition_sha256
                && receipt.account_semantic_sha256 == revision.account_semantic_sha256
                && receipt.track_input_generation == revision.track_input_generation
                && receipt.track_input_transition_sha256 == revision.track_input_transition_sha256
                && receipt.track_semantic_sha256 == revision.track_semantic_sha256
                && receipt.application_identity_sha256 == revision.application_identity_sha256
                && receipt.source_resume_sha256 == revision.source_resume_sha256
                && receipt.job_preferences_sha256 == revision.job_preferences_sha256,
            "exported canonical review receipt does not bind its exact revision"
        );
        anyhow::ensure!(
            receipt.reviewer_id == account_id
                && receipt.decision == "approved"
                && receipt.decided_at_ms >= revision.created_at_ms,
            "exported canonical review receipt metadata is invalid"
        );
        let canonical = canonical_track_policy_review_receipt(
            account_id,
            &receipt.career_track_id,
            &record,
            &receipt.policy_revision_id,
            receipt.policy_revision_no,
            &receipt.review_receipt_id,
            &receipt.reviewer_id,
            &receipt.decision,
            receipt.decided_at_ms,
        )?;
        anyhow::ensure!(
            canonical == receipt.canonical_review_receipt_json
                && track_policy_sha256(canonical.as_bytes()) == receipt.review_receipt_sha256,
            "exported canonical review receipt is corrupt"
        );
    }

    let mut policy_track = "";
    let mut policy_generation = 0_i64;
    let mut policy_predecessor: Option<&str> = None;
    let mut policy_time = 0_i64;
    for event in &export.head_transitions {
        if event.career_track_id != policy_track {
            policy_track = &event.career_track_id;
            policy_generation = 0;
            policy_predecessor = None;
            policy_time = 0;
        }
        policy_generation += 1;
        anyhow::ensure!(
            event.head_generation == policy_generation
                && event.previous_head_generation == policy_generation - 1
                && event.predecessor_head_transition_sha256.as_deref() == policy_predecessor
                && event.policy_revision_no == event.head_generation
                && event.updated_by == account_id
                && event.updated_at_ms >= policy_time,
            "exported policy-head transition chain is not contiguous"
        );
        let revision = export
            .revisions
            .iter()
            .find(|revision| {
                revision.career_track_id == event.career_track_id
                    && revision.revision_id == event.policy_revision_id
                    && revision.revision_no == event.policy_revision_no
            })
            .context("exported policy-head transition has no revision")?;
        let record = export_record(revision);
        let receipt = export
            .review_receipts
            .iter()
            .find(|receipt| {
                receipt.career_track_id == event.career_track_id
                    && receipt.policy_revision_id == event.policy_revision_id
                    && receipt.policy_revision_no == event.policy_revision_no
                    && receipt.review_receipt_id == event.review_receipt_id
                    && receipt.review_receipt_sha256 == event.review_receipt_sha256
            })
            .context("exported policy-head transition has no exact review receipt")?;
        anyhow::ensure!(
            event.canonical_policy_sha256 == revision.canonical_policy_sha256
                && event.taxonomy_sha256 == revision.taxonomy_sha256
                && event.taxonomy_activation_epoch == revision.taxonomy_activation_epoch
                && event.canonicalizer_schema_version == revision.canonicalizer_schema_version
                && event.canonicalizer_sha256 == revision.canonicalizer_sha256
                && event.account_input_generation == revision.account_input_generation
                && event.account_input_transition_sha256
                    == revision.account_input_transition_sha256
                && event.account_semantic_sha256 == revision.account_semantic_sha256
                && event.track_input_generation == revision.track_input_generation
                && event.track_input_transition_sha256 == revision.track_input_transition_sha256
                && event.track_semantic_sha256 == revision.track_semantic_sha256
                && event.application_identity_sha256 == revision.application_identity_sha256
                && event.source_resume_sha256 == revision.source_resume_sha256
                && event.job_preferences_sha256 == revision.job_preferences_sha256
                && receipt.canonical_policy_sha256 == event.canonical_policy_sha256
                && event.updated_at_ms >= receipt.decided_at_ms,
            "exported policy-head transition does not bind exact revision/receipt evidence"
        );
        let expected = track_policy_head_transition_sha256(
            account_id,
            &event.career_track_id,
            event.head_generation,
            event.previous_head_generation,
            &event.policy_revision_id,
            event.policy_revision_no,
            &record,
            &event.review_receipt_id,
            &event.review_receipt_sha256,
            event.predecessor_head_transition_sha256.as_deref(),
            &event.updated_by,
            event.updated_at_ms,
        )?;
        anyhow::ensure!(
            expected == event.head_transition_sha256,
            "exported policy-head transition digest is corrupt"
        );
        policy_predecessor = Some(&event.head_transition_sha256);
        policy_time = event.updated_at_ms;
    }
    let mut receipt_counts = BTreeMap::new();
    for receipt in &export.review_receipts {
        *receipt_counts
            .entry((
                receipt.career_track_id.as_str(),
                receipt.policy_revision_id.as_str(),
                receipt.policy_revision_no,
                receipt.canonical_policy_sha256.as_str(),
            ))
            .or_insert(0_usize) += 1;
    }
    let mut head_transition_counts = BTreeMap::new();
    for event in &export.head_transitions {
        *head_transition_counts
            .entry((
                event.career_track_id.as_str(),
                event.policy_revision_id.as_str(),
                event.policy_revision_no,
                event.canonical_policy_sha256.as_str(),
            ))
            .or_insert(0_usize) += 1;
    }
    for revision in &export.revisions {
        let key = (
            revision.career_track_id.as_str(),
            revision.revision_id.as_str(),
            revision.revision_no,
            revision.canonical_policy_sha256.as_str(),
        );
        anyhow::ensure!(
            receipt_counts.get(&key) == Some(&1) && head_transition_counts.get(&key) == Some(&1),
            "exported canonical policy revision is missing exact receipt/head-transition authority"
        );
    }
    for head in &export.heads {
        let terminal = export
            .head_transitions
            .iter()
            .rev()
            .find(|event| event.career_track_id == head.career_track_id)
            .context("policy head has no immutable transition history")?;
        anyhow::ensure!(
            serde_json::to_value(head)? == serde_json::to_value(terminal)?,
            "current policy head is not the terminal immutable transition"
        );
    }
    let policy_track_count = export
        .head_transitions
        .iter()
        .map(|event| event.career_track_id.as_str())
        .collect::<BTreeSet<_>>()
        .len();
    anyhow::ensure!(
        policy_track_count == export.heads.len(),
        "exported policy-head history is missing a current head"
    );
    Ok(())
}

pub fn export_canonical_track_policy_ledger(
    pool: &DbPool,
    account_id: &str,
) -> Result<CanonicalTrackPolicyLedgerExport> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut conn = pool.get()?;
            let tx = conn.transaction()?;
            let taxonomy_activation_transitions = {
                let mut stmt = tx.prepare(&format!(
                    "{TAXONOMY_ACTIVATION_EXPORT_SQL}
                       FROM jobs_track_policy_taxonomy_activation_events
                      ORDER BY activation_epoch"
                ))?;
                let values = stmt
                    .query_map([], export_activation_from_sqlite_row)?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                values
            };
            let taxonomy_activation_head = tx
                .query_row(
                    &format!(
                        "{TAXONOMY_ACTIVATION_EXPORT_SQL}
                           FROM jobs_track_policy_taxonomy_activation_head
                          WHERE singleton_id = 1"
                    ),
                    [],
                    export_activation_from_sqlite_row,
                )
                .optional()?;
            let account_input_transitions = {
                let mut stmt = tx.prepare(&format!(
                    "{ACCOUNT_INPUT_EXPORT_SQL}
                       FROM jobs_track_policy_account_input_transitions
                      WHERE account_id = ?1 ORDER BY input_generation"
                ))?;
                let values = stmt
                    .query_map(params![account_id], export_account_input_from_sqlite_row)?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                values
            };
            let account_input_head = tx
                .query_row(
                    &format!(
                        "{ACCOUNT_INPUT_HEAD_EXPORT_SQL}
                           FROM jobs_track_policy_account_input_heads
                          WHERE account_id = ?1"
                    ),
                    params![account_id],
                    export_account_input_from_sqlite_row,
                )
                .optional()?;
            let track_input_transitions = {
                let mut stmt = tx.prepare(&format!(
                    "{TRACK_INPUT_EXPORT_SQL}
                       FROM jobs_track_policy_track_input_transitions
                      WHERE account_id = ?1
                      ORDER BY career_track_id, input_generation"
                ))?;
                let values = stmt
                    .query_map(params![account_id], export_track_input_from_sqlite_row)?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                values
            };
            let track_input_heads = {
                let mut stmt = tx.prepare(&format!(
                    "{TRACK_INPUT_HEAD_EXPORT_SQL}
                       FROM jobs_track_policy_track_input_heads
                      WHERE account_id = ?1 ORDER BY career_track_id"
                ))?;
                let values = stmt
                    .query_map(params![account_id], export_track_input_from_sqlite_row)?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                values
            };
            let mut revision_stmt = tx.prepare(&format!(
                "{TRACK_POLICY_REVISION_EXPORT_SQL}
                  WHERE account_id = ?1 ORDER BY career_track_id, revision_no"
            ))?;
            let revision_rows = revision_stmt
                .query_map(params![account_id], export_policy_revision_from_sqlite_row)?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            drop(revision_stmt);
            let mut revisions = Vec::with_capacity(revision_rows.len());
            for (mut revision, ciphertext) in revision_rows {
                revision.canonical_policy_json = decrypt_export_policy_evidence(
                    &ciphertext,
                    &revision.canonical_policy_sha256,
                    "exported canonical Career Track policy",
                )?;
                revisions.push(revision);
            }

            let mut receipt_stmt = tx.prepare(&format!(
                "{TRACK_POLICY_RECEIPT_EXPORT_SQL}
                  WHERE account_id = ?1
                  ORDER BY career_track_id, policy_revision_no, review_receipt_id"
            ))?;
            let receipt_rows = receipt_stmt
                .query_map(params![account_id], export_policy_receipt_from_sqlite_row)?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            drop(receipt_stmt);
            let mut review_receipts = Vec::with_capacity(receipt_rows.len());
            for (mut receipt, ciphertext) in receipt_rows {
                receipt.canonical_review_receipt_json = decrypt_export_policy_evidence(
                    &ciphertext,
                    &receipt.review_receipt_sha256,
                    "exported canonical Career Track review receipt",
                )?;
                review_receipts.push(receipt);
            }

            let mut head_stmt = tx.prepare(&format!(
                "{TRACK_POLICY_HEAD_EXPORT_SQL}
                  WHERE account_id = ?1 ORDER BY career_track_id"
            ))?;
            let heads = head_stmt
                .query_map(params![account_id], export_policy_head_from_sqlite_row)?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            drop(head_stmt);
            let mut head_transition_stmt = tx.prepare(&format!(
                "{TRACK_POLICY_HEAD_TRANSITION_EXPORT_SQL}
                  WHERE account_id = ?1 ORDER BY career_track_id, head_generation"
            ))?;
            let head_transitions = head_transition_stmt
                .query_map(params![account_id], export_policy_head_from_sqlite_row)?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            drop(head_transition_stmt);
            let export = CanonicalTrackPolicyLedgerExport {
                taxonomy_activation_transitions,
                taxonomy_activation_head,
                account_input_transitions,
                account_input_head,
                track_input_transitions,
                track_input_heads,
                revisions,
                review_receipts,
                head_transitions,
                heads,
            };
            validate_track_policy_export(account_id, &export)?;
            tx.commit()?;
            Ok(export)
        }
        DbPool::Postgres(_) => {
            let mut conn = pool.get_pg()?;
            let mut tx = conn.transaction()?;
            tx.batch_execute("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")?;
            let taxonomy_activation_transitions = tx
                .query(
                    &format!(
                        "{TAXONOMY_ACTIVATION_EXPORT_SQL}
                           FROM jobs_track_policy_taxonomy_activation_events
                          ORDER BY activation_epoch"
                    ),
                    &[],
                )?
                .iter()
                .map(export_activation_from_postgres_row)
                .collect();
            let taxonomy_activation_head = tx
                .query_opt(
                    &format!(
                        "{TAXONOMY_ACTIVATION_EXPORT_SQL}
                           FROM jobs_track_policy_taxonomy_activation_head
                          WHERE singleton_id = 1"
                    ),
                    &[],
                )?
                .as_ref()
                .map(export_activation_from_postgres_row);
            let account_input_transitions = tx
                .query(
                    &format!(
                        "{ACCOUNT_INPUT_EXPORT_SQL}
                           FROM jobs_track_policy_account_input_transitions
                          WHERE account_id = $1 ORDER BY input_generation"
                    ),
                    &[&account_id],
                )?
                .iter()
                .map(export_account_input_from_postgres_row)
                .collect();
            let account_input_head = tx
                .query_opt(
                    &format!(
                        "{ACCOUNT_INPUT_HEAD_EXPORT_SQL}
                           FROM jobs_track_policy_account_input_heads
                          WHERE account_id = $1"
                    ),
                    &[&account_id],
                )?
                .as_ref()
                .map(export_account_input_from_postgres_row);
            let track_input_transitions = tx
                .query(
                    &format!(
                        "{TRACK_INPUT_EXPORT_SQL}
                           FROM jobs_track_policy_track_input_transitions
                          WHERE account_id = $1
                          ORDER BY career_track_id, input_generation"
                    ),
                    &[&account_id],
                )?
                .iter()
                .map(export_track_input_from_postgres_row)
                .collect();
            let track_input_heads = tx
                .query(
                    &format!(
                        "{TRACK_INPUT_HEAD_EXPORT_SQL}
                           FROM jobs_track_policy_track_input_heads
                          WHERE account_id = $1 ORDER BY career_track_id"
                    ),
                    &[&account_id],
                )?
                .iter()
                .map(export_track_input_from_postgres_row)
                .collect();
            let revisions = tx
                .query(
                    &format!(
                        "{TRACK_POLICY_REVISION_EXPORT_SQL}
                          WHERE account_id = $1 ORDER BY career_track_id, revision_no"
                    ),
                    &[&account_id],
                )?
                .iter()
                .map(export_policy_revision_from_postgres_row)
                .collect::<Vec<_>>()
                .into_iter()
                .map(|(mut revision, ciphertext)| {
                    revision.canonical_policy_json = decrypt_export_policy_evidence(
                        &ciphertext,
                        &revision.canonical_policy_sha256,
                        "exported canonical Career Track policy",
                    )?;
                    Ok(revision)
                })
                .collect::<Result<Vec<_>>>()?;
            let review_receipts = tx
                .query(
                    &format!(
                        "{TRACK_POLICY_RECEIPT_EXPORT_SQL}
                          WHERE account_id = $1
                          ORDER BY career_track_id, policy_revision_no, review_receipt_id"
                    ),
                    &[&account_id],
                )?
                .iter()
                .map(export_policy_receipt_from_postgres_row)
                .collect::<Vec<_>>()
                .into_iter()
                .map(|(mut receipt, ciphertext)| {
                    receipt.canonical_review_receipt_json = decrypt_export_policy_evidence(
                        &ciphertext,
                        &receipt.review_receipt_sha256,
                        "exported canonical Career Track review receipt",
                    )?;
                    Ok(receipt)
                })
                .collect::<Result<Vec<_>>>()?;
            let heads = tx
                .query(
                    &format!(
                        "{TRACK_POLICY_HEAD_EXPORT_SQL}
                          WHERE account_id = $1 ORDER BY career_track_id"
                    ),
                    &[&account_id],
                )?
                .iter()
                .map(export_policy_head_from_postgres_row)
                .collect();
            let head_transitions = tx
                .query(
                    &format!(
                        "{TRACK_POLICY_HEAD_TRANSITION_EXPORT_SQL}
                          WHERE account_id = $1
                          ORDER BY career_track_id, head_generation"
                    ),
                    &[&account_id],
                )?
                .iter()
                .map(export_policy_head_from_postgres_row)
                .collect();
            let export = CanonicalTrackPolicyLedgerExport {
                taxonomy_activation_transitions,
                taxonomy_activation_head,
                account_input_transitions,
                account_input_head,
                track_input_transitions,
                track_input_heads,
                revisions,
                review_receipts,
                head_transitions,
                heads,
            };
            validate_track_policy_export(account_id, &export)?;
            tx.commit()?;
            Ok(export)
        }
    })
}

#[cfg(test)]
mod canonical_track_policy_tests {
    use super::*;

    const LEDGER_ACCOUNT_ID: &str = "acct-track-ledger";

    fn canonical_ledger_test_pool() -> (DbPool, CareerTrack) {
        let path = std::env::temp_dir().join(format!(
            "bluey-track-ledger-test-{}-{}.sqlite3",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let pool = crate::db::open_pool(&path).expect("open ledger test pool");
        crate::db::run_migrations(&pool).expect("migrate ledger test pool");
        pool.get()
            .expect("ledger test connection")
            .execute(
                "INSERT INTO accounts (
                    id, email, password_hash, trial_seconds_remaining
                 ) VALUES (?1, 'ledger-owner@example.com', 'hash', 0)",
                params![LEDGER_ACCOUNT_ID],
            )
            .expect("insert ledger test account");
        let identity = ensure_primary_application_identity(
            &pool,
            LEDGER_ACCOUNT_ID,
            "ledger-owner@example.com",
        )
        .expect("create verified ledger identity");
        let now = now_ms();
        let asset = ResumeSourceAsset {
            id: "resume-source-ledger".to_string(),
            file_name: "private-ledger-resume.pdf".to_string(),
            media_type: "application/pdf".to_string(),
            file_type: "pdf".to_string(),
            storage_key: "jobs/acct-track-ledger/private-ledger-resume.pdf".to_string(),
            sha256: "a".repeat(64),
            size_bytes: 2_048,
            page_count: Some(2),
            template_status: "converted_layout".to_string(),
            created_at_ms: now,
            updated_at_ms: now,
        };
        let mut profile = default_profile("ledger-owner@example.com");
        profile.onboarding_complete = true;
        profile.source_resume_name = asset.file_name.clone();
        profile.source_resume_asset_id = asset.id.clone();
        profile.source_resume_sha256 = asset.sha256.clone();
        profile.source_resume_media_type = asset.media_type.clone();
        profile.source_resume_template_status = asset.template_status.clone();
        save_resume_source_asset(&pool, LEDGER_ACCOUNT_ID, &asset, &profile)
            .expect("save source resume for ledger test");
        let track = upsert_track(
            &pool,
            LEDGER_ACCOUNT_ID,
            &CareerTrack {
                id: "track-ledger".to_string(),
                name: "Software engineering".to_string(),
                role: "Software Engineer".to_string(),
                locations: vec!["New York, NY".to_string()],
                remote_preference: "hybrid_ok".to_string(),
                application_identity_id: Some(identity.id),
                policy: CareerTrackPolicy {
                    employment_types: vec!["full_time".to_string()],
                    work_authorizations: vec!["us_citizen".to_string()],
                    ..CareerTrackPolicy::default()
                },
                active: true,
                match_count: 0,
                created_at_ms: 0,
                updated_at_ms: 0,
            },
        )
        .expect("create approved ledger test Track");
        assert_eq!(track.policy.authority.review_state, "approved");
        (pool, track)
    }

    fn canonical_ledger_heads(pool: &DbPool, track_id: &str) -> (i64, i64, i64, String, String) {
        pool.get()
            .expect("ledger head connection")
            .query_row(
                "SELECT account_head.input_generation,
                        track_head.input_generation, policy_head.head_generation,
                        policy_head.policy_revision_id,
                        policy_head.review_receipt_id
                   FROM jobs_track_policy_account_input_heads AS account_head
                   JOIN jobs_track_policy_track_input_heads AS track_head
                     ON track_head.account_id = account_head.account_id
                   JOIN jobs_track_policy_heads AS policy_head
                     ON policy_head.account_id = track_head.account_id
                    AND policy_head.career_track_id = track_head.career_track_id
                  WHERE account_head.account_id = ?1
                    AND track_head.career_track_id = ?2",
                params![LEDGER_ACCOUNT_ID, track_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .expect("read canonical ledger heads")
    }

    fn assert_track_requires_review(pool: &DbPool, track_id: &str) {
        let track = list_tracks(pool, LEDGER_ACCOUNT_ID)
            .expect("list Track requiring review")
            .into_iter()
            .find(|value| value.id == track_id)
            .expect("review-required Track");
        assert_eq!(track.policy.authority.review_state, "needs_review");
    }

    fn canonical_ledger_test_posting(track_id: &str) -> JobPosting {
        JobPosting {
            id: "posting-ledger".to_string(),
            canonical_key: String::new(),
            source: "pasted_link".to_string(),
            external_id: "posting-ledger".to_string(),
            company: "Ledger Test Co".to_string(),
            title: "Software Engineer".to_string(),
            location: "New York, NY".to_string(),
            workplace: "hybrid".to_string(),
            canonical_url: "https://jobs.example.test/posting-ledger".to_string(),
            description: "Build secure systems with Rust.".to_string(),
            compensation: "$150k-$180k".to_string(),
            employment_type: "full_time".to_string(),
            track_id: track_id.to_string(),
            match_score: 90,
            matched_reasons: Vec::new(),
            missing_requirements: Vec::new(),
            posted_at_ms: Some(now_ms()),
            last_verified_at_ms: Some(now_ms()),
            availability_status: "active".to_string(),
            status: "matched".to_string(),
            created_at_ms: 0,
            updated_at_ms: 0,
            discovery_evidence: JobDiscoveryEvidence::default(),
            eligibility: None,
        }
    }

    fn remote_preference_track(value: &str) -> CareerTrack {
        CareerTrack {
            id: "track-remote-policy".to_string(),
            name: "Software engineering".to_string(),
            role: "Software Engineer".to_string(),
            locations: vec!["New York, NY".to_string()],
            remote_preference: value.to_string(),
            application_identity_id: None,
            policy: CareerTrackPolicy::default(),
            active: true,
            match_count: 0,
            created_at_ms: 0,
            updated_at_ms: 0,
        }
    }

    #[test]
    fn canonical_track_rejects_empty_or_unsupported_remote_preference() {
        for value in ["", "mostly remote", "teleport"] {
            let error = normalize_canonical_track(&remote_preference_track(value))
                .expect_err("unsupported remote preference must fail closed");
            assert!(error.to_string().contains("supported remote preference"));
        }
    }

    #[test]
    fn identical_track_and_transport_only_profile_retries_preserve_authority_heads() {
        let (pool, track) = canonical_ledger_test_pool();
        let before = canonical_ledger_heads(&pool, &track.id);

        let retried =
            upsert_track(&pool, LEDGER_ACCOUNT_ID, &track).expect("retry identical reviewed Track");
        assert_eq!(retried.policy.authority.review_state, "approved");
        assert_eq!(canonical_ledger_heads(&pool, &track.id), before);

        let mut profile =
            get_profile(&pool, LEDGER_ACCOUNT_ID, "").expect("load transport-only profile fixture");
        profile.onboarding_complete = false;
        profile.onboarding_step = 3;
        profile.updated_at_ms = profile.updated_at_ms.saturating_add(10_000);
        save_profile(&pool, LEDGER_ACCOUNT_ID, &profile)
            .expect("save incomplete transport-only profile");
        profile.onboarding_complete = true;
        profile.onboarding_step = 6;
        save_profile(&pool, LEDGER_ACCOUNT_ID, &profile)
            .expect("save completion transport-only profile");
        assert_eq!(canonical_ledger_heads(&pool, &track.id), before);
        assert_eq!(
            list_tracks(&pool, LEDGER_ACCOUNT_ID)
                .expect("read Track after transport-only profile saves")[0]
                .policy
                .authority
                .review_state,
            "approved"
        );
    }

    #[test]
    fn onboarding_order_and_retry_leave_returned_track_current_without_churn() {
        let (pool, track) = canonical_ledger_test_pool();
        let mut semantic_profile =
            get_profile(&pool, LEDGER_ACCOUNT_ID, "").expect("load onboarding profile");
        semantic_profile.headline = "Senior Software Engineer".to_string();
        semantic_profile.onboarding_step = 0;
        semantic_profile.onboarding_complete = false;
        save_profile(&pool, LEDGER_ACCOUNT_ID, &semantic_profile)
            .expect("save onboarding profile semantics first");
        let mut preferences =
            get_preferences(&pool, LEDGER_ACCOUNT_ID).expect("load onboarding preferences");
        preferences.desired_roles = vec!["Software Engineer".to_string()];
        save_preferences(&pool, LEDGER_ACCOUNT_ID, &preferences)
            .expect("save onboarding preferences");
        let reviewed = upsert_track(&pool, LEDGER_ACCOUNT_ID, &track)
            .expect("review Track after all semantic inputs");
        let before_completion = canonical_ledger_heads(&pool, &track.id);

        semantic_profile.onboarding_step = 6;
        semantic_profile.onboarding_complete = true;
        save_profile(&pool, LEDGER_ACCOUNT_ID, &semantic_profile)
            .expect("persist completion marker last");
        let workspace_track = list_tracks(&pool, LEDGER_ACCOUNT_ID)
            .expect("read current Track after onboarding completion")
            .remove(0);
        assert_eq!(workspace_track.policy.authority.review_state, "approved");
        assert_eq!(
            workspace_track.policy.authority.policy_revision_id,
            reviewed.policy.authority.policy_revision_id
        );
        assert_eq!(canonical_ledger_heads(&pool, &track.id), before_completion);

        save_profile(&pool, LEDGER_ACCOUNT_ID, &semantic_profile).expect("retry completion marker");
        save_preferences(&pool, LEDGER_ACCOUNT_ID, &preferences)
            .expect("retry onboarding preferences");
        upsert_track(&pool, LEDGER_ACCOUNT_ID, &track).expect("retry onboarding Track");
        assert_eq!(canonical_ledger_heads(&pool, &track.id), before_completion);
    }

    #[test]
    fn preferences_a_b_a_advances_generation_and_cannot_resurrect_review() {
        let (pool, track) = canonical_ledger_test_pool();
        let original = get_preferences(&pool, LEDGER_ACCOUNT_ID).expect("load preferences A");
        save_preferences(&pool, LEDGER_ACCOUNT_ID, &original).expect("persist preferences A");
        let reviewed =
            upsert_track(&pool, LEDGER_ACCOUNT_ID, &track).expect("review persisted preferences A");
        let baseline = canonical_ledger_heads(&pool, &track.id);

        let mut changed = original.clone();
        changed.desired_locations = vec!["Boston, MA".to_string()];
        save_preferences(&pool, LEDGER_ACCOUNT_ID, &changed).expect("save preferences B");
        assert_eq!(
            list_tracks(&pool, LEDGER_ACCOUNT_ID).expect("read Track after B")[0]
                .policy
                .authority
                .review_state,
            "needs_review"
        );
        save_preferences(&pool, LEDGER_ACCOUNT_ID, &original).expect("restore preferences A");
        let after_rollback = canonical_ledger_heads(&pool, &track.id);
        assert_eq!(after_rollback.0, baseline.0 + 2);
        assert_eq!(after_rollback.2, baseline.2);
        assert_eq!(
            list_tracks(&pool, LEDGER_ACCOUNT_ID).expect("read Track after A rollback")[0]
                .policy
                .authority
                .review_state,
            "needs_review"
        );

        let rereviewed =
            upsert_track(&pool, LEDGER_ACCOUNT_ID, &track).expect("review Track after A rollback");
        assert_eq!(
            rereviewed.policy.authority.policy_revision_no,
            reviewed.policy.authority.policy_revision_no + 1
        );
    }

    #[test]
    fn every_account_writer_a_b_a_advances_twice_and_stable_retry_is_a_noop() {
        let (pool, track) = canonical_ledger_test_pool();

        let original_profile = get_profile(&pool, LEDGER_ACCOUNT_ID, "").expect("load profile A");
        let profile_baseline = canonical_ledger_heads(&pool, &track.id);
        let mut changed_profile = original_profile.clone();
        changed_profile.summary = "semantic profile B".to_string();
        save_profile(&pool, LEDGER_ACCOUNT_ID, &changed_profile).expect("save profile B");
        save_profile(&pool, LEDGER_ACCOUNT_ID, &original_profile).expect("restore profile A");
        let profile_rollback = canonical_ledger_heads(&pool, &track.id);
        assert_eq!(profile_rollback.0, profile_baseline.0 + 2);
        assert_track_requires_review(&pool, &track.id);
        save_profile(&pool, LEDGER_ACCOUNT_ID, &original_profile).expect("retry profile A");
        assert_eq!(
            canonical_ledger_heads(&pool, &track.id).0,
            profile_rollback.0
        );
        upsert_track(&pool, LEDGER_ACCOUNT_ID, &track).expect("review restored profile A");

        let fact_a = upsert_fact(
            &pool,
            LEDGER_ACCOUNT_ID,
            &CareerFact {
                id: "fact-ledger-replay".to_string(),
                category: "skill".to_string(),
                label: "Primary language".to_string(),
                value: json!("Rust"),
                source: "user_entry".to_string(),
                verification_status: "confirmed".to_string(),
                confirmed_at_ms: Some(1),
                confirmed_by: Some("user".to_string()),
                schema_version: 1,
                created_at_ms: 1,
                updated_at_ms: 1,
            },
        )
        .expect("save fact A");
        upsert_track(&pool, LEDGER_ACCOUNT_ID, &track).expect("review fact A");
        let fact_baseline = canonical_ledger_heads(&pool, &track.id);
        let mut fact_b = fact_a.clone();
        fact_b.value = json!("Go");
        upsert_fact(&pool, LEDGER_ACCOUNT_ID, &fact_b).expect("save fact B");
        upsert_fact(&pool, LEDGER_ACCOUNT_ID, &fact_a).expect("restore fact A");
        let fact_rollback = canonical_ledger_heads(&pool, &track.id);
        assert_eq!(fact_rollback.0, fact_baseline.0 + 2);
        assert_track_requires_review(&pool, &track.id);
        upsert_fact(&pool, LEDGER_ACCOUNT_ID, &fact_a).expect("retry fact A");
        assert_eq!(canonical_ledger_heads(&pool, &track.id).0, fact_rollback.0);
        upsert_track(&pool, LEDGER_ACCOUNT_ID, &track).expect("review restored fact A");

        let identity_id = track
            .application_identity_id
            .as_deref()
            .expect("fixture identity id");
        let identity_a = get_application_identity(&pool, LEDGER_ACCOUNT_ID, identity_id)
            .expect("load identity A")
            .expect("fixture identity");
        let identity_baseline = canonical_ledger_heads(&pool, &track.id);
        let mut identity_b = identity_a.clone();
        identity_b.label = "Application identity B".to_string();
        save_application_identity(&pool, LEDGER_ACCOUNT_ID, &identity_b).expect("save identity B");
        save_application_identity(&pool, LEDGER_ACCOUNT_ID, &identity_a)
            .expect("restore identity A");
        let identity_rollback = canonical_ledger_heads(&pool, &track.id);
        assert_eq!(identity_rollback.0, identity_baseline.0 + 2);
        assert_track_requires_review(&pool, &track.id);
        save_application_identity(&pool, LEDGER_ACCOUNT_ID, &identity_a).expect("retry identity A");
        assert_eq!(
            canonical_ledger_heads(&pool, &track.id).0,
            identity_rollback.0
        );
        upsert_track(&pool, LEDGER_ACCOUNT_ID, &track).expect("review restored identity A");

        let asset_a = get_resume_source_asset(&pool, LEDGER_ACCOUNT_ID)
            .expect("load resume A")
            .expect("fixture resume");
        let profile_a = get_profile(&pool, LEDGER_ACCOUNT_ID, "").expect("load resume profile A");
        let resume_baseline = canonical_ledger_heads(&pool, &track.id);
        let mut asset_b = asset_a.clone();
        asset_b.sha256 = "b".repeat(64);
        let mut profile_b = profile_a.clone();
        profile_b.source_resume_sha256 = asset_b.sha256.clone();
        save_resume_source_asset(&pool, LEDGER_ACCOUNT_ID, &asset_b, &profile_b)
            .expect("save resume B");
        save_resume_source_asset(&pool, LEDGER_ACCOUNT_ID, &asset_a, &profile_a)
            .expect("restore resume A");
        let resume_rollback = canonical_ledger_heads(&pool, &track.id);
        assert_eq!(resume_rollback.0, resume_baseline.0 + 2);
        assert_track_requires_review(&pool, &track.id);
        save_resume_source_asset(&pool, LEDGER_ACCOUNT_ID, &asset_a, &profile_a)
            .expect("retry resume A");
        assert_eq!(
            canonical_ledger_heads(&pool, &track.id).0,
            resume_rollback.0
        );
    }

    #[test]
    fn reserved_looking_fact_value_keys_advance_semantics_and_revoke_old_policy() {
        for key in [
            "createdAtMs",
            "created_at_ms",
            "updatedAtMs",
            "updated_at_ms",
            "confirmedAtMs",
            "confirmed_at_ms",
        ] {
            let (pool, track) = canonical_ledger_test_pool();
            let value = |content: &str| {
                let mut object = serde_json::Map::new();
                object.insert(key.to_string(), Value::String(content.to_string()));
                Value::Object(object)
            };
            let mut fact = upsert_fact(
                &pool,
                LEDGER_ACCOUNT_ID,
                &CareerFact {
                    id: format!("fact-reserved-key-{key}"),
                    category: "achievement".to_string(),
                    label: format!("Reserved-looking fact key {key}"),
                    value: value("A"),
                    source: "user_entry".to_string(),
                    verification_status: "confirmed".to_string(),
                    confirmed_at_ms: Some(1),
                    confirmed_by: Some("user".to_string()),
                    schema_version: 1,
                    created_at_ms: 1,
                    updated_at_ms: 1,
                },
            )
            .expect("save reserved-looking fact value A");
            let reviewed = upsert_track(&pool, LEDGER_ACCOUNT_ID, &track)
                .expect("review Track against reserved-looking fact value A");
            assert_eq!(reviewed.policy.authority.review_state, "approved");
            let baseline = canonical_ledger_heads(&pool, &track.id);

            fact.value = value("B");
            upsert_fact(&pool, LEDGER_ACCOUNT_ID, &fact)
                .expect("save reserved-looking fact value B");
            let changed = canonical_ledger_heads(&pool, &track.id);
            assert_eq!(changed.0, baseline.0 + 1, "semantic generation for {key}");
            assert_eq!(changed.2, baseline.2, "policy head for {key}");
            assert_track_requires_review(&pool, &track.id);

            let conn = pool.get().expect("old policy validation connection");
            assert!(
                validate_track_policy_ledger_sqlite(&conn, LEDGER_ACCOUNT_ID, &reviewed).is_err(),
                "old reviewed policy remained authorized after {key} changed"
            );
        }
    }

    #[test]
    fn unreviewable_track_and_taxonomy_a_b_a_require_new_revisions() {
        let (pool, track) = canonical_ledger_test_pool();
        let track_baseline = canonical_ledger_heads(&pool, &track.id);
        let mut unreviewable = track.clone();
        unreviewable.role = "Intergalactic Wizard".to_string();
        let unreviewable = upsert_track(&pool, LEDGER_ACCOUNT_ID, &unreviewable)
            .expect("save unreviewable Track B");
        assert_eq!(unreviewable.policy.authority.review_state, "needs_review");
        assert_eq!(
            canonical_ledger_heads(&pool, &track.id).1,
            track_baseline.1 + 1
        );
        let restored =
            upsert_track(&pool, LEDGER_ACCOUNT_ID, &track).expect("restore and review Track A");
        let track_rollback = canonical_ledger_heads(&pool, &track.id);
        assert_eq!(track_rollback.1, track_baseline.1 + 2);
        assert_eq!(track_rollback.2, track_baseline.2 + 1);
        assert_eq!(restored.policy.authority.review_state, "approved");

        let mut conn = pool.get().expect("taxonomy rollback connection");
        let (epoch, transition, activated_at_ms): (i64, String, i64) = conn
            .query_row(
                "SELECT activation_epoch, activation_transition_sha256, activated_at_ms
                   FROM jobs_track_policy_taxonomy_activation_head
                  WHERE singleton_id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("read taxonomy A head");
        let canonicalizer_sha256 = track_policy_canonicalizer_sha256();
        let taxonomy_b_sha256 = "b".repeat(64);
        let taxonomy_b_transition = taxonomy_activation_transition_sha256(
            epoch + 1,
            epoch,
            "test-taxonomy-b",
            &taxonomy_b_sha256,
            TRACK_POLICY_CANONICALIZER_SCHEMA_VERSION,
            &canonicalizer_sha256,
            Some(&transition),
            activated_at_ms + 1,
        )
        .expect("construct taxonomy B transition");
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("start taxonomy B transition");
        tx.execute(
            "INSERT INTO jobs_track_policy_taxonomy_activation_events (
                activation_epoch, previous_activation_epoch, taxonomy_version,
                taxonomy_digest_sha256, canonicalizer_schema_version,
                canonicalizer_digest_sha256, activation_transition_sha256,
                predecessor_activation_transition_sha256, activated_at_ms
             ) VALUES (?1, ?2, 'test-taxonomy-b', ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                epoch + 1,
                epoch,
                taxonomy_b_sha256,
                TRACK_POLICY_CANONICALIZER_SCHEMA_VERSION,
                canonicalizer_sha256,
                taxonomy_b_transition,
                transition,
                activated_at_ms + 1,
            ],
        )
        .expect("insert taxonomy B transition");
        assert_eq!(
            tx.execute(
                "UPDATE jobs_track_policy_taxonomy_activation_head SET
                    activation_epoch = ?1, previous_activation_epoch = ?2,
                    taxonomy_version = 'test-taxonomy-b', taxonomy_digest_sha256 = ?3,
                    canonicalizer_schema_version = ?4,
                    canonicalizer_digest_sha256 = ?5,
                    activation_transition_sha256 = ?6,
                    predecessor_activation_transition_sha256 = ?7,
                    activated_at_ms = ?8
                  WHERE singleton_id = 1 AND activation_epoch = ?2
                    AND activation_transition_sha256 = ?7",
                params![
                    epoch + 1,
                    epoch,
                    taxonomy_b_sha256,
                    TRACK_POLICY_CANONICALIZER_SCHEMA_VERSION,
                    canonicalizer_sha256,
                    taxonomy_b_transition,
                    transition,
                    activated_at_ms + 1,
                ],
            )
            .expect("advance taxonomy head to B"),
            1
        );
        tx.commit().expect("commit taxonomy B transition");
        assert_track_requires_review(&pool, &track.id);
        activate_canonical_taxonomy_authority_sqlite(&mut conn)
            .expect("activate compiled taxonomy A after B");
        drop(conn);
        assert_track_requires_review(&pool, &track.id);
        let taxonomy_restored = upsert_track(&pool, LEDGER_ACCOUNT_ID, &track)
            .expect("review Track after taxonomy A rollback");
        assert_eq!(
            taxonomy_restored.policy.authority.policy_revision_no,
            restored.policy.authority.policy_revision_no + 1
        );
        assert!(taxonomy_restored.policy.authority.taxonomy_activation_epoch >= epoch + 2);
    }

    #[test]
    fn equal_semantic_fast_paths_reject_corrupt_transition_evidence() {
        let (account_pool, account_track) = canonical_ledger_test_pool();
        let account_conn = account_pool.get().expect("account corruption connection");
        account_conn
            .execute_batch(
                "DROP TRIGGER trg_jobs_track_policy_account_input_transitions_no_update;
                 UPDATE jobs_track_policy_account_input_transitions
                    SET changed_at_ms = changed_at_ms + 1
                  WHERE account_id = 'acct-track-ledger'
                    AND input_generation = (
                      SELECT input_generation
                        FROM jobs_track_policy_account_input_heads
                       WHERE account_id = 'acct-track-ledger'
                    );",
            )
            .expect("corrupt account input transition fixture");
        drop(account_conn);
        let profile = get_profile(&account_pool, LEDGER_ACCOUNT_ID, "")
            .expect("load equal account semantic profile");
        assert!(save_profile(&account_pool, LEDGER_ACCOUNT_ID, &profile).is_err());
        assert!(list_tracks(&account_pool, LEDGER_ACCOUNT_ID)
            .expect("list corrupt account Track")
            .iter()
            .all(|value| value.id != account_track.id
                || value.policy.authority.review_state == "needs_review"));

        let (track_pool, track) = canonical_ledger_test_pool();
        let track_conn = track_pool.get().expect("Track corruption connection");
        track_conn
            .execute_batch(
                "DROP TRIGGER trg_jobs_track_policy_track_input_transitions_no_update;
                 UPDATE jobs_track_policy_track_input_transitions
                    SET changed_at_ms = changed_at_ms + 1
                  WHERE account_id = 'acct-track-ledger'
                    AND career_track_id = 'track-ledger';",
            )
            .expect("corrupt Track input transition fixture");
        drop(track_conn);
        assert!(upsert_track(&track_pool, LEDGER_ACCOUNT_ID, &track).is_err());
    }

    #[test]
    fn equal_policy_fast_path_requires_exact_revision_receipt_and_head_event() {
        let (pool, track) = canonical_ledger_test_pool();
        let conn = pool.get().expect("policy chain corruption connection");
        let semantic_sha256: String = conn
            .query_row(
                "SELECT account_semantic_sha256
                   FROM jobs_track_policy_heads
                  WHERE account_id = ?1 AND career_track_id = ?2",
                params![LEDGER_ACCOUNT_ID, track.id],
                |row| row.get(0),
            )
            .expect("read bound account semantic digest");
        conn.execute_batch(
            "DROP TRIGGER trg_jobs_track_policy_revisions_no_update;
             DROP TRIGGER trg_jobs_track_policy_review_receipts_no_update;
             DROP TRIGGER trg_jobs_track_policy_head_transitions_no_update;",
        )
        .expect("disable immutable triggers only inside isolated tamper fixture");
        conn.execute(
            "UPDATE jobs_track_policy_revisions
                SET account_semantic_sha256 = ?3
              WHERE account_id = ?1 AND career_track_id = ?2",
            params![LEDGER_ACCOUNT_ID, track.id, "f".repeat(64)],
        )
        .expect("corrupt revision generation binding");
        drop(conn);
        assert!(upsert_track(&pool, LEDGER_ACCOUNT_ID, &track).is_err());

        let conn = pool.get().expect("restore revision corruption");
        conn.execute(
            "UPDATE jobs_track_policy_revisions
                SET account_semantic_sha256 = ?3
              WHERE account_id = ?1 AND career_track_id = ?2",
            params![LEDGER_ACCOUNT_ID, track.id, &semantic_sha256],
        )
        .expect("restore revision generation binding");
        conn.execute(
            "UPDATE jobs_track_policy_review_receipts
                SET account_semantic_sha256 = ?3
              WHERE account_id = ?1 AND career_track_id = ?2",
            params![LEDGER_ACCOUNT_ID, track.id, "e".repeat(64)],
        )
        .expect("corrupt receipt generation binding");
        drop(conn);
        assert!(upsert_track(&pool, LEDGER_ACCOUNT_ID, &track).is_err());

        let conn = pool.get().expect("restore receipt corruption");
        conn.execute(
            "UPDATE jobs_track_policy_review_receipts
                SET account_semantic_sha256 = ?3
              WHERE account_id = ?1 AND career_track_id = ?2",
            params![LEDGER_ACCOUNT_ID, track.id, &semantic_sha256],
        )
        .expect("restore receipt generation binding");
        conn.execute(
            "UPDATE jobs_track_policy_head_transitions
                SET updated_by = 'corrupt-reviewer'
              WHERE account_id = ?1 AND career_track_id = ?2",
            params![LEDGER_ACCOUNT_ID, track.id],
        )
        .expect("corrupt immutable head-transition binding");
        drop(conn);
        assert!(upsert_track(&pool, LEDGER_ACCOUNT_ID, &track).is_err());
    }

    #[test]
    fn idempotent_taxonomy_activation_rejects_corrupt_event_evidence() {
        let (pool, _) = canonical_ledger_test_pool();
        let conn = pool.get().expect("activation corruption connection");
        conn.execute_batch(
            "DROP TRIGGER trg_jobs_track_policy_taxonomy_activation_events_no_update;
             UPDATE jobs_track_policy_taxonomy_activation_events
                SET activated_at_ms = activated_at_ms + 1
              WHERE activation_epoch = (
                SELECT activation_epoch
                  FROM jobs_track_policy_taxonomy_activation_head
                 WHERE singleton_id = 1
              );",
        )
        .expect("corrupt activation event fixture");
        drop(conn);
        assert!(crate::db::run_migrations(&pool).is_err());
    }

    #[test]
    fn canonical_policy_ledger_encrypts_private_evidence_and_exports_owner_scope() {
        let (pool, track) = canonical_ledger_test_pool();
        let conn = pool.get().expect("ledger test connection");
        let (policy_ciphertext, policy_sha256): (String, String) = conn
            .query_row(
                "SELECT canonical_policy_ciphertext, canonical_policy_sha256
                   FROM jobs_track_policy_revisions
                  WHERE account_id = ?1 AND career_track_id = ?2",
                params![LEDGER_ACCOUNT_ID, track.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("read encrypted policy evidence");
        let (receipt_ciphertext, receipt_sha256): (String, String) = conn
            .query_row(
                "SELECT canonical_review_receipt_ciphertext, review_receipt_sha256
                   FROM jobs_track_policy_review_receipts
                  WHERE account_id = ?1 AND career_track_id = ?2",
                params![LEDGER_ACCOUNT_ID, track.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("read encrypted review receipt");
        drop(conn);

        for ciphertext in [&policy_ciphertext, &receipt_ciphertext] {
            assert!(ciphertext.starts_with(ENCRYPTED_PAYLOAD_PREFIX));
            assert!(!ciphertext.contains("New York"));
            assert!(!ciphertext.contains("ledger-owner@example.com"));
            assert!(!ciphertext.contains("us_citizen"));
        }
        let policy_plaintext = decrypt_payload(&policy_ciphertext).expect("decrypt policy");
        let receipt_plaintext = decrypt_payload(&receipt_ciphertext).expect("decrypt receipt");
        assert_eq!(
            track_policy_sha256(policy_plaintext.as_bytes()),
            policy_sha256
        );
        assert_eq!(
            track_policy_sha256(receipt_plaintext.as_bytes()),
            receipt_sha256
        );
        assert!(policy_plaintext.contains("New York, NY"));
        assert!(!policy_plaintext.contains("ledger-owner@example.com"));
        assert!(policy_plaintext.contains("us_citizen"));

        let export = export_canonical_track_policy_ledger(&pool, LEDGER_ACCOUNT_ID)
            .expect("export owner-scoped policy ledger");
        assert_eq!(export.revisions.len(), 1);
        assert_eq!(export.review_receipts.len(), 1);
        assert_eq!(export.heads.len(), 1);
        assert_eq!(export.revisions[0].canonical_policy_json, policy_plaintext);
        assert_eq!(
            export.review_receipts[0].canonical_review_receipt_json,
            receipt_plaintext
        );

        pool.get()
            .expect("ledger test connection")
            .execute(
                "INSERT INTO accounts (
                    id, email, password_hash, trial_seconds_remaining
                 ) VALUES ('acct-track-ledger-other', 'other@example.com', 'hash', 0)",
                [],
            )
            .expect("insert second account");
        let other = export_canonical_track_policy_ledger(&pool, "acct-track-ledger-other")
            .expect("export second account ledger");
        assert!(other.revisions.is_empty());
        assert!(other.review_receipts.is_empty());
        assert!(other.heads.is_empty());

        let mut copied_projection = track;
        copied_projection.id = "track-ledger-cross-account-copy".to_string();
        let copied_payload = to_json(&copied_projection, "cross-account Track copy")
            .expect("encrypt cross-account Track copy");
        pool.get()
            .expect("ledger test connection")
            .execute(
                "INSERT INTO jobs_tracks (
                    id, account_id, track_json, active, created_at_ms, updated_at_ms
                 ) VALUES (?1, 'acct-track-ledger-other', ?2, 1, ?3, ?3)",
                params![copied_projection.id, copied_payload, now_ms()],
            )
            .expect("insert copied mutable Track projection");
        let copied = list_tracks(&pool, "acct-track-ledger-other")
            .expect("list cross-account copied Track")
            .remove(0);
        assert_eq!(copied.policy.authority.review_state, "needs_review");
        assert_eq!(
            copied.policy.authority.review_reason_codes,
            vec!["policy_ledger_review_required"]
        );
    }

    #[test]
    fn owner_export_rejects_orphan_revision_without_receipt_and_head_transition() {
        let (pool, track) = canonical_ledger_test_pool();
        pool.get()
            .expect("orphan revision connection")
            .execute(
                "INSERT INTO jobs_track_policy_revisions (
                    revision_id, account_id, career_track_id, revision_no,
                    taxonomy_version, taxonomy_digest_sha256,
                    canonical_policy_sha256, canonical_policy_ciphertext,
                    canonical_role_id, canonical_role_family,
                    verified_application_identity_id,
                    verified_application_identity_sha256,
                    source_resume_asset_id, source_resume_sha256,
                    job_preferences_sha256, taxonomy_activation_epoch,
                    canonicalizer_schema_version, canonicalizer_digest_sha256,
                    account_input_generation, account_input_transition_sha256,
                    account_semantic_sha256, track_input_generation,
                    track_input_transition_sha256, track_semantic_sha256,
                    predecessor_revision_id, predecessor_revision_no,
                    predecessor_policy_sha256, compatibility_classification,
                    review_state, created_by, created_at_ms
                 )
                 SELECT ?3, account_id, career_track_id, revision_no + 1,
                        taxonomy_version, taxonomy_digest_sha256,
                        canonical_policy_sha256, canonical_policy_ciphertext,
                        canonical_role_id, canonical_role_family,
                        verified_application_identity_id,
                        verified_application_identity_sha256,
                        source_resume_asset_id, source_resume_sha256,
                        job_preferences_sha256, taxonomy_activation_epoch,
                        canonicalizer_schema_version, canonicalizer_digest_sha256,
                        account_input_generation, account_input_transition_sha256,
                        account_semantic_sha256, track_input_generation,
                        track_input_transition_sha256, track_semantic_sha256,
                        revision_id, revision_no, canonical_policy_sha256,
                        'review_required', review_state, created_by, created_at_ms + 1
                   FROM jobs_track_policy_revisions
                  WHERE account_id = ?1 AND career_track_id = ?2 AND revision_no = 1",
                params![
                    LEDGER_ACCOUNT_ID,
                    track.id,
                    "track-policy-orphan-revision-0002"
                ],
            )
            .expect("insert schema-valid orphan policy revision");

        let error = export_canonical_track_policy_ledger(&pool, LEDGER_ACCOUNT_ID)
            .expect_err("owner export must reject an orphan immutable revision");
        assert!(error
            .to_string()
            .contains("missing exact receipt/head-transition authority"));
    }

    #[test]
    fn mutable_track_policy_drift_fails_list_eligibility_and_execution_closed() {
        let (pool, track) = canonical_ledger_test_pool();
        let profile = get_profile(&pool, LEDGER_ACCOUNT_ID, "").expect("load ledger profile");
        let preferences = get_preferences(&pool, LEDGER_ACCOUNT_ID).expect("load preferences");
        let posting = upsert_posting(
            &pool,
            LEDGER_ACCOUNT_ID,
            &canonical_ledger_test_posting(&track.id),
            &profile,
            &preferences,
        )
        .expect("store ledger test posting");

        let conn = pool.get().expect("ledger test connection");
        let track_raw: String = conn
            .query_row(
                "SELECT track_json FROM jobs_tracks
                  WHERE account_id = ?1 AND id = ?2",
                params![LEDGER_ACCOUNT_ID, track.id],
                |row| row.get(0),
            )
            .expect("read mutable Track projection");
        let mut drifted: CareerTrack = parse_json(track_raw, "drifted Career Track")
            .expect("decrypt mutable Track projection");
        drifted.locations = vec!["San Francisco, CA".to_string()];
        let drifted_raw =
            to_json(&drifted, "drifted Career Track").expect("encrypt drifted Track projection");
        conn.execute(
            "UPDATE jobs_tracks SET track_json = ?3
              WHERE account_id = ?1 AND id = ?2",
            params![LEDGER_ACCOUNT_ID, track.id, drifted_raw],
        )
        .expect("tamper mutable Track projection");
        drop(conn);

        let projected = list_tracks(&pool, LEDGER_ACCOUNT_ID)
            .expect("list drifted Track")
            .remove(0);
        assert_eq!(projected.policy.authority.review_state, "needs_review");
        assert_eq!(
            projected.policy.authority.review_reason_codes,
            vec!["policy_ledger_review_required"]
        );
        let eligibility = evaluate_job_eligibility(&pool, LEDGER_ACCOUNT_ID, &posting, true, None)
            .expect("evaluate drifted Track");
        assert!(!eligibility.can_queue_local);
        assert!(!eligibility.can_queue_cloud);
        assert!(!eligibility.can_auto_submit);
        assert!(eligibility
            .review_reasons
            .iter()
            .any(|reason| reason.code == "career_track_policy_review_required"));

        let application = JobApplication {
            id: "application-ledger".to_string(),
            job_id: posting.id,
            resume_version_id: None,
            state: "ready".to_string(),
            submission_mode: "review_first".to_string(),
            match_score: 90,
            answers: Vec::new(),
            cover_letter: String::new(),
            receipt: Value::Null,
            run_id: None,
            created_at_ms: now_ms(),
            updated_at_ms: now_ms(),
            submitted_at_ms: None,
        };
        let mut conn = pool.get().expect("ledger test connection");
        let tx = conn
            .transaction()
            .expect("start execution test transaction");
        assert!(!current_execution_authorized_sqlite(
            &tx,
            LEDGER_ACCOUNT_ID,
            &application,
            ExecutionAuthorityRunner::Local,
        )
        .expect("check drifted execution authority"));
    }

    #[test]
    fn corrupt_policy_ciphertext_fails_list_and_export_closed() {
        let (pool, track) = canonical_ledger_test_pool();
        let conn = pool.get().expect("ledger test connection");
        conn.execute_batch("DROP TRIGGER trg_jobs_track_policy_revisions_no_update")
            .expect("disable immutable trigger only inside isolated tamper fixture");
        let corrupt = format!("{ENCRYPTED_PAYLOAD_PREFIX}{}", "A".repeat(40));
        conn.execute(
            "UPDATE jobs_track_policy_revisions
                SET canonical_policy_ciphertext = ?3
              WHERE account_id = ?1 AND career_track_id = ?2",
            params![LEDGER_ACCOUNT_ID, track.id, corrupt],
        )
        .expect("inject authenticated-ciphertext corruption fixture");
        drop(conn);

        let projected = list_tracks(&pool, LEDGER_ACCOUNT_ID)
            .expect("list Track with corrupt ledger")
            .remove(0);
        assert_eq!(projected.policy.authority.review_state, "needs_review");
        assert!(export_canonical_track_policy_ledger(&pool, LEDGER_ACCOUNT_ID).is_err());
    }
}
