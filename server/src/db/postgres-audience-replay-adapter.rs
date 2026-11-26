// Phase657 integrated source candidate; full integrated verification is pending.
// Schema/transaction logic here is not signature, serving, or effect authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SignatureAudienceShape {
    Six,
    Eight,
    Ten,
    Thirteen,
    Fourteen,
    Sixteen,
}

fn adapt_postgres_audience_replay<'a>(
    version: &str,
    sql: &'a str,
    shape: SignatureAudienceShape,
) -> Result<std::borrow::Cow<'a, str>> {
    use SignatureAudienceShape::{Eight, Fourteen, Sixteen, Ten, Thirteen};
    const TAIL_040: &str = "    'bluey-jobs-managed-cloud-trust-policy-v1'\n  ));";
    const TAIL_045: &str = "    'bluey-jobs-managed-cloud-serving-latch-v1'\n  ));";
    const MORE_13: &str = "    'bluey-jobs-managed-cloud-trust-policy-v1',\n    'bluey-jobs-managed-cloud-provisioning-v4',\n    'bluey-jobs-managed-cloud-preproduction-context-v1',\n    'bluey-jobs-managed-cloud-serving-latch-v1'\n  ));";
    const MORE_14_040: &str = "    'bluey-jobs-managed-cloud-trust-policy-v1',\n    'bluey-jobs-managed-cloud-provisioning-v4',\n    'bluey-jobs-managed-cloud-preproduction-context-v1',\n    'bluey-jobs-managed-cloud-serving-latch-v1',\n    'bluey-jobs-managed-cloud-process-attachment-v1'\n  ));";
    const MORE_14_045: &str = "    'bluey-jobs-managed-cloud-serving-latch-v1',\n    'bluey-jobs-managed-cloud-process-attachment-v1'\n  ));";
    const TAIL_14: &str = "    'bluey-jobs-managed-cloud-process-attachment-v1'\n  ));";
    const MORE_16: &str = "    'bluey-jobs-managed-cloud-process-attachment-v1',\n    'bluey-jobs-managed-cloud-release-v4',\n    'bluey-jobs-managed-cloud-activation-v4'\n  ));";

    let (boundary, replacement) = match (version, shape) {
        ("040_jobs_original_source_verifier_startup_authority.sql", Eight | Ten) => {
            (TAIL_040, None)
        }
        ("040_jobs_original_source_verifier_startup_authority.sql", Thirteen) => {
            (TAIL_040, Some(MORE_13))
        }
        ("040_jobs_original_source_verifier_startup_authority.sql", Fourteen | Sixteen) => {
            (TAIL_040, Some(MORE_14_040))
        }
        ("045_jobs_provisioned_runtime_startup.sql", Ten | Thirteen) => (TAIL_045, None),
        ("045_jobs_provisioned_runtime_startup.sql", Fourteen | Sixteen) => {
            (TAIL_045, Some(MORE_14_045))
        }
        _ => anyhow::bail!("unsupported signature audience replay transition"),
    };
    anyhow::ensure!(
        sql.matches(boundary).count() == 1,
        "PostgreSQL signature audience replay boundary changed"
    );
    let adapted = match replacement {
        Some(value) => std::borrow::Cow::Owned(sql.replacen(boundary, value, 1)),
        None => std::borrow::Cow::Borrowed(sql),
    };
    if shape == Sixteen {
        anyhow::ensure!(
            adapted.matches(TAIL_14).count() == 1,
            "PostgreSQL v4 signature audience replay boundary changed"
        );
        return Ok(std::borrow::Cow::Owned(
            adapted.replacen(TAIL_14, MORE_16, 1),
        ));
    }
    Ok(adapted)
}

#[cfg(test)]
mod phase657_audience_replay_adapter_tests {
    use super::*;

    const V040: &str = "040_jobs_original_source_verifier_startup_authority.sql";
    const V045: &str = "045_jobs_provisioned_runtime_startup.sql";
    const ATTACH: &str = "bluey-jobs-managed-cloud-process-attachment-v1";

    #[test]
    fn original_shapes_preserve_every_byte() {
        for shape in [SignatureAudienceShape::Eight, SignatureAudienceShape::Ten] {
            assert_eq!(
                adapt_postgres_audience_replay(
                    V040,
                    POSTGRES_JOBS_ORIGINAL_SOURCE_VERIFIER_STARTUP_AUTHORITY,
                    shape
                )
                .unwrap(),
                POSTGRES_JOBS_ORIGINAL_SOURCE_VERIFIER_STARTUP_AUTHORITY
            );
        }
        for shape in [
            SignatureAudienceShape::Ten,
            SignatureAudienceShape::Thirteen,
        ] {
            assert_eq!(
                adapt_postgres_audience_replay(
                    V045,
                    POSTGRES_JOBS_PROVISIONED_RUNTIME_STARTUP,
                    shape
                )
                .unwrap(),
                POSTGRES_JOBS_PROVISIONED_RUNTIME_STARTUP
            );
        }
    }

    #[test]
    fn widening_changes_only_the_exact_original_tail() {
        let source = POSTGRES_JOBS_ORIGINAL_SOURCE_VERIFIER_STARTUP_AUTHORITY;
        let thirteen =
            adapt_postgres_audience_replay(V040, source, SignatureAudienceShape::Thirteen).unwrap();
        let fourteen =
            adapt_postgres_audience_replay(V040, source, SignatureAudienceShape::Fourteen).unwrap();
        assert_eq!(
            thirteen.replace(
                "    'bluey-jobs-managed-cloud-trust-policy-v1',\n    'bluey-jobs-managed-cloud-provisioning-v4',\n    'bluey-jobs-managed-cloud-preproduction-context-v1',\n    'bluey-jobs-managed-cloud-serving-latch-v1'\n  ));",
                "    'bluey-jobs-managed-cloud-trust-policy-v1'\n  ));"
            ),
            source
        );
        assert_eq!(fourteen.matches(ATTACH).count(), 1);
        assert_eq!(
            fourteen.replace(
                "    'bluey-jobs-managed-cloud-serving-latch-v1',\n    'bluey-jobs-managed-cloud-process-attachment-v1'",
                "    'bluey-jobs-managed-cloud-serving-latch-v1'"
            ),
            thirteen.as_ref()
        );
        let source045 = POSTGRES_JOBS_PROVISIONED_RUNTIME_STARTUP;
        let widened045 =
            adapt_postgres_audience_replay(V045, source045, SignatureAudienceShape::Fourteen)
                .unwrap();
        assert_eq!(widened045.matches(ATTACH).count(), 1);
        assert_eq!(
            widened045.replace("    'bluey-jobs-managed-cloud-serving-latch-v1',\n    'bluey-jobs-managed-cloud-process-attachment-v1'", "    'bluey-jobs-managed-cloud-serving-latch-v1'"),
            source045
        );
    }

    #[test]
    fn malformed_boundaries_and_unsupported_transitions_refuse() {
        for (version, source, rejected) in [
            (
                V040,
                POSTGRES_JOBS_ORIGINAL_SOURCE_VERIFIER_STARTUP_AUTHORITY,
                vec![SignatureAudienceShape::Six],
            ),
            (
                V045,
                POSTGRES_JOBS_PROVISIONED_RUNTIME_STARTUP,
                vec![SignatureAudienceShape::Six, SignatureAudienceShape::Eight],
            ),
        ] {
            for shape in rejected {
                assert!(adapt_postgres_audience_replay(version, source, shape).is_err());
            }
            assert!(adapt_postgres_audience_replay(
                version,
                "no boundary",
                SignatureAudienceShape::Fourteen
            )
            .is_err());
            assert!(adapt_postgres_audience_replay(
                version,
                &format!("{source}{source}"),
                SignatureAudienceShape::Fourteen
            )
            .is_err());
        }
        assert!(adapt_postgres_audience_replay(
            "046",
            POSTGRES_JOBS_PROVISIONED_RUNTIME_STARTUP,
            SignatureAudienceShape::Fourteen
        )
        .is_err());
    }
}

// This is not a validator, a locking protocol, an operator integration, or a
// fresh-schema admission decision. No migration or authority is established.
