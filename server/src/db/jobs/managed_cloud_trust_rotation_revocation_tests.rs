// Offline signed fixtures only. PostgreSQL cases require exact owned-cluster custody.

const ROOT_A: (&str, u8) = ("phase656-root-a", 41);
const ROOT_B: (&str, u8) = ("phase656-root-b", 42);
const ROOT_C: (&str, u8) = ("phase656-root-c", 43);
const INCIDENT: (&str, u8) = ("phase656-incident", 44);
const ACTOR: &str = "phase656-offline-rotation-test";

fn encoded(bytes: impl AsRef<[u8]>) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn envelope<T: Serialize>(
    target: &T,
    generation: i64,
    role: &str,
    audience: &str,
    signed_at: i64,
    signers: &[(&str, u8)],
) -> ManagedCloudAuthorityEnvelope {
    let bytes = managed_cloud_canonical_json(target).unwrap();
    let digest = managed_cloud_sha256(&bytes);
    let signer_ids: Vec<_> = signers.iter().map(|(id, _)| *id).collect();
    let signer_digest = managed_cloud_digest(&signer_ids).unwrap();
    let mut set = ManagedCloudSignatureSetAuthority {
        version: 1,
        audience: MANAGED_CLOUD_SIGNATURE_SET_AUDIENCE.into(),
        signature_set_id: format!("rotation-{}-{}", &digest[..16], &signer_digest[..16]),
        trust_generation: generation,
        role: role.into(),
        target_audience: audience.into(),
        target_sha256: digest,
        signed_at_ms: signed_at,
        signatures: Vec::new(),
    };
    let payload = managed_cloud_signature_payload(&set).unwrap();
    set.signatures = signers
        .iter()
        .map(|(id, seed)| {
            let key = ed25519_dalek::SigningKey::from_bytes(&[*seed; 32]);
            ManagedCloudDetachedSignatureAuthority {
                key_id: (*id).into(),
                signature: encoded(key.sign(&payload).to_bytes()),
            }
        })
        .collect();
    set.signatures
        .sort_by(|left, right| left.key_id.cmp(&right.key_id));
    ManagedCloudAuthorityEnvelope {
        canonical_base64url: encoded(bytes),
        signature_set_base64url: encoded(managed_cloud_canonical_json(&set).unwrap()),
    }
}

fn fixture(
    pool: &DbPool,
) -> (
    ManagedCloudTrustPolicyAuthority,
    ManagedCloudTrustPolicyAuthority,
    i64,
) {
    let now = match pool {
        DbPool::Sqlite(_) => {
            managed_cloud_db_now_sqlite(&pool.get().unwrap().transaction().unwrap()).unwrap()
        }
        DbPool::Postgres(_) => {
            managed_cloud_db_now_postgres(&mut pool.get_pg().unwrap().transaction().unwrap())
                .unwrap()
        }
    };
    let roles = [
        "general_promotion",
        "incident",
        "promotion",
        "release",
        "root",
    ];
    let mut keys: Vec<_> = [
        ("phase656-general", 45, "general_promotion", 1),
        (INCIDENT.0, INCIDENT.1, "incident", 1),
        ("phase656-promotion", 46, "promotion", 1),
        ("phase656-release", 47, "release", 1),
        (ROOT_A.0, ROOT_A.1, "root", 1),
        (ROOT_B.0, ROOT_B.1, "root", 1),
        // Registered/revocable in generation1, but cannot satisfy its root quorum.
        (ROOT_C.0, ROOT_C.1, "root", 2),
    ]
    .into_iter()
    .map(|(id, seed, role, minimum)| ManagedCloudTrustKeyAuthority {
        key_id: id.into(),
        role: role.into(),
        public_key: encoded(
            ed25519_dalek::SigningKey::from_bytes(&[seed; 32])
                .verifying_key()
                .to_bytes(),
        ),
        state: "active".into(),
        valid_from_ms: now - 10_000,
        valid_until_ms: now + 3_600_000,
        minimum_trust_generation: minimum,
        maximum_trust_generation: 2,
    })
    .collect();
    keys.sort_by(|left, right| left.key_id.cmp(&right.key_id));
    let predecessor = ManagedCloudTrustPolicyAuthority {
        version: 1,
        audience: MANAGED_CLOUD_TRUST_POLICY_AUDIENCE.into(),
        policy_id: "phase656-rotation-policy-1".into(),
        trust_generation: 1,
        predecessor_policy_sha256: None,
        issued_at_ms: now - 5_000,
        valid_from_ms: now - 10_000,
        expires_at_ms: now + 3_600_000,
        roles: roles
            .into_iter()
            .map(|role| ManagedCloudTrustRoleAuthority {
                role: role.into(),
                threshold: 1,
            })
            .collect(),
        keys,
    };
    let root = predecessor
        .keys
        .iter()
        .find(|key| key.key_id == ROOT_A.0)
        .unwrap();
    let anchor = ManagedCloudRootTrustAnchor {
        version: 1,
        audience: MANAGED_CLOUD_ROOT_ANCHOR_AUDIENCE.into(),
        threshold: 1,
        keys: vec![ManagedCloudRootTrustKey {
            key_id: root.key_id.clone(),
            public_key: root.public_key.clone(),
            valid_from_ms: root.valid_from_ms,
            valid_until_ms: root.valid_until_ms,
        }],
    };
    validate_managed_cloud_root_anchor(&anchor).unwrap();
    let bootstrap = envelope(
        &predecessor,
        1,
        "root",
        MANAGED_CLOUD_TRUST_POLICY_AUDIENCE,
        now - 1_000,
        &[ROOT_A],
    );
    import_managed_cloud_trust_policy_with_bootstrap_anchor(pool, &bootstrap, ACTOR, Some(&anchor))
        .unwrap();
    let mut successor = predecessor.clone();
    successor.policy_id = "phase656-rotation-policy-2".into();
    successor.trust_generation = 2;
    successor.predecessor_policy_sha256 = Some(managed_cloud_digest(&predecessor).unwrap());
    successor
        .keys
        .retain(|key| key.role != "root" || key.key_id == ROOT_C.0);
    validate_managed_cloud_trust_policy(&successor).unwrap();
    (predecessor, successor, now)
}

fn revoke(
    pool: &DbPool,
    policy: &ManagedCloudTrustPolicyAuthority,
    signer: Option<(&str, u8)>,
    now: i64,
    future: bool,
) {
    let (kind, id, digest) = match signer {
        Some((id, seed)) => (
            "signing_key",
            id.to_owned(),
            managed_cloud_sha256(
                &ed25519_dalek::SigningKey::from_bytes(&[seed; 32])
                    .verifying_key()
                    .to_bytes(),
            ),
        ),
        None => (
            "trust_policy",
            policy.policy_id.clone(),
            managed_cloud_digest(policy).unwrap(),
        ),
    };
    let revocation = ManagedCloudRevocationAuthority {
        version: 1,
        audience: MANAGED_CLOUD_REVOCATION_AUDIENCE.into(),
        revocation_id: "phase656-rotation-revocation".into(),
        revocation_generation: 1,
        predecessor_revocation_sha256: None,
        trust_generation: 1,
        subject_kind: kind.into(),
        subject_id: id,
        subject_sha256: digest,
        reason_ref: "offline-root-rotation-regression".into(),
        issued_at_ms: now - 2_000,
        effective_at_ms: if future { now + 600_000 } else { now - 1_000 },
    };
    let request = ManagedCloudRevocationImportRequest {
        envelope: envelope(
            &revocation,
            1,
            "incident",
            MANAGED_CLOUD_REVOCATION_AUDIENCE,
            now - 1_000,
            &[INCIDENT],
        ),
    };
    import_managed_cloud_revocation(pool, &request, ACTOR).unwrap();
}

fn counts(pool: &DbPool) -> Vec<i64> {
    [
        "jobs_managed_cloud_trust_policies",
        "jobs_managed_cloud_signature_sets",
        "jobs_managed_cloud_trust_keys",
    ]
    .into_iter()
    .map(|table| {
        let sql = format!("SELECT COUNT(*) FROM {table}");
        match pool {
            DbPool::Sqlite(_) => pool
                .get()
                .unwrap()
                .query_row(&sql, [], |row| row.get(0))
                .unwrap(),
            DbPool::Postgres(_) => pool.get_pg().unwrap().query_one(&sql, &[]).unwrap().get(0),
        }
    })
    .collect()
}

#[derive(Clone, Copy)]
enum Case {
    PredecessorSigner,
    Policy,
    SuccessorOnly,
    Future,
}

fn assert_valid_union(
    predecessor: &ManagedCloudTrustPolicyAuthority,
    successor: &ManagedCloudTrustPolicyAuthority,
    signed: &ManagedCloudAuthorityEnvelope,
    now: i64,
) {
    let (bytes, signatures) = managed_cloud_decode_envelope(signed).unwrap();
    let set = managed_cloud_parse_canonical(&signatures).unwrap();
    verify_managed_cloud_rotation_union_signature_set(&bytes, &set, predecessor, successor, now)
        .unwrap();
}

fn exercise(pool: &DbPool, case: Case) {
    let (predecessor, successor, now) = fixture(pool);
    let signer = match case {
        Case::PredecessorSigner | Case::Future => Some(ROOT_A),
        Case::Policy => None,
        Case::SuccessorOnly => Some(ROOT_C),
    };
    revoke(
        pool,
        &predecessor,
        signer,
        now,
        matches!(case, Case::Future),
    );
    let before = counts(pool);
    let signed = envelope(
        &successor,
        2,
        "root",
        MANAGED_CLOUD_TRUST_POLICY_AUDIENCE,
        now - 1_000,
        &[ROOT_A, ROOT_C],
    );
    assert_valid_union(&predecessor, &successor, &signed, now);
    if matches!(case, Case::Future) {
        assert!(
            !import_managed_cloud_trust_policy(pool, &signed, ACTOR)
                .unwrap()
                .replayed
        );
        return;
    }
    assert!(matches!(
        import_managed_cloud_trust_policy(pool, &signed, ACTOR),
        Err(ManagedCloudRegistryError::Revoked)
    ));
    assert_eq!(
        counts(pool),
        before,
        "failed rotation must insert no signature/key/policy rows"
    );
    if matches!(case, Case::PredecessorSigner) {
        // A revoked extra signer rejects even if clean signers could meet both quorums.
        let extra = envelope(
            &successor,
            2,
            "root",
            MANAGED_CLOUD_TRUST_POLICY_AUDIENCE,
            now - 1_000,
            &[ROOT_A, ROOT_B, ROOT_C],
        );
        assert_valid_union(&predecessor, &successor, &extra, now);
        assert!(matches!(
            import_managed_cloud_trust_policy(pool, &extra, ACTOR),
            Err(ManagedCloudRegistryError::Revoked)
        ));
        let mut shared = successor.clone();
        shared.keys.push(
            predecessor
                .keys
                .iter()
                .find(|key| key.key_id == ROOT_A.0)
                .unwrap()
                .clone(),
        );
        shared
            .keys
            .sort_by(|left, right| left.key_id.cmp(&right.key_id));
        let shared_envelope = envelope(
            &shared,
            2,
            "root",
            MANAGED_CLOUD_TRUST_POLICY_AUDIENCE,
            now - 1_000,
            &[ROOT_A],
        );
        assert_valid_union(&predecessor, &shared, &shared_envelope, now);
        assert!(matches!(
            import_managed_cloud_trust_policy(pool, &shared_envelope, ACTOR),
            Err(ManagedCloudRegistryError::Revoked)
        ));
        assert_eq!(counts(pool), before);
        let clean = envelope(
            &successor,
            2,
            "root",
            MANAGED_CLOUD_TRUST_POLICY_AUDIENCE,
            now - 1_000,
            &[ROOT_B, ROOT_C],
        );
        assert_valid_union(&predecessor, &successor, &clean, now);
        let installed = import_managed_cloud_trust_policy(pool, &clean, ACTOR).unwrap();
        assert!(!installed.replayed);
        assert!(
            import_managed_cloud_trust_policy(pool, &clean, ACTOR)
                .unwrap()
                .replayed
        );
    }
    // Import replay remains a historical receipt, never fresh use permission.
    let bootstrap = envelope(
        &predecessor,
        1,
        "root",
        MANAGED_CLOUD_TRUST_POLICY_AUDIENCE,
        now - 1_000,
        &[ROOT_A],
    );
    assert!(
        import_managed_cloud_trust_policy(pool, &bootstrap, ACTOR)
            .unwrap()
            .replayed
    );
}

#[test]
fn sqlite_rotation_rejects_revoked_signer_and_accepts_clean_quorum() {
    exercise(
        &crate::db::jobs::tests::test_pool(),
        Case::PredecessorSigner,
    );
}
#[test]
fn sqlite_rotation_rejects_revoked_predecessor_policy() {
    exercise(&crate::db::jobs::tests::test_pool(), Case::Policy);
}
#[test]
fn sqlite_rotation_rejects_registered_revoked_successor_only_signer() {
    exercise(&crate::db::jobs::tests::test_pool(), Case::SuccessorOnly);
}
#[test]
fn sqlite_rotation_allows_future_effective_revocation() {
    exercise(&crate::db::jobs::tests::test_pool(), Case::Future);
}

fn postgres_case(name: &str, case: Case) {
    let Some(url) = crate::db::owned_postgres_test::owned_url(name) else {
        return;
    };
    let pool = crate::db::open_postgres_pool(&url).unwrap();
    let fresh: bool = pool
        .get_pg()
        .unwrap()
        .query_one(
            "SELECT to_regclass('public.bluey_schema_migrations') IS NULL",
            &[],
        )
        .unwrap()
        .get(0);
    assert!(fresh, "fresh separately owned database required");
    crate::db::run_migrations(&pool).unwrap();
    assert_eq!(
        counts(&pool),
        vec![0, 0, 0],
        "fresh separately owned database required"
    );
    exercise(&pool, case);
    println!("phase656_owned_trust_rotation_revocation_complete");
}
#[test]
fn postgres_rotation_rejects_revoked_signer_and_accepts_clean_quorum_when_owned() {
    postgres_case(
        concat!(
            "db::jobs::managed_cloud_trust_rotation_revocation_tests::",
            "postgres_rotation_rejects_revoked_signer_and_accepts_clean_quorum_when_owned"
        ),
        Case::PredecessorSigner,
    );
}
#[test]
fn postgres_rotation_rejects_revoked_predecessor_policy_when_owned() {
    postgres_case(
        concat!(
            "db::jobs::managed_cloud_trust_rotation_revocation_tests::",
            "postgres_rotation_rejects_revoked_predecessor_policy_when_owned"
        ),
        Case::Policy,
    );
}
#[test]
fn postgres_rotation_rejects_registered_revoked_successor_only_signer_when_owned() {
    postgres_case(
        concat!(
            "db::jobs::managed_cloud_trust_rotation_revocation_tests::",
            "postgres_rotation_rejects_registered_revoked_successor_only_signer_when_owned"
        ),
        Case::SuccessorOnly,
    );
}
#[test]
fn postgres_rotation_allows_future_effective_revocation_when_owned() {
    postgres_case(
        concat!(
            "db::jobs::managed_cloud_trust_rotation_revocation_tests::",
            "postgres_rotation_allows_future_effective_revocation_when_owned"
        ),
        Case::Future,
    );
}
