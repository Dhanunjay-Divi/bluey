use reqwest::Url;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProviderApplicationTargetPurpose {
    Submit,
    Confirmation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProviderApplicationTarget {
    pub provider: &'static str,
    pub host: String,
    pub tenant: String,
    pub job: String,
    pub variant: &'static str,
    pub provider_job_key: String,
}

const GREENHOUSE_JOB_QUERY_ALIASES: &[&str] = &[
    "gh_jid",
    "token",
    "job_id",
    "jobid",
    "posting_id",
    "postingid",
];
const LEVER_JOB_QUERY_ALIASES: &[&str] =
    &["posting_id", "postingid", "job_id", "jobid", "lever_job_id"];

/// Canonical provider-owned application target grammar shared by server
/// capability classification and provider receipt validation.
pub(crate) fn parse_provider_application_target(
    raw_url: &str,
    purpose: ProviderApplicationTargetPurpose,
) -> Option<ProviderApplicationTarget> {
    let (url, segments) = public_provider_url(raw_url)?;
    let host = url.host_str()?.to_ascii_lowercase();
    match host.as_str() {
        "boards.greenhouse.io" | "job-boards.greenhouse.io" => {
            greenhouse_target(&url, host, &segments, purpose)
        }
        "jobs.lever.co" | "jobs.eu.lever.co" => lever_target(&url, host, &segments, purpose),
        _ => None,
    }
}

fn public_provider_url(raw_url: &str) -> Option<(Url, Vec<String>)> {
    if raw_url.is_empty()
        || raw_url.len() > 2_048
        || raw_url.trim() != raw_url
        || !raw_url
            .get(..8)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"))
    {
        return None;
    }
    let url = Url::parse(raw_url).ok()?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || !has_exact_provider_authority(raw_url, &url)
    {
        return None;
    }
    let raw_path = raw_provider_path(raw_url);
    if raw_path.contains('%') || raw_path.contains("//") {
        return None;
    }
    let normalized_path = raw_path
        .strip_suffix('/')
        .filter(|path| !path.is_empty())
        .unwrap_or(raw_path);
    let segments = normalized_path
        .split('/')
        .skip(1)
        .map(str::to_string)
        .collect::<Vec<_>>();
    if segments.is_empty() || segments.iter().any(String::is_empty) {
        return None;
    }
    Some((url, segments))
}

fn has_exact_provider_authority(raw_url: &str, url: &Url) -> bool {
    let authority_and_suffix = &raw_url[8..];
    let authority_end = authority_and_suffix
        .find(['/', '?', '#'])
        .unwrap_or(authority_and_suffix.len());
    let authority = &authority_and_suffix[..authority_end];
    url.host_str()
        .is_some_and(|host| authority.eq_ignore_ascii_case(host))
}

fn raw_provider_path(raw_url: &str) -> &str {
    let authority_and_suffix = &raw_url[8..];
    let Some(delimiter) = authority_and_suffix.find(['/', '?', '#']) else {
        return "/";
    };
    if authority_and_suffix.as_bytes()[delimiter] != b'/' {
        return "/";
    }
    let path_and_suffix = &authority_and_suffix[delimiter..];
    let path_end = path_and_suffix
        .find(['?', '#'])
        .unwrap_or(path_and_suffix.len());
    &path_and_suffix[..path_end]
}

fn greenhouse_target(
    url: &Url,
    host: String,
    segments: &[String],
    purpose: ProviderApplicationTargetPurpose,
) -> Option<ProviderApplicationTarget> {
    let public_target = segments.len()
        == match purpose {
            ProviderApplicationTargetPurpose::Submit => 3,
            ProviderApplicationTargetPurpose::Confirmation => 4,
        }
        && segments.get(1)?.eq_ignore_ascii_case("jobs")
        && (purpose == ProviderApplicationTargetPurpose::Submit
            || segments.get(3)?.eq_ignore_ascii_case("confirmation"));
    let embedded_target = purpose == ProviderApplicationTargetPurpose::Submit
        && segments.len() == 2
        && segments.first()?.eq_ignore_ascii_case("embed")
        && segments.get(1)?.eq_ignore_ascii_case("job_app");
    if !public_target && !embedded_target {
        return None;
    }

    let path_tenant = public_target.then(|| segments[0].clone());
    let path_job = public_target.then(|| segments[2].clone());
    let tenant = one_identifier(
        query_alias_values(url, &["for"])
            .into_iter()
            .chain(path_tenant),
    )?;
    let job = one_identifier(
        query_alias_values(url, GREENHOUSE_JOB_QUERY_ALIASES)
            .into_iter()
            .chain(path_job),
    )?;
    Some(ProviderApplicationTarget {
        provider: "greenhouse",
        host,
        tenant: tenant.clone(),
        job: job.clone(),
        variant: if embedded_target {
            "greenhouse_embedded"
        } else {
            "greenhouse_public"
        },
        provider_job_key: format!("greenhouse:{tenant}:{job}"),
    })
}

fn lever_target(
    url: &Url,
    host: String,
    segments: &[String],
    purpose: ProviderApplicationTargetPurpose,
) -> Option<ProviderApplicationTarget> {
    let posting_target = purpose == ProviderApplicationTargetPurpose::Submit && segments.len() == 2;
    let application_target = segments.len() == 3
        && segments.get(2)?.eq_ignore_ascii_case(match purpose {
            ProviderApplicationTargetPurpose::Submit => "apply",
            ProviderApplicationTargetPurpose::Confirmation => "confirmation",
        });
    if !posting_target && !application_target {
        return None;
    }
    let tenant = identifier(segments.first()?)?;
    let job = identifier(segments.get(1)?)?;
    if query_alias_values(url, LEVER_JOB_QUERY_ALIASES)
        .iter()
        .any(|value| identifier(value).as_deref() != Some(job.as_str()))
    {
        return None;
    }
    Some(ProviderApplicationTarget {
        provider: "lever",
        host: host.clone(),
        tenant: tenant.clone(),
        job: job.clone(),
        variant: if application_target {
            "lever_application"
        } else {
            "lever_posting"
        },
        provider_job_key: format!("lever:{host}:{tenant}:{job}"),
    })
}

fn query_alias_values(url: &Url, aliases: &[&str]) -> Vec<String> {
    url.query_pairs()
        .filter(|(key, _)| aliases.iter().any(|alias| key.eq_ignore_ascii_case(alias)))
        .map(|(_, value)| value.into_owned())
        .collect()
}

fn one_identifier(values: impl Iterator<Item = String>) -> Option<String> {
    let values = values.collect::<Vec<_>>();
    let first = identifier(values.first()?)?;
    values
        .iter()
        .all(|value| identifier(value).as_deref() == Some(first.as_str()))
        .then_some(first)
}

fn identifier(value: &str) -> Option<String> {
    (!value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')))
    .then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct TargetVector {
        name: String,
        url: String,
        purpose: String,
        expected_target: Option<ExpectedTarget>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ExpectedTarget {
        provider: String,
        host: String,
        tenant: String,
        job: String,
        variant: String,
        provider_job_key: String,
    }

    #[test]
    fn matches_shared_typescript_target_vectors() {
        let vectors: Vec<TargetVector> = serde_json::from_str(include_str!(
            "../../jobs/automation/tests/fixtures/ats-target-vectors.json"
        ))
        .expect("shared ATS target vectors should parse");
        for vector in vectors {
            let purpose = match vector.purpose.as_str() {
                "submit" => ProviderApplicationTargetPurpose::Submit,
                "confirmation" => ProviderApplicationTargetPurpose::Confirmation,
                value => panic!("unknown purpose {value}"),
            };
            let actual = parse_provider_application_target(&vector.url, purpose);
            match vector.expected_target {
                Some(expected) => {
                    let actual = actual.unwrap_or_else(|| panic!("{} should parse", vector.name));
                    assert_eq!(actual.provider, expected.provider, "{}", vector.name);
                    assert_eq!(actual.host, expected.host, "{}", vector.name);
                    assert_eq!(actual.tenant, expected.tenant, "{}", vector.name);
                    assert_eq!(actual.job, expected.job, "{}", vector.name);
                    assert_eq!(actual.variant, expected.variant, "{}", vector.name);
                    assert_eq!(
                        actual.provider_job_key, expected.provider_job_key,
                        "{}",
                        vector.name
                    );
                }
                None => assert!(actual.is_none(), "{} should fail closed", vector.name),
            }
        }
    }

    #[test]
    fn applies_url_limit_to_utf8_bytes() {
        let oversized = format!(
            "https://jobs.lever.co/acme/posting-123#{}",
            "😀".repeat(600)
        );
        assert!(oversized.chars().count() < 2_048);
        assert!(parse_provider_application_target(
            &oversized,
            ProviderApplicationTargetPurpose::Submit,
        )
        .is_none());
    }
}
