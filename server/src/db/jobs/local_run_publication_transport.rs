// Pure transport checks only. The publication transaction must independently
// establish current ownership, reservation, entitlement and execution authority.
pub struct PublishLocalApplicationRun {
    pub account_id: String,
    pub expected_application: JobApplication,
    pub run_id: String,
    pub workflow_input: Value,
    pub browser_session: BrowserSession,
    pub ticket_hash: String,
    pub ticket_secret: String,
    pub expires_at_ms: i64,
}

pub struct PublishedLocalApplicationRun {
    pub application: JobApplication,
    pub browser_session: BrowserSession,
    pub ticket: LocalRunTicket,
}

#[derive(Debug, Error)]
pub enum LocalRunPublicationError {
    #[error("Invalid local run publication request")]
    InvalidRequest,
    #[error("Local run publication target not found")]
    Missing,
    #[error("Local run publication conflicts with current state")]
    Conflict,
}

fn validate_local_publication_transport(
    account_id: &str,
    application: &JobApplication,
    posting: &JobPosting,
    run_id: &str,
    session: &BrowserSession,
    payload: &Value,
) -> Result<()> {
    let conflict = || LocalRunPublicationError::Conflict;
    if !valid_approved_submission_identifier(account_id)
        || !valid_approved_submission_identifier(run_id)
        || posting.id != application.job_id
        || posting.canonical_key.is_empty()
        || session.id != run_id
        || session.runner != "local"
        || session.application_id.as_deref() != Some(application.id.as_str())
    {
        return Err(conflict().into());
    }
    let snapshot = approved_submission_snapshot(account_id, application).map_err(|_| conflict())?;
    let approved = application.receipt["approved_execution"]
        .as_object()
        .ok_or_else(conflict)?;
    let schema = approved
        .get("schema_version")
        .and_then(Value::as_i64)
        .ok_or_else(conflict)?;
    let identity = application
        .receipt
        .pointer("/application_identity/id")
        .and_then(Value::as_str)
        .ok_or_else(conflict)?;
    let resume = application
        .resume_version_id
        .as_deref()
        .ok_or_else(conflict)?;
    let destination = snapshot
        .job
        .get("canonicalUrl")
        .and_then(Value::as_str)
        .filter(|url| !url.is_empty() && *url == posting.canonical_url)
        .ok_or_else(conflict)?;
    let mut packet = snapshot.packet.clone();
    let fields = packet.as_object_mut().ok_or_else(conflict)?;
    for reserved in [
        "executionAuthority",
        "approvedPacketChecksum",
        "approvedExecutionSchemaVersion",
        "approvedExecutionAdmission",
    ] {
        if fields.contains_key(reserved) {
            return Err(conflict().into());
        }
    }
    fields.insert("approvedPacketChecksum".into(), json!(snapshot.checksum));
    fields.insert("approvedExecutionSchemaVersion".into(), json!(schema));
    if matches!(schema, 2 | 3) {
        fields.insert(
            "approvedExecutionAdmission".into(),
            approved.get("admission").cloned().ok_or_else(conflict)?,
        );
    }
    let expected = json!({
        "accountId": account_id, "applicationId": application.id,
        "jobId": posting.id, "canonicalJobKey": posting.canonical_key,
        "packetId": resume, "applicationIdentityId": identity,
        "browserProfileId": execution_browser_profile_id(account_id, identity),
        "packet": packet, "job": snapshot.job, "runner": "local",
        "url": destination, "idempotencyKey": run_id, "runId": run_id,
        "browserSessionId": session.id,
    });
    if payload != &expected {
        return Err(conflict().into());
    }
    Ok(())
}

#[cfg(test)]
mod local_run_publication_transport_tests {
    use super::*;

    fn fixture(schema: i64) -> (JobApplication, JobPosting, BrowserSession, Value) {
        let profile = execution_browser_profile_id("account", "identity");
        let packet = json!({"applicationId":"app", "jobId":"job", "resumeVersionId":"resume",
            "applicationIdentityId":"identity", "applicationEmail":"synthetic@example.test",
            "browserProfileId":profile, "answers":{}, "verifiedClaimIds":[]});
        let job = json!({"canonicalUrl":"https://example.test/job"});
        let admission = json!({"kind":"review_approval"});
        let checksum = approved_submission_checksum(
            schema,
            &packet,
            &job,
            (schema != 1).then_some(&admission),
        )
        .unwrap();
        let mut approved = json!({"schema_version":schema, "packet":packet,
            "job":job, "checksum":checksum});
        if schema != 1 {
            approved["admission"] = admission.clone();
        }
        let application: JobApplication =
            serde_json::from_value(json!({"id":"app", "job_id":"job",
            "resume_version_id":"resume", "submission_mode":"review_first",
            "receipt":{"application_identity":{"id":"identity", "email":"synthetic@example.test"},
                "approved_execution":approved}}))
            .unwrap();
        let posting: JobPosting = serde_json::from_value(json!({"id":"job", "canonical_key":"key",
            "company":"Synthetic", "title":"Role", "canonical_url":"https://example.test/job"}))
        .unwrap();
        let session: BrowserSession = serde_json::from_value(json!({"id":"run", "runner":"local",
            "application_id":"app"}))
        .unwrap();
        let mut transported = packet;
        transported["approvedPacketChecksum"] = json!(checksum);
        transported["approvedExecutionSchemaVersion"] = json!(schema);
        if schema != 1 {
            transported["approvedExecutionAdmission"] = admission;
        }
        let payload = json!({"accountId":"account", "applicationId":"app", "jobId":"job",
            "canonicalJobKey":"key", "packetId":"resume", "applicationIdentityId":"identity",
            "browserProfileId":profile, "packet":transported, "job":job, "runner":"local",
            "url":"https://example.test/job", "idempotencyKey":"run", "runId":"run",
            "browserSessionId":"run"});
        (application, posting, session, payload)
    }

    #[test]
    fn local_transport_accepts_exact_reviewed_snapshots() {
        for schema in [1, 2, 3] {
            let (app, job, session, payload) = fixture(schema);
            assert!(validate_local_publication_transport(
                "account", &app, &job, "run", &session, &payload
            )
            .is_ok());
        }
    }

    #[test]
    fn local_transport_rejects_each_modified_or_extra_field() {
        let (app, job, session, payload) = fixture(2);
        for pointer in [
            "/accountId",
            "/applicationId",
            "/jobId",
            "/canonicalJobKey",
            "/packetId",
            "/applicationIdentityId",
            "/browserProfileId",
            "/packet/answers",
            "/job/canonicalUrl",
            "/packet/approvedPacketChecksum",
            "/packet/approvedExecutionSchemaVersion",
            "/packet/approvedExecutionAdmission",
            "/runner",
            "/url",
            "/idempotencyKey",
            "/runId",
            "/browserSessionId",
        ] {
            let mut altered = payload.clone();
            *altered.pointer_mut(pointer).unwrap() = json!("changed");
            assert!(
                validate_local_publication_transport(
                    "account", &app, &job, "run", &session, &altered
                )
                .is_err(),
                "{pointer}"
            );
        }
        for pointer in ["extra", "executionAuthority"] {
            let mut altered = payload.clone();
            if pointer == "extra" {
                altered[pointer] = json!(true);
            } else {
                altered["packet"][pointer] = json!({});
            }
            assert!(validate_local_publication_transport(
                "account", &app, &job, "run", &session, &altered
            )
            .is_err());
        }
    }

    #[test]
    fn local_transport_rejects_legacy_session_and_changed_approved_snapshot() {
        let (mut app, job, mut session, payload) = fixture(1);
        session.id = "local-app".into();
        assert!(validate_local_publication_transport(
            "account", &app, &job, "run", &session, &payload
        )
        .is_err());
        session.id = "run".into();
        app.receipt["approved_execution"]["packet"]["answers"] = json!({"changed":"yes"});
        assert!(validate_local_publication_transport(
            "account", &app, &job, "run", &session, &payload
        )
        .is_err());
    }
}
