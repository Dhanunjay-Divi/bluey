use crate::db::{
    jobs::{
        self, ApplicationEvidence, Intervention, JobApplication, JobPosting, JobsProviderMessage,
    },
    DbPool,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::cmp::Reverse;

const STRONG_MATCH_SCORE: i64 = 60;
const MIN_MATCH_GAP: i64 = 20;

pub(crate) fn process_provider_message(
    pool: &DbPool,
    account_id: &str,
    message: &JobsProviderMessage,
) -> anyhow::Result<()> {
    let applications = jobs::list_applications(pool, account_id)?;
    let mut candidates = Vec::new();
    for application in applications {
        let Some(posting) = jobs::get_posting(pool, account_id, &application.job_id)? else {
            continue;
        };
        let score = correlation_score(message, &application, &posting);
        if score > 0 {
            candidates.push((score, application, posting));
        }
    }
    candidates.sort_by_key(|candidate| Reverse(candidate.0));
    let strongest = candidates.first();
    let runner_up_score = candidates.get(1).map(|item| item.0).unwrap_or(0);
    let Some((score, application, posting)) = strongest else {
        mark_needs_input(pool, account_id, message, None, "unmatched", 0.0, json!({}))?;
        return Ok(());
    };
    if *score < STRONG_MATCH_SCORE || score.saturating_sub(runner_up_score) < MIN_MATCH_GAP {
        mark_needs_input(
            pool,
            account_id,
            message,
            None,
            "ambiguous",
            (*score as f64 / 100.0).clamp(0.0, 1.0),
            json!({
                "top_score": score,
                "runner_up_score": runner_up_score
            }),
        )?;
        return Ok(());
    }

    let classification = classify_message(message);
    let confidence = classification_confidence(&classification);
    let evidence_kind = if classification == "interview" {
        "interview_event"
    } else {
        "status_email"
    };
    let evidence_id = stable_id(
        "mail-evidence",
        &format!(
            "{}:{}:{}",
            message.provider, message.external_id, application.id
        ),
    );
    jobs::save_application_evidence(
        pool,
        account_id,
        &ApplicationEvidence {
            id: evidence_id.clone(),
            application_id: application.id.clone(),
            kind: evidence_kind.to_string(),
            label: evidence_label(&classification).to_string(),
            provider: message.provider.clone(),
            file_name: String::new(),
            media_type: "message/rfc822-reference".to_string(),
            storage_key: String::new(),
            sha256: String::new(),
            resume_version_id: application.resume_version_id.clone(),
            occurred_at_ms: message.received_at_ms,
            metadata: json!({
                "external_id": message.external_id,
                "connection_id": message.connection_id,
                "classification": classification,
                "subject": message.subject,
                "company": posting.company,
                "job_title": posting.title,
                "correlation_score": score
            }),
            created_at_ms: 0,
        },
    )?;

    if classification == "acknowledgement" {
        jobs::update_provider_message_processing(
            pool,
            account_id,
            &message.id,
            Some(&application.id),
            "processed",
            &classification,
            confidence,
            json!({ "correlation_score": score, "evidence_id": stable_id(
                "mail-evidence",
                &format!("{}:{}:{}", message.provider, message.external_id, application.id)
            ) }),
        )?;
        return Ok(());
    }

    let intervention_kind = if classification == "assessment" {
        "assessment"
    } else {
        "unknown_question"
    };
    let intervention_id = stable_id(
        "mail-intervention",
        &format!(
            "{}:{}:{}",
            message.provider, message.external_id, application.id
        ),
    );
    jobs::save_intervention(
        pool,
        account_id,
        &Intervention {
            id: intervention_id.clone(),
            application_id: Some(application.id.clone()),
            kind: intervention_kind.to_string(),
            status: "open".to_string(),
            title: intervention_title(&classification, posting),
            detail: intervention_detail(&classification),
            choices: vec![
                "Review message".to_string(),
                "Keep current status".to_string(),
            ],
            resolution_kind: "answer".to_string(),
            resume_after_resolution: false,
            provider: message.provider.clone(),
            provider_message_id: message.external_id.clone(),
            expires_at_ms: None,
            metadata: json!({
                "classification": classification,
                "connection_id": message.connection_id,
                "subject": message.subject,
                "correlation_score": score,
                "evidence_id": evidence_id,
                "read_only": true
            }),
            created_at_ms: 0,
            resolved_at_ms: None,
        },
    )?;
    jobs::update_provider_message_processing(
        pool,
        account_id,
        &message.id,
        Some(&application.id),
        "needs_input",
        &classification,
        confidence,
        json!({
            "correlation_score": score,
            "intervention_id": intervention_id,
            "evidence_id": evidence_id
        }),
    )?;
    Ok(())
}

fn mark_needs_input(
    pool: &DbPool,
    account_id: &str,
    message: &JobsProviderMessage,
    application_id: Option<&str>,
    classification: &str,
    confidence: f64,
    metadata: Value,
) -> anyhow::Result<()> {
    jobs::update_provider_message_processing(
        pool,
        account_id,
        &message.id,
        application_id,
        "needs_input",
        classification,
        confidence,
        metadata,
    )?;
    Ok(())
}

fn correlation_score(
    message: &JobsProviderMessage,
    application: &JobApplication,
    posting: &JobPosting,
) -> i64 {
    let content = normalize(&format!("{} {}", message.subject, message.body_text));
    let sender_domain = message.sender.rsplit_once('@').map(|(_, domain)| domain);
    let identity_email = application
        .receipt
        .pointer("/application_identity/email")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mut score: i64 = 0;
    if !identity_email.is_empty()
        && message
            .recipients
            .iter()
            .any(|recipient| recipient.eq_ignore_ascii_case(&identity_email))
    {
        score += 45;
    }
    if contains_normalized(&content, &posting.company) {
        score += 30;
    }
    if contains_normalized(&content, &posting.title) {
        score += 25;
    }
    if let Some(domain) = sender_domain {
        if company_matches_domain(&posting.company, domain) {
            score += 35;
        }
    }
    score.min(100)
}

fn classify_message(message: &JobsProviderMessage) -> String {
    let content = normalize(&format!("{} {}", message.subject, message.body_text));
    let rules: &[(&str, &[&str])] = &[
        (
            "rejection",
            &[
                "not moving forward",
                "decided not to proceed",
                "other candidates",
                "unfortunately",
            ],
        ),
        (
            "offer",
            &["offer letter", "pleased to offer", "employment offer"],
        ),
        (
            "assessment",
            &[
                "assessment",
                "coding challenge",
                "take-home",
                "technical test",
            ],
        ),
        (
            "interview",
            &[
                "schedule an interview",
                "interview availability",
                "phone screen",
                "meet with",
            ],
        ),
        (
            "acknowledgement",
            &[
                "application received",
                "thank you for applying",
                "received your application",
            ],
        ),
        (
            "information_request",
            &[
                "additional information",
                "please provide",
                "confirm your",
                "reply with",
            ],
        ),
    ];
    for (classification, phrases) in rules {
        if phrases.iter().any(|phrase| content.contains(phrase)) {
            return (*classification).to_string();
        }
    }
    "unknown".to_string()
}

fn classification_confidence(classification: &str) -> f64 {
    match classification {
        "unknown" => 0.45,
        "information_request" => 0.72,
        _ => 0.9,
    }
}

fn evidence_label(classification: &str) -> &'static str {
    match classification {
        "acknowledgement" => "Application acknowledgement",
        "interview" => "Interview invitation",
        "assessment" => "Assessment request",
        "rejection" => "Employer decision",
        "offer" => "Offer message",
        "information_request" => "Employer question",
        _ => "Application message",
    }
}

fn intervention_title(classification: &str, posting: &JobPosting) -> String {
    match classification {
        "interview" => format!("Review interview message from {}", posting.company),
        "assessment" => format!("Assessment requested by {}", posting.company),
        "rejection" => format!("Review update from {}", posting.company),
        "offer" => format!("Review offer message from {}", posting.company),
        "information_request" => format!("{} needs information", posting.company),
        _ => format!("Review message from {}", posting.company),
    }
}

fn intervention_detail(classification: &str) -> String {
    match classification {
        "interview" => "Bluey matched an interview message to this application. Review it before changing the application outcome.".to_string(),
        "assessment" => "Bluey matched an assessment request to this application. Complete or review the assessment before continuing.".to_string(),
        "rejection" => "Bluey matched an employer decision to this application. Confirm the outcome in your tracker.".to_string(),
        "offer" => "Bluey matched an offer-related message to this application. Review the original message before recording an outcome.".to_string(),
        "information_request" => "The employer asked for more information. Review the message and choose what to send.".to_string(),
        _ => "Bluey matched a new employer message to this application. Review it before taking action.".to_string(),
    }
}

fn normalize(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn contains_normalized(content: &str, value: &str) -> bool {
    let value = normalize(value);
    !value.is_empty() && content.contains(&value)
}

fn company_matches_domain(company: &str, domain: &str) -> bool {
    let company = normalize(company).replace(' ', "");
    let domain = domain
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .replace('-', "");
    company.len() >= 4
        && domain.len() >= 4
        && (company.contains(&domain) || domain.contains(&company))
}

fn stable_id(namespace: &str, value: &str) -> String {
    let digest = Sha256::digest(format!("{namespace}:{value}").as_bytes());
    format!("{namespace}-{}", hex::encode(&digest[..16]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn application(email: &str) -> JobApplication {
        JobApplication {
            id: "application-1".to_string(),
            job_id: "job-1".to_string(),
            resume_version_id: None,
            state: "submitted".to_string(),
            submission_mode: "review_first".to_string(),
            match_score: 90,
            answers: Vec::new(),
            cover_letter: String::new(),
            receipt: json!({
                "application_identity": {
                    "id": "identity-1",
                    "email": email
                }
            }),
            run_id: None,
            created_at_ms: 1,
            updated_at_ms: 1,
            submitted_at_ms: Some(1),
        }
    }

    fn posting() -> JobPosting {
        JobPosting {
            id: "job-1".to_string(),
            canonical_key: "acme:engineer".to_string(),
            source: "greenhouse".to_string(),
            external_id: "greenhouse-job-1".to_string(),
            canonical_url: "https://jobs.example.com/1".to_string(),
            company: "Acme Labs".to_string(),
            title: "Software Engineer".to_string(),
            location: "New York, NY".to_string(),
            workplace: "hybrid".to_string(),
            description: String::new(),
            compensation: String::new(),
            employment_type: "full_time".to_string(),
            track_id: "track-1".to_string(),
            match_score: 90,
            matched_reasons: Vec::new(),
            missing_requirements: Vec::new(),
            posted_at_ms: Some(1),
            last_verified_at_ms: Some(1),
            availability_status: "active".to_string(),
            status: "matched".to_string(),
            created_at_ms: 1,
            updated_at_ms: 1,
            eligibility: None,
        }
    }

    fn message(subject: &str, body: &str) -> JobsProviderMessage {
        JobsProviderMessage {
            id: "message-1".to_string(),
            connection_id: "connection-1".to_string(),
            provider: "gmail".to_string(),
            external_id: "external-1".to_string(),
            sender: "recruiting@acmelabs.com".to_string(),
            recipients: vec!["candidate@example.com".to_string()],
            subject: subject.to_string(),
            body_text: body.to_string(),
            received_at_ms: 1,
            application_id: None,
            processing_status: "received".to_string(),
            classification: String::new(),
            confidence: 0.0,
            metadata: json!({}),
            processed_at_ms: None,
            created_at_ms: 1,
            updated_at_ms: 1,
        }
    }

    #[test]
    fn exact_identity_and_company_correlate_strongly() {
        assert_eq!(
            correlation_score(
                &message(
                    "Acme Labs application",
                    "Software Engineer application received"
                ),
                &application("candidate@example.com"),
                &posting()
            ),
            100
        );
    }

    #[test]
    fn outcome_classifier_prioritizes_decisions() {
        assert_eq!(
            classify_message(&message(
                "Application update",
                "Unfortunately we are not moving forward with your application."
            )),
            "rejection"
        );
        assert_eq!(
            classify_message(&message(
                "Interview",
                "Please schedule an interview with our team."
            )),
            "interview"
        );
    }

    #[test]
    fn deterministic_ids_replay_safely() {
        assert_eq!(
            stable_id("mail-evidence", "gmail:message:application"),
            stable_id("mail-evidence", "gmail:message:application")
        );
    }
}
