#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{
        self,
        jobs::{EmploymentEntry, ProjectEntry},
    };

    fn profile() -> CareerProfile {
        CareerProfile {
            headline: "Software Engineer".into(),
            summary: "Built reliable distributed systems for healthcare teams.".into(),
            skills: vec!["Rust".into(), "PostgreSQL".into(), "React".into()],
            employment: vec![EmploymentEntry {
                id: "work-1".into(),
                company: "Example Health".into(),
                title: "Software Engineer".into(),
                highlights: vec![
                    "Built reliable distributed systems.".into(),
                    "Reduced deployment time by 30 percent.".into(),
                ],
                ..Default::default()
            }],
            projects: vec![ProjectEntry {
                id: "project-1".into(),
                name: "Care Platform".into(),
                summary: "Created healthcare workflow software.".into(),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    fn valid_plan() -> ResumePlan {
        ResumePlan {
            headline_evidence_ids: vec!["profile:headline".into()],
            summary_evidence_ids: vec!["profile:summary".into()],
            skill_order: vec!["Rust".into(), "PostgreSQL".into()],
            employment_order: vec![0],
            employment_highlight_order: vec![HighlightOrder {
                entry_index: 0,
                highlight_indices: vec![1, 0],
            }],
            employment_highlight_rewrites: Vec::new(),
            project_order: vec![0],
            cover_letter: None,
        }
    }

    fn posting() -> JobPosting {
        serde_json::from_value(json!({
            "company": "Example",
            "title": "Engineer",
            "description": "Build reliable systems"
        }))
        .unwrap()
    }

    fn generation_pool_with_job() -> (crate::db::DbPool, String, String) {
        let path = std::env::temp_dir().join(format!(
            "bluey-jobs-resume-generation-boundary-{}-{}.sqlite3",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let pool = db::open_pool(&path).unwrap();
        db::run_migrations(&pool).unwrap();
        let account_id = uuid::Uuid::new_v4().to_string();
        let job_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();
        let posting_json = json!({
            "company": "Example",
            "title": "Engineer",
            "source": "test",
        })
        .to_string();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO accounts (id, email, password_hash, trial_seconds_remaining) \
                 VALUES (?1, 'generation-boundary@bluey.test', 'hash', 0)",
                rusqlite::params![account_id],
            )
            .unwrap();
        pool.get()
            .unwrap()
            .execute(
                "INSERT INTO jobs_postings (id, account_id, canonical_key, posting_json, source, \
                 canonical_url, company, title, location, match_score, status, created_at_ms, updated_at_ms) \
                 VALUES (?1, ?2, ?3, ?5, 'test', NULL, 'Example', 'Engineer', NULL, 0, \
                 'matched', ?4, ?4)",
                rusqlite::params![
                    job_id,
                    account_id,
                    format!("test:{job_id}"),
                    now,
                    posting_json
                ],
            )
            .unwrap();
        (pool, account_id, job_id)
    }

    #[test]
    fn parses_fenced_json() {
        let raw = format!(
            "```json\n{}\n```",
            serde_json::to_string(&valid_plan()).unwrap()
        );
        assert_eq!(parse_plan(&raw).unwrap(), valid_plan());
    }

    #[test]
    fn missing_rank_arrays_normalize_to_verified_profile_order() {
        let profile = profile();
        let catalog = EvidenceCatalog::from_profile(&profile);
        let normalized = normalize_plan(&profile, &catalog, parse_plan("{}").unwrap());
        assert_eq!(normalized.employment_order, vec![0]);
        assert_eq!(normalized.project_order, vec![0]);
        assert_eq!(normalized.skill_order, profile.skills);
        assert_eq!(
            normalized.employment_highlight_order[0].highlight_indices,
            vec![0, 1]
        );
        validate_plan(&profile, &catalog, normalized).unwrap();
    }

    #[test]
    fn rejects_unknown_skill_and_duplicate_evidence() {
        let profile = profile();
        let catalog = EvidenceCatalog::from_profile(&profile);
        let mut unknown_skill = valid_plan();
        unknown_skill.skill_order.push("Kubernetes".into());
        assert!(validate_plan(&profile, &catalog, unknown_skill).is_err());

        let mut duplicate_evidence = valid_plan();
        duplicate_evidence
            .summary_evidence_ids
            .push("profile:summary".into());
        assert!(validate_plan(&profile, &catalog, duplicate_evidence).is_err());

        let mut company_as_title = valid_plan();
        company_as_title.headline_evidence_ids = vec!["employment:0:company".into()];
        assert!(validate_plan(&profile, &catalog, company_as_title).is_err());
    }

    #[test]
    fn rejects_model_authored_short_credentials_titles_and_metrics() {
        let mut attempted = serde_json::to_value(valid_plan()).unwrap();
        attempted["headline"] = json!("CEO, PhD, AWS");
        attempted["summary"] = json!("Increased revenue by 30 percent using AI and ML.");
        assert!(parse_plan(&attempted.to_string()).is_err());
    }

    #[test]
    fn exact_composition_cannot_reassign_a_metric_between_evidence_records() {
        let mut profile = profile();
        profile.summary = "Increased revenue for the platform.".into();
        profile.employment[0].highlights[0] = "Reduced latency by 30 percent.".into();
        let mut plan = valid_plan();
        plan.summary_evidence_ids =
            vec!["profile:summary".into(), "employment:0:highlight:0".into()];
        let baseline = ResumeVersion {
            id: "resume-1".into(),
            job_id: "job-1".into(),
            version_no: 1,
            mode: "factual".into(),
            content: json!({"provenance": {}}),
            diff: json!({}),
            claim_ids: Vec::new(),
            checksum: "checksum".into(),
            created_at_ms: 1,
        };
        let generated = materialize(&profile, &posting(), &baseline, &plan, "model").unwrap();
        assert_eq!(
            generated.content["summary"],
            "Increased revenue for the platform. • Reduced latency by 30 percent."
        );
        assert_ne!(
            generated.content["summary"],
            "Increased revenue by 30 percent for the platform."
        );
    }

    #[test]
    fn preserves_legitimate_short_credentials_only_when_the_evidence_contains_them() {
        let mut profile = profile();
        profile.headline = "VP".into();
        profile.summary = "AWS and GCP certified engineer.".into();
        let catalog = EvidenceCatalog::from_profile(&profile);
        let plan = valid_plan();
        assert_eq!(
            catalog
                .compose_headline(&plan.headline_evidence_ids)
                .unwrap(),
            "VP"
        );
        assert_eq!(
            catalog.compose_summary(&plan.summary_evidence_ids).unwrap(),
            "AWS and GCP certified engineer."
        );
    }

    #[test]
    fn rejects_duplicate_or_missing_indexes() {
        let profile = profile();
        let catalog = EvidenceCatalog::from_profile(&profile);
        let mut invalid = valid_plan();
        invalid.employment_highlight_order[0].highlight_indices = vec![0, 0];
        assert!(validate_plan(&profile, &catalog, invalid).is_err());
    }

    #[test]
    fn normalizes_model_shape_without_adding_candidate_claims() {
        let mut profile = profile();
        profile.skills.push("REST APIs".into());
        let catalog = EvidenceCatalog::from_profile(&profile);
        let mut plan = valid_plan();
        plan.headline_evidence_ids = vec![
            "missing:evidence".into(),
            "profile:headline".into(),
            "profile:headline".into(),
        ];
        plan.summary_evidence_ids = vec![
            "profile:summary".into(),
            "missing:summary".into(),
            "profile:summary".into(),
        ];
        plan.skill_order = vec![
            "REST APIs".into(),
            "Rust".into(),
            "Rust".into(),
            "Unknown".into(),
        ];
        plan.employment_order.clear();
        plan.employment_highlight_order.clear();
        plan.project_order.clear();

        let normalized = normalize_plan(&profile, &catalog, plan);
        assert_eq!(normalized.headline_evidence_ids, vec!["profile:headline"]);
        assert_eq!(normalized.summary_evidence_ids, vec!["profile:summary"]);
        assert_eq!(normalized.skill_order, vec!["REST APIs", "Rust"]);
        assert_eq!(normalized.employment_order, vec![0]);
        assert_eq!(
            normalized.employment_highlight_order[0].highlight_indices,
            vec![0, 1]
        );
        assert_eq!(normalized.project_order, vec![0]);
        validate_plan(&profile, &catalog, normalized).unwrap();
    }

    #[test]
    fn materialized_resume_reorders_only_verified_evidence() {
        let profile = profile();
        let baseline = ResumeVersion {
            id: "resume-1".into(),
            job_id: "job-1".into(),
            version_no: 1,
            mode: "factual".into(),
            content: json!({"provenance": {}}),
            diff: json!({}),
            claim_ids: Vec::new(),
            checksum: "checksum".into(),
            created_at_ms: 1,
        };
        let generated =
            materialize(&profile, &posting(), &baseline, &valid_plan(), "model").unwrap();
        assert_eq!(
            generated.content["employment"][0]["highlights"][0],
            "Reduced deployment time by 30 percent."
        );
        assert_eq!(
            generated.content["provenance"]["resume_generation"]["kind"],
            "model"
        );
        assert_eq!(generated.diff["claims_added"], json!([]));
        assert!(generated.cover_letter.is_empty());
        assert_eq!(
            generated.public_provenance["cover_letter_included"],
            json!(false)
        );
    }

    #[test]
    fn materializes_only_evidence_grounded_cover_letter_paragraphs() {
        let mut profile = profile();
        profile.full_name = "Candidate Name".into();
        let baseline = ResumeVersion {
            id: "resume-1".into(),
            job_id: "job-1".into(),
            version_no: 1,
            mode: "factual".into(),
            content: json!({"provenance": {}}),
            diff: json!({}),
            claim_ids: Vec::new(),
            checksum: "checksum".into(),
            created_at_ms: 1,
        };
        let mut plan = valid_plan();
        plan.cover_letter = Some(CoverLetterPlan {
            paragraphs: vec![CoverLetterParagraph {
                source_evidence_ids: vec!["profile:summary".into()],
                text: "I built reliable distributed systems for healthcare teams.".into(),
            }],
        });

        let generated = materialize(&profile, &posting(), &baseline, &plan, "model").unwrap();

        assert!(generated
            .cover_letter
            .contains("I am applying for the Engineer role at Example."));
        assert!(generated
            .cover_letter
            .contains("I built reliable distributed systems for healthcare teams."));
        assert!(generated.cover_letter.ends_with("Sincerely,\nCandidate Name"));
        assert_eq!(
            generated.public_provenance["cover_letter_included"],
            json!(true)
        );
        assert_eq!(
            generated.public_provenance["cover_letter_sources"]["0"],
            json!(["profile:summary"])
        );
    }

    #[test]
    fn rejects_cover_letter_with_unknown_or_inflated_evidence() {
        let profile = profile();
        let catalog = EvidenceCatalog::from_profile(&profile);
        let mut unknown = valid_plan();
        unknown.cover_letter = Some(CoverLetterPlan {
            paragraphs: vec![CoverLetterParagraph {
                source_evidence_ids: vec!["profile:missing".into()],
                text: "I built reliable distributed systems for healthcare teams.".into(),
            }],
        });
        assert!(validate_plan(&profile, &catalog, unknown)
            .unwrap_err()
            .to_string()
            .contains("unknown cover letter evidence"));

        let mut unsupported_metric = valid_plan();
        unsupported_metric.cover_letter = Some(CoverLetterPlan {
            paragraphs: vec![CoverLetterParagraph {
                source_evidence_ids: vec!["profile:summary".into()],
                text:
                    "I built reliable distributed systems for healthcare teams with 99 percent uptime."
                        .into(),
            }],
        });
        assert!(validate_plan(&profile, &catalog, unsupported_metric)
            .unwrap_err()
            .to_string()
            .contains("unsupported protected claims"));

        let mut profile = profile;
        profile.summary = "Supported reliable distributed systems for healthcare teams.".into();
        let catalog = EvidenceCatalog::from_profile(&profile);
        let mut stronger_claim = valid_plan();
        stronger_claim.cover_letter = Some(CoverLetterPlan {
            paragraphs: vec![CoverLetterParagraph {
                source_evidence_ids: vec!["profile:summary".into()],
                text: "I led reliable distributed systems for healthcare teams.".into(),
            }],
        });
        assert!(validate_plan(&profile, &catalog, stronger_claim)
            .unwrap_err()
            .to_string()
            .contains("strengthened"));
    }

    #[test]
    fn rewrites_a_bullet_with_same_role_source_evidence_and_real_diff() {
        let mut profile = profile();
        profile.employment[0].location = "Indianapolis, IN".into();
        profile.employment[0].start_date = "2021-01".into();
        profile.employment[0].end_date = "2023-06".into();
        let baseline = ResumeVersion {
            id: "resume-1".into(),
            job_id: "job-1".into(),
            version_no: 1,
            mode: "factual".into(),
            content: json!({
                "contact": {"name": "Candidate Name", "email": "candidate@example.com"},
                "education": [{"school": "Indiana University", "degree": "MS"}],
                "certifications": ["AWS Certified Solutions Architect"],
                "template": {"id": "compact-ats", "accent": "blue"},
                "provenance": {},
            }),
            diff: json!({}),
            claim_ids: Vec::new(),
            checksum: "checksum".into(),
            created_at_ms: 1,
        };
        let mut plan = valid_plan();
        plan.employment_highlight_rewrites = vec![HighlightRewrite {
            entry_index: 0,
            highlight_index: 0,
            source_evidence_ids: vec!["employment:0:highlight:0".into()],
            text: "Engineered reliable distributed systems.".into(),
        }];

        let generated = materialize(&profile, &posting(), &baseline, &plan, "model").unwrap();
        assert_eq!(
            generated.content["employment"][0]["company"],
            "Example Health"
        );
        assert_eq!(
            generated.content["employment"][0]["title"],
            "Software Engineer"
        );
        assert_eq!(
            generated.content["employment"][0]["location"],
            "Indianapolis, IN"
        );
        assert_eq!(generated.content["employment"][0]["start_date"], "2021-01");
        assert_eq!(generated.content["employment"][0]["end_date"], "2023-06");
        assert_eq!(
            generated.content["employment"][0]["highlights"][1],
            "Engineered reliable distributed systems."
        );
        assert_eq!(generated.content["contact"], baseline.content["contact"]);
        assert_eq!(
            generated.content["education"],
            baseline.content["education"]
        );
        assert_eq!(
            generated.content["certifications"],
            baseline.content["certifications"]
        );
        assert_eq!(generated.content["template"], baseline.content["template"]);
        assert_eq!(
            generated.content["provenance"]["resume_generation"]["rewrite_sources"]
                ["/employment/0/highlights/1"],
            json!(["employment:0:highlight:0"])
        );
        assert_eq!(
            generated.diff["experience_rewrites"][0]["before"],
            "Built reliable distributed systems."
        );
        assert_eq!(
            generated.diff["experience_rewrites"][0]["after"],
            "Engineered reliable distributed systems."
        );
    }

    #[test]
    fn rejects_rewrite_with_new_metric_or_unrelated_skill() {
        let profile = profile();
        let catalog = EvidenceCatalog::from_profile(&profile);
        let mut new_metric = valid_plan();
        new_metric.employment_highlight_rewrites = vec![HighlightRewrite {
            entry_index: 0,
            highlight_index: 0,
            source_evidence_ids: vec!["employment:0:highlight:0".into()],
            text: "Engineered reliable distributed systems with 99 percent uptime.".into(),
        }];
        assert!(validate_plan(&profile, &catalog, new_metric)
            .unwrap_err()
            .to_string()
            .contains("unsupported protected claims"));

        let mut unrelated_skill = valid_plan();
        unrelated_skill.employment_highlight_rewrites = vec![HighlightRewrite {
            entry_index: 0,
            highlight_index: 0,
            source_evidence_ids: vec!["employment:0:highlight:0".into()],
            text: "Engineered reliable React distributed systems.".into(),
        }];
        assert!(validate_plan(&profile, &catalog, unrelated_skill).is_err());
    }

    #[test]
    fn rejects_cross_role_sources_and_unsupported_claim_strength() {
        let mut profile = profile();
        profile.employment[0].highlights[0] =
            "Supported reliable distributed systems delivery.".into();
        profile.employment.push(EmploymentEntry {
            id: "work-2".into(),
            company: "Other Company".into(),
            title: "Platform Engineer".into(),
            highlights: vec!["Led a PostgreSQL migration.".into()],
            ..Default::default()
        });
        let catalog = EvidenceCatalog::from_profile(&profile);
        let mut plan = valid_plan();
        plan.employment_order = vec![0, 1];
        plan.employment_highlight_order.push(HighlightOrder {
            entry_index: 1,
            highlight_indices: vec![0],
        });
        plan.employment_highlight_rewrites = vec![HighlightRewrite {
            entry_index: 0,
            highlight_index: 0,
            source_evidence_ids: vec![
                "employment:0:highlight:0".into(),
                "employment:1:highlight:0".into(),
            ],
            text: "Led reliable distributed systems delivery.".into(),
        }];
        assert!(validate_plan(&profile, &catalog, plan.clone())
            .unwrap_err()
            .to_string()
            .contains("another role"));

        plan.employment_highlight_rewrites[0].source_evidence_ids =
            vec!["employment:0:highlight:0".into()];
        assert!(validate_plan(&profile, &catalog, plan)
            .unwrap_err()
            .to_string()
            .contains("strengthened"));
    }

    #[test]
    fn prompt_caps_reject_oversized_profile_and_job_text() {
        let mut oversized_profile = profile();
        oversized_profile.employment[0].highlights = vec!["x".repeat(MAX_CANDIDATE_PROMPT_BYTES)];
        let posting = posting();
        let catalog = EvidenceCatalog::from_profile(&oversized_profile);
        assert!(user_prompt(&oversized_profile, &posting, &catalog).is_err());

        let profile = profile();
        let mut oversized_posting = posting;
        oversized_posting.description = "x".repeat(MAX_JOB_PROMPT_BYTES);
        let catalog = EvidenceCatalog::from_profile(&profile);
        assert!(user_prompt(&profile, &oversized_posting, &catalog).is_err());
    }

    #[test]
    fn prompt_optimizes_supported_packet_coverage_without_inflating_profile_fit() {
        let prompt = system_prompt();
        assert!(prompt.contains("same skill, tool, responsibility, or outcome"));
        assert!(prompt.contains("Never keyword-stuff"));
        assert!(prompt.contains("underlying profile fit"));
        assert!(prompt.contains("cited candidate evidence"));
        assert!(prompt.contains("required responsibilities and qualifications"));
        assert!(prompt.contains("preferred qualifications"));
        assert!(prompt.contains("retaining every source bullet exactly once"));
        assert!(prompt.contains("Preserve every number, percentage, duration"));
        assert!(prompt.contains("Do not force a metric into a bullet that has none"));
        assert!(prompt.contains("Keep every employer, title, location, date"));
        assert!(prompt.contains("when the change would be cosmetic only"));
    }

    #[test]
    fn exact_docx_prompt_and_plan_lock_preserve_the_complete_source_layout() {
        let mut profile = profile();
        profile.source_resume_template_status = "exact_docx".into();
        profile.skills = (0..17).map(|index| format!("Skill {index}")).collect();
        profile.employment.push(EmploymentEntry {
            id: "work-2".into(),
            company: "Earlier Company".into(),
            title: "Platform Engineer".into(),
            highlights: vec!["Maintained a service platform.".into()],
            ..Default::default()
        });
        profile.projects.push(ProjectEntry {
            id: "project-2".into(),
            name: "Earlier Project".into(),
            summary: "Created an internal service.".into(),
            ..Default::default()
        });
        let catalog = EvidenceCatalog::from_profile(&profile);
        let prompt = user_prompt(&profile, &posting(), &catalog).unwrap();
        assert!(prompt.contains("\"layout_policy\":\"preserve_source_docx\""));

        let requested = ResumePlan {
            headline_evidence_ids: vec!["profile:headline".into()],
            summary_evidence_ids: vec!["profile:summary".into()],
            skill_order: vec!["Skill 16".into(), "Skill 0".into()],
            employment_order: vec![1, 0],
            employment_highlight_order: vec![
                HighlightOrder {
                    entry_index: 0,
                    highlight_indices: vec![1, 0],
                },
                HighlightOrder {
                    entry_index: 1,
                    highlight_indices: vec![0],
                },
            ],
            employment_highlight_rewrites: vec![HighlightRewrite {
                entry_index: 0,
                highlight_index: 0,
                source_evidence_ids: vec!["employment:0:highlight:0".into()],
                text: "Engineered reliable distributed systems.".into(),
            }],
            project_order: vec![1, 0],
            cover_letter: None,
        };
        let locked =
            lock_plan_to_source_layout(&profile, normalize_plan(&profile, &catalog, requested));

        assert!(locked.headline_evidence_ids.is_empty());
        assert!(locked.summary_evidence_ids.is_empty());
        assert_eq!(locked.skill_order, profile.skills);
        assert_eq!(locked.employment_order, vec![0, 1]);
        assert_eq!(
            locked.employment_highlight_order[0].highlight_indices,
            vec![0, 1]
        );
        assert_eq!(locked.project_order, vec![0, 1]);
        assert_eq!(locked.employment_highlight_rewrites.len(), 1);
        validate_plan(&profile, &catalog, locked).unwrap();
    }

    #[test]
    fn exact_docx_materialization_changes_only_a_truth_guarded_same_role_bullet() {
        let mut profile = profile();
        profile.source_resume_template_status = "exact_docx".into();
        profile.skills = (0..17).map(|index| format!("Skill {index}")).collect();
        profile.employment.push(EmploymentEntry {
            id: "work-2".into(),
            company: "Earlier Company".into(),
            title: "Platform Engineer".into(),
            highlights: vec!["Maintained a service platform.".into()],
            ..Default::default()
        });
        profile.projects.push(ProjectEntry {
            id: "project-2".into(),
            name: "Earlier Project".into(),
            summary: "Created an internal service.".into(),
            ..Default::default()
        });
        let baseline = ResumeVersion {
            id: "resume-source".into(),
            job_id: "job-source".into(),
            version_no: 1,
            mode: "factual".into(),
            content: json!({
                "headline": "Original Word headline",
                "summary": "Original Word summary",
                "provenance": {},
            }),
            diff: json!({}),
            claim_ids: Vec::new(),
            checksum: "source-checksum".into(),
            created_at_ms: 1,
        };
        let requested = ResumePlan {
            headline_evidence_ids: vec!["profile:headline".into()],
            summary_evidence_ids: vec!["profile:summary".into()],
            skill_order: vec!["Skill 16".into()],
            employment_order: vec![1, 0],
            employment_highlight_order: vec![
                HighlightOrder {
                    entry_index: 0,
                    highlight_indices: vec![1, 0],
                },
                HighlightOrder {
                    entry_index: 1,
                    highlight_indices: vec![0],
                },
            ],
            employment_highlight_rewrites: vec![HighlightRewrite {
                entry_index: 0,
                highlight_index: 0,
                source_evidence_ids: vec!["employment:0:highlight:0".into()],
                text: "Engineered reliable distributed systems.".into(),
            }],
            project_order: vec![1, 0],
            cover_letter: None,
        };

        let generated =
            materialize(&profile, &posting(), &baseline, &requested, "model").unwrap();

        assert_eq!(generated.content["headline"], "Original Word headline");
        assert_eq!(generated.content["summary"], "Original Word summary");
        assert_eq!(generated.content["skills"], json!(profile.skills));
        assert_eq!(
            generated.content["employment"][0]["company"],
            "Example Health"
        );
        assert_eq!(
            generated.content["employment"][1]["company"],
            "Earlier Company"
        );
        assert_eq!(
            generated.content["employment"][0]["highlights"][0],
            "Engineered reliable distributed systems."
        );
        assert_eq!(
            generated.content["employment"][0]["highlights"][1],
            "Reduced deployment time by 30 percent."
        );
        assert_eq!(generated.content["projects"][0]["name"], "Care Platform");
        assert_eq!(generated.content["projects"][1]["name"], "Earlier Project");
        assert_eq!(
            generated.public_provenance["layout_policy"],
            "preserve_source_docx"
        );
        assert!(generated.diff.get("skill_emphasis").is_none());
        assert!(generated.diff.get("experience_emphasis").is_none());
        assert!(generated.diff.get("project_emphasis").is_none());
        assert!(generated.diff["layout_policy"]
            .as_str()
            .unwrap()
            .contains("Original Word layout"));
        assert_eq!(
            generated.diff["experience_rewrites"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn model_generation_is_default_off_and_requires_an_explicit_true_value() {
        assert!(!model_generation_enabled_value(None));
        assert!(!model_generation_enabled_value(Some("")));
        assert!(!model_generation_enabled_value(Some("false")));
        assert!(!model_generation_enabled_value(Some("surprise")));
        assert!(model_generation_enabled_value(Some("1")));
        assert!(model_generation_enabled_value(Some("YES")));
    }

    #[test]
    fn cost_and_attempt_boundaries_are_deterministic() {
        let known = estimated_route_bluey_cost("openai", "gpt-5.4-mini", 1_000, 500);
        assert!(known.is_some_and(|cost| cost > 0));
        assert!(estimated_route_bluey_cost("openai", "missing", 1_000, 500).is_none());
        let price = pricing::lookup("openai", "gpt-5.4-mini").unwrap();
        let completion = routing::Completion {
            text: String::new(),
            provider: "openai".into(),
            model: "gpt-5.4-mini".into(),
            input_tokens: 1_000,
            output_tokens: 500,
            usage_provenance: pricing::UsageProvenance::Exact,
        };
        assert_eq!(
            completed_bluey_cost(price, &completion),
            pricing::compute_cost(price, 1_000, 500).0
        );
        assert!(
            completed_bluey_cost(
                price,
                &routing::Completion {
                    input_tokens: i64::MAX,
                    output_tokens: i64::MAX,
                    ..completion
                }
            ) > MAX_ATTEMPT_BLUEY_COST_CENTS
        );
        assert_ne!(
            attempt_request_id("generation", "lease-a", 0, "openai", "model"),
            attempt_request_id("generation", "lease-b", 0, "openai", "model")
        );
        assert!(MODEL_GENERATION_DEADLINE < jobs_generation::RESERVATION_TTL);
    }

    #[test]
    fn provider_deadlines_leave_a_settlement_margin_before_the_lease_ttl() {
        assert!(
            MODEL_ATTEMPT_TIMEOUT * u32::try_from(MAX_PROVIDER_ATTEMPTS).unwrap()
                <= MODEL_GENERATION_DEADLINE
        );
        assert!(
            MODEL_GENERATION_DEADLINE + Duration::from_secs(30) < jobs_generation::RESERVATION_TTL
        );
    }

    #[test]
    fn cancellation_guard_marks_the_generation_failed_and_immediately_restartable() {
        let (pool, account_id, job_id) = generation_pool_with_job();
        let ResumeGenerationReservation::Start(first) =
            jobs_generation::reserve(&pool, &account_id, &job_id, "cancelled-generation").unwrap()
        else {
            panic!("first worker should own the generation")
        };
        {
            let _guard = GenerationReservationGuard::new(
                pool.clone(),
                &account_id,
                &job_id,
                "cancelled-generation",
                &first.reservation_token,
            );
        }
        let (status, failure_code): (String, Option<String>) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT status, failure_code FROM jobs_resume_generations \
                 WHERE account_id = ?1 AND generation_key = ?2",
                rusqlite::params![account_id, "cancelled-generation"],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(status, "failed");
        assert_eq!(failure_code.as_deref(), Some("cancelled"));
        let ResumeGenerationReservation::Start(second) =
            jobs_generation::reserve(&pool, &account_id, &job_id, "cancelled-generation").unwrap()
        else {
            panic!("cancelled generation should be immediately restartable")
        };
        assert_ne!(first.reservation_token, second.reservation_token);
    }

    #[test]
    fn provider_spend_keeps_the_jobs_packet_slot_consumed_after_generation_failure() {
        let (pool, account_id, job_id) = generation_pool_with_job();
        let ResumeGenerationReservation::Start(generation) =
            jobs_generation::reserve(&pool, &account_id, &job_id, "spent-generation").unwrap()
        else {
            panic!("generation reservation must start")
        };
        assert_eq!(
            jobs_generation_allowance::reserve(
                &pool,
                &account_id,
                &job_id,
                "spent-generation",
                &generation.reservation_token,
            )
            .unwrap(),
            AllowanceReservation::Reserved
        );
        let attempt_request_id = "spent-generation:attempt:0";
        let hold_token = match jobs_provider_cost_holds::reserve(
            &pool,
            &account_id,
            "spent-generation",
            &generation.reservation_token,
            attempt_request_id,
            "openai",
            "gpt-5.4-mini",
            2,
            MAX_GENERATION_BLUEY_COST_CENTS,
            crate::config::UpstreamSpendGuard {
                limit_cents: 100,
                window_hours: 24,
            },
        )
        .unwrap()
        {
            CostHoldReservation::Held { reservation_token } => reservation_token,
            other => panic!("expected provider hold, got {other:?}"),
        };
        jobs_provider_cost_holds::settle_with_usage(
            &pool,
            &account_id,
            attempt_request_id,
            &hold_token,
            2,
            pricing::UsageProvenance::Exact,
            &UsageEvent {
                request_id: attempt_request_id.into(),
                kind: "jobs_resume_generation_attempt".into(),
                task_type: Some("jobs_resume_tailoring_rejected_provider_error".into()),
                lane: Some("deep".into()),
                provider: Some("openai".into()),
                model: Some("gpt-5.4-mini".into()),
                input_tokens: 10,
                output_tokens: 0,
                latency_ms: 1,
                cost_cents_to_bluey: 2,
                cost_cents_to_customer: 0,
                was_speculative: false,
                was_fallback: false,
            },
        )
        .unwrap();
        jobs_generation::fail(
            &pool,
            &account_id,
            "spent-generation",
            &generation.reservation_token,
            "provider_failed",
        )
        .unwrap();

        assert!(
            !jobs_generation_allowance::release(
                &pool,
                &account_id,
                &job_id,
                "spent-generation",
                &generation.reservation_token,
            )
            .unwrap(),
            "a provider-billed generation must not refund its included packet slot"
        );
        let (used_packets, allowance_status): (i64, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT e.used_packets, r.status
                   FROM jobs_entitlements e
                   JOIN jobs_generation_allowance_reservations r
                     ON r.account_id = e.account_id
                  WHERE e.account_id = ?1 AND r.job_id = ?2",
                rusqlite::params![account_id, job_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(used_packets, 1);
        assert_eq!(allowance_status, "reserved");
    }

    #[test]
    fn failure_before_provider_exposure_releases_the_jobs_packet_slot() {
        let (pool, account_id, job_id) = generation_pool_with_job();
        let ResumeGenerationReservation::Start(generation) =
            jobs_generation::reserve(&pool, &account_id, &job_id, "unspent-generation").unwrap()
        else {
            panic!("generation reservation must start")
        };
        assert_eq!(
            jobs_generation_allowance::reserve(
                &pool,
                &account_id,
                &job_id,
                "unspent-generation",
                &generation.reservation_token,
            )
            .unwrap(),
            AllowanceReservation::Reserved
        );

        jobs_generation::fail(
            &pool,
            &account_id,
            "unspent-generation",
            &generation.reservation_token,
            "validation_failed",
        )
        .unwrap();
        assert!(
            jobs_generation_allowance::release(
                &pool,
                &account_id,
                &job_id,
                "unspent-generation",
                &generation.reservation_token,
            )
            .unwrap(),
            "a generation that never reached a provider must release its packet slot"
        );

        let (used_packets, allowance_status): (i64, String) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT e.used_packets, r.status
                   FROM jobs_entitlements e
                   JOIN jobs_generation_allowance_reservations r
                     ON r.account_id = e.account_id
                  WHERE e.account_id = ?1 AND r.job_id = ?2",
                rusqlite::params![account_id, job_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(used_packets, 0);
        assert_eq!(allowance_status, "released");
    }

    #[test]
    fn crossing_the_lease_ttl_cannot_commit_late_model_output() {
        let (pool, account_id, job_id) = generation_pool_with_job();
        let ResumeGenerationReservation::Start(first) =
            jobs_generation::reserve(&pool, &account_id, &job_id, "expired-generation").unwrap()
        else {
            panic!("first worker should own the generation")
        };
        pool.get()
            .unwrap()
            .execute(
                "UPDATE jobs_resume_generations SET updated_at_ms = 0 \
                 WHERE account_id = ?1 AND generation_key = ?2",
                rusqlite::params![account_id, "expired-generation"],
            )
            .unwrap();
        assert!(jobs_generation::complete(
            &pool,
            &account_id,
            "expired-generation",
            &first.reservation_token,
            &json!({"late": true}),
            "openai",
            "gpt-5.4-mini",
            10,
            10,
            1,
        )
        .is_err());
        let ResumeGenerationReservation::Start(second) =
            jobs_generation::reserve(&pool, &account_id, &job_id, "expired-generation").unwrap()
        else {
            panic!("expired generation should be reclaimable")
        };
        assert_ne!(first.reservation_token, second.reservation_token);
    }

    #[test]
    fn rejected_provider_output_is_durably_settled_before_retry() {
        let path = std::env::temp_dir().join(format!(
            "bluey-jobs-resume-attempt-accounting-{}-{}.sqlite3",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let pool = db::open_pool(&path).unwrap();
        db::run_migrations(&pool).unwrap();
        let account = db::accounts::Account::create(&pool, "attempt@bluey.test", "hash").unwrap();
        let completion = routing::Completion {
            text: "rejected".into(),
            provider: "openai".into(),
            model: "gpt-5.4-mini".into(),
            input_tokens: 100,
            output_tokens: 50,
            usage_provenance: pricing::UsageProvenance::Exact,
        };
        let request_id = attempt_request_id(
            "generation",
            "reservation",
            0,
            &completion.provider,
            &completion.model,
        );
        assert!(matches!(
            jobs_provider_cost_holds::reserve(
                &pool,
                &account.id,
                "generation",
                "reservation",
                &request_id,
                &completion.provider,
                &completion.model,
                3,
                MAX_GENERATION_BLUEY_COST_CENTS,
                crate::config::UpstreamSpendGuard {
                    limit_cents: 4,
                    window_hours: 24,
                },
            )
            .unwrap(),
            CostHoldReservation::Held { .. }
        ));
        let mut attempt = ProviderAttemptGuard::new(
            pool.clone(),
            &account.id,
            request_id.clone(),
            "reservation",
            &completion.provider,
            &completion.model,
            0,
            100,
            3,
        );
        attempt
            .settle(&completion, 3, AttemptOutcome::RejectedTruth)
            .unwrap();
        let (task_type, provider, model, cost): (String, String, String, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT task_type, provider, model, cost_cents_to_bluey FROM usage_events \
                 WHERE account_id = ?1 AND kind = 'jobs_resume_generation_attempt'",
                rusqlite::params![account.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(task_type, AttemptOutcome::RejectedTruth.task_type());
        assert_eq!(provider, "openai");
        assert_eq!(model, "gpt-5.4-mini");
        assert_eq!(cost, 3);
        assert!(matches!(
            jobs_provider_cost_holds::reserve(
                &pool,
                &account.id,
                "generation-2",
                "reservation-2",
                "second-attempt",
                "openai",
                "gpt-5.4-mini",
                1,
                MAX_GENERATION_BLUEY_COST_CENTS,
                crate::config::UpstreamSpendGuard {
                    limit_cents: 4,
                    window_hours: 24,
                },
            )
            .unwrap(),
            CostHoldReservation::Held { .. }
        ));
        // The provider attempt event and durable hold are one exposure, not
        // two. The exact boundary admits cost 3 + 1, then denies another cent.
        assert_eq!(
            jobs_provider_cost_holds::reserve(
                &pool,
                &account.id,
                "generation-3",
                "reservation-3",
                "third-attempt",
                "openai",
                "gpt-5.4-mini",
                1,
                MAX_GENERATION_BLUEY_COST_CENTS,
                crate::config::UpstreamSpendGuard {
                    limit_cents: 4,
                    window_hours: 24,
                },
            )
            .unwrap(),
            CostHoldReservation::GlobalLimit
        );
    }

    #[test]
    fn jobs_provider_settlement_failure_stops_before_customer_root_and_reconciles_conservatively() {
        let path = std::env::temp_dir().join(format!(
            "bluey-jobs-resume-settlement-failure-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let pool = db::open_pool(&path).unwrap();
        db::run_migrations(&pool).unwrap();
        let account =
            db::accounts::Account::create(&pool, "settlement-failure@bluey.test", "hash").unwrap();
        let request_id = "settlement-failure:attempt:0";
        let reservation_token = "settlement-failure-token";
        assert!(matches!(
            jobs_provider_cost_holds::reserve(
                &pool,
                &account.id,
                "settlement-failure-generation",
                reservation_token,
                request_id,
                "openai",
                "gpt-5.4-mini",
                5,
                MAX_GENERATION_BLUEY_COST_CENTS,
                crate::config::UpstreamSpendGuard {
                    limit_cents: 100,
                    window_hours: 24,
                },
            )
            .unwrap(),
            CostHoldReservation::Held { .. }
        ));
        let completion = routing::Completion {
            text: "valid provider result".into(),
            provider: "openai".into(),
            model: "gpt-5.4-mini".into(),
            input_tokens: 10,
            output_tokens: 5,
            usage_provenance: pricing::UsageProvenance::Exact,
        };
        let mut attempt = ProviderAttemptGuard::new(
            pool.clone(),
            &account.id,
            request_id.into(),
            reservation_token,
            "openai",
            "gpt-5.4-mini",
            0,
            10,
            5,
        );
        jobs_provider_cost_holds::fail_next_settlement_for_test();
        assert!(attempt
            .settle(&completion, 2, AttemptOutcome::Accepted)
            .is_err());

        let (status_before_drop, root_events_before_drop): (String, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT h.status,
                        (SELECT COUNT(*) FROM usage_events e
                          WHERE e.account_id = ?1 AND e.kind = 'jobs_resume_generation')
                   FROM jobs_provider_cost_holds h",
                rusqlite::params![account.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(status_before_drop, "held");
        assert_eq!(root_events_before_drop, 0);

        drop(attempt);
        let (status, settled_cost, provenance, root_events): (String, i64, String, i64) = pool
            .get()
            .unwrap()
            .query_row(
                "SELECT h.status, h.settled_cost_cents, h.usage_provenance,
                        (SELECT COUNT(*) FROM usage_events e
                          WHERE e.account_id = ?1 AND e.kind = 'jobs_resume_generation')
                   FROM jobs_provider_cost_holds h",
                rusqlite::params![account.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(status, "settled");
        assert_eq!(settled_cost, 5);
        assert_eq!(provenance, pricing::UsageProvenance::Missing.as_str());
        assert_eq!(root_events, 0);
    }

    #[test]
    fn jobs_usage_provenance_controls_hold_shrink_overrun_and_next_admission() {
        let cases = [
            (
                "exact-lower",
                pricing::UsageProvenance::Exact,
                "openai",
                2,
                2,
                5,
                3,
                true,
                pricing::UsageProvenance::Exact,
            ),
            (
                "estimated-lower",
                pricing::UsageProvenance::Estimated,
                "openai",
                2,
                5,
                5,
                1,
                false,
                pricing::UsageProvenance::Estimated,
            ),
            (
                "missing-zero",
                pricing::UsageProvenance::Missing,
                "openai",
                0,
                5,
                5,
                1,
                false,
                pricing::UsageProvenance::Missing,
            ),
            (
                "route-mismatch",
                pricing::UsageProvenance::Exact,
                "crossed-provider",
                2,
                5,
                5,
                1,
                false,
                pricing::UsageProvenance::Missing,
            ),
            (
                "exact-overrun",
                pricing::UsageProvenance::Exact,
                "openai",
                7,
                7,
                7,
                1,
                false,
                pricing::UsageProvenance::Exact,
            ),
        ];
        for (
            case,
            provenance,
            returned_provider,
            reported_cost,
            expected_settled,
            next_limit,
            next_cost,
            expect_next_held,
            expected_provenance,
        ) in cases
        {
            let path = std::env::temp_dir().join(format!(
                "bluey-jobs-provenance-{case}-{}.sqlite3",
                uuid::Uuid::new_v4()
            ));
            let pool = db::open_pool(&path).unwrap();
            db::run_migrations(&pool).unwrap();
            let account =
                db::accounts::Account::create(&pool, &format!("{case}@bluey.test"), "hash")
                    .unwrap();
            let request_id = format!("{case}:attempt:0");
            let reservation_token = format!("{case}-token");
            assert!(matches!(
                jobs_provider_cost_holds::reserve(
                    &pool,
                    &account.id,
                    case,
                    &reservation_token,
                    &request_id,
                    "openai",
                    "gpt-5.4-mini",
                    5,
                    MAX_GENERATION_BLUEY_COST_CENTS,
                    crate::config::UpstreamSpendGuard {
                        limit_cents: 100,
                        window_hours: 24,
                    },
                )
                .unwrap(),
                CostHoldReservation::Held { .. }
            ));
            let completion = routing::Completion {
                text: "result".into(),
                provider: returned_provider.into(),
                model: "gpt-5.4-mini".into(),
                input_tokens: if provenance == pricing::UsageProvenance::Missing {
                    0
                } else {
                    10
                },
                output_tokens: if provenance == pricing::UsageProvenance::Missing {
                    0
                } else {
                    5
                },
                usage_provenance: provenance,
            };
            let mut attempt = ProviderAttemptGuard::new(
                pool.clone(),
                &account.id,
                request_id,
                &reservation_token,
                "openai",
                "gpt-5.4-mini",
                0,
                10,
                5,
            );
            let result = attempt.settle(&completion, reported_cost, AttemptOutcome::RejectedTruth);
            assert_eq!(result.is_err(), case == "exact-overrun", "{case}");

            let (settled_cost, stored_provenance): (i64, String) = pool
                .get()
                .unwrap()
                .query_row(
                    "SELECT settled_cost_cents, usage_provenance
                       FROM jobs_provider_cost_holds",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            assert_eq!(settled_cost, expected_settled, "{case}");
            assert_eq!(stored_provenance, expected_provenance.as_str(), "{case}");

            let next = jobs_provider_cost_holds::reserve(
                &pool,
                &account.id,
                &format!("{case}-next"),
                &format!("{case}-next-token"),
                &format!("{case}:attempt:1"),
                "openai",
                "gpt-5.4-mini",
                next_cost,
                MAX_GENERATION_BLUEY_COST_CENTS,
                crate::config::UpstreamSpendGuard {
                    limit_cents: next_limit,
                    window_hours: 24,
                },
            )
            .unwrap();
            assert_eq!(
                matches!(next, CostHoldReservation::Held { .. }),
                expect_next_held,
                "{case}"
            );
        }
    }

    #[test]
    fn deterministic_fallback_preserves_the_exact_baseline_order_and_diff() {
        let baseline = ResumeVersion {
            id: String::new(),
            job_id: "job-1".into(),
            version_no: 0,
            mode: "factual".into(),
            content: json!({
                "headline": "Software Engineer",
                "employment": [{"id": "work-2"}, {"id": "work-1"}],
                "provenance": {"candidate_truth_fingerprint": "truth"},
            }),
            diff: json!({"experience_emphasis": [{"moved_to_top": "Verified result"}]}),
            claim_ids: vec!["fact-1".into()],
            checksum: "checksum".into(),
            created_at_ms: 1,
        };
        let generated = deterministic_fallback(&baseline).unwrap();
        assert_eq!(
            generated.content["employment"],
            baseline.content["employment"]
        );
        assert_eq!(
            generated.diff["experience_emphasis"],
            baseline.diff["experience_emphasis"]
        );
        assert_eq!(
            generated.content["provenance"]["resume_generation"]["kind"],
            "deterministic_fallback"
        );
    }
}
