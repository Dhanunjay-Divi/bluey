pub(crate) fn validate_resume_rewrite_claims(
    profile: &CareerProfile,
    entry_index: usize,
    source: &str,
    rewritten: &str,
) -> Result<()> {
    validate_grounded_application_text(profile, &[source], rewritten)?;

    let rewritten_search = normalize_resume_search_text(rewritten);
    let source_search = normalize_resume_search_text(source);
    for (other_index, employment) in profile.employment.iter().enumerate() {
        if other_index == entry_index {
            continue;
        }
        for protected in [
            &employment.company,
            &employment.title,
            &employment.location,
        ] {
            let phrase = normalize_resume_search_text(protected);
            if phrase.len() >= 3
                && resume_contains_normalized_phrase(&rewritten_search, &phrase)
                && !resume_contains_normalized_phrase(&source_search, &phrase)
            {
                anyhow::bail!("employment rewrite introduced evidence from another role")
            }
        }
    }
    Ok(())
}

pub(crate) fn validate_grounded_application_text(
    profile: &CareerProfile,
    sources: &[&str],
    generated: &str,
) -> Result<()> {
    let source = sources.join(" ");
    let source_anchors = protected_resume_claim_anchors(profile, &source);
    let generated_anchors = protected_resume_claim_anchors(profile, generated);
    let unsupported = generated_anchors
        .difference(&source_anchors)
        .cloned()
        .collect::<Vec<_>>();
    if !unsupported.is_empty() {
        anyhow::bail!(
            "generated application text introduced unsupported protected claims: {}",
            unsupported.join(", ")
        )
    }

    for family in resume_claim_verb_families() {
        if resume_contains_any_word(generated, family)
            && !resume_contains_any_word(&source, family)
        {
            anyhow::bail!("generated application text strengthened a claim beyond its evidence")
        }
    }

    let generated_search = normalize_resume_search_text(generated);
    let source_search = normalize_resume_search_text(&source);
    for employment in &profile.employment {
        for protected in [
            &employment.company,
            &employment.title,
            &employment.location,
        ] {
            let phrase = normalize_resume_search_text(protected);
            if phrase.len() >= 3
                && resume_contains_normalized_phrase(&generated_search, &phrase)
                && !resume_contains_normalized_phrase(&source_search, &phrase)
            {
                anyhow::bail!("generated application text introduced uncited employment evidence")
            }
        }
    }

    let source_words = meaningful_resume_words(&source);
    let generated_words = meaningful_resume_words(generated);
    if generated_words.len() >= 4 {
        let overlap = generated_words.intersection(&source_words).count();
        if overlap * 100 < generated_words.len() * 20 {
            anyhow::bail!("generated application text is not sufficiently grounded in evidence")
        }
    }
    Ok(())
}

fn protected_resume_claim_anchors(
    profile: &CareerProfile,
    text: &str,
) -> BTreeSet<String> {
    let mut anchors = BTreeSet::new();
    for raw in text.split(|character: char| {
        !(character.is_alphanumeric() || matches!(character, '.' | '%' | '$' | '+' | '#' | '-'))
    }) {
        let token = raw.trim_matches(|character: char| character == '.' || character == '-');
        if token.is_empty() {
            continue;
        }
        let lower = token.to_ascii_lowercase();
        if token.chars().any(|character| character.is_ascii_digit()) {
            let number = token
                .chars()
                .filter(|character| character.is_ascii_digit() || *character == '.')
                .collect::<String>();
            if !number.is_empty() {
                anchors.insert(format!("number:{number}"));
            }
        }
        if token.contains('%') || lower == "percent" || lower == "percentage" {
            anchors.insert("unit:percent".to_string());
        }
        if token.contains('$') || matches!(lower.as_str(), "usd" | "dollar" | "dollars") {
            anchors.insert("unit:currency".to_string());
        }
        let alphabetic = token
            .chars()
            .filter(|character| character.is_alphabetic())
            .collect::<String>();
        if alphabetic.len() >= 2 && alphabetic.chars().all(|character| character.is_uppercase()) {
            anchors.insert(format!("term:{}", alphabetic.to_ascii_lowercase()));
        }
        if token.contains('+') || token.contains('#') || resume_has_internal_uppercase(token) {
            anchors.insert(format!("term:{lower}"));
        }
    }

    let searchable = normalize_resume_search_text(text);
    for (kind, values) in [
        ("skill", profile.skills.as_slice()),
        ("certification", profile.certifications.as_slice()),
    ] {
        for value in values {
            let phrase = normalize_resume_search_text(value);
            if phrase.len() >= 2 && resume_contains_normalized_phrase(&searchable, &phrase) {
                anchors.insert(format!("{kind}:{phrase}"));
            }
        }
    }
    anchors
}

fn resume_has_internal_uppercase(value: &str) -> bool {
    value
        .chars()
        .skip(1)
        .any(|character| character.is_uppercase())
}

fn normalize_resume_search_text(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || matches!(character, '+' | '#') {
                character.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn resume_contains_normalized_phrase(haystack: &str, needle: &str) -> bool {
    format!(" {haystack} ").contains(&format!(" {needle} "))
}

fn resume_contains_any_word(value: &str, words: &[&str]) -> bool {
    let normalized = normalize_resume_search_text(value);
    words
        .iter()
        .any(|word| resume_contains_normalized_phrase(&normalized, word))
}

fn resume_claim_verb_families() -> &'static [&'static [&'static str]] {
    &[
        &[
            "led",
            "lead",
            "managed",
            "manage",
            "owned",
            "own",
            "directed",
            "spearheaded",
            "mentored",
            "supervised",
        ],
        &[
            "built",
            "build",
            "developed",
            "develop",
            "created",
            "create",
            "implemented",
            "implement",
            "engineered",
            "designed",
            "design",
            "architected",
            "launched",
        ],
        &[
            "improved",
            "improve",
            "optimized",
            "optimize",
            "enhanced",
            "streamlined",
            "accelerated",
        ],
        &["reduced", "reduce", "decreased", "cut", "lowered", "saved"],
        &["increased", "increase", "grew", "raised", "boosted"],
        &["automated", "automate"],
        &["scaled", "scale"],
        &["secured", "secure", "hardened"],
        &[
            "resulted",
            "resulting",
            "enabled",
            "enabling",
            "achieved",
            "drove",
            "driving",
        ],
    ]
}

fn meaningful_resume_words(value: &str) -> BTreeSet<String> {
    const STOP_WORDS: &[&str] = &[
        "a", "an", "and", "as", "at", "by", "for", "from", "in", "into", "of", "on", "or",
        "the", "to", "with", "using", "through", "across", "that", "this", "their", "its",
    ];
    normalize_resume_search_text(value)
        .split_whitespace()
        .filter(|word| word.len() >= 3 && !STOP_WORDS.contains(word))
        .map(stem_resume_word)
        .collect()
}

fn stem_resume_word(word: &str) -> String {
    if word.len() > 5 && word.ends_with("ing") {
        return word[..word.len() - 3].to_string();
    }
    if word.len() > 4 && word.ends_with("ed") {
        return word[..word.len() - 2].to_string();
    }
    if word.len() > 4 && word.ends_with('s') {
        return word[..word.len() - 1].to_string();
    }
    word.to_string()
}

fn validate_prepared_resume_content(
    content: &Value,
    baseline: &Value,
    profile: &CareerProfile,
    identity: &ApplicationIdentity,
    posting: &JobPosting,
) -> Result<()> {
    let object = content
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("generated resume content must be an object"))?;
    let allowed_top_level = [
        "target",
        "contact",
        "headline",
        "summary",
        "skills",
        "employment",
        "education",
        "projects",
        "certifications",
        "source_resume_name",
        "provenance",
    ];
    if let Some(key) = object
        .keys()
        .find(|key| !allowed_top_level.contains(&key.as_str()))
    {
        anyhow::bail!("generated resume contains an unsupported field: {key}")
    }
    validate_object_keys(
        content.get("target"),
        &["job_id", "company", "title", "location"],
        "target",
    )?;
    validate_object_keys(
        content.get("contact"),
        &[
            "name",
            "email",
            "phone",
            "location",
            "linkedin_url",
            "portfolio_url",
        ],
        "contact",
    )?;

    validate_exact_text(content, "/target/job_id", &posting.id, "target job")?;
    validate_exact_text(content, "/target/company", &posting.company, "target company")?;
    validate_exact_text(content, "/target/title", &posting.title, "target title")?;
    validate_exact_text(content, "/target/location", &posting.location, "target location")?;
    validate_exact_text(content, "/contact/name", &profile.full_name, "candidate name")?;
    validate_exact_text(content, "/contact/email", &identity.email, "application email")?;
    validate_exact_text(content, "/contact/phone", &profile.phone, "candidate phone")?;
    validate_exact_text(
        content,
        "/contact/location",
        &profile.current_location,
        "candidate location",
    )?;
    validate_exact_text(
        content,
        "/contact/linkedin_url",
        &profile.linkedin_url,
        "LinkedIn URL",
    )?;
    validate_exact_text(
        content,
        "/contact/portfolio_url",
        &profile.portfolio_url,
        "portfolio URL",
    )?;
    validate_exact_text(
        content,
        "/source_resume_name",
        &profile.source_resume_name,
        "source resume name",
    )?;

    validate_headline_and_summary(content, baseline, profile)?;
    validate_resume_skills(content, profile)?;
    validate_exact_value(
        content.get("education"),
        &serde_json::to_value(&profile.education)?,
        "education",
    )?;
    validate_exact_value(
        content.get("certifications"),
        &serde_json::to_value(&profile.certifications)?,
        "certifications",
    )?;
    validate_exact_permutation(
        content.get("projects"),
        serde_json::to_value(&profile.projects)?,
        "projects",
    )?;
    validate_resume_employment(content, baseline, profile)
}

fn validate_object_keys(value: Option<&Value>, allowed: &[&str], label: &str) -> Result<()> {
    let object = value
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("generated resume {label} must be an object"))?;
    if let Some(key) = object
        .keys()
        .find(|key| !allowed.contains(&key.as_str()))
    {
        anyhow::bail!("generated resume {label} contains an unsupported field: {key}")
    }
    Ok(())
}

fn validate_exact_text(content: &Value, path: &str, expected: &str, label: &str) -> Result<()> {
    if content.pointer(path).and_then(Value::as_str) != Some(expected) {
        anyhow::bail!("generated resume changed the {label}")
    }
    Ok(())
}

fn validate_exact_value(actual: Option<&Value>, expected: &Value, label: &str) -> Result<()> {
    if actual != Some(expected) {
        anyhow::bail!("generated resume changed {label}")
    }
    Ok(())
}

fn validate_exact_permutation(actual: Option<&Value>, expected: Value, label: &str) -> Result<()> {
    let actual = actual
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("generated resume {label} must be an array"))?;
    let expected = expected
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("candidate {label} must be an array"))?;
    if actual.len() != expected.len() {
        anyhow::bail!("generated resume changed the number of {label}")
    }
    let mut used = BTreeSet::new();
    for item in actual {
        let Some(index) = expected
            .iter()
            .enumerate()
            .find(|(index, candidate)| !used.contains(index) && *candidate == item)
            .map(|(index, _)| index)
        else {
            anyhow::bail!("generated resume changed a {label} record")
        };
        used.insert(index);
    }
    Ok(())
}

fn validate_headline_and_summary(
    content: &Value,
    baseline: &Value,
    profile: &CareerProfile,
) -> Result<()> {
    let headline = content
        .get("headline")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("generated resume headline must be text"))?;
    let baseline_headline = baseline
        .get("headline")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let valid_headline = headline == baseline_headline
        || headline == profile.headline
        || profile.employment.iter().any(|entry| headline == entry.title)
        || profile.projects.iter().any(|project| headline == project.role);
    if !valid_headline {
        anyhow::bail!("generated resume introduced an unsupported headline")
    }

    let summary = content
        .get("summary")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("generated resume summary must be text"))?;
    let baseline_summary = baseline
        .get("summary")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if summary == baseline_summary || summary == profile.summary {
        return Ok(());
    }
    let allowed = profile
        .certifications
        .iter()
        .chain(
            profile
                .employment
                .iter()
                .flat_map(|entry| entry.highlights.iter()),
        )
        .chain(profile.projects.iter().map(|project| &project.summary))
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.as_str())
        .collect::<BTreeSet<_>>();
    let parts = summary.split(" • ").collect::<Vec<_>>();
    if parts.is_empty()
        || parts.iter().any(|part| !allowed.contains(part))
        || parts.iter().collect::<BTreeSet<_>>().len() != parts.len()
    {
        anyhow::bail!("generated resume introduced an unsupported summary")
    }
    Ok(())
}

fn validate_resume_skills(content: &Value, profile: &CareerProfile) -> Result<()> {
    let skills = content
        .get("skills")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("generated resume skills must be an array"))?;
    let mut seen = BTreeSet::new();
    for skill in skills {
        let skill = skill
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("generated resume skill must be text"))?;
        if !profile.skills.iter().any(|candidate| candidate == skill) {
            anyhow::bail!("generated resume introduced an unsupported skill: {skill}")
        }
        if !seen.insert(skill) {
            anyhow::bail!("generated resume duplicated a skill: {skill}")
        }
    }
    Ok(())
}

fn validate_resume_employment(
    content: &Value,
    baseline: &Value,
    profile: &CareerProfile,
) -> Result<()> {
    let rendered = content
        .get("employment")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("generated resume employment must be an array"))?;
    if rendered.len() != profile.employment.len() {
        anyhow::bail!("generated resume changed the number of employment records")
    }
    let baseline_entries = baseline
        .get("employment")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("baseline resume employment must be an array"))?;
    let rewrite_sources = resume_rewrite_source_map(content, profile)?;
    let mut used_entries = BTreeSet::new();
    let mut used_rewrites = BTreeSet::new();

    for (output_index, entry) in rendered.iter().enumerate() {
        let source_index = find_source_employment_index(entry, profile, &used_entries)?;
        used_entries.insert(source_index);
        let source = &profile.employment[source_index];
        let baseline_entry = baseline_entries
            .iter()
            .find(|candidate| same_employment_entry(candidate, source));
        validate_employment_metadata(entry, source, baseline_entry)?;
        let highlights = entry
            .get("highlights")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow::anyhow!("generated resume highlights must be an array"))?;
        if highlights.len() != source.highlights.len() {
            anyhow::bail!("generated resume changed the number of bullets for a role")
        }
        let mut used_highlights = BTreeSet::new();
        for (highlight_index, highlight) in highlights.iter().enumerate() {
            let text = highlight
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("generated resume bullet must be text"))?;
            let path = format!("/employment/{output_index}/highlights/{highlight_index}");
            if let Some(sources) = rewrite_sources.get(&path) {
                let (entry_index, original_index) = sources
                    .first()
                    .and_then(|source| parse_resume_highlight_source_id(source))
                    .ok_or_else(|| anyhow::anyhow!("resume rewrite has no anchor source"))?;
                if entry_index != source_index || !used_highlights.insert(original_index) {
                    anyhow::bail!("resume rewrite does not map one-to-one to its original bullet")
                }
                used_rewrites.insert(path);
                if text.trim().is_empty() {
                    anyhow::bail!("generated resume rewrite is empty")
                }
                let source_text = sources
                    .iter()
                    .map(|source_id| {
                        let (entry_index, highlight_index) =
                            parse_resume_highlight_source_id(source_id).ok_or_else(|| {
                                anyhow::anyhow!(
                                    "resume rewrite source is not an employment highlight"
                                )
                            })?;
                        profile
                            .employment
                            .get(entry_index)
                            .and_then(|entry| entry.highlights.get(highlight_index))
                            .map(String::as_str)
                            .ok_or_else(|| {
                                anyhow::anyhow!(
                                    "resume rewrite source is outside the evidence revision"
                                )
                            })
                    })
                    .collect::<Result<Vec<_>>>()?
                    .join(" ");
                validate_resume_rewrite_claims(profile, source_index, &source_text, text)?;
            } else {
                let Some(original_index) = source
                    .highlights
                    .iter()
                    .enumerate()
                    .find(|(index, original)| {
                        !used_highlights.contains(index) && original.as_str() == text
                    })
                    .map(|(index, _)| index)
                else {
                    anyhow::bail!("generated resume changed a bullet without rewrite evidence")
                };
                used_highlights.insert(original_index);
            }
        }
        if used_highlights.len() != source.highlights.len() {
            anyhow::bail!("generated resume omitted or duplicated an original bullet")
        }
    }
    if used_entries.len() != profile.employment.len() || used_rewrites.len() != rewrite_sources.len()
    {
        anyhow::bail!("generated resume employment evidence is incomplete")
    }
    Ok(())
}

fn find_source_employment_index(
    rendered: &Value,
    profile: &CareerProfile,
    used: &BTreeSet<usize>,
) -> Result<usize> {
    if let Some(id) = rendered
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
    {
        return profile
            .employment
            .iter()
            .enumerate()
            .find(|(index, entry)| !used.contains(index) && entry.id == id)
            .map(|(index, _)| index)
            .ok_or_else(|| anyhow::anyhow!("generated resume changed an employment identity"));
    }
    profile
        .employment
        .iter()
        .enumerate()
        .find(|(index, entry)| !used.contains(index) && same_employment_entry(rendered, entry))
        .map(|(index, _)| index)
        .ok_or_else(|| anyhow::anyhow!("generated resume changed an employment record"))
}

fn validate_employment_metadata(
    rendered: &Value,
    source: &EmploymentEntry,
    baseline: Option<&Value>,
) -> Result<()> {
    validate_object_keys(
        Some(rendered),
        &[
            "id",
            "company",
            "title",
            "location",
            "start_date",
            "end_date",
            "current",
            "highlights",
        ],
        "employment record",
    )?;
    let exact_text = |field: &str, expected: &str| {
        rendered.get(field).and_then(Value::as_str) == Some(expected)
            || baseline.and_then(|entry| entry.get(field)).and_then(Value::as_str)
                == rendered.get(field).and_then(Value::as_str)
    };
    for (field, expected) in [
        ("id", source.id.as_str()),
        ("company", source.company.as_str()),
        ("title", source.title.as_str()),
        ("location", source.location.as_str()),
        ("start_date", source.start_date.as_str()),
        ("end_date", source.end_date.as_str()),
    ] {
        if !exact_text(field, expected) {
            anyhow::bail!("generated resume changed employment {field}")
        }
    }
    let current = rendered
        .get("current")
        .and_then(Value::as_bool)
        .ok_or_else(|| anyhow::anyhow!("generated resume employment current flag is missing"))?;
    if current != source.current {
        anyhow::bail!("generated resume changed employment current status")
    }
    Ok(())
}
