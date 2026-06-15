//! Adaptive Resolver core — Track A1+A2 (heuristics + cache + validation only).
//!
//! See `docs/work/PLAN-ADAPTIVE-RESOLVER.md` §3 for the full contract. The
//! principle: **stop encoding answers; encode how to find the answer when you
//! don't have it.** Session/connector/drive readers are version-pinned by
//! assumption today — when an app changes its on-disk shape, the pinned reader
//! silently returns nothing. This module is the self-healing layer that lets a
//! reader re-derive *where* a message's text/role/order live, generically,
//! whenever the shape changes.
//!
//! Pipeline (this slice — **no AI yet**):
//!
//! ```text
//! resolve(cache, signature, samples)
//!   1. cache.get(signature)   → instant, free (the common case)
//!   2. detect_recipe(samples) → free; schema-agnostic structure-shape guess
//!   3. validate_recipe(...)   → deterministic gate; reject if not coherent
//!   4. cache.put(...)         → solved for this shape until it changes again
//! ```
//!
//! The AI fallback from the plan (Track A3) is **not** implemented here. There
//! is one clearly-marked extension point in [`resolve`] where it will slot in,
//! always *behind* the same [`validate_recipe`] gate.
//!
//! Security invariants (design §7), enforced here:
//!
//! - **Recipes are field-*paths* only** — `text_path`, `role_path`,
//!   `order_path`, and a small role value→canonical map. They never hold message
//!   content or secret values, so the on-disk cache stores shapes, never data.
//! - **Pure logic.** The only I/O is reading/writing the recipe-cache JSON file;
//!   no network, no subprocess, no AI in this slice.
//! - **Fail-soft.** A missing or corrupt cache file is treated as an empty
//!   cache; nothing here panics on bad input, and no `unwrap`/`expect` is used
//!   outside tests.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A stable key for "this kind of data" — `agent[:version]:<shape-fingerprint>`.
///
/// The fingerprint captures the sample's **top-level structure only** (sorted
/// top-level keys, or for an array the keys of its first element) — never the
/// values. This keys the recipe cache so a recipe is re-derived only when the
/// shape actually changes, and a stale recipe is never reused across a version
/// bump (the version is part of the key when known).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FormatSignature(pub String);

impl FormatSignature {
    /// The underlying stable key string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for FormatSignature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Build a [`FormatSignature`] from an agent name, optional version, and a
/// sample value whose **top-level structure** is fingerprinted.
///
/// - Object → its sorted top-level keys.
/// - Array → the sorted top-level keys of the first element (the per-message
///   shape), prefixed so an array shape never collides with an object shape.
/// - Scalar/empty → a fixed marker for the value's JSON kind.
///
/// Only key *names* contribute; values are ignored. Two samples with the same
/// set of top-level keys produce the same signature regardless of contents.
pub fn signature(agent: &str, version: Option<&str>, sample: &Value) -> FormatSignature {
    let fingerprint = shape_fingerprint(sample);
    let key = match version {
        Some(v) => format!("{agent}:{v}:{fingerprint}"),
        None => format!("{agent}::{fingerprint}"),
    };
    FormatSignature(key)
}

/// Fingerprint the top-level structure of `value` (keys only, never values).
fn shape_fingerprint(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&str> = map.keys().map(String::as_str).collect();
            keys.sort_unstable();
            format!("obj{{{}}}", keys.join(","))
        }
        Value::Array(items) => {
            // Fingerprint the first element's shape — the per-message shape for
            // a conversation array. An empty array carries no shape.
            let inner = items.first().map(shape_fingerprint).unwrap_or_default();
            format!("arr[{inner}]")
        }
        Value::String(_) => "str".to_string(),
        Value::Number(_) => "num".to_string(),
        Value::Bool(_) => "bool".to_string(),
        Value::Null => "null".to_string(),
    }
}

/// A recipe for extracting normalized turns from an agent's message objects.
///
/// Every field is a *path* or a value→canonical map — never content. Paths are
/// simple dotted accessors (`a.b.c`) into a JSON object; see [`get_path`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ExtractionRecipe {
    /// Dotted path to a message's text body (e.g. `text`, `message.content`).
    pub text_path: String,
    /// Dotted path to the field carrying the speaker role, if one was found.
    pub role_path: Option<String>,
    /// Maps raw role values (as their JSON string form, e.g. `"user"`, `"1"`)
    /// to a canonical role: `user`, `assistant`, or `system`. Inspectable so a
    /// caller (or a future audit) can see exactly how roles were interpreted.
    pub role_map: BTreeMap<String, String>,
    /// Dotted path to a monotonic ordering field (index/seq/timestamp), if any.
    /// When `None`, the caller orders by file/array position.
    pub order_path: Option<String>,
}

/// Resolve a simple dotted path (`a.b.c`) into a JSON object, one key per
/// segment. Returns `None` if any segment is missing or a non-object is
/// traversed. Empty path returns `None`. Read-only; never mutates.
pub fn get_path<'v>(value: &'v Value, path: &str) -> Option<&'v Value> {
    if path.is_empty() {
        return None;
    }
    let mut current = value;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}

/// Read a path as a borrowed `&str`, or `None` if missing / not a string.
fn get_str<'v>(value: &'v Value, path: &str) -> Option<&'v str> {
    get_path(value, path).and_then(Value::as_str)
}

/// Render the *value* at `path` to the canonical string used as a `role_map`
/// key: strings as-is, integers as their decimal form. Other kinds → `None`
/// (booleans/floats/objects aren't sensible role discriminators).
fn role_key_at(value: &Value, path: &str) -> Option<String> {
    match get_path(value, path)? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) if n.is_i64() || n.is_u64() => Some(n.to_string()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Heuristic shape detection (Track A2)
// ---------------------------------------------------------------------------

/// Candidate text fields to try by name before falling back to "the dominant
/// long-string field". Dotted entries cover the common nested layouts.
const COMMON_TEXT_PATHS: &[&str] = &[
    "text",
    "content",
    "body",
    "message.content",
    "message.text",
    "value",
];

/// Candidate role fields to try by name before the generic small-domain scan.
const COMMON_ROLE_PATHS: &[&str] = &[
    "role",
    "type",
    "sender",
    "author",
    "speaker",
    "message.role",
    "from",
];

/// Maximum number of distinct values a field may take to be considered a *role*
/// discriminator (roles are a tiny closed set; a free-text field is not).
const MAX_ROLE_CARDINALITY: usize = 6;

/// Detect an [`ExtractionRecipe`] from a handful of sample message objects,
/// schema-agnostically. Returns `None` if no plausible text field is found.
///
/// Strategy:
/// - **text_path**: the (possibly nested) field whose value is the most
///   consistent, longest *string* across samples. Common names
///   ([`COMMON_TEXT_PATHS`]) win ties, but an unknown long-string field is
///   accepted as the body when no common name fits.
/// - **role_path + role_map**: a field whose values form a small closed set of
///   strings or small ints. String values `user`/`assistant`/`system` map
///   directly (case-insensitively); anything else (e.g. Cursor's `1`/`2`) maps
///   its two most common values to `user`/`assistant` by **first-occurrence
///   order**, leaving the full map inspectable.
/// - **order_path**: a numeric field that is monotonic (non-decreasing) across
///   the samples in order; else `None` (caller uses file/array order).
pub fn detect_recipe(samples: &[Value]) -> Option<ExtractionRecipe> {
    if samples.is_empty() {
        return None;
    }

    let text_path = detect_text_path(samples)?;
    let (role_path, role_map) = detect_role(samples, &text_path);
    let order_path = detect_order_path(samples, &text_path, role_path.as_deref());

    Some(ExtractionRecipe {
        text_path,
        role_path,
        role_map,
        order_path,
    })
}

/// Collect every dotted path (root keys + one level of nesting) that ever holds
/// a string value across the samples. One level of nesting is enough for the
/// known layouts (`message.content`) and keeps the search bounded.
fn candidate_string_paths(samples: &[Value]) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    for sample in samples {
        if let Value::Object(map) = sample {
            for (key, val) in map {
                match val {
                    Value::String(_) => {
                        seen.insert(key.clone());
                    }
                    Value::Object(inner) => {
                        for (inner_key, inner_val) in inner {
                            if inner_val.is_string() {
                                seen.insert(format!("{key}.{inner_key}"));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    seen.into_iter().collect()
}

/// Score a candidate text path: how many samples have a non-empty string there,
/// and the total length of those strings (longer bodies score higher). The
/// pair sorts so coverage dominates, then length.
fn score_text_path(samples: &[Value], path: &str) -> (usize, usize) {
    let mut hits = 0usize;
    let mut total_len = 0usize;
    for sample in samples {
        if let Some(s) = get_str(sample, path) {
            if !s.trim().is_empty() {
                hits += 1;
                total_len += s.chars().count();
            }
        }
    }
    (hits, total_len)
}

/// Pick the dominant long-string field as the body. Common names are tried
/// first and win ties; an unknown field is accepted when it has the best
/// coverage/length. Returns `None` if nothing holds a non-empty string.
fn detect_text_path(samples: &[Value]) -> Option<String> {
    // Common names first: if one has full coverage, take it immediately so a
    // short `role` string can never out-rank the real body by name.
    for name in COMMON_TEXT_PATHS {
        let (hits, _) = score_text_path(samples, name);
        if hits == samples.len() && hits > 0 {
            return Some((*name).to_string());
        }
    }

    let mut best: Option<(String, (usize, usize))> = None;
    let mut consider = |path: String, score: (usize, usize)| {
        if score.0 == 0 {
            return;
        }
        match &best {
            Some((_, best_score)) if *best_score >= score => {}
            _ => best = Some((path, score)),
        }
    };

    // Common names get a coverage bonus so a known body field beats an unknown
    // field of equal coverage but is still overridden by a clearly larger one.
    for name in COMMON_TEXT_PATHS {
        let (hits, len) = score_text_path(samples, name);
        if hits > 0 {
            consider((*name).to_string(), (hits, len.saturating_add(1)));
        }
    }
    for path in candidate_string_paths(samples) {
        let score = score_text_path(samples, &path);
        consider(path, score);
    }

    best.map(|(path, _)| path)
}

/// Collect every dotted path (root + one nesting level) usable as a role
/// discriminator: its values are all strings or all small integers.
fn candidate_role_paths(samples: &[Value]) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    for sample in samples {
        if let Value::Object(map) = sample {
            for (key, val) in map {
                match val {
                    Value::String(_) | Value::Number(_) => {
                        seen.insert(key.clone());
                    }
                    Value::Object(inner) => {
                        for (inner_key, inner_val) in inner {
                            if inner_val.is_string() || inner_val.is_number() {
                                seen.insert(format!("{key}.{inner_key}"));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    seen.into_iter().collect()
}

/// Map a raw role value to a canonical role by name, case-insensitively.
/// Returns `None` for values that don't obviously name a known role (callers
/// then assign by first-occurrence order).
fn canonical_role_name(raw: &str) -> Option<&'static str> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "user" | "human" => Some("user"),
        "assistant" | "ai" | "bot" | "model" => Some("assistant"),
        "system" | "developer" => Some("system"),
        _ => None,
    }
}

/// Detect the role field and build its value→canonical map. Returns
/// `(None, empty)` when no small-domain field is found — extraction still works
/// (every turn gets the same role), and [`validate_recipe`] decides if that is
/// acceptable for the sample.
fn detect_role(samples: &[Value], text_path: &str) -> (Option<String>, BTreeMap<String, String>) {
    // Ordered candidates: common names first, then any other small-domain field
    // (never the text field itself).
    let mut candidates: Vec<String> = Vec::new();
    for name in COMMON_ROLE_PATHS {
        candidates.push((*name).to_string());
    }
    for path in candidate_role_paths(samples) {
        if !candidates.contains(&path) {
            candidates.push(path);
        }
    }

    for path in candidates {
        if path == text_path {
            continue;
        }
        // Gather the raw role values in first-occurrence order, requiring the
        // field to be present (as a string/int) on most samples.
        let mut ordered: Vec<String> = Vec::new();
        let mut present = 0usize;
        for sample in samples {
            if let Some(key) = role_key_at(sample, &path) {
                present += 1;
                if !ordered.contains(&key) {
                    ordered.push(key);
                }
            }
        }
        let coverage_ok = present * 2 >= samples.len();
        let cardinality_ok = !ordered.is_empty() && ordered.len() <= MAX_ROLE_CARDINALITY;
        if !(coverage_ok && cardinality_ok) {
            continue;
        }

        if let Some(map) = build_role_map(&ordered) {
            return (Some(path), map);
        }
    }

    (None, BTreeMap::new())
}

/// Build a role map from the distinct raw values, in first-occurrence order.
///
/// Named values (`user`/`assistant`/`system`, case-insensitively) map directly.
/// Unnamed values (e.g. `1`, `2`) are assigned `user` then `assistant` by
/// first-occurrence order, so the earliest-seen unnamed value is the user turn.
/// Returns `None` only if the field is empty (caller skips it).
fn build_role_map(ordered: &[String]) -> Option<BTreeMap<String, String>> {
    if ordered.is_empty() {
        return None;
    }
    let mut map = BTreeMap::new();
    // Slots for unnamed values, consumed in order: first → user, second → on.
    let unnamed_slots = ["user", "assistant"];
    let mut next_unnamed = 0usize;
    for raw in ordered {
        let canonical = match canonical_role_name(raw) {
            Some(name) => name.to_string(),
            None => {
                let slot = unnamed_slots.get(next_unnamed).copied().unwrap_or("system");
                next_unnamed += 1;
                slot.to_string()
            }
        };
        map.insert(raw.clone(), canonical);
    }
    Some(map)
}

/// Detect a monotonic (non-decreasing) numeric ordering field across the
/// samples in their given order. The text and role fields are excluded.
/// Returns the first qualifying path, or `None`.
fn detect_order_path(
    samples: &[Value],
    text_path: &str,
    role_path: Option<&str>,
) -> Option<String> {
    if samples.len() < 2 {
        return None;
    }
    let mut candidates = std::collections::BTreeSet::new();
    for sample in samples {
        if let Value::Object(map) = sample {
            for (key, val) in map {
                if val.is_number() {
                    candidates.insert(key.clone());
                }
            }
        }
    }

    for path in candidates {
        if path == text_path || Some(path.as_str()) == role_path {
            continue;
        }
        let mut values = Vec::with_capacity(samples.len());
        let mut complete = true;
        for sample in samples {
            match get_path(sample, &path).and_then(Value::as_f64) {
                Some(n) => values.push(n),
                None => {
                    complete = false;
                    break;
                }
            }
        }
        if !complete || values.len() < 2 {
            continue;
        }
        let non_decreasing = values.windows(2).all(|w| w[1] >= w[0]);
        let strictly_varies = values.windows(2).any(|w| w[1] > w[0]);
        if non_decreasing && strictly_varies {
            return Some(path);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Validation gate (deterministic — must pass before ANY recipe is trusted)
// ---------------------------------------------------------------------------

/// Deterministic gate: does applying `recipe` to `samples` yield a coherent
/// extraction? This must pass before any recipe — heuristic **or** future-AI —
/// is trusted or cached.
///
/// Accepts only if:
/// - the recipe extracts **≥1 non-empty text**, and
/// - role assignment is plausible: either there is no role field (single-role
///   extraction is allowed) **or**, when a role field is declared, every raw
///   role value present in the samples is mapped (no value falls through to a
///   blind default).
///
/// Rejects a recipe that extracts nothing, or one whose declared role field
/// leaves sample values unmapped (the "assign every message the same garbage"
/// failure mode).
pub fn validate_recipe(recipe: &ExtractionRecipe, samples: &[Value]) -> bool {
    if recipe.text_path.is_empty() || samples.is_empty() {
        return false;
    }

    let mut non_empty_text = 0usize;
    for sample in samples {
        if let Some(s) = get_str(sample, &recipe.text_path) {
            if !s.trim().is_empty() {
                non_empty_text += 1;
            }
        }
    }
    if non_empty_text == 0 {
        return false;
    }

    // If a role field is declared, every raw value present must be in the map.
    if let Some(role_path) = &recipe.role_path {
        let mut saw_role_value = false;
        for sample in samples {
            if let Some(key) = role_key_at(sample, role_path) {
                saw_role_value = true;
                if !recipe.role_map.contains_key(&key) {
                    return false;
                }
            }
        }
        // A declared role field that never resolves on the samples is incoherent.
        if !saw_role_value {
            return false;
        }
    }

    true
}

// ---------------------------------------------------------------------------
// Recipe cache (persisted JSON; shapes only, never secrets/content)
// ---------------------------------------------------------------------------

/// A persisted map of [`FormatSignature`] → [`ExtractionRecipe`].
///
/// Backed by a single JSON file. The cache stores **shapes/recipes only** —
/// field paths and a role map — never message content or secret values. It is
/// fail-soft: a missing or corrupt file loads as an empty cache and never
/// panics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecipeCache {
    /// Recipes keyed by their signature string.
    recipes: BTreeMap<String, ExtractionRecipe>,
}

impl RecipeCache {
    /// An empty cache, not backed by any file yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up a cached recipe for `sig`.
    pub fn get(&self, sig: &FormatSignature) -> Option<ExtractionRecipe> {
        self.recipes.get(&sig.0).cloned()
    }

    /// Insert or replace the recipe for `sig`. Does not write to disk; call
    /// [`RecipeCache::save`] to persist.
    pub fn put(&mut self, sig: FormatSignature, recipe: ExtractionRecipe) {
        self.recipes.insert(sig.0, recipe);
    }

    /// Number of cached recipes (primarily for tests/inspection).
    pub fn len(&self) -> usize {
        self.recipes.len()
    }

    /// True when no recipes are cached.
    pub fn is_empty(&self) -> bool {
        self.recipes.is_empty()
    }

    /// Load a cache from `path`. **Fail-soft**: a missing file or unparseable
    /// JSON yields an empty cache rather than an error — a corrupt cache must
    /// never break resolution, only cost a re-derive.
    pub fn load(path: &Path) -> Self {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Persist the cache to `path`, creating parent directories as needed.
    /// Returns an error only on a real I/O failure (the caller may choose to
    /// ignore it — a failed write just means the next run re-derives).
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let json = serde_json::to_vec_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }
}

/// Default on-disk location for the recipe cache, under the user's app-support
/// directory. **Never hardcodes an absolute path**: it resolves the home dir
/// from the environment (`HOME`, then `USERPROFILE`) and returns `None` when
/// neither is set, so the caller can fall back to an injected path.
///
/// On macOS this lands at
/// `~/Library/Application Support/bluey/recipes.json`; on other platforms it
/// uses a `.bluey` dotfile dir to stay dependency-free.
pub fn default_cache_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    let mut path = PathBuf::from(home);
    if cfg!(target_os = "macos") {
        path.push("Library");
        path.push("Application Support");
        path.push("bluey");
    } else {
        path.push(".bluey");
    }
    path.push("recipes.json");
    Some(path)
}

// ---------------------------------------------------------------------------
// Resolver entry + apply
// ---------------------------------------------------------------------------

/// Resolve an [`ExtractionRecipe`] for `samples` keyed by `sig`, consulting the
/// cache first and the heuristic detector second. The returned recipe is always
/// one that has passed [`validate_recipe`] on these samples.
///
/// 1. `cache.get(sig)` — if present **and** it still validates on these
///    samples, return it (the common, free, instant case).
/// 2. [`detect_recipe`] — if it produces a recipe that validates, cache it
///    (in-memory; caller persists via [`RecipeCache::save`]) and return it.
/// 3. otherwise `None` — the caller falls back to its pinned reader or an
///    honest "can't read this format".
///
/// A cached recipe that *fails* re-validation (e.g. the shape drifted but the
/// signature collided, or the cache was hand-edited) is discarded and the
/// detector is retried, so a bad cache entry self-heals on next use.
pub fn resolve(
    cache: &mut RecipeCache,
    sig: &FormatSignature,
    samples: &[Value],
) -> Option<ExtractionRecipe> {
    // 1. Cache hit, but only trusted if it still validates on these samples.
    if let Some(cached) = cache.get(sig) {
        if validate_recipe(&cached, samples) {
            return Some(cached);
        }
    }

    // 2. Heuristic detection, gated by validation before we cache or return it.
    if let Some(recipe) = detect_recipe(samples) {
        if validate_recipe(&recipe, samples) {
            cache.put(sig.clone(), recipe.clone());
            return Some(recipe);
        }
    }

    // EXTENSION POINT (A3): if detect_recipe fails, an AI fallback would sample
    // the structure and propose a recipe here — still gated by validate_recipe
    // before caching. Not implemented in this slice (heuristics + cache only).

    None
}

/// Apply `recipe` to `messages`, returning `(role, text)` pairs.
///
/// - `role` is the canonical mapped string (`user`/`assistant`/`system`). When
///   no role field is declared, or a value isn't in the map, the role is
///   `"other"`.
/// - Messages whose text is missing or empty (after trimming) are skipped.
/// - Output is ordered by `order_path` when present (non-decreasing); otherwise
///   the input order is preserved. Ordering is stable.
pub fn apply_recipe(recipe: &ExtractionRecipe, messages: &[Value]) -> Vec<(String, String)> {
    // Pair each surviving message with its order key, preserving input index as
    // a stable tiebreaker.
    let mut indexed: Vec<(usize, f64, String, String)> = Vec::new();
    for (idx, message) in messages.iter().enumerate() {
        let text = match get_str(message, &recipe.text_path) {
            Some(s) if !s.trim().is_empty() => s.to_string(),
            _ => continue,
        };
        let role = resolve_role(recipe, message);
        let order_key = recipe
            .order_path
            .as_deref()
            .and_then(|p| get_path(message, p))
            .and_then(Value::as_f64)
            .unwrap_or(idx as f64);
        indexed.push((idx, order_key, role, text));
    }

    if recipe.order_path.is_some() {
        // Stable sort by order key, then original index for ties.
        indexed.sort_by(|a, b| {
            a.1.partial_cmp(&b.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0))
        });
    }

    indexed
        .into_iter()
        .map(|(_, _, role, text)| (role, text))
        .collect()
}

/// Resolve a single message's canonical role via the recipe, defaulting to
/// `"other"` when no role field is declared or the value isn't mapped.
fn resolve_role(recipe: &ExtractionRecipe, message: &Value) -> String {
    let Some(role_path) = &recipe.role_path else {
        return "other".to_string();
    };
    match role_key_at(message, role_path) {
        Some(key) => recipe
            .role_map
            .get(&key)
            .cloned()
            .unwrap_or_else(|| "other".to_string()),
        None => "other".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // --- signature ---------------------------------------------------------

    #[test]
    fn signature_is_stable_for_same_structure() {
        let a = json!({"role": "user", "content": "hello"});
        let b = json!({"role": "assistant", "content": "a much longer body here"});
        // Same top-level keys, different values → identical signature.
        assert_eq!(signature("claude", None, &a), signature("claude", None, &b));
    }

    #[test]
    fn signature_differs_for_different_top_level_keys() {
        let a = json!({"role": "user", "content": "hi"});
        let b = json!({"type": 1, "text": "hi"});
        assert_ne!(signature("cursor", None, &a), signature("cursor", None, &b));
    }

    #[test]
    fn signature_includes_version_and_agent() {
        let sample = json!({"content": "x", "role": "user"});
        assert_ne!(
            signature("cursor", Some("0.42"), &sample),
            signature("cursor", Some("0.43"), &sample),
        );
        assert_ne!(
            signature("cursor", None, &sample),
            signature("claude", None, &sample)
        );
    }

    #[test]
    fn signature_array_uses_first_element_shape() {
        let arr = json!([{"role": "user", "content": "a"}, {"role": "assistant", "content": "b"}]);
        let obj = json!({"role": "user", "content": "a"});
        // An array shape is distinct from a bare object of the same element keys.
        assert_ne!(signature("x", None, &arr), signature("x", None, &obj));
        // But two arrays with the same element shape match.
        let arr2 = json!([{"content": "zzz", "role": "system"}]);
        assert_eq!(signature("x", None, &arr), signature("x", None, &arr2));
    }

    // --- get_path ----------------------------------------------------------

    #[test]
    fn get_path_resolves_nested_and_misses() {
        let v = json!({"message": {"role": "user", "content": "deep"}, "top": 3});
        assert_eq!(get_path(&v, "top"), Some(&json!(3)));
        assert_eq!(get_path(&v, "message.content"), Some(&json!("deep")));
        assert_eq!(get_path(&v, "message.missing"), None);
        assert_eq!(get_path(&v, "nope"), None);
        assert_eq!(get_path(&v, "top.deeper"), None); // can't descend into a scalar
        assert_eq!(get_path(&v, ""), None);
    }

    // --- detect_recipe: Cursor-like (int roles) ----------------------------

    #[test]
    fn detect_recipe_cursor_like_int_roles() {
        let samples = vec![
            json!({"type": 1, "text": "hi"}),
            json!({"type": 2, "text": "yo"}),
        ];
        let recipe = detect_recipe(&samples).expect("should detect");
        assert_eq!(recipe.text_path, "text");
        assert_eq!(recipe.role_path.as_deref(), Some("type"));
        // First-occurrence: 1 → user, 2 → assistant.
        assert_eq!(recipe.role_map.get("1").map(String::as_str), Some("user"));
        assert_eq!(
            recipe.role_map.get("2").map(String::as_str),
            Some("assistant")
        );
        assert!(validate_recipe(&recipe, &samples));
    }

    // --- detect_recipe: Claude-like (string roles) -------------------------

    #[test]
    fn detect_recipe_claude_like_string_roles() {
        let samples = vec![
            json!({"role": "user", "content": "q"}),
            json!({"role": "assistant", "content": "a"}),
        ];
        let recipe = detect_recipe(&samples).expect("should detect");
        assert_eq!(recipe.text_path, "content");
        assert_eq!(recipe.role_path.as_deref(), Some("role"));
        assert_eq!(
            recipe.role_map.get("user").map(String::as_str),
            Some("user")
        );
        assert_eq!(
            recipe.role_map.get("assistant").map(String::as_str),
            Some("assistant")
        );
        assert!(validate_recipe(&recipe, &samples));
    }

    // --- detect_recipe: nested layout --------------------------------------

    #[test]
    fn detect_recipe_nested_message_object() {
        let samples = vec![
            json!({"message": {"role": "user", "content": "x"}}),
            json!({"message": {"role": "assistant", "content": "a longer reply"}}),
        ];
        let recipe = detect_recipe(&samples).expect("should detect");
        assert_eq!(recipe.text_path, "message.content");
        assert_eq!(recipe.role_path.as_deref(), Some("message.role"));
        assert!(validate_recipe(&recipe, &samples));
        let pairs = apply_recipe(&recipe, &samples);
        assert_eq!(
            pairs,
            vec![
                ("user".to_string(), "x".to_string()),
                ("assistant".to_string(), "a longer reply".to_string()),
            ]
        );
    }

    #[test]
    fn detect_recipe_returns_none_without_text() {
        // No string field anywhere → no plausible body.
        let samples = vec![json!({"type": 1, "seq": 0}), json!({"type": 2, "seq": 1})];
        assert!(detect_recipe(&samples).is_none());
    }

    #[test]
    fn detect_recipe_picks_dominant_long_string() {
        // No common name; the long unknown field beats the short tag field.
        let samples = vec![
            json!({"tag": "u", "payload": "this is the real message body, quite long"}),
            json!({"tag": "a", "payload": "another genuinely long assistant reply here"}),
        ];
        let recipe = detect_recipe(&samples).expect("should detect");
        assert_eq!(recipe.text_path, "payload");
        // `tag` is the small-domain field → role.
        assert_eq!(recipe.role_path.as_deref(), Some("tag"));
    }

    #[test]
    fn detect_recipe_finds_monotonic_order_field() {
        let samples = vec![
            json!({"role": "user", "content": "first", "seq": 0}),
            json!({"role": "assistant", "content": "second", "seq": 1}),
            json!({"role": "user", "content": "third", "seq": 2}),
        ];
        let recipe = detect_recipe(&samples).expect("should detect");
        assert_eq!(recipe.order_path.as_deref(), Some("seq"));
    }

    // --- validate_recipe ---------------------------------------------------

    #[test]
    fn validate_rejects_all_empty_text() {
        let samples = vec![
            json!({"role": "user", "content": ""}),
            json!({"role": "ai", "content": "  "}),
        ];
        let recipe = ExtractionRecipe {
            text_path: "content".to_string(),
            role_path: Some("role".to_string()),
            role_map: BTreeMap::new(),
            order_path: None,
        };
        assert!(!validate_recipe(&recipe, &samples));
    }

    #[test]
    fn validate_rejects_unmapped_role_values() {
        let samples = vec![json!({"role": "user", "content": "hi"})];
        let mut role_map = BTreeMap::new();
        role_map.insert("assistant".to_string(), "assistant".to_string()); // "user" missing
        let recipe = ExtractionRecipe {
            text_path: "content".to_string(),
            role_path: Some("role".to_string()),
            role_map,
            order_path: None,
        };
        assert!(!validate_recipe(&recipe, &samples));
    }

    #[test]
    fn validate_accepts_single_role_extraction() {
        // No role field declared → single-role extraction is allowed.
        let samples = vec![json!({"content": "hi"}), json!({"content": "there"})];
        let recipe = ExtractionRecipe {
            text_path: "content".to_string(),
            role_path: None,
            role_map: BTreeMap::new(),
            order_path: None,
        };
        assert!(validate_recipe(&recipe, &samples));
    }

    #[test]
    fn validate_rejects_role_path_that_never_resolves() {
        let samples = vec![json!({"content": "hi"})];
        let recipe = ExtractionRecipe {
            text_path: "content".to_string(),
            role_path: Some("role".to_string()), // not present on any sample
            role_map: BTreeMap::new(),
            order_path: None,
        };
        assert!(!validate_recipe(&recipe, &samples));
    }

    // --- cache -------------------------------------------------------------

    #[test]
    fn cache_round_trips_through_tempfile() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("nested").join("recipes.json");
        let sig = signature(
            "claude",
            Some("1.0"),
            &json!({"role": "user", "content": "x"}),
        );
        let recipe = ExtractionRecipe {
            text_path: "content".to_string(),
            role_path: Some("role".to_string()),
            role_map: BTreeMap::from([("user".to_string(), "user".to_string())]),
            order_path: None,
        };
        let mut cache = RecipeCache::new();
        cache.put(sig.clone(), recipe.clone());
        cache.save(&path).expect("save");

        let reloaded = RecipeCache::load(&path);
        assert_eq!(reloaded.len(), 1);
        assert_eq!(reloaded.get(&sig), Some(recipe));
    }

    #[test]
    fn cache_missing_file_is_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("does-not-exist.json");
        let cache = RecipeCache::load(&path);
        assert!(cache.is_empty());
    }

    #[test]
    fn cache_corrupt_file_is_empty_no_panic() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("corrupt.json");
        std::fs::write(&path, b"{ this is not valid json ]]]").expect("write");
        let cache = RecipeCache::load(&path);
        assert!(cache.is_empty());
    }

    // --- apply_recipe ------------------------------------------------------

    #[test]
    fn apply_recipe_orders_and_skips_empties() {
        let recipe = ExtractionRecipe {
            text_path: "text".to_string(),
            role_path: Some("type".to_string()),
            role_map: BTreeMap::from([
                ("1".to_string(), "user".to_string()),
                ("2".to_string(), "assistant".to_string()),
            ]),
            order_path: Some("seq".to_string()),
        };
        // Deliberately out of order, with one empty body to skip.
        let messages = vec![
            json!({"type": 2, "text": "second", "seq": 2}),
            json!({"type": 1, "text": "first", "seq": 1}),
            json!({"type": 1, "text": "", "seq": 3}),
        ];
        let pairs = apply_recipe(&recipe, &messages);
        assert_eq!(
            pairs,
            vec![
                ("user".to_string(), "first".to_string()),
                ("assistant".to_string(), "second".to_string()),
            ]
        );
    }

    #[test]
    fn apply_recipe_preserves_input_order_without_order_path() {
        let recipe = ExtractionRecipe {
            text_path: "content".to_string(),
            role_path: None,
            role_map: BTreeMap::new(),
            order_path: None,
        };
        let messages = vec![json!({"content": "one"}), json!({"content": "two"})];
        let pairs = apply_recipe(&recipe, &messages);
        assert_eq!(
            pairs,
            vec![
                ("other".to_string(), "one".to_string()),
                ("other".to_string(), "two".to_string()),
            ]
        );
    }

    #[test]
    fn apply_recipe_unmapped_role_becomes_other() {
        let recipe = ExtractionRecipe {
            text_path: "content".to_string(),
            role_path: Some("role".to_string()),
            role_map: BTreeMap::from([("user".to_string(), "user".to_string())]),
            order_path: None,
        };
        let messages = vec![json!({"role": "weird", "content": "hi"})];
        let pairs = apply_recipe(&recipe, &messages);
        assert_eq!(pairs, vec![("other".to_string(), "hi".to_string())]);
    }

    // --- resolve (end-to-end) ---------------------------------------------

    #[test]
    fn resolve_detects_then_caches_then_hits() {
        let samples = vec![
            json!({"role": "user", "content": "q"}),
            json!({"role": "assistant", "content": "a"}),
        ];
        let sig = signature("claude", Some("1.0"), &samples[0]);
        let mut cache = RecipeCache::new();
        assert!(cache.is_empty());

        // First call: detector runs, recipe validates, gets cached.
        let first = resolve(&mut cache, &sig, &samples).expect("resolve");
        assert_eq!(first.text_path, "content");
        assert_eq!(cache.len(), 1);

        // Second call: served from cache (still validates), cache unchanged.
        let second = resolve(&mut cache, &sig, &samples).expect("resolve cached");
        assert_eq!(first, second);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn resolve_returns_none_when_unreadable() {
        let samples = vec![json!({"type": 1}), json!({"type": 2})];
        let sig = signature("mystery", None, &samples[0]);
        let mut cache = RecipeCache::new();
        assert!(resolve(&mut cache, &sig, &samples).is_none());
        assert!(cache.is_empty());
    }

    #[test]
    fn resolve_discards_stale_cache_entry_and_reheals() {
        let samples = vec![
            json!({"role": "user", "content": "q"}),
            json!({"role": "assistant", "content": "a"}),
        ];
        let sig = signature("claude", Some("1.0"), &samples[0]);
        let mut cache = RecipeCache::new();
        // Seed a bogus recipe that won't validate (wrong text path).
        cache.put(
            sig.clone(),
            ExtractionRecipe {
                text_path: "nonexistent".to_string(),
                role_path: None,
                role_map: BTreeMap::new(),
                order_path: None,
            },
        );
        // resolve must reject the stale entry, re-detect, and return a good one.
        let healed = resolve(&mut cache, &sig, &samples).expect("reheal");
        assert_eq!(healed.text_path, "content");
    }
}
