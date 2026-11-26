use super::*;

fn memory(question: &str, value: &str, scope: &str, scope_id: Option<&str>) -> AnswerMemory {
    AnswerMemory {
        id: format!("{scope}-{value}"),
        key: normalize_answer_memory_key(question),
        question: question.into(),
        value: value.into(),
        scope: scope.into(),
        scope_id: scope_id.map(str::to_owned),
        confirmed: true,
        source: "user".into(),
        created_at_ms: 1,
        updated_at_ms: 1,
        last_used_at_ms: None,
        use_count: 0,
    }
}

#[test]
fn company_memory_scope_binds_both_identity_fields_with_unambiguous_encoding() {
    let scope =
        company_memory_scope_from_identity("account-a", "employer-a", "example.test").unwrap();
    assert!(scope.starts_with("company-v1-"));
    assert_eq!(scope.len(), "company-v1-".len() + 64);
    assert_eq!(
        scope,
        company_memory_scope_from_identity("account-a", "employer-a", "example.test").unwrap()
    );
    assert_ne!(
        scope,
        company_memory_scope_from_identity("account-b", "employer-a", "example.test").unwrap()
    );
    assert_ne!(
        scope,
        company_memory_scope_from_identity("account-a", "employer-b", "example.test").unwrap()
    );
    assert_ne!(
        scope,
        company_memory_scope_from_identity("account-a", "employer-a", "other.test").unwrap()
    );
    assert_ne!(
        company_memory_scope_from_identity("account-a", "ab", "c").unwrap(),
        company_memory_scope_from_identity("account-a", "a", "bc").unwrap()
    );
    let mut public = Sha256::new();
    public.update(b"bluey-jobs-verified-company-memory-scope-v1\0");
    for value in ["employer-a", "example.test"] {
        public.update((value.len() as u64).to_be_bytes());
        public.update(value.as_bytes());
    }
    assert_ne!(
        scope,
        format!("company-v1-{}", hex::encode(public.finalize()))
    );
}

#[test]
fn company_memory_requires_exact_verified_scope_and_preserves_rank() {
    let scope =
        company_memory_scope_from_identity("account-a", "employer-a", "example.test").unwrap();
    let entries = vec![
        memory("Why apply?", "account", "account", None),
        memory("Why apply?", "track", "track", Some("track-a")),
        memory("Why apply?", "legacy", "company", Some("example")),
        memory("Why apply?", "company", "company", Some(&scope)),
    ];
    let selected = select_answer_memory(entries.clone(), "track-a", Some(&scope)).unwrap();
    assert_eq!(selected[0]["value"], "company");
    let selected = select_answer_memory(entries.clone(), "track-a", None).unwrap();
    assert_eq!(selected[0]["value"], "track");
    let other =
        company_memory_scope_from_identity("account-a", "employer-b", "example.test").unwrap();
    let selected = select_answer_memory(entries, "track-b", Some(&other)).unwrap();
    assert_eq!(selected[0]["value"], "account");
}

#[test]
fn company_memory_rejects_mismatched_question_and_same_rank_conflicts_without_values() {
    let mut mismatch = memory("Manager email", "private-value", "account", None);
    mismatch.key = "email".into();
    let error = select_answer_memory(vec![mismatch], "track-a", None).unwrap_err();
    assert!(error
        .to_string()
        .contains("answer_memory_question_review_required"));
    assert!(!error.to_string().contains("private-value"));
    let error = select_answer_memory(
        vec![
            memory("Question?", "secret-one", "account", None),
            memory("Question?", "secret-two", "account", None),
        ],
        "track-a",
        None,
    )
    .unwrap_err();
    assert!(error
        .to_string()
        .contains("answer_memory_conflict_review_required"));
    assert!(!error.to_string().contains("secret-one"));
    assert!(!error.to_string().contains("secret-two"));
}

#[test]
fn company_memory_ignores_unconfirmed_or_inapplicable_rows() {
    let mut unconfirmed = memory("Question?", "unconfirmed", "account", None);
    unconfirmed.confirmed = false;
    let mut other = memory("Question?", "other", "company", Some("legacy"));
    other.key = "mismatch".into();
    let selected = select_answer_memory(vec![unconfirmed, other], "track-a", None).unwrap();
    assert!(selected.is_empty());
}

#[test]
fn existing_company_memory_is_retained_but_requires_current_verified_scope() {
    let scope =
        company_memory_scope_from_identity("account-a", "employer-a", "example.test").unwrap();
    let answer = json!({
        "source": "answer_memory", "memory_id": "memory-1", "scope": "company",
        "scope_id": scope, "key": "why apply", "question": "Why apply?", "value": "private"
    });
    let saved = vec![answer.clone()];
    assert!(require_current_answer_memory_provenance(&saved, "track-a", Some(&scope)).is_ok());
    // A policy or head rotation leaves the canonical employer pair and scope unchanged.
    let rotated_scope =
        company_memory_scope_from_identity("account-a", "employer-a", "example.test").unwrap();
    assert!(
        require_current_answer_memory_provenance(&saved, "track-a", Some(&rotated_scope)).is_ok()
    );
    for current in [None, Some("legacy-name"), Some("company-v1-other")] {
        let error =
            require_current_answer_memory_provenance(&saved, "track-a", current).unwrap_err();
        assert!(error
            .to_string()
            .contains("legacy_company_answer_memory_review_required"));
        assert!(!error.to_string().contains("private"));
        assert_eq!(saved, vec![answer.clone()]);
    }
    let mut legacy = answer.clone();
    legacy["scope_id"] = json!("legacy-name");
    let original = legacy.clone();
    let error =
        require_current_answer_memory_provenance(&[legacy.clone()], "track-a", Some(&scope))
            .unwrap_err();
    assert!(error
        .to_string()
        .contains("legacy_company_answer_memory_review_required"));
    assert_eq!(legacy, original);
}

#[test]
fn existing_memory_requires_explicit_valid_provenance_for_account_and_track() {
    let account = json!({"source":"answer_memory", "memory_id":"m1", "scope":"account",
        "scope_id":null, "key":"question", "question":"Question?", "value":"answer"});
    let track = json!({"source":"answer_memory", "memory_id":"m2", "scope":"track",
        "scope_id":"track-a", "key":"question", "question":"Question?", "value":"answer"});
    assert!(require_current_answer_memory_provenance(
        &[account.clone(), track.clone()],
        "track-a",
        None
    )
    .is_ok());
    let mut missing = account.clone();
    missing.as_object_mut().unwrap().remove("scope");
    let mut no_id = account.clone();
    no_id.as_object_mut().unwrap().remove("memory_id");
    let mut empty_id = account.clone();
    empty_id["memory_id"] = json!(" \t ");
    let mut mismatch = account;
    mismatch["key"] = json!("different");
    for bad in [missing, no_id, empty_id, mismatch, track] {
        let error = require_current_answer_memory_provenance(&[bad], "track-b", None).unwrap_err();
        assert!(error
            .to_string()
            .contains("answer_memory_provenance_review_required"));
    }
}
