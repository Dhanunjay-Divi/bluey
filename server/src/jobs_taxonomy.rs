//! Immutable, server-authoritative role, skill, and geography taxonomy for Bluey Jobs.
//!
//! Target-role resolution is deliberately independent from posting classification. A posting can
//! be classified for ranking or evidence without silently selecting a candidate's Career Track.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::OnceLock;

const REGISTRY_BYTES: &[u8] = include_bytes!("../../jobs/taxonomy/canonical-v1.json");
const TAXONOMY_VERSION: &str = "bluey-jobs-taxonomy-v1-2026-08-25";
const TAXONOMY_SHA256: &str = "facdb3593457b6585ea83c9f735c42616e7cf03caa6e0369be9154f542dd7254";

static REGISTRY: OnceLock<Result<CanonicalTaxonomyRegistry, String>> = OnceLock::new();

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalTaxonomyRegistry {
    pub schema_version: u32,
    pub taxonomy_version: String,
    pub role_families: Vec<RoleFamily>,
    pub roles: Vec<RoleEntry>,
    pub ambiguous_role_aliases: Vec<AmbiguousRoleAlias>,
    pub skills: Vec<SkillEntry>,
    pub countries: Vec<CountryEntry>,
    pub subdivisions: Vec<SubdivisionEntry>,
    pub metros: Vec<MetroEntry>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoleFamily {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoleEntry {
    pub id: String,
    pub label: String,
    pub family_id: String,
    pub aliases: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AmbiguousRoleAlias {
    pub alias: String,
    pub candidate_role_ids: Vec<String>,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SkillEntry {
    pub id: String,
    pub label: String,
    pub aliases: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CountryEntry {
    pub code: String,
    pub label: String,
    pub aliases: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SubdivisionEntry {
    pub country_code: String,
    pub code: String,
    pub label: String,
    pub aliases: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MetroEntry {
    pub id: String,
    pub label: String,
    pub country_code: String,
    pub subdivision_ids: Vec<String>,
    pub aliases: Vec<String>,
    pub cities: Vec<MetroCityEntry>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MetroCityEntry {
    pub id: String,
    pub label: String,
    pub subdivision_id: Option<String>,
    pub aliases: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TaxonomyBinding {
    pub schema_version: u32,
    pub taxonomy_version: String,
    pub digest_algorithm: String,
    pub digest_sha256: String,
}

pub fn registry_bytes() -> &'static [u8] {
    REGISTRY_BYTES
}

pub fn taxonomy_version() -> &'static str {
    TAXONOMY_VERSION
}

pub fn taxonomy_sha256() -> String {
    TAXONOMY_SHA256.to_string()
}

pub fn taxonomy_binding() -> TaxonomyBinding {
    TaxonomyBinding {
        schema_version: 1,
        taxonomy_version: TAXONOMY_VERSION.to_string(),
        digest_algorithm: "sha256".to_string(),
        digest_sha256: TAXONOMY_SHA256.to_string(),
    }
}

pub fn registry_json() -> Result<Value, &'static str> {
    let registry = registry()?;
    serde_json::to_value(registry).map_err(|_| "validated taxonomy could not be serialized")
}

pub fn registry() -> Result<&'static CanonicalTaxonomyRegistry, &'static str> {
    match REGISTRY.get_or_init(load_and_validate_registry) {
        Ok(registry) => Ok(registry),
        Err(error) => Err(error.as_str()),
    }
}

fn load_and_validate_registry() -> Result<CanonicalTaxonomyRegistry, String> {
    let observed_sha256 = hex::encode(Sha256::digest(REGISTRY_BYTES));
    if observed_sha256 != TAXONOMY_SHA256 {
        return Err("embedded taxonomy digest does not match its immutable binding".to_string());
    }

    let registry: CanonicalTaxonomyRegistry = serde_json::from_slice(REGISTRY_BYTES)
        .map_err(|error| format!("embedded taxonomy is invalid JSON: {error}"))?;
    validate_registry(&registry)?;
    Ok(registry)
}

fn validate_registry(registry: &CanonicalTaxonomyRegistry) -> Result<(), String> {
    if registry.schema_version != 1 || registry.taxonomy_version != TAXONOMY_VERSION {
        return Err("embedded taxonomy schema or version is not supported".to_string());
    }

    let mut family_ids = HashSet::new();
    for family in &registry.role_families {
        require_nonempty(&family.id, "role family id")?;
        require_nonempty(&family.label, "role family label")?;
        if !family_ids.insert(family.id.as_str()) {
            return Err(format!("duplicate role family id: {}", family.id));
        }
    }

    let mut role_ids = HashSet::new();
    let mut role_aliases: BTreeMap<String, &str> = BTreeMap::new();
    for role in &registry.roles {
        require_nonempty(&role.id, "role id")?;
        require_nonempty(&role.label, "role label")?;
        if !role_ids.insert(role.id.as_str()) {
            return Err(format!("duplicate role id: {}", role.id));
        }
        if !family_ids.contains(role.family_id.as_str()) {
            return Err(format!("unknown role family: {}", role.family_id));
        }
        for alias in std::iter::once(&role.id)
            .chain(std::iter::once(&role.label))
            .chain(&role.aliases)
        {
            let normalized = normalize_target_role_phrase(alias);
            require_nonempty(&normalized, "role alias")?;
            if let Some(existing) = role_aliases.insert(normalized, role.id.as_str()) {
                if existing != role.id {
                    return Err(format!(
                        "role alias is shared by {existing} and {}",
                        role.id
                    ));
                }
            }
        }
    }

    let mut ambiguous_aliases = HashSet::new();
    for ambiguous in &registry.ambiguous_role_aliases {
        let normalized = normalize_target_role_phrase(&ambiguous.alias);
        require_nonempty(&normalized, "ambiguous role alias")?;
        if role_aliases.contains_key(&normalized) || !ambiguous_aliases.insert(normalized) {
            return Err(format!(
                "ambiguous role alias is not unique: {}",
                ambiguous.alias
            ));
        }
        let candidates: BTreeSet<_> = ambiguous.candidate_role_ids.iter().collect();
        if candidates.len() < 2
            || candidates
                .iter()
                .any(|candidate| !role_ids.contains(candidate.as_str()))
        {
            return Err(format!(
                "ambiguous role alias has invalid candidates: {}",
                ambiguous.alias
            ));
        }
    }

    let mut skill_ids = HashSet::new();
    let mut skill_aliases: BTreeMap<String, &str> = BTreeMap::new();
    for skill in &registry.skills {
        require_nonempty(&skill.id, "skill id")?;
        require_nonempty(&skill.label, "skill label")?;
        if !skill_ids.insert(skill.id.as_str()) {
            return Err(format!("duplicate skill id: {}", skill.id));
        }
        for alias in std::iter::once(&skill.label)
            .chain(&skill.aliases)
            .chain(std::iter::once(&skill.id))
        {
            require_nonempty(alias, "skill alias")?;
            if !alias.is_ascii() {
                return Err(format!("skill alias is not ASCII: {alias}"));
            }
            let normalized = alias.trim().to_ascii_lowercase();
            if let Some(existing) = skill_aliases.insert(normalized, skill.id.as_str()) {
                if existing != skill.id {
                    return Err(format!(
                        "skill alias is shared by {existing} and {}",
                        skill.id
                    ));
                }
            }
        }
    }

    let mut country_codes = HashSet::new();
    let mut country_aliases: BTreeMap<String, &str> = BTreeMap::new();
    for country in &registry.countries {
        require_nonempty(&country.code, "country code")?;
        require_nonempty(&country.label, "country label")?;
        if !country_codes.insert(country.code.as_str()) {
            return Err(format!("duplicate country code: {}", country.code));
        }
        for alias in std::iter::once(&country.code)
            .chain(std::iter::once(&country.label))
            .chain(&country.aliases)
        {
            let normalized = normalize_words(alias);
            require_nonempty(&normalized, "country alias")?;
            if let Some(existing) = country_aliases.insert(normalized, country.code.as_str()) {
                if existing != country.code {
                    return Err(format!(
                        "country alias is shared by {existing} and {}",
                        country.code
                    ));
                }
            }
        }
    }

    let mut subdivision_ids = HashSet::new();
    for subdivision in &registry.subdivisions {
        require_nonempty(&subdivision.code, "subdivision code")?;
        require_nonempty(&subdivision.label, "subdivision label")?;
        if !country_codes.contains(subdivision.country_code.as_str()) {
            return Err(format!(
                "subdivision references unknown country: {}-{}",
                subdivision.country_code, subdivision.code
            ));
        }
        let id = subdivision_id(subdivision);
        if !subdivision_ids.insert(id.clone()) {
            return Err(format!("duplicate subdivision id: {id}"));
        }
        for alias in std::iter::once(&subdivision.code)
            .chain(std::iter::once(&subdivision.label))
            .chain(&subdivision.aliases)
        {
            require_nonempty(&normalize_words(alias), "subdivision alias")?;
        }
    }

    let mut metro_ids = HashSet::new();
    let mut metro_aliases: BTreeMap<String, &str> = BTreeMap::new();
    let mut city_ids = HashSet::new();
    let mut city_aliases: BTreeMap<String, &str> = BTreeMap::new();
    for metro in &registry.metros {
        require_nonempty(&metro.id, "metro id")?;
        require_nonempty(&metro.label, "metro label")?;
        if !metro_ids.insert(metro.id.as_str()) {
            return Err(format!("duplicate metro id: {}", metro.id));
        }
        if !country_codes.contains(metro.country_code.as_str()) {
            return Err(format!("metro references unknown country: {}", metro.id));
        }
        let distinct_subdivisions: BTreeSet<_> = metro.subdivision_ids.iter().collect();
        if distinct_subdivisions.len() != metro.subdivision_ids.len()
            || metro.subdivision_ids.iter().any(|subdivision| {
                !subdivision_ids.contains(subdivision)
                    || !subdivision.starts_with(&format!("{}-", metro.country_code))
            })
        {
            return Err(format!("metro has invalid subdivisions: {}", metro.id));
        }
        for alias in metro_terms(metro) {
            let normalized = normalize_words(alias);
            require_nonempty(&normalized, "metro alias")?;
            if let Some(existing) = metro_aliases.insert(normalized, metro.id.as_str()) {
                if existing != metro.id {
                    return Err(format!(
                        "metro alias is shared by {existing} and {}",
                        metro.id
                    ));
                }
            }
        }
        if metro.cities.is_empty() {
            return Err(format!("metro has no bounded cities: {}", metro.id));
        }
        for city in &metro.cities {
            require_nonempty(&city.id, "city id")?;
            require_nonempty(&city.label, "city label")?;
            if !city_ids.insert(city.id.as_str()) {
                return Err(format!("duplicate city id: {}", city.id));
            }
            if city
                .subdivision_id
                .as_ref()
                .is_some_and(|subdivision| !metro.subdivision_ids.contains(subdivision))
            {
                return Err(format!(
                    "city subdivision is outside its metro: {}",
                    city.id
                ));
            }
            for alias in city_terms(city) {
                let normalized = normalize_words(alias);
                require_nonempty(&normalized, "city alias")?;
                let key = format!(
                    "{}:{}:{normalized}",
                    metro.country_code,
                    city.subdivision_id.as_deref().unwrap_or("*")
                );
                if let Some(existing) = city_aliases.insert(key, city.id.as_str()) {
                    if existing != city.id {
                        return Err(format!(
                            "city alias is shared by {existing} and {}",
                            city.id
                        ));
                    }
                }
            }
        }
    }

    Ok(())
}

fn require_nonempty(value: &str, field: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{field} must not be empty"))
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TargetRoleResolution {
    Known {
        raw: String,
        normalized: String,
        role_id: String,
        label: String,
        family_id: String,
    },
    Ambiguous {
        raw: String,
        normalized: String,
        candidate_role_ids: Vec<String>,
        reason: String,
    },
    CustomReview {
        raw: String,
        normalized: String,
        reason: TargetRoleReviewReason,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetRoleReviewReason {
    Empty,
    UnknownAlias,
    RegistryUnavailable,
}

pub fn resolve_target_role(raw: &str) -> TargetRoleResolution {
    let normalized = normalize_target_role_phrase(raw);
    if normalized.is_empty() {
        return TargetRoleResolution::CustomReview {
            raw: raw.to_string(),
            normalized,
            reason: TargetRoleReviewReason::Empty,
        };
    }

    let registry = match registry() {
        Ok(registry) => registry,
        Err(_) => {
            return TargetRoleResolution::CustomReview {
                raw: raw.to_string(),
                normalized,
                reason: TargetRoleReviewReason::RegistryUnavailable,
            };
        }
    };

    if let Some(ambiguous) = registry
        .ambiguous_role_aliases
        .iter()
        .find(|alias| normalize_target_role_phrase(&alias.alias) == normalized)
    {
        return TargetRoleResolution::Ambiguous {
            raw: raw.to_string(),
            normalized,
            candidate_role_ids: ambiguous.candidate_role_ids.clone(),
            reason: ambiguous.reason.clone(),
        };
    }

    let mut candidates = registry.roles.iter().filter(|role| {
        role_terms(role).any(|alias| normalize_target_role_phrase(alias) == normalized)
    });
    let role = match (candidates.next(), candidates.next()) {
        (Some(role), None) => role,
        _ => {
            return TargetRoleResolution::CustomReview {
                raw: raw.to_string(),
                normalized,
                reason: TargetRoleReviewReason::UnknownAlias,
            };
        }
    };
    TargetRoleResolution::Known {
        raw: raw.to_string(),
        normalized,
        role_id: role.id.clone(),
        label: role.label.clone(),
        family_id: role.family_id.clone(),
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PostingRoleClassification {
    Known {
        raw: String,
        normalized: String,
        family_id: String,
        matched_role_ids: Vec<String>,
    },
    Ambiguous {
        raw: String,
        normalized: String,
        candidate_family_ids: Vec<String>,
        matched_role_ids: Vec<String>,
    },
    Unknown {
        raw: String,
        normalized: String,
        reason: PostingRoleUnknownReason,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PostingRoleUnknownReason {
    Empty,
    NoKnownFamily,
    RegistryUnavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PostingRoleSpanMatch {
    start: usize,
    end: usize,
    role_ids: Vec<String>,
}

pub fn classify_posting_role(raw: &str) -> PostingRoleClassification {
    let normalized = normalize_words(raw);
    if normalized.is_empty() {
        return PostingRoleClassification::Unknown {
            raw: raw.to_string(),
            normalized,
            reason: PostingRoleUnknownReason::Empty,
        };
    }
    let registry = match registry() {
        Ok(registry) => registry,
        Err(_) => {
            return PostingRoleClassification::Unknown {
                raw: raw.to_string(),
                normalized,
                reason: PostingRoleUnknownReason::RegistryUnavailable,
            };
        }
    };

    let mut span_matches = Vec::new();
    for role in &registry.roles {
        for alias in role_terms(role) {
            let normalized_alias = normalize_words(alias);
            // Two-letter aliases such as `QA` and `DE` are useful when the
            // complete posting title is that alias, but are too weak to act as
            // arbitrary title spans (`QA Coordinator`, `DE&I Specialist`). A
            // longer registered alias such as `QA Engineer` remains eligible.
            if normalized_alias.split_whitespace().count() == 1
                && normalized_alias.chars().count() <= 2
                && normalized != normalized_alias
            {
                continue;
            }
            for (start, end) in normalized_phrase_spans(&normalized, &normalized_alias) {
                span_matches.push(PostingRoleSpanMatch {
                    start,
                    end,
                    role_ids: vec![role.id.clone()],
                });
            }
        }
    }
    for ambiguous in &registry.ambiguous_role_aliases {
        let normalized_alias = normalize_words(&ambiguous.alias);
        for (start, end) in normalized_phrase_spans(&normalized, &normalized_alias) {
            span_matches.push(PostingRoleSpanMatch {
                start,
                end,
                role_ids: ambiguous.candidate_role_ids.clone(),
            });
        }
    }

    let matched_role_ids: BTreeSet<_> = span_matches
        .iter()
        .enumerate()
        .filter(|(candidate_index, candidate)| {
            !span_matches.iter().enumerate().any(|(other_index, other)| {
                candidate_index != &other_index
                    && other.start <= candidate.start
                    && other.end >= candidate.end
                    && (other.start < candidate.start || other.end > candidate.end)
            })
        })
        .flat_map(|(_, matched)| matched.role_ids.iter().cloned())
        .collect();

    let mut family_ids = BTreeSet::new();
    for role_id in &matched_role_ids {
        if let Some(role) = registry.roles.iter().find(|role| role.id == *role_id) {
            family_ids.insert(role.family_id.clone());
        }
    }
    let matched_role_ids: Vec<_> = matched_role_ids.into_iter().collect();
    if family_ids.len() == 1 {
        let family_id = family_ids.into_iter().next().unwrap_or_default();
        PostingRoleClassification::Known {
            raw: raw.to_string(),
            normalized,
            family_id,
            matched_role_ids,
        }
    } else if family_ids.len() > 1 {
        PostingRoleClassification::Ambiguous {
            raw: raw.to_string(),
            normalized,
            candidate_family_ids: family_ids.into_iter().collect(),
            matched_role_ids,
        }
    } else {
        PostingRoleClassification::Unknown {
            raw: raw.to_string(),
            normalized,
            reason: PostingRoleUnknownReason::NoKnownFamily,
        }
    }
}

fn normalized_phrase_spans(haystack: &str, needle: &str) -> Vec<(usize, usize)> {
    if needle.is_empty() {
        return Vec::new();
    }
    haystack
        .match_indices(needle)
        .filter_map(|(start, _)| {
            let end = start + needle.len();
            let before = haystack[..start].chars().next_back();
            let after = haystack[end..].chars().next();
            (!before.is_some_and(char::is_alphanumeric)
                && !after.is_some_and(char::is_alphanumeric))
            .then_some((start, end))
        })
        .collect()
}

fn role_terms(role: &RoleEntry) -> impl Iterator<Item = &str> {
    std::iter::once(role.id.as_str())
        .chain(std::iter::once(role.label.as_str()))
        .chain(role.aliases.iter().map(String::as_str))
}

fn normalize_target_role_phrase(value: &str) -> String {
    let normalized = normalize_words(value);
    let mut words: Vec<_> = normalized.split_whitespace().collect();
    loop {
        if words.starts_with(&["entry", "level"]) || words.starts_with(&["mid", "level"]) {
            words.drain(..2);
        } else if words.first().is_some_and(|word| {
            matches!(
                *word,
                "junior" | "jr" | "senior" | "sr" | "staff" | "lead" | "principal"
            )
        }) {
            words.remove(0);
        } else {
            break;
        }
    }
    if words
        .last()
        .is_some_and(|word| matches!(*word, "i" | "ii" | "iii" | "iv" | "1" | "2" | "3" | "4"))
    {
        words.pop();
    }
    words.join(" ")
}

fn normalize_words(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    let mut pending_space = false;
    for character in value.chars().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() {
            if pending_space && !normalized.is_empty() {
                normalized.push(' ');
            }
            normalized.push(character);
            pending_space = false;
        } else {
            pending_space = true;
        }
    }
    normalized
}

fn contains_normalized_phrase(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let padded_haystack = format!(" {haystack} ");
    let padded_needle = format!(" {needle} ");
    padded_haystack.contains(&padded_needle)
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SkillAliasResolution {
    Known {
        raw: String,
        skill_id: String,
        label: String,
    },
    Ambiguous {
        raw: String,
        candidate_skill_ids: Vec<String>,
    },
    Unknown {
        raw: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SkillMatch {
    pub skill_id: String,
    pub label: String,
    pub matched_text: String,
    pub byte_start: usize,
    pub byte_end: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SkillTextClassification {
    pub raw: String,
    pub matches: Vec<SkillMatch>,
}

pub fn resolve_skill_alias(raw: &str) -> SkillAliasResolution {
    let registry = match registry() {
        Ok(registry) => registry,
        Err(_) => {
            return SkillAliasResolution::Unknown {
                raw: raw.to_string(),
            }
        }
    };
    let needle = raw.trim();
    let mut matches = BTreeSet::new();
    for skill in &registry.skills {
        if skill_terms(skill).any(|alias| alias.eq_ignore_ascii_case(needle)) {
            matches.insert(skill.id.clone());
        }
    }
    if matches.len() == 1 {
        let skill_id = matches.into_iter().next().unwrap_or_default();
        let skill = registry.skills.iter().find(|skill| skill.id == skill_id);
        match skill {
            Some(skill) => SkillAliasResolution::Known {
                raw: raw.to_string(),
                skill_id: skill.id.clone(),
                label: skill.label.clone(),
            },
            None => SkillAliasResolution::Unknown {
                raw: raw.to_string(),
            },
        }
    } else if matches.len() > 1 {
        SkillAliasResolution::Ambiguous {
            raw: raw.to_string(),
            candidate_skill_ids: matches.into_iter().collect(),
        }
    } else {
        SkillAliasResolution::Unknown {
            raw: raw.to_string(),
        }
    }
}

pub fn classify_skills_in_text(raw: &str) -> SkillTextClassification {
    let registry = match registry() {
        Ok(registry) => registry,
        Err(_) => {
            return SkillTextClassification {
                raw: raw.to_string(),
                matches: Vec::new(),
            };
        }
    };
    let lowered = raw.to_ascii_lowercase();
    let mut candidates = Vec::new();
    for skill in &registry.skills {
        let mut seen_aliases = HashSet::new();
        for alias in skill_terms(skill) {
            let needle = alias.trim().to_ascii_lowercase();
            if needle.is_empty()
                || !skill_term_is_safe_in_free_text(&skill.id, &needle)
                || !seen_aliases.insert(needle.clone())
            {
                continue;
            }
            for (start, _) in lowered.match_indices(&needle) {
                let end = start + needle.len();
                if has_skill_boundaries(&lowered, start, end) {
                    candidates.push(RawSkillMatch {
                        skill_id: skill.id.as_str(),
                        label: skill.label.as_str(),
                        start,
                        end,
                    });
                }
            }
        }
    }

    candidates.sort_by(|left, right| {
        (right.end - right.start)
            .cmp(&(left.end - left.start))
            .then_with(|| left.start.cmp(&right.start))
            .then_with(|| left.skill_id.cmp(right.skill_id))
    });
    let mut accepted: Vec<RawSkillMatch<'_>> = Vec::new();
    for candidate in candidates {
        if accepted
            .iter()
            .all(|accepted| !ranges_overlap(&candidate, accepted))
        {
            accepted.push(candidate);
        }
    }
    accepted.sort_by(|left, right| {
        left.start
            .cmp(&right.start)
            .then_with(|| left.skill_id.cmp(right.skill_id))
    });

    let mut emitted_skill_ids = HashSet::new();
    let matches = accepted
        .into_iter()
        .filter(|candidate| emitted_skill_ids.insert(candidate.skill_id))
        .map(|candidate| SkillMatch {
            skill_id: candidate.skill_id.to_string(),
            label: candidate.label.to_string(),
            matched_text: raw
                .get(candidate.start..candidate.end)
                .unwrap_or_default()
                .to_string(),
            byte_start: candidate.start,
            byte_end: candidate.end,
        })
        .collect();
    SkillTextClassification {
        raw: raw.to_string(),
        matches,
    }
}

pub fn skill_matches_text(text: &str, skill: &str) -> bool {
    let skill_id = match resolve_skill_alias(skill) {
        SkillAliasResolution::Known { skill_id, .. } => skill_id,
        SkillAliasResolution::Ambiguous { .. } | SkillAliasResolution::Unknown { .. } => {
            return false;
        }
    };
    classify_skills_in_text(text)
        .matches
        .iter()
        .any(|matched| matched.skill_id == skill_id)
}

fn skill_terms(skill: &SkillEntry) -> impl Iterator<Item = &str> {
    std::iter::once(skill.id.as_str())
        .chain(std::iter::once(skill.label.as_str()))
        .chain(skill.aliases.iter().map(String::as_str))
}

fn skill_term_is_safe_in_free_text(skill_id: &str, normalized_term: &str) -> bool {
    !matches!(skill_id, "go" | "r" | "c") || normalized_term != skill_id
}

#[derive(Clone, Copy)]
struct RawSkillMatch<'a> {
    skill_id: &'a str,
    label: &'a str,
    start: usize,
    end: usize,
}

fn has_skill_boundaries(value: &str, start: usize, end: usize) -> bool {
    let before = value[..start].chars().next_back();
    let after = value[end..].chars().next();
    !before.is_some_and(is_skill_word_character) && !after.is_some_and(is_skill_word_character)
}

fn is_skill_word_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

fn ranges_overlap(left: &RawSkillMatch<'_>, right: &RawSkillMatch<'_>) -> bool {
    left.start < right.end && right.start < left.end
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct GeographyInput {
    pub remote: Option<bool>,
    pub country: Option<String>,
    pub subdivision: Option<String>,
    pub metro: Option<String>,
    pub city: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkplaceKind {
    Remote,
    Hybrid,
    Onsite,
}

impl WorkplaceKind {
    fn from_legacy_remote(remote: Option<bool>) -> Option<Self> {
        remote.map(|remote| if remote { Self::Remote } else { Self::Onsite })
    }

    fn legacy_remote(self) -> Option<bool> {
        match self {
            Self::Remote => Some(true),
            Self::Hybrid => None,
            Self::Onsite => Some(false),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkplaceUnknownReason {
    EmptyEvidence,
    UnsupportedEvidence,
    NegatedEvidence,
    MixedEvidence,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum WorkplaceClassification {
    Known {
        raw_workplace: String,
        raw_location: String,
        kind: WorkplaceKind,
    },
    Unknown {
        raw_workplace: String,
        raw_location: String,
        reason: WorkplaceUnknownReason,
    },
}

impl WorkplaceClassification {
    pub fn kind(&self) -> Option<WorkplaceKind> {
        match self {
            Self::Known { kind, .. } => Some(*kind),
            Self::Unknown { .. } => None,
        }
    }
}

pub fn classify_workplace(raw: &str) -> WorkplaceClassification {
    classify_posting_workplace(raw, "")
}

pub fn classify_posting_workplace(
    workplace_raw: &str,
    location_raw: &str,
) -> WorkplaceClassification {
    let unknown = |reason| WorkplaceClassification::Unknown {
        raw_workplace: workplace_raw.to_string(),
        raw_location: location_raw.to_string(),
        reason,
    };
    if workplace_raw.trim().is_empty() && location_raw.trim().is_empty() {
        return unknown(WorkplaceUnknownReason::EmptyEvidence);
    }
    let normalized_workplace = normalize_words(workplace_raw);
    if matches!(
        normalized_workplace.as_str(),
        "unknown" | "mixed" | "unverified"
    ) {
        return unknown(WorkplaceUnknownReason::UnsupportedEvidence);
    }

    let normalized = normalize_words(&format!("{workplace_raw} {location_raw}"));
    if workplace_has_negated_evidence(&normalized) {
        return unknown(WorkplaceUnknownReason::NegatedEvidence);
    }

    let remote = workplace_remote_phrases()
        .iter()
        .any(|phrase| contains_normalized_phrase(&normalized, phrase));
    let hybrid = contains_normalized_phrase(&normalized, "hybrid");
    let onsite = workplace_onsite_phrases()
        .iter()
        .any(|phrase| contains_normalized_phrase(&normalized, phrase))
        || normalized == "office";
    let signal_count = usize::from(remote) + usize::from(hybrid) + usize::from(onsite);
    if signal_count > 1 {
        return unknown(WorkplaceUnknownReason::MixedEvidence);
    }
    let kind = if remote {
        WorkplaceKind::Remote
    } else if hybrid {
        WorkplaceKind::Hybrid
    } else if onsite {
        WorkplaceKind::Onsite
    } else {
        return unknown(WorkplaceUnknownReason::UnsupportedEvidence);
    };
    WorkplaceClassification::Known {
        raw_workplace: workplace_raw.to_string(),
        raw_location: location_raw.to_string(),
        kind,
    }
}

fn workplace_remote_phrases() -> &'static [&'static str] {
    &["remote", "work from home", "wfh"]
}

fn workplace_onsite_phrases() -> &'static [&'static str] {
    &["onsite", "on site", "in office", "office based"]
}

fn workplace_markers() -> impl Iterator<Item = &'static str> {
    workplace_remote_phrases()
        .iter()
        .copied()
        .chain(std::iter::once("hybrid"))
        .chain(workplace_onsite_phrases().iter().copied())
        .chain(std::iter::once("office"))
}

fn workplace_has_negated_evidence(normalized: &str) -> bool {
    workplace_markers().any(|marker| workplace_marker_is_negated(normalized, marker))
}

fn workplace_marker_is_negated(normalized: &str, marker: &str) -> bool {
    [
        "no",
        "not",
        "not a",
        "not an",
        "not currently",
        "not currently a",
        "not currently an",
        "no longer",
        "not fully",
        "not completely",
        "not entirely",
        "not exclusively",
        "not strictly",
        "not 100",
        "not 100 percent",
        "non",
        "without",
        "cannot",
        "can not",
        "cannot be",
        "can not be",
        "do not allow",
        "does not allow",
        "do not permit",
        "does not permit",
        "not eligible for",
    ]
    .iter()
    .any(|prefix| contains_normalized_phrase(normalized, &format!("{prefix} {marker}")))
        || [
            "no",
            "not",
            "false",
            "none",
            "disabled",
            "unavailable",
            "currently unavailable",
            "temporarily unavailable",
            "not available",
            "not offered",
            "not permitted",
            "not supported",
            "not eligible",
            "not possible",
            "prohibited",
            "disallowed",
            "excluded",
            "is disabled",
            "is unavailable",
            "is not available",
            "is not currently available",
            "is not offered",
            "is not currently offered",
            "is not permitted",
            "is not currently permitted",
            "is not supported",
            "is not currently supported",
            "is not eligible",
            "is not currently eligible",
            "is not possible",
            "is prohibited",
            "is disallowed",
            "is excluded",
            "are not available",
            "are not currently available",
            "are not offered",
            "are not currently offered",
            "are not permitted",
            "are not currently permitted",
            "are not supported",
            "are not currently supported",
            "are not eligible",
            "are not currently eligible",
            "are not possible",
            "work unavailable",
            "work not available",
            "option unavailable",
            "option not available",
        ]
        .iter()
        .any(|suffix| contains_normalized_phrase(normalized, &format!("{marker} {suffix}")))
        || ["roles", "positions", "jobs", "candidates", "options"]
            .iter()
            .any(|subject| {
                [
                    "unavailable",
                    "currently unavailable",
                    "temporarily unavailable",
                    "not available",
                    "not offered",
                    "not permitted",
                    "not supported",
                    "not eligible",
                    "prohibited",
                    "disallowed",
                    "excluded",
                    "are unavailable",
                    "are not available",
                    "are not currently available",
                    "are not offered",
                    "are not currently offered",
                    "are not permitted",
                    "are not currently permitted",
                    "are not supported",
                    "are not currently supported",
                    "are not eligible",
                    "are not currently eligible",
                    "are prohibited",
                    "are disallowed",
                    "are excluded",
                ]
                .iter()
                .any(|suffix| {
                    contains_normalized_phrase(normalized, &format!("{marker} {subject} {suffix}"))
                })
            })
}

fn contains_workplace_evidence(normalized: &str) -> bool {
    workplace_markers().any(|marker| contains_normalized_phrase(normalized, marker))
}

fn has_unclassified_workplace_location_words(normalized: &str) -> bool {
    normalized.split_whitespace().any(|word| {
        !matches!(
            word,
            "remote"
                | "hybrid"
                | "onsite"
                | "on"
                | "site"
                | "in"
                | "office"
                | "based"
                | "work"
                | "from"
                | "home"
                | "wfh"
                | "fully"
                | "completely"
                | "entirely"
                | "exclusively"
                | "strictly"
                | "only"
                | "100"
                | "percent"
                | "role"
                | "roles"
                | "position"
                | "positions"
                | "job"
                | "jobs"
                | "workplace"
                | "option"
                | "options"
        )
    })
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct NormalizedGeography {
    pub workplace: Option<WorkplaceKind>,
    pub remote: Option<bool>,
    pub country_code: Option<String>,
    pub subdivision_id: Option<String>,
    pub metro_id: Option<String>,
    pub city_id: Option<String>,
    pub city: Option<String>,
}

impl NormalizedGeography {
    pub fn canonical_ids(&self) -> Vec<String> {
        let mut ids = Vec::new();
        match self
            .workplace
            .or_else(|| WorkplaceKind::from_legacy_remote(self.remote))
        {
            Some(WorkplaceKind::Remote) => ids.push("workplace:remote".to_string()),
            Some(WorkplaceKind::Hybrid) => ids.push("workplace:hybrid".to_string()),
            Some(WorkplaceKind::Onsite) => ids.push("workplace:onsite".to_string()),
            None => {}
        }
        if let Some(country) = &self.country_code {
            ids.push(format!("country:{country}"));
        }
        if let Some(subdivision) = &self.subdivision_id {
            ids.push(format!("subdivision:{subdivision}"));
        }
        if let Some(metro) = &self.metro_id {
            ids.push(format!("metro:{metro}"));
        }
        if let Some(city_id) = &self.city_id {
            ids.push(format!("city:{city_id}"));
            return ids;
        }
        if let Some(city) = &self.city {
            let scope = self
                .subdivision_id
                .as_deref()
                .or(self.country_code.as_deref())
                .unwrap_or("unknown");
            ids.push(format!("city:{scope}:{city}"));
        }
        ids
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GeographyField {
    Location,
    Country,
    Subdivision,
    Metro,
    City,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GeographyUnknownReason {
    EmptyInput,
    EmptyField,
    UnknownCountry,
    UnknownSubdivision,
    UnknownMetro,
    UnknownCity,
    CountrySubdivisionConflict,
    MetroContextConflict,
    CityMetroConflict,
    ExcludedJurisdiction,
    RegistryUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GeographyAmbiguityReason {
    MultipleCountries,
    MultipleSubdivisions,
    MultipleMetros,
    MultipleCities,
    CountryOrSubdivision,
    CityOrJurisdiction,
    MissingCityContext,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum GeographyClassification {
    Known {
        raw: String,
        input: GeographyInput,
        normalized: NormalizedGeography,
    },
    Ambiguous {
        raw: String,
        input: GeographyInput,
        field: GeographyField,
        candidates: Vec<String>,
        reason: GeographyAmbiguityReason,
    },
    Unknown {
        raw: String,
        input: GeographyInput,
        field: Option<GeographyField>,
        reason: GeographyUnknownReason,
    },
}

pub fn normalize_geography(raw: &str) -> GeographyClassification {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return geography_unknown(
            raw,
            GeographyInput::default(),
            None,
            GeographyUnknownReason::EmptyInput,
        );
    }
    let registry = match registry() {
        Ok(registry) => registry,
        Err(_) => {
            return geography_unknown(
                raw,
                GeographyInput::default(),
                None,
                GeographyUnknownReason::RegistryUnavailable,
            );
        }
    };

    let normalized_raw = normalize_words(trimmed);
    let workplace = classify_workplace(trimmed).kind();
    let remote = workplace.and_then(WorkplaceKind::legacy_remote);
    if geography_has_relational_exclusion(registry, &normalized_raw) {
        return geography_unknown(
            raw,
            GeographyInput {
                remote,
                ..GeographyInput::default()
            },
            Some(GeographyField::Location),
            GeographyUnknownReason::ExcludedJurisdiction,
        );
    }
    let segments: Vec<_> = trimmed
        .split([',', '|', '/', ';'])
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect();
    let segment_words: Vec<_> = segments
        .iter()
        .map(|segment| normalize_words(segment))
        .collect();

    let metro_candidates = metro_mentions(registry, &normalized_raw, &segment_words);
    if metro_candidates.len() > 1 {
        return geography_ambiguous(
            raw,
            GeographyInput {
                remote,
                metro: Some(trimmed.to_string()),
                ..GeographyInput::default()
            },
            GeographyField::Metro,
            metro_candidates
                .iter()
                .map(|metro| metro.id.clone())
                .collect(),
            GeographyAmbiguityReason::MultipleMetros,
        );
    }
    if let Some(metro) = metro_candidates.first() {
        return normalize_explicit_metro(
            raw, trimmed, remote, workplace, &segments, metro, registry,
        );
    }

    let mut country_candidates =
        country_mentions(registry, trimmed, &normalized_raw, &segment_words);
    let mut subdivision_candidates =
        subdivision_mentions(registry, trimmed, &normalized_raw, &segment_words);

    if segments.len() == 2 {
        if let Some(tail) = segment_words.last() {
            let postal_subdivisions: Vec<_> = subdivision_candidates
                .iter()
                .filter(|subdivision| normalize_words(&subdivision.code) == *tail)
                .cloned()
                .collect();
            let city_segment = segments.first().copied().unwrap_or_default();
            let postal_has_city_context = postal_subdivisions.iter().any(|subdivision| {
                !exact_cities(
                    registry,
                    city_segment,
                    Some(&subdivision.country_code),
                    Some(&subdivision_id(subdivision)),
                )
                .is_empty()
            });
            if postal_subdivisions.len() == 1 && postal_has_city_context {
                subdivision_candidates = postal_subdivisions;
                country_candidates.retain(|country| normalize_words(&country.code) != *tail);
            }
        }
    }

    let country_ids: BTreeSet<_> = country_candidates
        .iter()
        .map(|country| country.code.clone())
        .collect();
    let subdivision_ids: BTreeSet<_> = subdivision_candidates
        .iter()
        .map(|subdivision| subdivision_id(subdivision))
        .collect();

    if segments.len() == 2
        && segment_words
            .first()
            .is_some_and(|segment| contains_workplace_evidence(segment))
        && !country_ids.is_empty()
        && !subdivision_ids.is_empty()
    {
        return geography_ambiguous(
            raw,
            GeographyInput {
                remote,
                ..GeographyInput::default()
            },
            GeographyField::Location,
            country_ids
                .iter()
                .map(|code| format!("country:{code}"))
                .chain(subdivision_ids.iter().map(|id| format!("subdivision:{id}")))
                .collect(),
            GeographyAmbiguityReason::CountryOrSubdivision,
        );
    }

    for (city_index, segment) in segments.iter().enumerate() {
        let city_ids: BTreeSet<_> = exact_cities(registry, segment, None, None)
            .iter()
            .map(|city| city.city.id.clone())
            .collect();
        let segment_is_jurisdiction = !exact_countries(registry, segment).is_empty()
            || !exact_subdivisions(registry, segment, None).is_empty();
        let has_independent_context = segments.iter().enumerate().any(|(index, context)| {
            index != city_index
                && (!exact_countries(registry, context).is_empty()
                    || !exact_subdivisions(registry, context, None).is_empty())
        });
        if !city_ids.is_empty() && segment_is_jurisdiction && !has_independent_context {
            return geography_ambiguous(
                raw,
                GeographyInput {
                    remote,
                    city: Some((*segment).to_string()),
                    ..GeographyInput::default()
                },
                GeographyField::Location,
                country_ids
                    .iter()
                    .map(|code| format!("country:{code}"))
                    .chain(subdivision_ids.iter().map(|id| format!("subdivision:{id}")))
                    .chain(city_ids.iter().map(|id| format!("city:{id}")))
                    .collect(),
                GeographyAmbiguityReason::CityOrJurisdiction,
            );
        }
    }

    if segments.len() == 1 && country_ids.len() == 1 && subdivision_ids.len() == 1 {
        let country_code = country_ids.iter().next();
        let subdivision_country = subdivision_candidates
            .first()
            .map(|subdivision| &subdivision.country_code);
        if country_code != subdivision_country {
            return geography_ambiguous(
                raw,
                GeographyInput {
                    remote,
                    country: None,
                    subdivision: None,
                    metro: None,
                    city: None,
                },
                GeographyField::Location,
                country_ids
                    .iter()
                    .map(|code| format!("country:{code}"))
                    .chain(subdivision_ids.iter().map(|id| format!("subdivision:{id}")))
                    .collect(),
                GeographyAmbiguityReason::CountryOrSubdivision,
            );
        }
    }
    if country_ids.len() > 1 {
        return geography_ambiguous(
            raw,
            GeographyInput {
                remote,
                ..GeographyInput::default()
            },
            GeographyField::Country,
            country_ids.into_iter().collect(),
            GeographyAmbiguityReason::MultipleCountries,
        );
    }
    if subdivision_ids.len() > 1 {
        return geography_ambiguous(
            raw,
            GeographyInput {
                remote,
                ..GeographyInput::default()
            },
            GeographyField::Subdivision,
            subdivision_ids.into_iter().collect(),
            GeographyAmbiguityReason::MultipleSubdivisions,
        );
    }

    let country = country_ids.into_iter().next();
    let subdivision = subdivision_candidates.first().copied();
    if let (Some(country), Some(subdivision)) = (&country, subdivision) {
        if country != &subdivision.country_code {
            return geography_unknown(
                raw,
                GeographyInput {
                    remote,
                    country: Some(country.clone()),
                    subdivision: Some(subdivision_id(subdivision)),
                    metro: None,
                    city: None,
                },
                Some(GeographyField::Subdivision),
                GeographyUnknownReason::CountrySubdivisionConflict,
            );
        }
    }
    let country = country.or_else(|| subdivision.map(|value| value.country_code.clone()));
    let subdivision_context = subdivision.map(subdivision_id);
    let city = raw_city_segment(
        &segments,
        &segment_words,
        workplace.is_some(),
        !country_candidates.is_empty(),
        !subdivision_candidates.is_empty(),
        registry,
    );
    if workplace.is_some()
        && country.is_none()
        && subdivision.is_none()
        && city.is_none()
        && has_unclassified_workplace_location_words(&normalized_raw)
    {
        return geography_unknown(
            raw,
            GeographyInput {
                remote,
                ..GeographyInput::default()
            },
            Some(GeographyField::Country),
            GeographyUnknownReason::UnknownCountry,
        );
    }
    if city.is_some() && country.is_none() && subdivision.is_none() {
        return geography_ambiguous(
            raw,
            GeographyInput {
                remote,
                country: None,
                subdivision: None,
                metro: None,
                city,
            },
            GeographyField::City,
            Vec::new(),
            GeographyAmbiguityReason::MissingCityContext,
        );
    }
    if workplace.is_none() && country.is_none() && subdivision.is_none() && city.is_none() {
        return geography_unknown(
            raw,
            GeographyInput::default(),
            None,
            GeographyUnknownReason::UnknownCountry,
        );
    }

    let resolved_city = match city.as_deref() {
        Some(city) => {
            let candidates = exact_cities(
                registry,
                city,
                country.as_deref(),
                subdivision_context.as_deref(),
            );
            match candidates.as_slice() {
                [] => {
                    return geography_unknown(
                        raw,
                        GeographyInput {
                            remote,
                            country,
                            subdivision: subdivision.map(subdivision_id),
                            metro: None,
                            city: Some(city.to_string()),
                        },
                        Some(GeographyField::City),
                        GeographyUnknownReason::UnknownCity,
                    );
                }
                [city] => Some(*city),
                cities => {
                    return geography_ambiguous(
                        raw,
                        GeographyInput {
                            remote,
                            country,
                            subdivision: subdivision.map(subdivision_id),
                            metro: None,
                            city: Some(city.to_string()),
                        },
                        GeographyField::City,
                        cities.iter().map(|city| city.city.id.clone()).collect(),
                        GeographyAmbiguityReason::MultipleCities,
                    );
                }
            }
        }
        None => None,
    };
    let normalized_subdivision = subdivision
        .map(subdivision_id)
        .or_else(|| resolved_city.and_then(|city| city.city.subdivision_id.clone()));
    let normalized_city = resolved_city.map(|city| normalize_words(&city.city.label));
    GeographyClassification::Known {
        raw: raw.to_string(),
        input: GeographyInput {
            remote,
            country: country.clone(),
            subdivision: normalized_subdivision.clone(),
            metro: None,
            city,
        },
        normalized: NormalizedGeography {
            workplace,
            remote,
            country_code: country,
            subdivision_id: normalized_subdivision,
            metro_id: resolved_city.map(|city| city.metro.id.clone()),
            city_id: resolved_city.map(|city| city.city.id.clone()),
            city: normalized_city,
        },
    }
}

pub fn normalize_geography_input(input: GeographyInput) -> GeographyClassification {
    let raw = geography_input_raw(&input);
    let registry = match registry() {
        Ok(registry) => registry,
        Err(_) => {
            return geography_unknown(
                &raw,
                input.clone(),
                None,
                GeographyUnknownReason::RegistryUnavailable,
            );
        }
    };
    if input.remote.is_none()
        && input.country.is_none()
        && input.subdivision.is_none()
        && input.metro.is_none()
        && input.city.is_none()
    {
        return geography_unknown(
            &raw,
            input.clone(),
            None,
            GeographyUnknownReason::EmptyInput,
        );
    }

    let mut country = match input.country.as_deref() {
        Some(value) if value.trim().is_empty() => {
            return geography_unknown(
                &raw,
                input.clone(),
                Some(GeographyField::Country),
                GeographyUnknownReason::EmptyField,
            );
        }
        Some(value) => match exact_countries(registry, value).as_slice() {
            [] => {
                return geography_unknown(
                    &raw,
                    input.clone(),
                    Some(GeographyField::Country),
                    GeographyUnknownReason::UnknownCountry,
                );
            }
            [country] => Some(country.code.clone()),
            countries => {
                return geography_ambiguous(
                    &raw,
                    input.clone(),
                    GeographyField::Country,
                    countries
                        .iter()
                        .map(|country| country.code.clone())
                        .collect(),
                    GeographyAmbiguityReason::MultipleCountries,
                );
            }
        },
        None => None,
    };

    let subdivision = match input.subdivision.as_deref() {
        Some(value) if value.trim().is_empty() => {
            return geography_unknown(
                &raw,
                input.clone(),
                Some(GeographyField::Subdivision),
                GeographyUnknownReason::EmptyField,
            );
        }
        Some(value) => {
            let candidates = exact_subdivisions(registry, value, country.as_deref());
            match candidates.as_slice() {
                [] => {
                    let known_without_country = exact_subdivisions(registry, value, None);
                    let reason = if country.is_some() && !known_without_country.is_empty() {
                        GeographyUnknownReason::CountrySubdivisionConflict
                    } else {
                        GeographyUnknownReason::UnknownSubdivision
                    };
                    return geography_unknown(
                        &raw,
                        input.clone(),
                        Some(GeographyField::Subdivision),
                        reason,
                    );
                }
                [subdivision] => Some(*subdivision),
                subdivisions => {
                    return geography_ambiguous(
                        &raw,
                        input.clone(),
                        GeographyField::Subdivision,
                        subdivisions
                            .iter()
                            .map(|subdivision| subdivision_id(subdivision))
                            .collect(),
                        GeographyAmbiguityReason::MultipleSubdivisions,
                    );
                }
            }
        }
        None => None,
    };
    let mut subdivision_id = subdivision.map(subdivision_id);
    country = country.or_else(|| subdivision.map(|value| value.country_code.clone()));

    let explicit_metro = match input.metro.as_deref() {
        Some(value) if value.trim().is_empty() => {
            return geography_unknown(
                &raw,
                input.clone(),
                Some(GeographyField::Metro),
                GeographyUnknownReason::EmptyField,
            );
        }
        Some(value) => {
            let candidates = exact_metros(
                registry,
                value,
                country.as_deref(),
                subdivision_id.as_deref(),
            );
            match candidates.as_slice() {
                [] => {
                    let known_without_context = exact_metros(registry, value, None, None);
                    let reason = if known_without_context.is_empty() {
                        GeographyUnknownReason::UnknownMetro
                    } else {
                        GeographyUnknownReason::MetroContextConflict
                    };
                    return geography_unknown(
                        &raw,
                        input.clone(),
                        Some(GeographyField::Metro),
                        reason,
                    );
                }
                [metro] => Some(*metro),
                metros => {
                    return geography_ambiguous(
                        &raw,
                        input.clone(),
                        GeographyField::Metro,
                        metros.iter().map(|metro| metro.id.clone()).collect(),
                        GeographyAmbiguityReason::MultipleMetros,
                    );
                }
            }
        }
        None => None,
    };
    if let Some(metro) = explicit_metro {
        country = Some(metro.country_code.clone());
        if subdivision_id.is_none() && metro.subdivision_ids.len() == 1 {
            subdivision_id = metro.subdivision_ids.first().cloned();
        }
    }

    let resolved_city = match input.city.as_deref() {
        Some(value) => {
            let normalized = normalize_words(value);
            if normalized.is_empty() {
                return geography_unknown(
                    &raw,
                    input.clone(),
                    Some(GeographyField::City),
                    GeographyUnknownReason::EmptyField,
                );
            }
            if country.is_none() {
                return geography_ambiguous(
                    &raw,
                    input.clone(),
                    GeographyField::City,
                    Vec::new(),
                    GeographyAmbiguityReason::MissingCityContext,
                );
            }
            let candidates = exact_cities(
                registry,
                &normalized,
                country.as_deref(),
                subdivision_id.as_deref(),
            );
            match candidates.as_slice() {
                [] => {
                    return geography_unknown(
                        &raw,
                        input.clone(),
                        Some(GeographyField::City),
                        GeographyUnknownReason::UnknownCity,
                    );
                }
                [city] => Some(*city),
                cities => {
                    return geography_ambiguous(
                        &raw,
                        input.clone(),
                        GeographyField::City,
                        cities.iter().map(|city| city.city.id.clone()).collect(),
                        GeographyAmbiguityReason::MultipleCities,
                    );
                }
            }
        }
        None => None,
    };
    if let (Some(metro), Some(city)) = (explicit_metro, resolved_city) {
        if metro.id != city.metro.id {
            return geography_unknown(
                &raw,
                input.clone(),
                Some(GeographyField::City),
                GeographyUnknownReason::CityMetroConflict,
            );
        }
    }
    if let Some(city) = resolved_city {
        country = Some(city.metro.country_code.clone());
        if subdivision_id.is_none() {
            subdivision_id = city.city.subdivision_id.clone();
        }
    }
    let metro = explicit_metro.or_else(|| resolved_city.map(|city| city.metro));

    GeographyClassification::Known {
        raw,
        input: input.clone(),
        normalized: NormalizedGeography {
            workplace: WorkplaceKind::from_legacy_remote(input.remote),
            remote: input.remote,
            country_code: country,
            subdivision_id,
            metro_id: metro.map(|metro| metro.id.clone()),
            city_id: resolved_city.map(|city| city.city.id.clone()),
            city: resolved_city.map(|city| normalize_words(&city.city.label)),
        },
    }
}

fn geography_input_raw(input: &GeographyInput) -> String {
    let mut parts = Vec::new();
    if let Some(remote) = input.remote {
        parts.push(if remote { "remote" } else { "not remote" }.to_string());
    }
    parts.extend(input.metro.iter().cloned());
    parts.extend(input.city.iter().cloned());
    parts.extend(input.subdivision.iter().cloned());
    parts.extend(input.country.iter().cloned());
    parts.join(", ")
}

fn geography_unknown(
    raw: &str,
    input: GeographyInput,
    field: Option<GeographyField>,
    reason: GeographyUnknownReason,
) -> GeographyClassification {
    GeographyClassification::Unknown {
        raw: raw.to_string(),
        input,
        field,
        reason,
    }
}

fn geography_ambiguous(
    raw: &str,
    input: GeographyInput,
    field: GeographyField,
    candidates: Vec<String>,
    reason: GeographyAmbiguityReason,
) -> GeographyClassification {
    GeographyClassification::Ambiguous {
        raw: raw.to_string(),
        input,
        field,
        candidates,
        reason,
    }
}

fn exact_countries<'a>(
    registry: &'a CanonicalTaxonomyRegistry,
    value: &str,
) -> Vec<&'a CountryEntry> {
    let normalized = normalize_words(value);
    registry
        .countries
        .iter()
        .filter(|country| country_terms(country).any(|alias| normalize_words(alias) == normalized))
        .collect()
}

fn geography_has_relational_exclusion(
    registry: &CanonicalTaxonomyRegistry,
    normalized_raw: &str,
) -> bool {
    let mut terms = BTreeSet::new();
    for country in &registry.countries {
        terms.extend(country_terms(country).map(normalize_words));
    }
    for subdivision in &registry.subdivisions {
        terms.insert(normalize_words(&subdivision_id(subdivision)));
        terms.extend(subdivision_terms(subdivision).map(normalize_words));
    }
    for metro in &registry.metros {
        terms.extend(metro_terms(metro).map(normalize_words));
        for city in &metro.cities {
            terms.extend(city_terms(city).map(normalize_words));
        }
    }
    terms.retain(|term| term.chars().count() >= 2);

    const PREFIXES: &[&str] = &[
        "outside",
        "outside of",
        "outside the",
        "outside of the",
        "except",
        "except in",
        "except within",
        "except the",
        "except for",
        "except for the",
        "excluding",
        "excluding candidates in",
        "excluding roles in",
        "excluding positions in",
        "excluding jobs in",
        "exclude",
        "exclude candidates in",
        "exclude roles in",
        "exclude positions in",
        "exclude jobs in",
        "non",
        "not",
        "not in",
        "not located in",
        "not available in",
        "not offered in",
        "not permitted in",
        "not supported in",
        "not eligible in",
        "unavailable in",
        "prohibited in",
        "disallowed in",
        "anywhere but",
        "anywhere but in",
        "everywhere but",
        "other than",
        "other than in",
        "but not",
        "but not in",
        "apart from",
    ];
    const SUFFIXES: &[&str] = &[
        "excluded",
        "is excluded",
        "not allowed",
        "is not allowed",
        "not available",
        "is not available",
        "not offered",
        "is not offered",
        "not permitted",
        "is not permitted",
        "not supported",
        "is not supported",
        "not eligible",
        "is not eligible",
        "unavailable",
        "is unavailable",
        "prohibited",
        "is prohibited",
        "disallowed",
        "is disallowed",
    ];
    terms.into_iter().any(|term| {
        PREFIXES
            .iter()
            .any(|prefix| contains_normalized_phrase(normalized_raw, &format!("{prefix} {term}")))
            || SUFFIXES.iter().any(|suffix| {
                contains_normalized_phrase(normalized_raw, &format!("{term} {suffix}"))
            })
    })
}

fn exact_subdivisions<'a>(
    registry: &'a CanonicalTaxonomyRegistry,
    value: &str,
    country_code: Option<&str>,
) -> Vec<&'a SubdivisionEntry> {
    let normalized = normalize_words(value);
    registry
        .subdivisions
        .iter()
        .filter(|subdivision| {
            country_code.is_none_or(|country| subdivision.country_code == country)
                && (normalize_words(&subdivision_id(subdivision)) == normalized
                    || subdivision_terms(subdivision)
                        .any(|alias| normalize_words(alias) == normalized))
        })
        .collect()
}

fn exact_metros<'a>(
    registry: &'a CanonicalTaxonomyRegistry,
    value: &str,
    country_code: Option<&str>,
    subdivision_id: Option<&str>,
) -> Vec<&'a MetroEntry> {
    let normalized = normalize_words(value);
    registry
        .metros
        .iter()
        .filter(|metro| {
            country_code.is_none_or(|country| metro.country_code == country)
                && subdivision_id.is_none_or(|subdivision| {
                    metro.subdivision_ids.iter().any(|id| id == subdivision)
                })
                && metro_terms(metro).any(|alias| normalize_words(alias) == normalized)
        })
        .collect()
}

#[derive(Clone, Copy)]
struct ResolvedCity<'a> {
    metro: &'a MetroEntry,
    city: &'a MetroCityEntry,
}

fn exact_cities<'a>(
    registry: &'a CanonicalTaxonomyRegistry,
    value: &str,
    country_code: Option<&str>,
    subdivision_id: Option<&str>,
) -> Vec<ResolvedCity<'a>> {
    let normalized = normalize_words(value);
    registry
        .metros
        .iter()
        .filter(|metro| country_code.is_none_or(|country| metro.country_code == country))
        .flat_map(|metro| {
            metro
                .cities
                .iter()
                .filter(|city| {
                    subdivision_id.is_none_or(|subdivision| {
                        city.subdivision_id.as_deref() == Some(subdivision)
                    }) && city_terms(city).any(|alias| normalize_words(alias) == normalized)
                })
                .map(|city| ResolvedCity { metro, city })
        })
        .collect()
}

fn metro_mentions<'a>(
    registry: &'a CanonicalTaxonomyRegistry,
    normalized_raw: &str,
    segments: &[String],
) -> Vec<&'a MetroEntry> {
    registry
        .metros
        .iter()
        .filter(|metro| {
            metro_terms(metro).any(|alias| {
                let normalized = normalize_words(alias);
                segments.iter().any(|segment| segment == &normalized)
                    || contains_normalized_phrase(normalized_raw, &normalized)
            })
        })
        .collect()
}

fn normalize_explicit_metro(
    raw: &str,
    trimmed: &str,
    remote: Option<bool>,
    workplace: Option<WorkplaceKind>,
    segments: &[&str],
    metro: &MetroEntry,
    registry: &CanonicalTaxonomyRegistry,
) -> GeographyClassification {
    let normalized_raw = normalize_words(raw);
    for country in &registry.countries {
        let named_country = std::iter::once(country.label.as_str())
            .chain(country.aliases.iter().map(String::as_str))
            .map(normalize_words)
            .any(|alias| alias.len() > 2 && contains_normalized_phrase(&normalized_raw, &alias));
        let code_is_subdivision = metro
            .subdivision_ids
            .iter()
            .any(|subdivision| subdivision.ends_with(&format!("-{}", country.code)));
        let conflicting_code = country.code != metro.country_code
            && contains_case_sensitive_token(raw, &country.code)
            && !code_is_subdivision;
        if country.code != metro.country_code && (named_country || conflicting_code) {
            return geography_unknown(
                raw,
                GeographyInput {
                    remote,
                    metro: Some(trimmed.to_string()),
                    ..GeographyInput::default()
                },
                Some(GeographyField::Metro),
                GeographyUnknownReason::MetroContextConflict,
            );
        }
    }
    let mut compatible_subdivisions = BTreeSet::new();
    for segment in segments {
        let normalized = normalize_words(segment);
        let describes_metro = metro_terms(metro)
            .any(|alias| contains_normalized_phrase(&normalized, &normalize_words(alias)));
        if describes_metro || contains_workplace_evidence(&normalized) {
            continue;
        }
        let countries = exact_countries(registry, segment);
        let subdivisions = exact_subdivisions(registry, segment, None);
        let compatible_country = countries
            .iter()
            .any(|country| country.code == metro.country_code);
        let compatible_here: Vec<_> = subdivisions
            .iter()
            .map(|subdivision| subdivision_id(subdivision))
            .filter(|subdivision| metro.subdivision_ids.contains(subdivision))
            .collect();
        if countries.is_empty() && subdivisions.is_empty() {
            return geography_unknown(
                raw,
                GeographyInput {
                    remote,
                    metro: Some(trimmed.to_string()),
                    ..GeographyInput::default()
                },
                Some(GeographyField::Metro),
                GeographyUnknownReason::MetroContextConflict,
            );
        }
        if !compatible_country && compatible_here.is_empty() {
            return geography_unknown(
                raw,
                GeographyInput {
                    remote,
                    metro: Some(trimmed.to_string()),
                    ..GeographyInput::default()
                },
                Some(GeographyField::Metro),
                GeographyUnknownReason::MetroContextConflict,
            );
        }
        compatible_subdivisions.extend(compatible_here);
    }
    if compatible_subdivisions.len() > 1 {
        return geography_ambiguous(
            raw,
            GeographyInput {
                remote,
                metro: Some(trimmed.to_string()),
                ..GeographyInput::default()
            },
            GeographyField::Subdivision,
            compatible_subdivisions.into_iter().collect(),
            GeographyAmbiguityReason::MultipleSubdivisions,
        );
    }
    let subdivision = compatible_subdivisions
        .into_iter()
        .next()
        .or_else(|| (metro.subdivision_ids.len() == 1).then(|| metro.subdivision_ids[0].clone()));
    GeographyClassification::Known {
        raw: raw.to_string(),
        input: GeographyInput {
            remote,
            country: Some(metro.country_code.clone()),
            subdivision: subdivision.clone(),
            metro: Some(trimmed.to_string()),
            city: None,
        },
        normalized: NormalizedGeography {
            workplace,
            remote,
            country_code: Some(metro.country_code.clone()),
            subdivision_id: subdivision,
            metro_id: Some(metro.id.clone()),
            city_id: None,
            city: None,
        },
    }
}

fn country_mentions<'a>(
    registry: &'a CanonicalTaxonomyRegistry,
    raw: &str,
    normalized_raw: &str,
    segments: &[String],
) -> Vec<&'a CountryEntry> {
    registry
        .countries
        .iter()
        .filter(|country| {
            country_terms(country).any(|alias| {
                let normalized = normalize_words(alias);
                segments.iter().any(|segment| segment == &normalized)
                    || (normalized.len() > 2
                        && contains_normalized_phrase(normalized_raw, &normalized))
                    || (alias == country.code && contains_case_sensitive_token(raw, &country.code))
            })
        })
        .collect()
}

fn subdivision_mentions<'a>(
    registry: &'a CanonicalTaxonomyRegistry,
    raw: &str,
    normalized_raw: &str,
    segments: &[String],
) -> Vec<&'a SubdivisionEntry> {
    registry
        .subdivisions
        .iter()
        .filter(|subdivision| {
            subdivision_terms(subdivision).any(|alias| {
                let normalized = normalize_words(alias);
                segments.iter().any(|segment| segment == &normalized)
                    || (normalized.len() > 2
                        && contains_normalized_phrase(normalized_raw, &normalized))
                    || (alias == subdivision.code
                        && contains_case_sensitive_token(raw, &subdivision.code))
            })
        })
        .collect()
}

fn country_terms(country: &CountryEntry) -> impl Iterator<Item = &str> {
    std::iter::once(country.code.as_str())
        .chain(std::iter::once(country.label.as_str()))
        .chain(country.aliases.iter().map(String::as_str))
}

fn subdivision_terms(subdivision: &SubdivisionEntry) -> impl Iterator<Item = &str> {
    std::iter::once(subdivision.code.as_str())
        .chain(std::iter::once(subdivision.label.as_str()))
        .chain(subdivision.aliases.iter().map(String::as_str))
}

fn metro_terms(metro: &MetroEntry) -> impl Iterator<Item = &str> {
    std::iter::once(metro.id.as_str())
        .chain(std::iter::once(metro.label.as_str()))
        .chain(metro.aliases.iter().map(String::as_str))
}

fn city_terms(city: &MetroCityEntry) -> impl Iterator<Item = &str> {
    std::iter::once(city.id.as_str())
        .chain(std::iter::once(city.label.as_str()))
        .chain(city.aliases.iter().map(String::as_str))
}

fn subdivision_id(subdivision: &SubdivisionEntry) -> String {
    format!("{}-{}", subdivision.country_code, subdivision.code)
}

fn contains_case_sensitive_token(value: &str, needle: &str) -> bool {
    value.match_indices(needle).any(|(start, _)| {
        let end = start + needle.len();
        let before = value[..start].chars().next_back();
        let after = value[end..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

fn raw_city_segment(
    segments: &[&str],
    normalized_segments: &[String],
    has_workplace: bool,
    has_country: bool,
    has_subdivision: bool,
    registry: &CanonicalTaxonomyRegistry,
) -> Option<String> {
    if segments.len() == 1 {
        if has_workplace || has_country || has_subdivision {
            return None;
        }
        return normalized_segments.first().cloned();
    }
    if let Some((_, (_, normalized))) =
        segments
            .iter()
            .zip(normalized_segments)
            .enumerate()
            .find(|(city_index, (segment, _))| {
                !exact_cities(registry, segment, None, None).is_empty()
                    && segments.iter().enumerate().any(|(index, context)| {
                        index != *city_index
                            && (!exact_countries(registry, context).is_empty()
                                || !exact_subdivisions(registry, context, None).is_empty())
                    })
            })
    {
        return Some(normalized.clone());
    }
    segments
        .iter()
        .zip(normalized_segments)
        .find(|(segment, _)| {
            exact_countries(registry, segment).is_empty()
                && exact_subdivisions(registry, segment, None).is_empty()
                && !contains_workplace_evidence(&normalize_words(segment))
        })
        .map(|(_, normalized)| normalized.clone())
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GeographyDimension {
    Workplace,
    Country,
    Subdivision,
    Metro,
    City,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GeographyReviewReason {
    AmbiguousPreference,
    AmbiguousPosting,
    UnknownPreference,
    UnknownPosting,
    AmbiguousPostingWorkplace,
    UnknownPostingWorkplace,
    MissingPostingWorkplace,
    MissingPostingCountry,
    MissingPostingSubdivision,
    MissingPostingMetro,
    MissingPostingCity,
    NoAllowedLocations,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum GeographyMatch {
    Proven,
    NotMatched { dimension: GeographyDimension },
    ReviewRequired { reason: GeographyReviewReason },
}

impl GeographyMatch {
    pub fn is_proven_match(&self) -> bool {
        matches!(self, Self::Proven)
    }
}

pub fn match_geography(
    preference: &GeographyClassification,
    posting: &GeographyClassification,
) -> GeographyMatch {
    let preference = match preference {
        GeographyClassification::Known { normalized, .. } => normalized,
        GeographyClassification::Ambiguous { .. } => {
            return GeographyMatch::ReviewRequired {
                reason: GeographyReviewReason::AmbiguousPreference,
            };
        }
        GeographyClassification::Unknown { .. } => {
            return GeographyMatch::ReviewRequired {
                reason: GeographyReviewReason::UnknownPreference,
            };
        }
    };
    let posting = match posting {
        GeographyClassification::Known { normalized, .. } => normalized,
        GeographyClassification::Ambiguous { .. } => {
            return GeographyMatch::ReviewRequired {
                reason: GeographyReviewReason::AmbiguousPosting,
            };
        }
        GeographyClassification::Unknown { .. } => {
            return GeographyMatch::ReviewRequired {
                reason: GeographyReviewReason::UnknownPosting,
            };
        }
    };

    if let Some(expected_workplace) = normalized_workplace(preference) {
        match normalized_workplace(posting) {
            Some(observed) if observed != expected_workplace => {
                return GeographyMatch::NotMatched {
                    dimension: GeographyDimension::Workplace,
                };
            }
            None => {
                return GeographyMatch::ReviewRequired {
                    reason: GeographyReviewReason::MissingPostingWorkplace,
                };
            }
            Some(_) => {}
        }
    }
    if let Err(decision) = match_required_geography_part(
        preference.country_code.as_deref(),
        posting.country_code.as_deref(),
        GeographyDimension::Country,
        GeographyReviewReason::MissingPostingCountry,
    ) {
        return decision;
    }
    if let Err(decision) = match_required_geography_part(
        preference.subdivision_id.as_deref(),
        posting.subdivision_id.as_deref(),
        GeographyDimension::Subdivision,
        GeographyReviewReason::MissingPostingSubdivision,
    ) {
        return decision;
    }
    if preference.city_id.is_some() {
        if let Err(decision) = match_required_geography_part(
            preference.city_id.as_deref(),
            posting.city_id.as_deref(),
            GeographyDimension::City,
            GeographyReviewReason::MissingPostingCity,
        ) {
            return decision;
        }
    } else if preference.metro_id.is_some() {
        if let Err(decision) = match_required_geography_part(
            preference.metro_id.as_deref(),
            posting.metro_id.as_deref(),
            GeographyDimension::Metro,
            GeographyReviewReason::MissingPostingMetro,
        ) {
            return decision;
        }
    } else if let Err(decision) = match_required_geography_part(
        preference.city.as_deref(),
        posting.city.as_deref(),
        GeographyDimension::City,
        GeographyReviewReason::MissingPostingCity,
    ) {
        return decision;
    }
    GeographyMatch::Proven
}

fn normalized_workplace(geography: &NormalizedGeography) -> Option<WorkplaceKind> {
    geography
        .workplace
        .or_else(|| WorkplaceKind::from_legacy_remote(geography.remote))
}

fn match_required_geography_part(
    expected: Option<&str>,
    observed: Option<&str>,
    dimension: GeographyDimension,
    missing_reason: GeographyReviewReason,
) -> Result<(), GeographyMatch> {
    match (expected, observed) {
        (Some(expected), Some(observed)) if expected != observed => {
            Err(GeographyMatch::NotMatched { dimension })
        }
        (Some(_), None) => Err(GeographyMatch::ReviewRequired {
            reason: missing_reason,
        }),
        _ => Ok(()),
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum GeographyMatchDecision {
    Allowed { matched_allowed_raw: String },
    Denied { dimensions: Vec<GeographyDimension> },
    ReviewRequired { reason: GeographyReviewReason },
}

impl GeographyMatchDecision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed { .. })
    }
}

pub fn geography_allows(
    posting_raw: &str,
    allowed_raws: &[String],
    workplace: &str,
) -> GeographyMatchDecision {
    if allowed_raws.is_empty() {
        return GeographyMatchDecision::ReviewRequired {
            reason: GeographyReviewReason::NoAllowedLocations,
        };
    }
    let posting_workplace = classify_posting_workplace(workplace, posting_raw);
    let workplace_kind = match posting_workplace {
        WorkplaceClassification::Known { kind, .. } => kind,
        WorkplaceClassification::Unknown {
            reason: WorkplaceUnknownReason::MixedEvidence,
            ..
        } => {
            return GeographyMatchDecision::ReviewRequired {
                reason: GeographyReviewReason::AmbiguousPostingWorkplace,
            };
        }
        WorkplaceClassification::Unknown { .. } => {
            return GeographyMatchDecision::ReviewRequired {
                reason: GeographyReviewReason::UnknownPostingWorkplace,
            };
        }
    };
    let mut posting = normalize_geography(posting_raw);
    apply_workplace_kind(&mut posting, workplace_kind);
    let mut denied_dimensions = BTreeSet::new();
    let mut first_review = None;
    for allowed_raw in allowed_raws {
        let allowed = normalize_geography(allowed_raw);
        match match_geography(&allowed, &posting) {
            GeographyMatch::Proven => {
                return GeographyMatchDecision::Allowed {
                    matched_allowed_raw: allowed_raw.clone(),
                };
            }
            GeographyMatch::NotMatched { dimension } => {
                denied_dimensions.insert(dimension);
            }
            GeographyMatch::ReviewRequired { reason } => {
                first_review.get_or_insert(reason);
            }
        }
    }
    if let Some(reason) = first_review {
        GeographyMatchDecision::ReviewRequired { reason }
    } else {
        GeographyMatchDecision::Denied {
            dimensions: denied_dimensions.into_iter().collect(),
        }
    }
}

fn apply_workplace_kind(classification: &mut GeographyClassification, workplace: WorkplaceKind) {
    if let GeographyClassification::Known { normalized, .. } = classification {
        normalized.workplace = Some(workplace);
        normalized.remote = workplace.legacy_remote();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_is_bound_and_covers_portal_contract() {
        let registry = registry().expect("the embedded registry must be valid");
        assert_eq!(registry.schema_version, 1);
        assert_eq!(registry.taxonomy_version, taxonomy_version());
        assert_eq!(registry.roles.len(), 49);
        assert_eq!(registry.skills.len(), 44);
        assert_eq!(registry.subdivisions.len(), 64);
        assert_eq!(registry.metros.len(), 38);
        assert_eq!(
            registry
                .metros
                .iter()
                .map(|metro| metro.cities.len())
                .sum::<usize>(),
            44
        );
        assert_eq!(
            hex::encode(Sha256::digest(registry_bytes())),
            taxonomy_sha256()
        );

        let portal = include_str!("../../jobs/portal/src/data/career-suggestions.ts");
        let role_variants = quoted_array(portal, "ROLE_SUGGESTIONS");
        assert_eq!(role_variants.len(), 6);
        for role in role_variants {
            assert!(
                matches!(
                    resolve_target_role(role),
                    TargetRoleResolution::Known { .. }
                ),
                "portal target role lacks server authority: {role}"
            );
        }
        let skills = quoted_array(portal, "SKILL_SUGGESTIONS");
        assert_eq!(skills.len(), 37);
        for skill in skills {
            assert!(
                matches!(
                    resolve_skill_alias(skill),
                    SkillAliasResolution::Known { .. }
                ),
                "portal skill lacks server authority: {skill}"
            );
        }
        let locations = quoted_array(portal, "LOCATION_SUGGESTIONS");
        assert_eq!(locations.len(), 43);
        for location in locations {
            assert!(
                matches!(
                    normalize_geography(location),
                    GeographyClassification::Known { .. }
                ),
                "portal location lacks server authority: {location}"
            );
        }
    }

    #[test]
    fn registry_deserialization_rejects_unknown_fields_at_every_level() {
        let registry_value: Value =
            serde_json::from_slice(registry_bytes()).expect("embedded registry JSON");
        let mut root_with_extra = registry_value.clone();
        root_with_extra
            .as_object_mut()
            .expect("registry object")
            .insert("unsupported".to_string(), Value::Bool(true));
        assert!(serde_json::from_value::<CanonicalTaxonomyRegistry>(root_with_extra).is_err());

        for pointer in [
            "/role_families/0",
            "/roles/0",
            "/ambiguous_role_aliases/0",
            "/skills/0",
            "/countries/0",
            "/subdivisions/0",
            "/metros/0",
            "/metros/0/cities/0",
        ] {
            let mut with_extra = registry_value.clone();
            with_extra
                .pointer_mut(pointer)
                .and_then(Value::as_object_mut)
                .unwrap_or_else(|| panic!("missing registry object at {pointer}"))
                .insert("unsupported".to_string(), Value::Bool(true));
            assert!(
                serde_json::from_value::<CanonicalTaxonomyRegistry>(with_extra).is_err(),
                "registry accepted an unknown field at {pointer}"
            );
        }
    }

    #[test]
    fn registry_validation_includes_role_ids_in_deterministic_alias_collisions() {
        let mut conflicting = registry().expect("validated registry").clone();
        let first_role_id = conflicting.roles[0].id.clone();
        conflicting.roles[1].aliases.push(first_role_id);

        assert!(
            validate_registry(&conflicting)
                .is_err_and(|error| error.contains("role alias is shared")),
            "role IDs must participate in the same deterministic alias authority as labels"
        );
    }

    #[test]
    fn target_role_aliases_are_deterministic_and_preserve_raw_values() {
        let cases = [
            ("SWE", "software-engineer", "software-engineering"),
            ("SDE", "software-engineer", "software-engineering"),
            ("CRA", "clinical-research-associate", "clinical-research"),
            ("CRC", "clinical-research-coordinator", "clinical-research"),
            (
                "Principal Software Engineer III",
                "software-engineer",
                "software-engineering",
            ),
        ];
        for (raw, expected_role, expected_family) in cases {
            assert!(matches!(
                resolve_target_role(raw),
                TargetRoleResolution::Known {
                    raw: observed_raw,
                    role_id,
                    family_id,
                    ..
                } if observed_raw == raw
                    && role_id == expected_role
                    && family_id == expected_family
            ));
        }
    }

    #[test]
    fn pm_and_tpm_require_explicit_target_role_review() {
        assert!(matches!(
            resolve_target_role("PM"),
            TargetRoleResolution::Ambiguous {
                candidate_role_ids,
                ..
            } if candidate_role_ids == [
                "product-manager",
                "project-manager",
                "program-manager"
            ]
        ));
        assert!(matches!(
            resolve_target_role("tpm"),
            TargetRoleResolution::Ambiguous {
                candidate_role_ids,
                ..
            } if candidate_role_ids == [
                "technical-product-manager",
                "technical-program-manager"
            ]
        ));
    }

    #[test]
    fn posting_classification_never_selects_an_ambiguous_target_role() {
        assert!(matches!(
            classify_posting_role("Senior Product Manager"),
            PostingRoleClassification::Known { family_id, .. }
                if family_id == "product-management"
        ));
        assert!(matches!(
            resolve_target_role("PM"),
            TargetRoleResolution::Ambiguous { .. }
        ));
        assert!(matches!(
            classify_posting_role("Clinical Operations Manager"),
            PostingRoleClassification::Known { family_id, .. }
                if family_id == "clinical-research"
        ));
        assert!(matches!(
            classify_posting_role("Healthcare Data Analyst"),
            PostingRoleClassification::Known { family_id, .. }
                if family_id == "healthcare-data"
        ));
        assert!(matches!(
            classify_posting_role("Product Manager / Project Manager"),
            PostingRoleClassification::Ambiguous { .. }
        ));
    }

    #[test]
    fn posting_role_classification_preserves_distinct_spans_and_suppresses_contained_aliases() {
        for (raw, expected_families) in [
            (
                "PM / Software Engineer",
                vec![
                    "product-management",
                    "program-management",
                    "project-management",
                    "software-engineering",
                ],
            ),
            (
                "Software Engineer - PM",
                vec![
                    "product-management",
                    "program-management",
                    "project-management",
                    "software-engineering",
                ],
            ),
            (
                "Recruiter / Software Engineer",
                vec!["people-and-talent", "software-engineering"],
            ),
            (
                "Technical Product Manager / Recruiter",
                vec!["people-and-talent", "product-management"],
            ),
            (
                "TPM / Software Engineer",
                vec![
                    "product-management",
                    "program-management",
                    "software-engineering",
                ],
            ),
        ] {
            assert!(
                matches!(
                    classify_posting_role(raw),
                    PostingRoleClassification::Ambiguous {
                        candidate_family_ids,
                        ..
                    } if candidate_family_ids == expected_families
                ),
                "distinct title spans must remain ambiguous: {raw}"
            );
        }

        for (raw, expected_family, expected_roles) in [
            (
                "Technical Product Manager",
                "product-management",
                vec!["technical-product-manager"],
            ),
            (
                "Technical Program Manager",
                "program-management",
                vec!["technical-program-manager"],
            ),
            (
                "Growth Marketing Manager",
                "marketing-and-growth",
                vec!["growth-marketing-manager"],
            ),
            (
                "Senior Software Engineer",
                "software-engineering",
                vec!["software-engineer"],
            ),
            (
                "SWE / SDE",
                "software-engineering",
                vec!["software-engineer"],
            ),
        ] {
            assert!(
                matches!(
                    classify_posting_role(raw),
                    PostingRoleClassification::Known {
                        family_id,
                        matched_role_ids,
                        ..
                    } if family_id == expected_family && matched_role_ids == expected_roles
                ),
                "contained aliases must not poison the longer role: {raw}"
            );
        }
    }

    #[test]
    fn posting_role_classification_does_not_promote_terse_alias_substrings() {
        for raw in ["QA Coordinator", "DE&I Specialist"] {
            assert!(
                matches!(
                    classify_posting_role(raw),
                    PostingRoleClassification::Unknown {
                        reason: PostingRoleUnknownReason::NoKnownFamily,
                        ..
                    }
                ),
                "terse alias span was treated as proven title authority: {raw}"
            );
        }

        assert!(matches!(
            classify_posting_role("QA"),
            PostingRoleClassification::Known { family_id, .. }
                if family_id == "software-engineering"
        ));
        assert!(matches!(
            classify_posting_role("Senior QA Engineer"),
            PostingRoleClassification::Known { family_id, .. }
                if family_id == "software-engineering"
        ));
    }

    #[test]
    fn skill_matching_is_symbol_and_token_aware() {
        let raw = "Go, R, C, C++, C#, F#, .NET, Objective-C, Node.js, and AI";
        let ids: BTreeSet<_> = classify_skills_in_text(raw)
            .matches
            .into_iter()
            .map(|matched| matched.skill_id)
            .collect();
        for expected in [
            "cpp",
            "csharp",
            "fsharp",
            "dotnet",
            "objective-c",
            "nodejs",
            "artificial-intelligence",
        ] {
            assert!(ids.contains(expected), "missing skill match: {expected}");
        }
        for unsafe_bare_term in ["go", "r", "c"] {
            assert!(
                !ids.contains(unsafe_bare_term),
                "terse prose token was treated as a proven skill: {unsafe_bare_term}"
            );
        }
        let contextual_ids: BTreeSet<_> =
            classify_skills_in_text("Golang, R programming, and C language")
                .matches
                .into_iter()
                .map(|matched| matched.skill_id)
                .collect();
        for expected in ["go", "r", "c"] {
            assert!(
                contextual_ids.contains(expected),
                "contextual language skill was not recognized: {expected}"
            );
        }
        assert!(skill_matches_text("Production services in Golang", "Go"));
        assert!(!skill_matches_text("Go to market, then ready to go", "Go"));
        assert!(!skill_matches_text("Partner with R&D", "R"));
        assert!(!skill_matches_text("Present to the C-suite", "C"));
        assert!(!skill_matches_text(
            "Google, Rust, Cargo, painting, and nodejsness",
            "Go"
        ));
        assert!(!skill_matches_text("Rust", "R"));
        assert!(!skill_matches_text("Cargo", "C"));
        assert!(!skill_matches_text("painting", "AI"));
        assert!(!skill_matches_text("nodejsness", "Node.js"));
    }

    #[test]
    fn typed_geography_distinguishes_remote_canada_from_us_only() {
        let canada = normalize_geography_input(GeographyInput {
            remote: Some(true),
            country: Some("Canada".to_string()),
            ..GeographyInput::default()
        });
        let united_states = normalize_geography_input(GeographyInput {
            remote: Some(true),
            country: Some("US only".to_string()),
            ..GeographyInput::default()
        });
        assert_eq!(
            match_geography(&canada, &united_states),
            GeographyMatch::NotMatched {
                dimension: GeographyDimension::Country
            }
        );
        assert!(match_geography(&canada, &canada).is_proven_match());
    }

    #[test]
    fn geography_is_typed_and_fails_closed_for_unknown_or_ambiguous_input() {
        assert!(matches!(
            normalize_geography("CA"),
            GeographyClassification::Ambiguous {
                reason: GeographyAmbiguityReason::CountryOrSubdivision,
                ..
            }
        ));
        assert!(matches!(
            normalize_geography("New York"),
            GeographyClassification::Ambiguous {
                reason: GeographyAmbiguityReason::CityOrJurisdiction,
                ..
            }
        ));
        assert!(matches!(
            normalize_geography("New York, Remote"),
            GeographyClassification::Ambiguous {
                reason: GeographyAmbiguityReason::CityOrJurisdiction,
                ..
            }
        ));
        assert!(matches!(
            normalize_geography("New York, CA"),
            GeographyClassification::Ambiguous {
                reason: GeographyAmbiguityReason::MultipleSubdivisions,
                ..
            }
        ));
        assert!(matches!(
            normalize_geography_input(GeographyInput {
                city: Some("Springfield".to_string()),
                ..GeographyInput::default()
            }),
            GeographyClassification::Ambiguous {
                reason: GeographyAmbiguityReason::MissingCityContext,
                ..
            }
        ));
        for raw in [
            "Remote outside United States",
            "Remote except Canada",
            "Remote except in California",
            "Remote except within California",
            "Remote except the United States",
            "Remote except for the United States",
            "Remote - US excluded",
            "Remote excluding California",
            "Remote not available in California",
            "Remote - non-US",
            "Remote - not US",
            "Remote anywhere but in California",
            "Remote other than in California",
            "Remote but not California",
            "Remote - California not available",
            "Remote - California not supported",
            "Remote - US not eligible",
            "Remote outside San Francisco Bay Area",
            "Remote not in New York, NY",
            "Remote except Toronto, Canada",
        ] {
            assert!(
                matches!(
                    normalize_geography(raw),
                    GeographyClassification::Unknown {
                        raw: preserved,
                        reason: GeographyUnknownReason::ExcludedJurisdiction,
                        ..
                    } if preserved == raw
                ),
                "relational exclusion was treated as positive geography: {raw}"
            );
        }
        for raw in ["Remote, CA", "Remote, IN", "Hybrid, IN"] {
            assert!(
                matches!(
                    normalize_geography(raw),
                    GeographyClassification::Ambiguous {
                        reason: GeographyAmbiguityReason::CountryOrSubdivision,
                        ..
                    }
                ),
                "workplace evidence collapsed country/subdivision ambiguity: {raw}"
            );
        }
        assert!(matches!(
            normalize_geography_input(GeographyInput {
                country: Some("Atlantis".to_string()),
                ..GeographyInput::default()
            }),
            GeographyClassification::Unknown {
                reason: GeographyUnknownReason::UnknownCountry,
                ..
            }
        ));
    }

    #[test]
    fn raw_geography_normalizes_portal_locations_and_enforces_country_scope() {
        assert!(matches!(
            normalize_geography("Toronto, Canada"),
            GeographyClassification::Known {
                normalized: NormalizedGeography {
                    country_code: Some(country),
                    city: Some(city),
                    ..
                },
                ..
            } if country == "CA" && city == "toronto"
        ));
        assert!(matches!(
            normalize_geography("San Francisco, CA"),
            GeographyClassification::Known {
                normalized: NormalizedGeography {
                    country_code: Some(country),
                    subdivision_id: Some(subdivision),
                    city: Some(city),
                    ..
                },
                ..
            } if country == "US" && subdivision == "US-CA" && city == "san francisco"
        ));
        assert!(matches!(
            normalize_geography("New York, NY"),
            GeographyClassification::Known {
                normalized: NormalizedGeography {
                    country_code: Some(country),
                    subdivision_id: Some(subdivision),
                    metro_id: Some(metro),
                    city_id: Some(city),
                    ..
                },
                ..
            } if country == "US"
                && subdivision == "US-NY"
                && metro == "US-NY-new-york-metro"
                && city == "US-NY-new-york"
        ));
        assert!(geography_allows(
            "Remote - United States",
            &["Remote - United States".to_string()],
            "remote"
        )
        .is_allowed());
        assert!(!geography_allows(
            "Remote - United States",
            &["Remote - Canada".to_string()],
            "remote"
        )
        .is_allowed());
        assert_eq!(
            geography_allows(
                "Remote outside United States",
                &["Remote - United States".to_string()],
                "remote",
            ),
            GeographyMatchDecision::ReviewRequired {
                reason: GeographyReviewReason::UnknownPosting
            }
        );
        assert_eq!(
            geography_allows(
                "Remote - United States",
                &["Remote except United States".to_string()],
                "remote",
            ),
            GeographyMatchDecision::ReviewRequired {
                reason: GeographyReviewReason::UnknownPreference
            }
        );
    }

    #[test]
    fn workplace_classification_is_typed_and_adversarial_evidence_fails_closed() {
        for (raw, expected) in [
            ("Fully remote", WorkplaceKind::Remote),
            ("Hybrid", WorkplaceKind::Hybrid),
            ("On-site", WorkplaceKind::Onsite),
        ] {
            assert!(matches!(
                classify_workplace(raw),
                WorkplaceClassification::Known {
                    raw_workplace,
                    raw_location,
                    kind,
                } if raw_workplace == raw && raw_location.is_empty() && kind == expected
            ));
        }
        for (raw, reason) in [
            ("not remote", WorkplaceUnknownReason::NegatedEvidence),
            ("not fully remote", WorkplaceUnknownReason::NegatedEvidence),
            (
                "not currently remote",
                WorkplaceUnknownReason::NegatedEvidence,
            ),
            (
                "not completely remote",
                WorkplaceUnknownReason::NegatedEvidence,
            ),
            ("not 100% remote", WorkplaceUnknownReason::NegatedEvidence),
            (
                "remote is not currently available",
                WorkplaceUnknownReason::NegatedEvidence,
            ),
            (
                "remote roles are not currently offered",
                WorkplaceUnknownReason::NegatedEvidence,
            ),
            (
                "remote currently unavailable",
                WorkplaceUnknownReason::NegatedEvidence,
            ),
            (
                "remote temporarily unavailable",
                WorkplaceUnknownReason::NegatedEvidence,
            ),
            ("no longer remote", WorkplaceUnknownReason::NegatedEvidence),
            ("no work from home", WorkplaceUnknownReason::NegatedEvidence),
            ("remote: false", WorkplaceUnknownReason::NegatedEvidence),
            (
                "onsite unavailable",
                WorkplaceUnknownReason::NegatedEvidence,
            ),
            ("onsite prohibited", WorkplaceUnknownReason::NegatedEvidence),
            ("onsite excluded", WorkplaceUnknownReason::NegatedEvidence),
            ("hybrid prohibited", WorkplaceUnknownReason::NegatedEvidence),
            ("hybrid excluded", WorkplaceUnknownReason::NegatedEvidence),
            (
                "hybrid roles prohibited",
                WorkplaceUnknownReason::NegatedEvidence,
            ),
            (
                "remote positions unavailable",
                WorkplaceUnknownReason::NegatedEvidence,
            ),
            (
                "hybrid not offered",
                WorkplaceUnknownReason::NegatedEvidence,
            ),
            ("cannot be remote", WorkplaceUnknownReason::NegatedEvidence),
            (
                "does not allow remote",
                WorkplaceUnknownReason::NegatedEvidence,
            ),
            (
                "remote is not supported",
                WorkplaceUnknownReason::NegatedEvidence,
            ),
            ("remote or hybrid", WorkplaceUnknownReason::MixedEvidence),
            ("hybrid / onsite", WorkplaceUnknownReason::MixedEvidence),
            ("flexible", WorkplaceUnknownReason::UnsupportedEvidence),
            ("", WorkplaceUnknownReason::EmptyEvidence),
        ] {
            assert!(matches!(
                classify_workplace(raw),
                WorkplaceClassification::Unknown {
                    raw_workplace,
                    reason: observed,
                    ..
                } if raw_workplace == raw && observed == reason
            ));
        }
        assert!(matches!(
            classify_posting_workplace("unknown", "Remote - United States"),
            WorkplaceClassification::Unknown {
                raw_workplace,
                raw_location,
                reason: WorkplaceUnknownReason::UnsupportedEvidence,
            } if raw_workplace == "unknown" && raw_location == "Remote - United States"
        ));

        assert!(matches!(
            normalize_geography("Hybrid, New York, NY"),
            GeographyClassification::Known {
                normalized: NormalizedGeography {
                    workplace: Some(WorkplaceKind::Hybrid),
                    remote: None,
                    ..
                },
                ..
            }
        ));
        assert!(matches!(
            normalize_geography("Hybrid"),
            GeographyClassification::Known {
                normalized: NormalizedGeography {
                    workplace: Some(WorkplaceKind::Hybrid),
                    remote: None,
                    ..
                },
                ..
            }
        ));
        assert!(matches!(
            normalize_geography("Remote - Latin America"),
            GeographyClassification::Unknown {
                reason: GeographyUnknownReason::UnknownCountry,
                ..
            }
        ));
        assert_eq!(
            geography_allows(
                "Remote - United States",
                &["Remote - United States".to_string()],
                "not remote",
            ),
            GeographyMatchDecision::ReviewRequired {
                reason: GeographyReviewReason::UnknownPostingWorkplace
            }
        );
        assert_eq!(
            geography_allows(
                "United States",
                &["Remote - United States".to_string()],
                "hybrid",
            ),
            GeographyMatchDecision::Denied {
                dimensions: vec![GeographyDimension::Workplace]
            }
        );
        assert_eq!(
            geography_allows(
                "United States",
                &["United States".to_string()],
                "remote or hybrid",
            ),
            GeographyMatchDecision::ReviewRequired {
                reason: GeographyReviewReason::AmbiguousPostingWorkplace
            }
        );
        assert!(geography_allows(
            "Remote - United States",
            &["Remote - United States".to_string()],
            "remote",
        )
        .is_allowed());
    }

    #[test]
    fn metro_scope_matches_member_cities_without_broadening_exact_city_scope() {
        let metro = normalize_geography("San Francisco Bay Area");
        let san_francisco = normalize_geography("San Francisco, CA");
        let san_jose = normalize_geography("San Jose, CA");
        assert!(matches!(
            &metro,
            GeographyClassification::Known {
                normalized: NormalizedGeography {
                    metro_id: Some(metro_id),
                    city_id: None,
                    ..
                },
                ..
            } if metro_id == "US-CA-san-francisco-bay-area"
        ));
        assert!(match_geography(&metro, &san_francisco).is_proven_match());
        assert!(match_geography(&metro, &san_jose).is_proven_match());
        assert_eq!(
            match_geography(&san_francisco, &san_jose),
            GeographyMatch::NotMatched {
                dimension: GeographyDimension::City
            }
        );

        let ids = match san_francisco {
            GeographyClassification::Known { normalized, .. } => normalized.canonical_ids(),
            other => panic!("expected known San Francisco geography, got {other:?}"),
        };
        assert!(ids.contains(&"metro:US-CA-san-francisco-bay-area".to_string()));
        assert!(ids.contains(&"city:US-CA-san-francisco".to_string()));
        assert!(matches!(
            normalize_geography("San Francisco Bay Area Canada"),
            GeographyClassification::Unknown {
                reason: GeographyUnknownReason::MetroContextConflict,
                ..
            }
        ));
    }

    #[test]
    fn cross_subdivision_metro_and_unknown_city_outcomes_remain_typed() {
        let washington_metro = normalize_geography("DMV");
        let arlington = normalize_geography("Arlington, VA");
        assert!(match_geography(&washington_metro, &arlington).is_proven_match());

        assert!(matches!(
            normalize_geography_input(GeographyInput {
                country: Some("US".to_string()),
                metro: Some("Imaginary Metro".to_string()),
                ..GeographyInput::default()
            }),
            GeographyClassification::Unknown {
                reason: GeographyUnknownReason::UnknownMetro,
                ..
            }
        ));
        assert!(matches!(
            normalize_geography_input(GeographyInput {
                country: Some("US".to_string()),
                city: Some("Imaginary City".to_string()),
                ..GeographyInput::default()
            }),
            GeographyClassification::Unknown {
                reason: GeographyUnknownReason::UnknownCity,
                ..
            }
        ));
    }

    fn quoted_array<'a>(source: &'a str, constant: &str) -> Vec<&'a str> {
        let marker = format!("export const {constant} = [");
        let Some((_, tail)) = source.split_once(&marker) else {
            return Vec::new();
        };
        let Some((body, _)) = tail.split_once("];") else {
            return Vec::new();
        };
        body.lines()
            .filter_map(|line| {
                let trimmed = line.trim().trim_end_matches(',');
                trimmed
                    .strip_prefix('"')
                    .and_then(|value| value.strip_suffix('"'))
            })
            .collect()
    }
}
