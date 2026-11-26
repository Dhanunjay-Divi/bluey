// Reservation identity is additive to the display-name guard. Employer IDs are
// in this database's pinned integrity trust chain, not a Track/email/policy
// generation namespace. A policy or signature rotation cannot reset history.

#[derive(Debug, Clone, PartialEq, Eq)]
struct AttemptEmployerBinding {
    canonical_employer_id: String,
    canonical_employer_domain: String,
    employer_attestation_sha256: String,
}

impl AttemptEmployerBinding {
    fn valid(&self) -> bool {
        job_integrity_token(&self.canonical_employer_id, 1, 240)
            && job_integrity_domain(&self.canonical_employer_domain)
            && job_integrity_hex64(&self.employer_attestation_sha256)
    }

    fn same_employer(&self, other: &Self) -> bool {
        self.canonical_employer_id == other.canonical_employer_id
            && self.canonical_employer_domain == other.canonical_employer_domain
    }
}

/// This reads a frozen receipt, not an independent source of trust. Admission
/// callers must first verify its exact equality with current signed authority
/// under their existing publication/account locks. Never derive it from a name.
fn frozen_attempt_employer_binding(application: &JobApplication) -> Result<AttemptEmployerBinding> {
    let receipt = application_job_integrity_receipt(application)?
        .ok_or_else(|| attempt_review_required("attempt_employer_identity_review_required"))?;
    let binding = AttemptEmployerBinding {
        canonical_employer_id: receipt.canonical_employer_id,
        canonical_employer_domain: receipt.canonical_employer_domain,
        employer_attestation_sha256: receipt.attestation_sha256,
    };
    if !binding.valid() {
        return Err(attempt_review_required(
            "attempt_employer_identity_review_required",
        ));
    }
    Ok(binding)
}

fn attempt_reservation_employer_binding(
    reservation: &AttemptReservation,
) -> Option<AttemptEmployerBinding> {
    let binding = AttemptEmployerBinding {
        canonical_employer_id: reservation.canonical_employer_id.clone()?,
        canonical_employer_domain: reservation.canonical_employer_domain.clone()?,
        employer_attestation_sha256: reservation.employer_attestation_sha256.clone()?,
    };
    binding.valid().then_some(binding)
}

fn posting_canonical_employer_identity(posting: &JobPosting) -> Option<(&str, &str)> {
    let evidence = &posting.discovery_evidence;
    if evidence.employer_verification_status != "verified" {
        return None;
    }
    let id = evidence.employer_id.as_deref()?;
    let domain = evidence.canonical_employer_domain.as_deref()?;
    (job_integrity_token(id, 1, 240) && job_integrity_domain(domain)).then_some((id, domain))
}

fn attempt_reservation_from_sqlite_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<AttemptReservation> {
    Ok(AttemptReservation {
        id: row.get(0)?,
        application_id: row.get(1)?,
        company_key: row.get(2)?,
        period_key: row.get(3)?,
        runner: row.get(4)?,
        status: row.get(5)?,
        reserved_at_ms: row.get(6)?,
        updated_at_ms: row.get(7)?,
        canonical_employer_id: row.get(8)?,
        canonical_employer_domain: row.get(9)?,
        employer_attestation_sha256: row.get(10)?,
    })
}

fn attempt_reservation_from_postgres_row(row: postgres::Row) -> AttemptReservation {
    AttemptReservation {
        id: row.get(0),
        application_id: row.get(1),
        company_key: row.get(2),
        period_key: row.get(3),
        runner: row.get(4),
        status: row.get(5),
        reserved_at_ms: row.get(6),
        updated_at_ms: row.get(7),
        canonical_employer_id: row.get(8),
        canonical_employer_domain: row.get(9),
        employer_attestation_sha256: row.get(10),
    }
}

fn require_attempt_employer_admission(
    own_application_id: &str,
    company_key: &str,
    expected: &AttemptEmployerBinding,
    relevant: &[AttemptReservation],
) -> Result<()> {
    for reservation in relevant {
        let binding = attempt_reservation_employer_binding(reservation);
        if reservation.application_id == own_application_id {
            if active_attempt_status(&reservation.status) {
                if binding.as_ref() != Some(expected) {
                    return Err(attempt_review_required(
                        "attempt_employer_identity_review_required",
                    ));
                }
            } else if reservation.status != "released"
                || binding
                    .as_ref()
                    .is_some_and(|binding| !binding.same_employer(expected))
            {
                return Err(attempt_review_required(
                    "attempt_employer_identity_review_required",
                ));
            }
            continue;
        }
        if !active_attempt_status(&reservation.status) {
            continue;
        }
        if reservation.company_key == company_key
            || binding
                .as_ref()
                .is_some_and(|binding| binding.same_employer(expected))
        {
            return Err(attempt_blocked("company_application_exists"));
        }
        let Some(binding) = binding else {
            return Err(attempt_review_required(
                "legacy_company_identity_review_required",
            ));
        };
        if binding.canonical_employer_id == expected.canonical_employer_id
            || binding.canonical_employer_domain == expected.canonical_employer_domain
        {
            return Err(attempt_review_required(
                "canonical_employer_mapping_conflict",
            ));
        }
    }
    Ok(())
}

fn require_attempt_employer_admission_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application: &JobApplication,
    posting: &JobPosting,
    binding: &AttemptEmployerBinding,
) -> Result<()> {
    let company_key = normalize_company_key(&posting.company);
    let mut statement = tx.prepare(
        "SELECT id,application_id,company_key,period_key,runner,status,
                reserved_at_ms,updated_at_ms,canonical_employer_id,
                canonical_employer_domain,employer_attestation_sha256
           FROM jobs_attempt_reservations
          WHERE account_id=?1 AND (application_id=?2 OR (
                status IN ('reserved','running','side_effect_unknown','submitted') AND (
                    company_key=?3 OR canonical_employer_id=?4 OR canonical_employer_domain=?5
                    OR canonical_employer_id IS NULL OR canonical_employer_domain IS NULL
                    OR employer_attestation_sha256 IS NULL)))
          ORDER BY (application_id=?2) DESC,application_id LIMIT 2",
    )?;
    let relevant = statement
        .query_map(
            params![
                account_id,
                application.id,
                company_key,
                binding.canonical_employer_id,
                binding.canonical_employer_domain
            ],
            attempt_reservation_from_sqlite_row,
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    require_attempt_employer_admission(&application.id, &company_key, binding, &relevant)
}

fn require_attempt_employer_admission_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application: &JobApplication,
    posting: &JobPosting,
    binding: &AttemptEmployerBinding,
) -> Result<()> {
    // Existing account/entitlement serialization owns write order. These reads
    // introduce no new cross-application locks; unique indexes are the last fence.
    let company_key = normalize_company_key(&posting.company);
    let relevant = tx
        .query(
            "SELECT id,application_id,company_key,period_key,runner,status,
                reserved_at_ms,updated_at_ms,canonical_employer_id,
                canonical_employer_domain,employer_attestation_sha256
           FROM jobs_attempt_reservations
          WHERE account_id=$1 AND (application_id=$2 OR (
                status IN ('reserved','running','side_effect_unknown','submitted') AND (
                    company_key=$3 OR canonical_employer_id=$4 OR canonical_employer_domain=$5
                    OR canonical_employer_id IS NULL OR canonical_employer_domain IS NULL
                    OR employer_attestation_sha256 IS NULL)))
          ORDER BY (application_id=$2) DESC,application_id LIMIT 2",
            &[
                &account_id,
                &application.id,
                &company_key,
                &binding.canonical_employer_id,
                &binding.canonical_employer_domain,
            ],
        )?
        .into_iter()
        .map(attempt_reservation_from_postgres_row)
        .collect::<Vec<_>>();
    require_attempt_employer_admission(&application.id, &company_key, binding, &relevant)
}

fn own_attempt_employer_allows_current_authority(
    application: &JobApplication,
    reservations: &[AttemptReservation],
) -> Result<bool> {
    // Admission resolves current authority before inserting its first attempt.
    // Execution paths separately require the existing owned reservation/lease;
    // whenever present, its binding must match, including exact active replay.
    let Some(own) = reservations
        .iter()
        .find(|row| row.application_id == application.id)
    else {
        return Ok(true);
    };
    let binding = attempt_reservation_employer_binding(own);
    if own.status == "released" && binding.is_none() {
        return Ok(true);
    }
    let expected = frozen_attempt_employer_binding(application)?;
    if active_attempt_status(&own.status) {
        return Ok(binding.as_ref() == Some(&expected));
    }
    Ok(own.status == "released"
        && binding
            .as_ref()
            .is_none_or(|binding| binding.same_employer(&expected)))
}

fn require_existing_attempt_employer_binding(
    application: &JobApplication,
    reservation: &AttemptReservation,
) -> Result<()> {
    let expected = frozen_attempt_employer_binding(application)?;
    if attempt_reservation_employer_binding(reservation).as_ref() != Some(&expected) {
        return Err(attempt_review_required(
            "attempt_employer_identity_review_required",
        ));
    }
    Ok(())
}

fn require_attempt_reservation_transition(previous: &str, next: &str) -> Result<()> {
    if previous == "submitted" && next != "submitted" {
        anyhow::bail!("a submitted application attempt is terminal")
    }
    if previous == "side_effect_unknown"
        && !matches!(next, "side_effect_unknown" | "submitted" | "released")
    {
        anyhow::bail!("an unknown application attempt requires reconciliation before execution")
    }
    Ok(())
}

fn require_complete_terminal_attempt_history_sqlite(
    tx: &rusqlite::Transaction<'_>,
    account_id: &str,
    application_id: &str,
) -> Result<()> {
    // Old tracking/import history may predate reservation authority. Never
    // assume an absent/released slot proves that a recorded submit did not occur.
    let unresolved: bool = tx.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM jobs_applications application
            WHERE application.account_id=?1 AND application.id<>?2
              AND application.state IN ('submitted','side_effect_unknown')
              AND NOT EXISTS (
                SELECT 1 FROM jobs_attempt_reservations attempt
                 WHERE attempt.account_id=application.account_id
                   AND attempt.application_id=application.id
                   AND attempt.status IN ('reserved','running','side_effect_unknown','submitted')))",
        params![account_id, application_id],
        |row| row.get(0),
    )?;
    if unresolved {
        return Err(attempt_review_required(
            "legacy_company_identity_review_required",
        ));
    }
    Ok(())
}

fn require_complete_terminal_attempt_history_postgres(
    tx: &mut postgres::Transaction<'_>,
    account_id: &str,
    application_id: &str,
) -> Result<()> {
    let unresolved: bool = tx.query_one(
        "SELECT EXISTS(
           SELECT 1 FROM jobs_applications application
            WHERE application.account_id=$1 AND application.id<>$2
              AND application.state IN ('submitted','side_effect_unknown')
              AND NOT EXISTS (
                SELECT 1 FROM jobs_attempt_reservations attempt
                 WHERE attempt.account_id=application.account_id
                   AND attempt.application_id=application.id
                   AND attempt.status IN ('reserved','running','side_effect_unknown','submitted')))",
        &[&account_id, &application_id],
    )?.get(0);
    if unresolved {
        return Err(attempt_review_required(
            "legacy_company_identity_review_required",
        ));
    }
    Ok(())
}

fn require_complete_terminal_attempt_history(
    pool: &DbPool,
    account_id: &str,
    application_id: &str,
) -> Result<()> {
    crate::db::run_blocking_db(|| match pool {
        DbPool::Sqlite(_) => {
            let mut connection = pool.get()?;
            let tx = connection.transaction()?;
            require_complete_terminal_attempt_history_sqlite(&tx, account_id, application_id)?;
            tx.commit()?;
            Ok(())
        }
        DbPool::Postgres(_) => {
            let mut connection = pool.get_pg()?;
            let mut tx = connection.transaction()?;
            require_complete_terminal_attempt_history_postgres(
                &mut tx,
                account_id,
                application_id,
            )?;
            tx.commit()?;
            Ok(())
        }
    })
}

#[cfg(test)]
mod attempt_employer_identity_tests {
    use super::*;

    fn employer(id: &str, domain: &str, sha: &str) -> AttemptEmployerBinding {
        AttemptEmployerBinding {
            canonical_employer_id: id.to_string(),
            canonical_employer_domain: domain.to_string(),
            employer_attestation_sha256: sha.repeat(64),
        }
    }

    fn reservation(status: &str, binding: Option<&AttemptEmployerBinding>) -> AttemptReservation {
        AttemptReservation {
            id: "attempt-first".to_string(),
            application_id: "first".to_string(),
            company_key: "acme".to_string(),
            period_key: "2026-10-28".to_string(),
            runner: "local".to_string(),
            status: status.to_string(),
            reserved_at_ms: 1,
            updated_at_ms: 1,
            canonical_employer_id: binding.map(|value| value.canonical_employer_id.clone()),
            canonical_employer_domain: binding.map(|value| value.canonical_employer_domain.clone()),
            employer_attestation_sha256: binding
                .map(|value| value.employer_attestation_sha256.clone()),
        }
    }

    fn review_code(error: anyhow::Error, expected: &str) {
        assert!(
            matches!(
                error.downcast_ref::<ApplicationAttemptAdmissionError>(),
                Some(ApplicationAttemptAdmissionError::ReviewRequired { reason_code })
                    if reason_code == expected
            ),
            "expected specific review code {expected}: {error}"
        );
    }

    #[test]
    fn canonical_alias_and_policy_rotation_do_not_create_a_second_employer_namespace() {
        let first = employer("employer-acme", "acme.example", "a");
        for sha in ["a", "b"] {
            let current = employer("employer-acme", "acme.example", sha);
            let error = require_attempt_employer_admission(
                "second",
                "acmetechnologies",
                &current,
                &[reservation("reserved", Some(&first))],
            )
            .unwrap_err();
            assert!(matches!(
                error.downcast_ref::<ApplicationAttemptAdmissionError>(),
                Some(ApplicationAttemptAdmissionError::Blocked { reason_code })
                    if reason_code == "company_application_exists"
            ));
        }
    }

    #[test]
    fn distinct_employers_remain_clear_but_display_guard_is_preserved() {
        let first = employer("employer-acme", "acme.example", "a");
        let other = employer("employer-other", "other.example", "b");
        let rows = [reservation("running", Some(&first))];
        require_attempt_employer_admission("second", "othercompany", &other, &rows).unwrap();
        let error =
            require_attempt_employer_admission("second", "acme", &other, &rows).unwrap_err();
        assert!(matches!(
            error.downcast_ref::<ApplicationAttemptAdmissionError>(),
            Some(ApplicationAttemptAdmissionError::Blocked { reason_code })
                if reason_code == "company_application_exists"
        ));
    }

    #[test]
    fn conflicting_id_or_domain_mappings_require_review() {
        let first = employer("employer-acme", "acme.example", "a");
        for conflict in [
            employer("employer-acme", "other.example", "b"),
            employer("employer-other", "acme.example", "b"),
        ] {
            review_code(
                require_attempt_employer_admission(
                    "second",
                    "otherdisplay",
                    &conflict,
                    &[reservation("reserved", Some(&first))],
                )
                .unwrap_err(),
                "canonical_employer_mapping_conflict",
            );
        }
    }

    #[test]
    fn every_active_legacy_binding_requires_review_without_name_fallback() {
        let current = employer("employer-new", "new.example", "a");
        for status in ["reserved", "running", "side_effect_unknown", "submitted"] {
            review_code(
                require_attempt_employer_admission(
                    "second",
                    "differentdisplay",
                    &current,
                    &[reservation(status, None)],
                )
                .unwrap_err(),
                "legacy_company_identity_review_required",
            );
        }
    }

    #[test]
    fn released_peer_history_does_not_consume_an_active_employer_slot() {
        let current = employer("employer-acme", "acme.example", "a");
        for binding in [None, Some(&current)] {
            require_attempt_employer_admission(
                "second",
                "acme",
                &current,
                &[reservation("released", binding)],
            )
            .unwrap();
        }
    }

    #[test]
    fn exact_active_replay_and_released_renewal_have_different_binding_rules() {
        let first = employer("employer-acme", "acme.example", "a");
        let renewed = employer("employer-acme", "acme.example", "b");
        require_attempt_employer_admission(
            "first",
            "acme",
            &first,
            &[reservation("reserved", Some(&first))],
        )
        .unwrap();
        review_code(
            require_attempt_employer_admission(
                "first",
                "acme",
                &renewed,
                &[reservation("reserved", Some(&first))],
            )
            .unwrap_err(),
            "attempt_employer_identity_review_required",
        );
        require_attempt_employer_admission(
            "first",
            "acme",
            &renewed,
            &[reservation("released", Some(&first))],
        )
        .unwrap();
        require_attempt_employer_admission(
            "first",
            "acme",
            &renewed,
            &[reservation("released", None)],
        )
        .unwrap();
        review_code(
            require_attempt_employer_admission(
                "first",
                "acme",
                &employer("different", "different.example", "c"),
                &[reservation("released", Some(&first))],
            )
            .unwrap_err(),
            "attempt_employer_identity_review_required",
        );
    }

    #[test]
    fn partial_or_malformed_stored_bindings_are_not_authority() {
        let valid = employer("employer-acme", "acme.example", "a");
        let mut row = reservation("reserved", Some(&valid));
        row.employer_attestation_sha256 = None;
        assert!(attempt_reservation_employer_binding(&row).is_none());
        row.employer_attestation_sha256 = Some("A".repeat(64));
        assert!(attempt_reservation_employer_binding(&row).is_none());
        row.employer_attestation_sha256 = Some("a".repeat(64));
        row.canonical_employer_domain = Some("intranet".to_string());
        assert!(attempt_reservation_employer_binding(&row).is_none());
    }

    #[test]
    fn legacy_serialized_reservations_remain_readable_but_unbound() {
        let row: AttemptReservation = serde_json::from_value(json!({
            "id": "attempt-first", "application_id": "first", "company_key": "acme",
            "period_key": "2026-10-28", "runner": "local", "status": "submitted",
            "reserved_at_ms": 1, "updated_at_ms": 1,
        }))
        .unwrap();
        assert!(attempt_reservation_employer_binding(&row).is_none());
    }

    #[test]
    fn irreversible_attempt_states_require_reconciliation_before_retry() {
        for next in [
            "reserved",
            "running",
            "failed",
            "released",
            "side_effect_unknown",
        ] {
            assert!(require_attempt_reservation_transition("submitted", next).is_err());
        }
        require_attempt_reservation_transition("submitted", "submitted").unwrap();
        for next in ["reserved", "running", "failed"] {
            assert!(require_attempt_reservation_transition("side_effect_unknown", next).is_err());
        }
        for next in ["side_effect_unknown", "submitted", "released"] {
            require_attempt_reservation_transition("side_effect_unknown", next).unwrap();
        }
        require_attempt_reservation_transition("released", "reserved").unwrap();
        require_attempt_reservation_transition("running", "side_effect_unknown").unwrap();
    }

    #[test]
    fn incomplete_terminal_history_holds_other_work_without_rewriting_history() {
        let mut connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE jobs_applications (id TEXT, account_id TEXT, state TEXT);
             CREATE TABLE jobs_attempt_reservations (
               application_id TEXT, account_id TEXT, status TEXT);
             INSERT INTO jobs_applications VALUES ('old','owner','submitted');
             INSERT INTO jobs_applications VALUES ('private','other','side_effect_unknown');",
            )
            .unwrap();
        let tx = connection.transaction().unwrap();
        review_code(
            require_complete_terminal_attempt_history_sqlite(&tx, "owner", "new").unwrap_err(),
            "legacy_company_identity_review_required",
        );
        // Recording/reconciling the historical application itself remains available.
        require_complete_terminal_attempt_history_sqlite(&tx, "owner", "old").unwrap();
        require_complete_terminal_attempt_history_sqlite(&tx, "unrelated", "new").unwrap();
        tx.execute(
            "INSERT INTO jobs_attempt_reservations VALUES ('old','owner','released')",
            [],
        )
        .unwrap();
        review_code(
            require_complete_terminal_attempt_history_sqlite(&tx, "owner", "new").unwrap_err(),
            "legacy_company_identity_review_required",
        );
        tx.execute(
            "UPDATE jobs_attempt_reservations SET status='submitted'",
            [],
        )
        .unwrap();
        require_complete_terminal_attempt_history_sqlite(&tx, "owner", "new").unwrap();
        // No cross-account row can stand in for the owner's retained slot.
        tx.execute(
            "UPDATE jobs_attempt_reservations SET account_id='other'",
            [],
        )
        .unwrap();
        review_code(
            require_complete_terminal_attempt_history_sqlite(&tx, "owner", "new").unwrap_err(),
            "legacy_company_identity_review_required",
        );
        let history: (String, String, String) = tx
            .query_row(
                "SELECT id,account_id,state FROM jobs_applications WHERE id='old'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(history, ("old".into(), "owner".into(), "submitted".into()));
        tx.rollback().unwrap();
    }
}
