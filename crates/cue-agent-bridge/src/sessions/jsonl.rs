//! Newline-delimited JSON decoder for Claude Code and Codex sessions.
//!
//! Storage layout (design §3):
//! - Claude Code: `~/.claude/projects/<encoded-cwd>/<session-id>.jsonl`
//! - Codex:       `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`
//!
//! Each line is a standalone JSON object describing a turn or event. Shapes
//! vary across versions, so parsing is intentionally tolerant: any line that
//! does not yield a usable role + text is skipped, and the rest still decode.
//!
//! For a [`SessionStore`] whose `path` is a *directory*, every `*.jsonl` file
//! under it is one session (id = file stem). If `path` points directly at a
//! `*.jsonl` file, that single file is the only session.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{mtime_epoch_string, SessionReader};
use crate::{Role, SessionRef, SessionStore, Transcript, Turn};

/// Decoder for Claude Code / Codex JSONL session files.
pub struct JsonlReader;

const TITLE_SNIPPET_CHARS: usize = 80;

/// Cap on a single JSONL line we will parse. A normal turn is well under this;
/// a line larger than this is treated as corrupt/oversized and skipped so a
/// single pathological line can never blow up memory.
const MAX_LINE_BYTES: usize = 4 * 1024 * 1024;

impl SessionReader for JsonlReader {
    fn list(&self, store: &SessionStore, limit: usize) -> anyhow::Result<Vec<SessionRef>> {
        let mut refs: Vec<SessionRef> = enumerate_files(&store.path)
            .into_iter()
            .filter_map(|file| session_ref_for(&file))
            .collect();
        // Most-recent first by mtime epoch string (zero-padded compare is fine
        // for equal-width epochs; fall back to numeric for safety).
        refs.sort_by(|a, b| {
            let an = a.updated_at.parse::<u64>().unwrap_or(0);
            let bn = b.updated_at.parse::<u64>().unwrap_or(0);
            bn.cmp(&an)
        });
        refs.truncate(limit);
        Ok(refs)
    }

    fn read(&self, store: &SessionStore, id: &str, max_turns: usize) -> anyhow::Result<Transcript> {
        let file = resolve_file(&store.path, id, store);
        read_transcript_file(&file, max_turns)
    }
}

/// Stream a single Claude/Codex `*.jsonl` transcript file into a [`Transcript`],
/// bounded to `max_turns`. Exposed `pub(crate)` so the Claude-app index reader
/// can follow a `cliSessionId` into the shared `~/.claude/projects` store and
/// reuse the exact same line-decoding (no divergent parsing). Fail-soft: an
/// unreadable file errors; a malformed/oversized line is skipped, not fatal.
pub(crate) fn read_transcript_file(file: &Path, max_turns: usize) -> anyhow::Result<Transcript> {
    let mut turns = Vec::new();
    if max_turns == 0 {
        return Ok(Transcript { turns });
    }
    // Gemini's legacy `.json` sessions are ONE pretty-printed JSON object with a
    // `messages[]` array — line-by-line decoding is meaningless for them (a
    // physical line is a fragment, and one stray valid line could yield a
    // partial transcript). Route `.json` straight to whole-file parsing; the
    // `.jsonl` streaming path below is left completely untouched (zero risk to
    // Claude/Codex/Copilot, whose stores are always `.jsonl`).
    if file.extension().and_then(|e| e.to_str()) == Some("json") {
        if let Some(msgs) = whole_file_messages(file) {
            for value in msgs.iter().take(max_turns) {
                if let Some(turn) = turn_from_value(value) {
                    turns.push(turn);
                }
            }
        }
        return Ok(Transcript { turns });
    }
    // Stream line-by-line and stop at `max_turns` — never load the whole
    // file (sessions reach 10 MB+). A pathologically long single line is
    // bounded by `MAX_LINE_BYTES` so a corrupt/huge line can't blow memory.
    let handle = std::fs::File::open(file)
        .map_err(|_| crate::BridgeError::Unreadable(file.to_path_buf()))?;
    let reader = BufReader::new(handle);
    for line in reader.lines() {
        // An I/O error mid-file ends the read with what we have, fail-soft.
        let Ok(line) = line else { break };
        let line = line.trim();
        if line.is_empty() || line.len() > MAX_LINE_BYTES {
            continue;
        }
        // Skip malformed lines; never abort the whole transcript.
        let value: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Some(turn) = turn_from_value(&value) {
            turns.push(turn);
            if turns.len() >= max_turns {
                break;
            }
        }
    }
    Ok(Transcript { turns })
}

/// Parse a whole file as a single JSON object and return its `messages` array,
/// for Gemini's legacy `chats/session-*.json` format. Bounded by file size so a
/// huge file can't be slurped; fail-soft (returns `None` on any problem).
fn whole_file_messages(file: &Path) -> Option<Vec<Value>> {
    // A conversation transcript object is small relative to a streamed JSONL log;
    // cap the whole-file read so this fallback can't blow memory on a stray large
    // `.json`.
    const MAX_WHOLE_FILE_BYTES: u64 = 16 * 1024 * 1024;
    let meta = std::fs::metadata(file).ok()?;
    if meta.len() > MAX_WHOLE_FILE_BYTES {
        return None;
    }
    let text = std::fs::read_to_string(file).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    value
        .get("messages")
        .and_then(Value::as_array)
        .map(|a| a.to_vec())
}

/// Enumerate candidate `*.jsonl` files for a store path.
///
/// Handles three layouts:
/// - `path` is a single `*.jsonl` file → just that file.
/// - `path` is a flat directory of `*.jsonl` files (Codex `…/YYYY/MM/DD/`).
/// - `path` is a directory of **project subdirectories** each holding
///   `*.jsonl` files (Claude `~/.claude/projects/<encoded-cwd>/<id>.jsonl`) →
///   recurse one level into the subdirectories.
fn enumerate_files(path: &Path) -> Vec<PathBuf> {
    if path.is_file() {
        return vec![path.to_path_buf()];
    }
    // Recurse to a bounded depth so both layouts are covered:
    //   Claude: `projects/<encoded-cwd>/*.jsonl`            (1 level)
    //   Codex:  `sessions/YYYY/MM/DD/rollout-*.jsonl`       (3 levels)
    const MAX_DEPTH: usize = 4;
    let mut files = Vec::new();
    collect_jsonl(path, MAX_DEPTH, &mut files);
    files
}

/// Append session files at `dir` and recurse into subdirs up to `depth`.
/// Read-only, fail-soft: an unreadable dir is skipped.
///
/// Collects `*.jsonl` everywhere. Also collects legacy `*.json` **only inside a
/// `chats/` directory** — that is Gemini CLI's pre-JSONL layout
/// (`~/.gemini/tmp/<token>/chats/session-*.json`, a single JSON object with a
/// `messages[]` array). Scoping the `.json` pickup to `chats/` keeps stray
/// config `.json` files (and other agents' stores, which have no `chats/` dir)
/// out of the listing — so this cannot disturb Claude/Codex/Copilot.
fn collect_jsonl(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let in_chats_dir = dir.file_name().is_some_and(|n| n == "chats");
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_file() {
            let ext = p.extension().and_then(|e| e.to_str());
            if ext == Some("jsonl") || (in_chats_dir && ext == Some("json")) {
                out.push(p);
            }
        } else if p.is_dir() && depth > 0 {
            collect_jsonl(&p, depth - 1, out);
        }
    }
}

/// Resolve a session `id` (file stem) back to its `*.jsonl` path, searching the
/// flat dir and one level of project subdirectories.
fn resolve_file(path: &Path, id: &str, _store: &SessionStore) -> PathBuf {
    if path.is_file() {
        return path.to_path_buf();
    }
    let flat = path.join(format!("{id}.jsonl"));
    if flat.is_file() {
        return flat;
    }
    // The id is a file stem; the file may be nested (Claude 1 level, Codex by
    // date). Match on the file stem OR — for layouts whose stem is generic
    // (Copilot `events`, Antigravity brain `transcript`) — on the derived
    // session-dir id, mirroring how `session_ref_for` assigns the id.
    enumerate_files(path)
        .into_iter()
        .find(|f| {
            let stem = f.file_stem().map(|s| s.to_string_lossy().into_owned());
            stem.as_deref() == Some(id) || session_id_from_dir(f).as_deref() == Some(id)
        })
        .unwrap_or(flat)
}

/// Some agents name EVERY session file the same generic stem and put the real
/// session id in an ancestor **directory** name, so the file stem is useless as
/// an id. This returns the real id for those layouts, or `None` for normal
/// `<id>.jsonl` files (where the stem IS the id).
///
/// Handled layouts:
/// - **Copilot CLI:** `…/session-state/<id>/events.jsonl` — id = the immediate
///   parent dir. (Stem is always `events`.)
/// - **Antigravity brain:** `…/brain/<id>/.system_generated/logs/transcript.jsonl`
///   — id = the dir 3 levels up. (Stem is always `transcript`.)
fn session_id_from_dir(file: &Path) -> Option<String> {
    let stem = file.file_stem()?.to_str()?;
    let session_dir = match stem {
        // Copilot: events.jsonl lives directly under the <id> dir.
        "events" => file.parent()?,
        // Antigravity brain: transcript.jsonl is 3 levels under the <id> dir.
        "transcript" => file.parent()?.parent()?.parent()?,
        // Normal layout — the file stem is the id.
        _ => return None,
    };
    session_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
}

/// Decode a Claude-encoded project directory name back into a filesystem path.
///
/// Claude names each project dir by replacing path separators with `-` and
/// prefixing a leading `-` (e.g. `-Users-ms-Developer-Bluey`). The encoding is
/// lossy — a real directory named `Claude-Design` encodes the same as a nested
/// `Claude/Design` — so we resolve it against the real filesystem: greedily
/// keep joining `-`-separated tokens into the current path component, only
/// descending when the longer component does not exist on disk. This recovers
/// hyphenated folder names (`Claude-Design`, `Job-Scraper`) correctly when they
/// exist, and falls back to the naive `-`→`/` split otherwise.
fn decode_project_dir(dir_name: &str) -> Option<String> {
    if !dir_name.starts_with('-') {
        return None;
    }
    let tokens: Vec<&str> = dir_name[1..].split('-').collect();
    if tokens.is_empty() {
        return None;
    }

    let mut path = std::path::PathBuf::from("/");
    let mut component = String::new();
    for (i, tok) in tokens.iter().enumerate() {
        let candidate = if component.is_empty() {
            (*tok).to_string()
        } else {
            format!("{component}-{tok}")
        };
        // Prefer extending the current component if the hyphenated form exists
        // on disk; otherwise treat the `-` as a path separator.
        let extended = path.join(&candidate);
        let is_last = i + 1 == tokens.len();
        if extended.exists() {
            component = candidate;
            if is_last {
                path = extended;
            }
        } else if !component.is_empty() {
            path.push(&component);
            component = (*tok).to_string();
            if is_last {
                path.push(&component);
            }
        } else {
            component = candidate;
            if is_last {
                path.push(&component);
            }
        }
    }
    Some(path.to_string_lossy().into_owned())
}

/// Build a [`SessionRef`] from a file: id = stem, updated_at = mtime epoch,
/// title = first user-message snippet, project = decoded parent dir name.
fn session_ref_for(file: &Path) -> Option<SessionRef> {
    // Most layouts name the file by session id (`<id>.jsonl`). Some put the id in
    // an ancestor dir and use a generic stem (Copilot `<id>/events.jsonl`,
    // Antigravity brain `<id>/…/transcript.jsonl`) — derive the id from the dir.
    let stem = file.file_stem()?.to_string_lossy().into_owned();
    let id = session_id_from_dir(file).unwrap_or(stem);
    let updated_at = mtime_epoch_string(file);
    // Project resolution, in priority order. Claude's encoded-cwd dir name
    // (`-Users-ms-…`) decodes directly. When that doesn't apply (the parent dir
    // is a date / `chats` / a UUID for Codex/Gemini/Copilot), fall back to a
    // format-specific extractor that reads the cwd the format DOES carry. Each
    // is gated by a structural cue (not an agent name) and is read-only +
    // fail-soft, so it degrades to `None` rather than erroring.
    let project = file
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .and_then(|name| decode_project_dir(&name))
        .or_else(|| project_from_format(file));
    // Title priority:
    //   1. Claude's own stored title — `customTitle` (user-set) or `aiTitle`
    //      (Claude-generated). These are clean, human-readable titles that
    //      Claude itself shows; ~18% of CLI sessions carry one. Prefer them.
    //   2. The first real-topic user message → cleaned.
    //   3. A project-based fallback so the row is never blank (Codex rollouts /
    //      sessions whose first turns are all boilerplate).
    let title = super::clean_title(stored_title(file), TITLE_SNIPPET_CHARS)
        .or_else(|| super::clean_title(first_user_snippet(file), TITLE_SNIPPET_CHARS))
        .or_else(|| super::fallback_label(project.as_deref(), &updated_at));
    Some(SessionRef {
        id,
        title,
        updated_at,
        project,
    })
}

/// Resolve a session's project/cwd from the on-disk format when the Claude
/// encoded-dir scheme doesn't apply. Tries each format-specific extractor; the
/// cues (file stem, parent-dir name, line-1 `type`) are structural, never an
/// agent name. Read-only, bounded, fail-soft → `None` on any miss.
fn project_from_format(file: &Path) -> Option<String> {
    // Codex: `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`, line 1 is
    // `{"type":"session_meta","payload":{"cwd":"…"}}`.
    if let Some(cwd) = codex_session_meta_cwd(file) {
        return Some(cwd);
    }
    // Copilot: `~/.copilot/session-state/<id>/events.jsonl` — the sibling
    // `workspace.yaml` carries `cwd:`.
    if let Some(cwd) = copilot_workspace_cwd(file) {
        return Some(cwd);
    }
    // Gemini: `~/.gemini/tmp/<token>/chats/*.jsonl` — `~/.gemini/projects.json`
    // maps a real cwd path to that token; invert it.
    if let Some(cwd) = gemini_token_cwd(file) {
        return Some(cwd);
    }
    None
}

/// Codex: read `payload.cwd` from the first-line `session_meta` record. Only the
/// first line is read (cheap). Gated on the record `type` being `session_meta`,
/// so it never fires for other formats.
fn codex_session_meta_cwd(file: &Path) -> Option<String> {
    let handle = std::fs::File::open(file).ok()?;
    let mut first = String::new();
    BufReader::new(handle).read_line(&mut first).ok()?;
    let v: Value = serde_json::from_str(first.trim()).ok()?;
    if v.get("type").and_then(Value::as_str) != Some("session_meta") {
        return None;
    }
    let cwd = v
        .get("payload")
        .and_then(|p| p.get("cwd"))
        .and_then(Value::as_str)?;
    (!cwd.is_empty()).then(|| cwd.to_string())
}

/// Copilot: read `cwd:` from the sibling `workspace.yaml` next to an
/// `events.jsonl`. Gated on the file stem being `events`. A minimal line-scan
/// (no YAML dep): the first `cwd: <path>` line wins.
fn copilot_workspace_cwd(file: &Path) -> Option<String> {
    if file.file_stem()?.to_str()? != "events" {
        return None;
    }
    let yaml = file.parent()?.join("workspace.yaml");
    let text = std::fs::read_to_string(&yaml).ok()?;
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("cwd:") {
            let cwd = rest.trim().trim_matches(['"', '\'']);
            if !cwd.is_empty() {
                return Some(cwd.to_string());
            }
        }
    }
    None
}

/// Gemini: the session lives in `…/tmp/<token>/chats/<file>.jsonl`; the token is
/// the grandparent dir name. `~/.gemini/projects.json` maps `{ "<cwd>": "<token>" }`
/// — invert it to recover the real cwd. Gated on the parent dir being `chats`.
/// Legacy SHA-named tmp dirs aren't in projects.json and resolve to `None`.
fn gemini_token_cwd(file: &Path) -> Option<String> {
    let parent = file.parent()?;
    if parent.file_name()?.to_str()? != "chats" {
        return None;
    }
    let token = parent.parent()?.file_name()?.to_str()?.to_string();
    // `…/tmp/<token>/chats` → the `.gemini` dir is three levels above the token.
    let gemini_dir = parent.parent()?.parent()?.parent()?;
    let projects_json = gemini_dir.join("projects.json");
    let text = std::fs::read_to_string(&projects_json).ok()?;
    let map: Value = serde_json::from_str(&text).ok()?;
    // Shape: a top-level object (or a `projects` sub-object) of cwd → token.
    let obj = map.get("projects").unwrap_or(&map).as_object()?;
    for (cwd, tok) in obj {
        if tok.as_str() == Some(token.as_str()) && !cwd.is_empty() {
            return Some(cwd.clone());
        }
    }
    None
}

/// Read Claude's own stored session title, if present: a `customTitle`
/// (user-set, via `/rename`) or `aiTitle` (Claude-generated) record. These are
/// emitted as dedicated JSONL lines, e.g. `{"type":"ai-title","aiTitle":"…"}`,
/// and re-emitted as the title is updated, so the LAST occurrence is the current
/// one. `customTitle` outranks `aiTitle`. Returns `None` for non-Claude formats
/// (Codex/Copilot/etc. have no such records) so their title path is unchanged.
///
/// Bounded by line COUNT, not just line bytes: a session can reach 10 MB+, so we
/// scan at most `MAX_TITLE_LINES_SCANNED` lines and keep the freshest title seen.
/// Real title records sit within this budget; the cap guarantees this never
/// stalls on a pathological file (the prior reader had no title-record path at
/// all, so this is strictly additive and fail-soft).
fn stored_title(file: &Path) -> Option<String> {
    const MAX_TITLE_LINES_SCANNED: usize = 50_000;
    let handle = std::fs::File::open(file).ok()?;
    let reader = BufReader::new(handle);
    let mut ai_title: Option<String> = None;
    let mut custom_title: Option<String> = None;
    for (scanned, line) in reader.lines().enumerate() {
        if scanned >= MAX_TITLE_LINES_SCANNED {
            break;
        }
        let Ok(line) = line else { break };
        let line = line.trim();
        // Cheap pre-filter: only parse lines that mention a title field. The vast
        // majority of lines (turns, tool calls) are skipped without JSON parsing.
        if line.len() > MAX_LINE_BYTES
            || !(line.contains("aiTitle") || line.contains("customTitle"))
        {
            continue;
        }
        let Ok(value): Result<Value, _> = serde_json::from_str(line) else {
            continue;
        };
        if let Some(s) = value.get("customTitle").and_then(Value::as_str) {
            if !s.trim().is_empty() {
                custom_title = Some(s.to_string()); // last wins
            }
        }
        if let Some(s) = value.get("aiTitle").and_then(Value::as_str) {
            if !s.trim().is_empty() {
                ai_title = Some(s.to_string()); // last wins
            }
        }
    }
    custom_title.or(ai_title)
}

/// Find the first user message that is a REAL topic (not boilerplate), to use
/// as a title. Streams line-by-line and skips session-continuation banners,
/// system prompts, and Bluey's own prompts ([`is_boilerplate_title`]), scanning
/// a bounded number of turns before giving up. Never loads the whole file.
fn first_user_snippet(file: &Path) -> Option<String> {
    // Bound how many user turns we inspect — a real topic is near the top; if
    // the first several are all boilerplate, fall back (project+date) rather
    // than scanning a 10 MB file.
    const MAX_USER_TURNS_SCANNED: usize = 8;
    let handle = std::fs::File::open(file).ok()?;
    let reader = BufReader::new(handle);
    let mut user_turns_seen = 0;
    for line in reader.lines() {
        let Ok(line) = line else { break };
        let line = line.trim();
        if line.is_empty() || line.len() > MAX_LINE_BYTES {
            continue;
        }
        let value: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Some(turn) = turn_from_value(&value) {
            if turn.role == Role::User && !turn.text.trim().is_empty() {
                if !super::is_boilerplate_title(&turn.text) {
                    return Some(turn.text); // raw; clean_title snippets it
                }
                user_turns_seen += 1;
                if user_turns_seen >= MAX_USER_TURNS_SCANNED {
                    return None;
                }
            }
        }
    }
    // Legacy single-object `.json` fallback (Gemini): same first-real-user-turn
    // search over the whole-file `messages[]`.
    if let Some(msgs) = whole_file_messages(file) {
        for value in msgs.iter().take(MAX_USER_TURNS_SCANNED * 2) {
            if let Some(turn) = turn_from_value(value) {
                if turn.role == Role::User
                    && !turn.text.trim().is_empty()
                    && !super::is_boilerplate_title(&turn.text)
                {
                    return Some(turn.text);
                }
            }
        }
    }
    None
}

/// Map one JSON event to a [`Turn`], tolerant of Claude and Codex shapes.
///
/// Recognized shapes (first match wins):
/// - Claude: `{"type":"user"|"assistant", "message":{"role":_, "content":_}}`
/// - Generic: `{"role":_, "content":_}` or `{"role":_, "text":_}`
/// - Codex rollout: `{"type":"response_item", "payload":{"type":"message",
///   "role":_, "content":[{"type":"input_text","text":_}]}}` — everything is
///   wrapped in `payload`, with text in `input_text`/`output_text` blocks.
///
/// Returns `None` for events with no usable role+text (tool calls, summaries,
/// system metadata) so they are silently skipped.
fn turn_from_value(value: &Value) -> Option<Turn> {
    // Codex wraps the real event under `payload`; unwrap it first so the same
    // role/content logic applies to Claude and Codex alike.
    let outer = value.get("payload").unwrap_or(value);
    // GitHub Copilot CLI nests role/content under a `data` object (sibling to
    // the dotted `type`), e.g. `{"type":"user.message","data":{"content":…}}`.
    // Descend into it so Copilot turns parse. This is a no-op for every other
    // vendor: Claude/Codex/Cursor lines have no top-level `data` key (verified
    // against real session stores), so `unwrap_or(outer)` returns `outer`
    // unchanged and cannot regress the working path. The role is still read
    // from the OUTER object's `type` (Copilot's dotted role lives there).
    let inner = outer.get("data").unwrap_or(outer);
    // Prefer a nested `message` object (Claude), else the (data-unwrapped) level.
    let msg = inner.get("message").unwrap_or(inner);

    let role_str = msg
        .get("role")
        .and_then(Value::as_str)
        .or_else(|| outer.get("type").and_then(Value::as_str))?;
    let role = match role_str {
        "user" => Role::User,
        "assistant" => Role::Assistant,
        "system" => Role::System,
        // Antigravity brain-transcript dialect: USER_INPUT / MODEL_* / etc.
        s if s.eq_ignore_ascii_case("user_input") => Role::User,
        s if s.starts_with("MODEL") || s.starts_with("ASSISTANT") => Role::Assistant,
        // Gemini CLI dialect: assistant turns are tagged "gemini".
        s if s.eq_ignore_ascii_case("gemini") => Role::Assistant,
        // GitHub Copilot CLI dialect: dotted event types like "user.message" /
        // "assistant.message". Match on the segment before the dot so the
        // shared reader recognizes Copilot turns instead of dropping them to
        // Role::Other (which left Copilot sessions untitleable).
        s if s.split('.').next() == Some("user") => Role::User,
        s if s.split('.').next() == Some("assistant") => Role::Assistant,
        _ => Role::Other,
    };

    let text = extract_text(msg).or_else(|| extract_text(inner))?;
    let text = text.trim().to_string();
    if text.is_empty() {
        return None;
    }
    Some(Turn { role, text })
}

/// Extract human-readable text from a message object's `content`/`text` field.
///
/// `content` may be a plain string or an array of content blocks (Anthropic
/// shape: `[{"type":"text","text":"…"}, …]`). Non-text blocks are ignored.
fn extract_text(msg: &Value) -> Option<String> {
    if let Some(s) = msg.get("text").and_then(Value::as_str) {
        if !s.is_empty() {
            return Some(s.to_string());
        }
    }
    match msg.get("content") {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(Value::Array(blocks)) => {
            let mut parts = Vec::new();
            for block in blocks {
                if let Some(s) = block.get("text").and_then(Value::as_str) {
                    if !s.is_empty() {
                        parts.push(s.to_string());
                    }
                } else if let Some(s) = block.as_str() {
                    if !s.is_empty() {
                        parts.push(s.to_string());
                    }
                }
            }
            if parts.is_empty() {
                None
            } else {
                Some(parts.join("\n"))
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SessionFormat;
    use std::io::Write;

    fn store_at(path: PathBuf) -> SessionStore {
        SessionStore {
            path,
            format: SessionFormat::Jsonl,
        }
    }

    #[test]
    fn read_parses_turns_skips_malformed_and_bounds_max_turns() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("sess-abc.jsonl");
        let mut f = std::fs::File::create(&file).expect("create");
        // Claude-style nested message, string content.
        writeln!(
            f,
            r#"{{"type":"user","message":{{"role":"user","content":"Hello there"}}}}"#
        )
        .unwrap();
        // Anthropic content-block array.
        writeln!(
            f,
            r#"{{"type":"assistant","message":{{"role":"assistant","content":[{{"type":"text","text":"Hi back"}}]}}}}"#
        )
        .unwrap();
        // Malformed line — must be skipped, not panic.
        writeln!(f, r#"{{not valid json,,,"#).unwrap();
        // Generic top-level shape.
        writeln!(f, r#"{{"role":"user","text":"Third turn"}}"#).unwrap();
        // A tool/system event with no usable text — skipped.
        writeln!(f, r#"{{"type":"tool_use","id":"t1"}}"#).unwrap();
        drop(f);

        let reader = JsonlReader;
        let store = store_at(dir.path().to_path_buf());

        let t = reader.read(&store, "sess-abc", 10).expect("read");
        assert_eq!(t.turns.len(), 3, "malformed + textless lines skipped");
        assert_eq!(t.turns[0].role, Role::User);
        assert_eq!(t.turns[0].text, "Hello there");
        assert_eq!(t.turns[1].role, Role::Assistant);
        assert_eq!(t.turns[1].text, "Hi back");
        assert_eq!(t.turns[2].text, "Third turn");

        // max_turns bound.
        let bounded = reader.read(&store, "sess-abc", 2).expect("read bounded");
        assert_eq!(bounded.turns.len(), 2);
    }

    #[test]
    fn list_enumerates_files_and_extracts_title() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("sess-1.jsonl");
        let mut f = std::fs::File::create(&file).expect("create");
        writeln!(
            f,
            r#"{{"type":"user","message":{{"role":"user","content":"What is the auth flow?"}}}}"#
        )
        .unwrap();
        drop(f);

        let reader = JsonlReader;
        let store = store_at(dir.path().to_path_buf());
        let refs = reader.list(&store, 10).expect("list");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].id, "sess-1");
        assert_eq!(refs[0].title.as_deref(), Some("What is the auth flow?"));
        assert!(refs[0].updated_at.parse::<u64>().is_ok());
    }

    #[test]
    fn decode_project_recovers_hyphenated_dir_that_exists_on_disk() {
        // Build a real folder tree:  <tmp>/My-Project  (hyphen is part of name)
        let dir = tempfile::tempdir().expect("tempdir");
        let base = dir.path();
        let project = base.join("My-Project");
        std::fs::create_dir_all(&project).expect("mkdir");

        // Encode it the way Claude would: leading '-', separators → '-'.
        let encoded = format!("-{}", base.join("My-Project").to_string_lossy()).replace('/', "-");
        let decoded = decode_project_dir(&encoded).expect("decoded");
        // The hyphen in "My-Project" must be preserved (folder exists on disk).
        assert!(
            decoded.ends_with("My-Project"),
            "expected hyphen preserved, got {decoded}"
        );
        assert!(
            !decoded.contains("My/Project"),
            "hyphen wrongly split: {decoded}"
        );
    }

    #[test]
    fn decode_project_falls_back_to_naive_split_when_absent() {
        // A path that does not exist → naive '-'→'/' split, never panics.
        let decoded = decode_project_dir("-no-such-path-here-xyz").expect("decoded");
        assert!(decoded.starts_with('/'));
    }

    #[test]
    fn claude_stored_title_beats_first_user_message() {
        // A Claude session whose first user turn is a real topic, but which also
        // carries Claude's own `ai-title` (and later a `customTitle`). The stored
        // title must win, and `customTitle` must outrank `aiTitle`. The freshest
        // (last) record wins.
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("sess-titled.jsonl");
        let mut f = std::fs::File::create(&file).expect("create");
        writeln!(
            f,
            r#"{{"type":"user","message":{{"role":"user","content":"fix the parser bug pls"}}}}"#
        )
        .unwrap();
        writeln!(
            f,
            r#"{{"type":"ai-title","aiTitle":"Stale title","sessionId":"x"}}"#
        )
        .unwrap();
        writeln!(
            f,
            r#"{{"type":"ai-title","aiTitle":"Fix the JSONL parser bug","sessionId":"x"}}"#
        )
        .unwrap();
        writeln!(
            f,
            r#"{{"type":"user","message":{{"role":"user","content":"another turn"}}}}"#
        )
        .unwrap();
        writeln!(f, r#"{{"customTitle":"My pinned name"}}"#).unwrap();
        drop(f);

        let reader = JsonlReader;
        let store = store_at(dir.path().to_path_buf());
        let refs = reader.list(&store, 10).expect("list");
        assert_eq!(
            refs[0].title.as_deref(),
            Some("My pinned name"),
            "customTitle outranks aiTitle and first-message"
        );

        // Without a customTitle, the freshest aiTitle wins over the first message.
        let file2 = dir.path().join("sess-ai.jsonl");
        let mut g = std::fs::File::create(&file2).expect("create");
        writeln!(
            g,
            r#"{{"type":"user","message":{{"role":"user","content":"fix the parser bug pls"}}}}"#
        )
        .unwrap();
        writeln!(
            g,
            r#"{{"type":"ai-title","aiTitle":"Fix the JSONL parser bug"}}"#
        )
        .unwrap();
        drop(g);
        let refs2 = reader
            .list(&store_at(dir.path().to_path_buf()), 10)
            .expect("list");
        let ai = refs2.iter().find(|r| r.id == "sess-ai").expect("found");
        assert_eq!(ai.title.as_deref(), Some("Fix the JSONL parser bug"));

        // A session with NO stored title falls back to the first user message
        // exactly as before (no-op proof for the 82% of sessions without one).
        let file3 = dir.path().join("sess-plain.jsonl");
        let mut h = std::fs::File::create(&file3).expect("create");
        writeln!(
            h,
            r#"{{"type":"user","message":{{"role":"user","content":"What is the auth flow?"}}}}"#
        )
        .unwrap();
        drop(h);
        let refs3 = reader
            .list(&store_at(dir.path().to_path_buf()), 10)
            .expect("list");
        let plain = refs3.iter().find(|r| r.id == "sess-plain").expect("found");
        assert_eq!(plain.title.as_deref(), Some("What is the auth flow?"));
    }

    #[test]
    fn copilot_data_nested_turns_parse_and_claude_codex_unaffected() {
        // GitHub Copilot CLI: role from dotted `type`, text under `data.content`.
        // Previously dropped (we only unwrapped `payload`/`message`) → 0 turns,
        // 0 titles. The `data` unwrap must now surface these.
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("copilot-x.jsonl");
        let mut f = std::fs::File::create(&file).expect("create");
        writeln!(
            f,
            r#"{{"type":"system.message","data":{{"role":"system","content":"You are the GitHub Copilot CLI"}},"id":"a","timestamp":"t"}}"#
        )
        .unwrap();
        writeln!(
            f,
            r#"{{"type":"user.message","data":{{"content":"reply with LIVEPROOF7"}},"id":"b","timestamp":"t"}}"#
        )
        .unwrap();
        writeln!(
            f,
            r#"{{"type":"assistant.message","data":{{"content":"LIVEPROOF7","model":"claude-haiku-4.5"}},"id":"c"}}"#
        )
        .unwrap();
        // Lifecycle line with no text — still skipped.
        writeln!(f, r#"{{"type":"session.start","data":{{"cwd":"/x"}}}}"#).unwrap();
        drop(f);

        let reader = JsonlReader;
        let store = store_at(dir.path().to_path_buf());
        let t = reader.read(&store, "copilot-x", 10).expect("read");
        assert_eq!(
            t.turns.len(),
            3,
            "system+user+assistant parse, lifecycle skipped"
        );
        assert_eq!(t.turns[0].role, Role::System);
        assert_eq!(t.turns[1].role, Role::User);
        assert_eq!(t.turns[1].text, "reply with LIVEPROOF7");
        assert_eq!(t.turns[2].role, Role::Assistant);
        assert_eq!(t.turns[2].text, "LIVEPROOF7");

        // Title derives from the first user turn (was 0 before the fix).
        let refs = reader.list(&store, 10).expect("list");
        assert_eq!(refs[0].title.as_deref(), Some("reply with LIVEPROOF7"));

        // No-op proof: Claude (`message`-nested) and Codex (`payload`-nested)
        // shapes have no top-level `data` key, so they parse exactly as before.
        let claude = serde_json::json!({
            "type":"user","message":{"role":"user","content":"claude turn"}
        });
        let codex = serde_json::json!({
            "type":"response_item",
            "payload":{"type":"message","role":"assistant",
                       "content":[{"type":"output_text","text":"codex turn"}]}
        });
        let gemini = serde_json::json!({"type":"gemini","content":"gemini reply"});
        assert_eq!(turn_from_value(&claude).unwrap().text, "claude turn");
        assert_eq!(turn_from_value(&claude).unwrap().role, Role::User);
        assert_eq!(turn_from_value(&codex).unwrap().text, "codex turn");
        assert_eq!(turn_from_value(&codex).unwrap().role, Role::Assistant);
        // Gemini assistant role mapping.
        assert_eq!(turn_from_value(&gemini).unwrap().role, Role::Assistant);
    }

    #[test]
    fn project_resolved_from_format_for_codex_copilot_gemini() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();

        // --- Codex: rollout with a session_meta line carrying payload.cwd ---
        let codex = root.join("2026").join("06").join("15");
        std::fs::create_dir_all(&codex).unwrap();
        let codex_file = codex.join("rollout-2026-06-15T00-00-00-abc.jsonl");
        std::fs::write(
            &codex_file,
            "{\"type\":\"session_meta\",\"payload\":{\"cwd\":\"/Users/ms/Developer/Bluey\"}}\n{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"hi\"}]}}\n",
        )
        .unwrap();
        assert_eq!(
            session_ref_for(&codex_file)
                .and_then(|r| r.project)
                .as_deref(),
            Some("/Users/ms/Developer/Bluey"),
            "Codex project comes from session_meta.payload.cwd"
        );

        // --- Copilot: events.jsonl with a sibling workspace.yaml `cwd:` ---
        let cop = root.join("session-state").join("uuid-xyz");
        std::fs::create_dir_all(&cop).unwrap();
        std::fs::write(
            cop.join("events.jsonl"),
            "{\"type\":\"user.message\",\"data\":{\"content\":\"hello\"}}\n",
        )
        .unwrap();
        std::fs::write(
            cop.join("workspace.yaml"),
            "name: thing\ncwd: /Users/ms/Desktop/Heyloo\ngit_root: /Users/ms/Desktop/Heyloo\n",
        )
        .unwrap();
        let cop_file = cop.join("events.jsonl");
        assert_eq!(
            session_ref_for(&cop_file)
                .and_then(|r| r.project)
                .as_deref(),
            Some("/Users/ms/Desktop/Heyloo"),
            "Copilot project comes from sibling workspace.yaml cwd"
        );

        // --- Gemini: chats/*.jsonl under <token>, projects.json maps cwd→token ---
        let gem_root = root.join(".gemini");
        let chats = gem_root.join("tmp").join("bluey").join("chats");
        std::fs::create_dir_all(&chats).unwrap();
        std::fs::write(
            gem_root.join("projects.json"),
            "{\"/Users/ms/Developer/Bluey\":\"bluey\",\"/Users/ms\":\"ms\"}",
        )
        .unwrap();
        let gem_file = chats.join("session-2026-06-15T00-00-abcd.jsonl");
        std::fs::write(&gem_file, "{\"type\":\"user\",\"content\":\"hi\"}\n").unwrap();
        assert_eq!(
            session_ref_for(&gem_file)
                .and_then(|r| r.project)
                .as_deref(),
            Some("/Users/ms/Developer/Bluey"),
            "Gemini project comes from inverting projects.json token map"
        );
    }

    #[test]
    fn copilot_session_id_is_parent_dir_not_events_stem() {
        // Copilot CLI: `session-state/<UUID>/events.jsonl`. The id must be the
        // <UUID> parent dir, NOT the stem "events" (which would collide across
        // every session). read() must also resolve by that derived id.
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("session-state");
        let uuid = "6ff60527-b096-4a34-a875-fbdf71c6d61e";
        let sess = root.join(uuid);
        std::fs::create_dir_all(&sess).unwrap();
        std::fs::write(
            sess.join("events.jsonl"),
            "{\"type\":\"user.message\",\"data\":{\"content\":\"hello copilot\"}}\n",
        )
        .unwrap();

        let reader = JsonlReader;
        let store = store_at(dir.path().to_path_buf());
        let refs = reader.list(&store, 10).expect("list");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].id, uuid, "id is the UUID dir, not 'events'");
        assert_eq!(refs[0].title.as_deref(), Some("hello copilot"));

        // read() resolves by the derived UUID id (not the stem).
        let t = reader.read(&store, uuid, 10).expect("read");
        assert_eq!(t.turns.len(), 1);
        assert_eq!(t.turns[0].text, "hello copilot");
    }

    #[test]
    fn gemini_legacy_json_single_object_with_messages_array() {
        // Gemini's pre-JSONL format: ONE pretty-printed JSON object with a
        // `messages[]` array, living in a `chats/` dir. The line-oriented reader
        // yields nothing; the whole-file fallback must decode it. `.json` is only
        // picked up inside `chats/`.
        let dir = tempfile::tempdir().expect("tempdir");
        let chats = dir.path().join("tok").join("chats");
        std::fs::create_dir_all(&chats).unwrap();
        let file = chats.join("session-2026-06-01T10-00-abcd1234.json");
        std::fs::write(
            &file,
            r#"{
  "sessionId": "abcd1234",
  "projectHash": "tok",
  "startTime": "2026-06-01T10:00:00Z",
  "lastUpdated": "2026-06-01T10:05:00Z",
  "messages": [
    {"id":"1","timestamp":"t","type":"user","content":"set up the registry row"},
    {"id":"2","timestamp":"t","type":"gemini","content":"Done."}
  ]
}"#,
        )
        .unwrap();
        // A stray top-level .json (NOT in chats/) must be ignored.
        std::fs::write(dir.path().join("settings.json"), r#"{"theme":"dark"}"#).unwrap();

        let reader = JsonlReader;
        let store = store_at(dir.path().to_path_buf());

        // list: the legacy session is found and titled from its first user msg;
        // settings.json is not picked up.
        let refs = reader.list(&store, 10).expect("list");
        assert_eq!(
            refs.len(),
            1,
            "only the chats/*.json session, not settings.json"
        );
        assert_eq!(refs[0].id, "session-2026-06-01T10-00-abcd1234");
        assert_eq!(refs[0].title.as_deref(), Some("set up the registry row"));

        // read: both turns decode, gemini → Assistant.
        let t = reader
            .read(&store, "session-2026-06-01T10-00-abcd1234", 10)
            .expect("read");
        assert_eq!(t.turns.len(), 2);
        assert_eq!(t.turns[0].role, Role::User);
        assert_eq!(t.turns[0].text, "set up the registry row");
        assert_eq!(t.turns[1].role, Role::Assistant);
        assert_eq!(t.turns[1].text, "Done.");
    }

    #[test]
    fn list_respects_limit() {
        let dir = tempfile::tempdir().expect("tempdir");
        for i in 0..5 {
            let file = dir.path().join(format!("s{i}.jsonl"));
            let mut f = std::fs::File::create(&file).expect("create");
            writeln!(f, r#"{{"role":"user","content":"hi {i}"}}"#).unwrap();
        }
        let reader = JsonlReader;
        let store = store_at(dir.path().to_path_buf());
        let refs = reader.list(&store, 2).expect("list");
        assert_eq!(refs.len(), 2);
    }
}
