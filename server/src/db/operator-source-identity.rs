// Scoped source drift detection for the PostgreSQL audience operator.
// This is deliberately not a signed artifact or complete build provenance record.
use sha2::{Digest, Sha256};

const HEADER: &[u8] = b"bluey-postgres-audience-operator-source-v3\0";

const INPUTS: &[(&str, &[u8])] = &[
    (
        "server/src/db/operator-source-identity.rs",
        include_bytes!("operator-source-identity.rs"),
    ),
    ("server/src/db/operator.rs", include_bytes!("operator.rs")),
    ("server/src/db/mod.rs", include_bytes!("mod.rs")),
    ("server/Cargo.toml", include_bytes!("../../Cargo.toml")),
    (
        "server/src/bin/bluey-postgres-audience-operator.rs",
        include_bytes!("../bin/bluey-postgres-audience-operator.rs"),
    ),
    (
        "server/src/db/migration-ledger-name.rs",
        include_bytes!("migration-ledger-name.rs"),
    ),
    (
        "server/src/db/migration-ledger-catalog.rs",
        include_bytes!("migration-ledger-catalog.rs"),
    ),
    (
        "server/src/db/postgres-audience-selected-ledger-driver.rs",
        include_bytes!("postgres-audience-selected-ledger-driver.rs"),
    ),
    (
        "server/src/db/postgres-audience-operator-entry.rs",
        include_bytes!("postgres-audience-operator-entry.rs"),
    ),
    (
        "server/src/db/postgres-audience-shared-driver.rs",
        include_bytes!("postgres-audience-shared-driver.rs"),
    ),
    (
        "server/src/db/postgres-audience-replay-adapter.rs",
        include_bytes!("postgres-audience-replay-adapter.rs"),
    ),
    (
        "server/src/db/postgres-audience-replay-driver.rs",
        include_bytes!("postgres-audience-replay-driver.rs"),
    ),
    (
        "server/src/db/postgres-audience-shared-setup.sql",
        include_bytes!("postgres-audience-shared-setup.sql"),
    ),
    (
        "server/src/db/postgres-audience-shared-validate.sql",
        include_bytes!("postgres-audience-shared-validate.sql"),
    ),
    (
        "server/src/db/postgres-audience-shared-forward.sql",
        include_bytes!("postgres-audience-shared-forward.sql"),
    ),
    (
        "server/src/db/postgres-audience-replay-setup-legacy.sql",
        include_bytes!("postgres-audience-replay-setup-legacy.sql"),
    ),
    (
        "server/src/db/postgres-audience-replay-validate-legacy.sql",
        include_bytes!("postgres-audience-replay-validate-legacy.sql"),
    ),
    (
        "infra/postgres/server-runtime/036_jobs_signed_job_integrity_authority.sql",
        include_bytes!("../../../infra/postgres/server-runtime/036_jobs_signed_job_integrity_authority.sql"),
    ),
    (
        "infra/postgres/server-runtime/040_jobs_original_source_verifier_startup_authority.sql",
        include_bytes!("../../../infra/postgres/server-runtime/040_jobs_original_source_verifier_startup_authority.sql"),
    ),
    (
        "infra/postgres/server-runtime/045_jobs_provisioned_runtime_startup.sql",
        include_bytes!("../../../infra/postgres/server-runtime/045_jobs_provisioned_runtime_startup.sql"),
    ),
    (
        "infra/postgres/server-runtime/049_jobs_process_attachment_signature_audience.sql",
        include_bytes!("../../../infra/postgres/server-runtime/049_jobs_process_attachment_signature_audience.sql"),
    ),
    (
        "infra/postgres/server-runtime/052_jobs_managed_cloud_release_v4_audiences.sql",
        include_bytes!("../../../infra/postgres/server-runtime/052_jobs_managed_cloud_release_v4_audiences.sql"),
    ),
];

fn append_decimal(frame: &mut Vec<u8>, value: usize) {
    frame.extend_from_slice(value.to_string().as_bytes());
    frame.push(b':');
}

fn framed_source_records(records: &[(&str, &[u8])]) -> Vec<u8> {
    let mut frame = Vec::new();
    frame.extend_from_slice(HEADER);
    append_decimal(&mut frame, records.len());
    for (path, contents) in records {
        append_decimal(&mut frame, path.len());
        frame.extend_from_slice(path.as_bytes());
        append_decimal(&mut frame, contents.len());
        frame.extend_from_slice(contents);
    }
    frame
}

/// Returns the SHA-256 identity for the fixed, documented operator source closure.
pub fn compiled_operator_source_identity() -> String {
    let mut hasher = Sha256::new();
    hasher.update(framed_source_records(INPUTS));
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::{compiled_operator_source_identity, framed_source_records, HEADER, INPUTS};

    #[test]
    fn framing_is_unambiguous_for_nul_and_non_ascii_content() {
        let records = [("é", b"a\0\xff".as_slice()), ("x", b"yz".as_slice())];
        let mut expected = HEADER.to_vec();
        expected.extend_from_slice(b"2:2:");
        expected.extend_from_slice("é".as_bytes());
        expected.extend_from_slice(b"3:a\0\xff1:x2:yz");
        assert_eq!(framed_source_records(&records), expected);
    }

    #[test]
    fn framing_counts_utf8_bytes_not_characters() {
        let records = [("é", "λ".as_bytes())];
        let mut expected = HEADER.to_vec();
        expected.extend_from_slice(b"1:2:");
        expected.extend_from_slice("é".as_bytes());
        expected.extend_from_slice(b"2:");
        expected.extend_from_slice("λ".as_bytes());
        assert_eq!(framed_source_records(&records), expected);
    }

    #[test]
    fn identity_is_lowercase_sha256_for_the_fixed_closure() {
        let identity = compiled_operator_source_identity();
        assert_eq!(identity.len(), 64);
        assert!(identity
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
        assert_eq!(INPUTS.len(), 22);
        assert_eq!(INPUTS[0].0, "server/src/db/operator-source-identity.rs");
    }
}
