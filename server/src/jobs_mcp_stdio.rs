//! Minimal local MCP stdio transport for the Jobs read boundary.
//!
//! This module deliberately has no credential, database, process, or network
//! knowledge.  Its caller supplies an already-authorized account-scoped reader.
//! Only JSON-RPC messages are emitted to the supplied output writer.

use std::{
    fmt,
    io::{self, BufRead, Write},
};

use serde::{
    de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor},
    Serialize,
};
use serde_json::{json, Map, Value};

const PROTOCOL_VERSION: &str = "2025-11-25";
const TOOL_NAME: &str = "get_job_matches";
const DISCOVERY_SHORTLIST_PROJECTION: &str = "current_track_discovery_shortlist";
const MAX_FRAME_BYTES: usize = 8 * 1024;
const MAX_SESSION_BYTES: usize = 256 * 1024;
const MAX_SESSION_FRAMES: usize = 256;
// Leave room for initialize, initialized, tools/list and retries even
// when the client reads one lead per page.
pub(crate) const MAX_SHORTLIST_ROWS: usize = 240;
const _: () = assert!(MAX_SHORTLIST_ROWS + 3 <= MAX_SESSION_FRAMES);
// Reserve space for ten maximum-sized projected rows, each with worst-case
// JSON escaping in both structured JSON and its text-only client equivalent.
const MAX_OUTPUT_BYTES: usize = 160 * 1024;
const CURSOR_PREFIX: &str = "bjmcp1_";
const CURSOR_HEX_BYTES: usize = 64;

/// Closed transport input. The reader derives the authorized track; callers
/// cannot name account, grant, provider, or track coordinates on stdio.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MatchRequest<'a> {
    Start { limit: usize },
    Continue { cursor: &'a str },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiscoveryLead {
    job_id: String,
    title: String,
    company: String,
    location: String,
    match_score: i64,
}

impl DiscoveryLead {
    pub(crate) fn new(
        job_id: String,
        title: String,
        company: String,
        location: String,
        match_score: i64,
    ) -> Self {
        Self {
            job_id,
            title,
            company,
            location,
            match_score,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MatchResult {
    track_id: String,
    projection: &'static str,
    requires_original_verification: bool,
    matches: Vec<DiscoveryLead>,
    next_cursor: Option<String>,
    shortlist_truncated: bool,
}

impl MatchResult {
    pub(crate) fn discovery_shortlist(
        track_id: String,
        matches: Vec<DiscoveryLead>,
        next_cursor: Option<String>,
        shortlist_truncated: bool,
    ) -> Self {
        Self {
            track_id,
            projection: DISCOVERY_SHORTLIST_PROJECTION,
            requires_original_verification: true,
            matches,
            next_cursor,
            shortlist_truncated,
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Phase {
    New,
    Initializing,
    Ready,
}

enum Frame {
    Complete(Vec<u8>),
    Oversized,
    Incomplete,
}

// Value's normal map parser replaces an earlier duplicate. Reject ambiguity
// at any depth before dispatching a method or passing arguments to the reader.
struct StrictJson(Value);

impl<'de> Deserialize<'de> for StrictJson {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct StrictVisitor;
        impl<'de> Visitor<'de> for StrictVisitor {
            type Value = StrictJson;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("unambiguous JSON")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<StrictJson, A::Error> {
                let mut output = Map::new();
                while let Some((key, StrictJson(value))) = map.next_entry::<String, StrictJson>()? {
                    if output.insert(key, value).is_some() {
                        return Err(de::Error::custom("duplicate JSON field"));
                    }
                }
                Ok(StrictJson(Value::Object(output)))
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<StrictJson, A::Error> {
                let mut output = Vec::new();
                while let Some(StrictJson(value)) = seq.next_element()? {
                    output.push(value);
                }
                Ok(StrictJson(Value::Array(output)))
            }

            fn visit_unit<E: de::Error>(self) -> Result<StrictJson, E> {
                Ok(StrictJson(Value::Null))
            }

            fn visit_bool<E: de::Error>(self, value: bool) -> Result<StrictJson, E> {
                Ok(StrictJson(value.into()))
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> Result<StrictJson, E> {
                Ok(StrictJson(value.into()))
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<StrictJson, E> {
                Ok(StrictJson(value.into()))
            }

            fn visit_f64<E: de::Error>(self, value: f64) -> Result<StrictJson, E> {
                Ok(StrictJson(value.into()))
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<StrictJson, E> {
                Ok(StrictJson(value.into()))
            }
        }
        deserializer.deserialize_any(StrictVisitor)
    }
}

fn only(object: &Map<String, Value>, expected: &[&str]) -> bool {
    object.keys().all(|key| expected.contains(&key.as_str()))
}

fn bounded_string(value: Option<&Value>, maximum: usize) -> Option<&str> {
    let value = value?.as_str()?;
    (value.len() <= maximum && !value.is_empty()).then_some(value)
}

fn valid_id(value: &Value) -> bool {
    bounded_string(Some(value), 128).is_some()
        || value.as_i64().is_some_and(|number| {
            (-9_007_199_254_740_991..=9_007_199_254_740_991).contains(&number)
        })
        || value
            .as_u64()
            .is_some_and(|number| number <= 9_007_199_254_740_991)
}

fn valid_projected_id(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b':'))
}

fn valid_projected_text(value: &str, maximum: usize, allow_empty: bool) -> bool {
    value.len() <= maximum
        && (allow_empty || !value.trim().is_empty())
        && !value.chars().any(char::is_control)
}

fn valid_projected_lead(lead: &DiscoveryLead) -> bool {
    valid_projected_id(&lead.job_id, 128)
        && valid_projected_text(&lead.title, 1024, false)
        && valid_projected_text(&lead.company, 768, false)
        && valid_projected_text(&lead.location, 512, true)
        && (0..=100).contains(&lead.match_score)
}

fn valid_cursor(value: &str) -> bool {
    value.len() == CURSOR_PREFIX.len() + CURSOR_HEX_BYTES
        && value.starts_with(CURSOR_PREFIX)
        && value[CURSOR_PREFIX.len()..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn canonical_cursor(value: Option<&Value>) -> Option<&str> {
    let value = bounded_string(value, CURSOR_PREFIX.len() + CURSOR_HEX_BYTES)?;
    valid_cursor(value).then_some(value)
}

fn valid_result(result: &MatchResult) -> bool {
    valid_projected_id(&result.track_id, 64)
        && result.matches.len() <= 10
        && result.matches.iter().all(valid_projected_lead)
        && match result.next_cursor.as_deref() {
            None => true,
            Some(cursor) => valid_cursor(cursor),
        }
}

fn no_params(value: Option<&Value>) -> bool {
    value.is_none() || value.is_some_and(|value| value.as_object().is_some_and(Map::is_empty))
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn success(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn write_message<W: Write>(output: &mut W, message: &Value) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(message)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    bytes.push(b'\n');
    if bytes.len() > MAX_OUTPUT_BYTES {
        let fallback = error(
            message.get("id").cloned().unwrap_or(Value::Null),
            -32000,
            "Output limit reached",
        );
        bytes = serde_json::to_vec(&fallback)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        bytes.push(b'\n');
    }
    output.write_all(&bytes)?;
    output.flush()
}

fn read_frame<R: BufRead>(input: &mut R, session_bytes: &mut usize) -> io::Result<Option<Frame>> {
    let mut frame = Vec::new();
    let mut oversized = false;
    loop {
        let buffer = input.fill_buf()?;
        if buffer.is_empty() {
            return Ok((!frame.is_empty() || oversized).then_some(Frame::Incomplete));
        }
        let newline = buffer.iter().position(|byte| *byte == b'\n');
        let consumed = newline.map_or(buffer.len(), |position| position + 1);
        *session_bytes = session_bytes.saturating_add(consumed);
        if *session_bytes > MAX_SESSION_BYTES {
            input.consume(consumed);
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "session input limit reached",
            ));
        }
        let payload = &buffer[..newline.unwrap_or(buffer.len())];
        if !oversized {
            if frame.len().saturating_add(payload.len()) > MAX_FRAME_BYTES {
                frame.clear();
                oversized = true;
            } else {
                frame.extend_from_slice(payload);
            }
        }
        input.consume(consumed);
        if newline.is_some() {
            return Ok(Some(if oversized {
                Frame::Oversized
            } else {
                Frame::Complete(frame)
            }));
        }
    }
}

fn valid_initialize(params: Option<&Value>) -> bool {
    let Some(params) = params.and_then(Value::as_object) else {
        return false;
    };
    if !only(params, &["protocolVersion", "capabilities", "clientInfo"])
        || params.get("protocolVersion").and_then(Value::as_str) != Some(PROTOCOL_VERSION)
        || !params.get("capabilities").is_some_and(Value::is_object)
    {
        return false;
    }
    let Some(client) = params.get("clientInfo").and_then(Value::as_object) else {
        return false;
    };
    only(client, &["name", "version"])
        && bounded_string(client.get("name"), 128).is_some()
        && bounded_string(client.get("version"), 128).is_some()
}

fn list_params(params: Option<&Value>) -> bool {
    no_params(params)
        || params.is_some_and(|params| {
            params.as_object().is_some_and(|object| {
                only(object, &["cursor"])
                    && object.get("cursor").and_then(Value::as_str) == Some("")
            })
        })
}

fn tool_definition() -> Value {
    json!({
        "name": TOOL_NAME,
        "title": "Read Jobs discovery leads",
        "description": "Reads a current discovery shortlist for an authorized Career Track. Every result is a lead that requires original-posting verification; this tool does not establish that a job is open or eligible, and it never writes or applies.",
        "inputSchema": {
            "type": "object",
            "oneOf": [
                {"additionalProperties": false,
                 "properties": {"limit": {"type": "integer", "minimum": 1, "maximum": 10}}},
                {"required": ["cursor"], "additionalProperties": false,
                 "properties": {"cursor": {"type": "string", "minLength": 71,
                 "maxLength": 71, "pattern": "^bjmcp1_[0-9a-f]{64}$"}}}
            ]
        },
        "outputSchema": {
            "type": "object",
            "required": ["trackId", "projection", "requiresOriginalVerification", "matches", "nextCursor", "shortlistTruncated"],
            "additionalProperties": false,
            "properties": {
                "trackId": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": 64,
                    "pattern": "^[A-Za-z0-9_.:-]+$"
                },
                "projection": {"type": "string", "const": DISCOVERY_SHORTLIST_PROJECTION},
                "requiresOriginalVerification": {"type": "boolean", "const": true},
                "nextCursor": {"oneOf": [
                    {"type": "null"},
                    {"type": "string", "minLength": 71, "maxLength": 71, "pattern": "^bjmcp1_[0-9a-f]{64}$"}
                ]},
                "shortlistTruncated": {"type": "boolean"},
                "matches": {
                    "type": "array",
                    "maxItems": 10,
                    "items": {
                        "type": "object",
                        "required": ["jobId", "title", "company", "location", "matchScore"],
                        "additionalProperties": false,
                        "properties": {
                            "jobId": {
                                "type": "string",
                                "minLength": 1,
                                "maxLength": 128,
                                "pattern": "^[A-Za-z0-9_.:-]+$"
                            },
                            "title": {"type": "string", "minLength": 1, "maxLength": 1024},
                            "company": {"type": "string", "minLength": 1, "maxLength": 768},
                            "location": {"type": "string", "maxLength": 512},
                            "matchScore": {"type": "integer", "minimum": 0, "maximum": 100}
                        }
                    }
                }
            }
        },
        "annotations": {
            "readOnlyHint": true,
            "destructiveHint": false,
            "idempotentHint": true,
            "openWorldHint": false
        }
    })
}

fn tool_call<F>(params: Option<&Value>, reader: &mut F) -> Result<Value, Value>
where
    F: FnMut(MatchRequest<'_>) -> Result<Option<MatchResult>, ()>,
{
    let Some(params) = params.and_then(Value::as_object) else {
        return Err(error(Value::Null, -32602, "Invalid match parameters"));
    };
    if !only(params, &["name", "arguments"])
        || params.get("name").and_then(Value::as_str) != Some(TOOL_NAME)
    {
        return Err(error(
            Value::Null,
            -32602,
            "Unknown tool or invalid parameters",
        ));
    }
    let Some(arguments) = params.get("arguments").and_then(Value::as_object) else {
        return Err(error(Value::Null, -32602, "Invalid match parameters"));
    };
    if !only(arguments, &["limit", "cursor"]) {
        return Err(error(Value::Null, -32602, "Invalid match parameters"));
    }
    let request = match (arguments.get("limit"), arguments.get("cursor")) {
        (Some(_), Some(_)) => return Err(error(Value::Null, -32602, "Invalid match parameters")),
        (None, Some(_)) => MatchRequest::Continue {
            cursor: canonical_cursor(arguments.get("cursor"))
                .ok_or_else(|| error(Value::Null, -32602, "Invalid match parameters"))?,
        },
        (limit, None) => MatchRequest::Start {
            limit: match limit {
                None => 10,
                Some(value) => value
                    .as_u64()
                    .filter(|limit| (1..=10).contains(limit))
                    .map(|limit| limit as usize)
                    .ok_or_else(|| error(Value::Null, -32602, "Invalid match parameters"))?,
            },
        },
    };
    let unavailable = || {
        json!({
            "isError": true,
            "content": [{"type": "text", "text": "Career Track unavailable"}]
        })
    };
    match reader(request) {
        Ok(Some(result)) if valid_result(&result) => {
            let Ok(result) = serde_json::to_value(result) else {
                return Ok(unavailable());
            };
            Ok(json!({
                "isError": false,
                "structuredContent": result,
                "content": [{"type": "text", "text": result.to_string()}]
            }))
        }
        _ => Ok(unavailable()),
    }
}

/// Serves one 2025-11-25 MCP stdio session over a caller-provided reader and writer.
///
/// The injected `reader` must enforce account/grant/revocation authority. `None`
/// is deliberately indistinguishable from a backend refusal to avoid track-oracles.
pub(crate) fn run<R: BufRead, W: Write, F>(
    mut input: R,
    mut output: W,
    mut reader: F,
) -> io::Result<()>
where
    F: FnMut(MatchRequest<'_>) -> Result<Option<MatchResult>, ()>,
{
    let mut phase = Phase::New;
    let mut session_bytes = 0;
    let mut frames = 0;
    while let Some(frame) = match read_frame(&mut input, &mut session_bytes) {
        Ok(frame) => frame,
        Err(_) => {
            write_message(
                &mut output,
                &error(Value::Null, -32000, "Input limit reached"),
            )?;
            return Ok(());
        }
    } {
        frames += 1;
        if frames > MAX_SESSION_FRAMES {
            write_message(
                &mut output,
                &error(Value::Null, -32000, "Frame limit reached"),
            )?;
            return Ok(());
        }
        let value = match frame {
            Frame::Oversized => {
                write_message(
                    &mut output,
                    &error(Value::Null, -32700, "Frame exceeds limit"),
                )?;
                continue;
            }
            Frame::Incomplete => {
                write_message(&mut output, &error(Value::Null, -32700, "Incomplete frame"))?;
                return Ok(());
            }
            Frame::Complete(frame) => match serde_json::from_slice::<StrictJson>(&frame) {
                Ok(StrictJson(value)) => value,
                Err(_) => {
                    write_message(
                        &mut output,
                        &error(Value::Null, -32700, "Unable to parse frame"),
                    )?;
                    continue;
                }
            },
        };
        let Some(request) = value.as_object() else {
            write_message(&mut output, &error(Value::Null, -32600, "Invalid request"))?;
            continue;
        };
        if !only(request, &["jsonrpc", "id", "method", "params"])
            || request.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
            || bounded_string(request.get("method"), 128).is_none()
            || request.get("id").is_some_and(|id| !valid_id(id))
        {
            write_message(&mut output, &error(Value::Null, -32600, "Invalid request"))?;
            continue;
        }
        let method = request
            .get("method")
            .and_then(Value::as_str)
            .expect("validated method");
        let id = request.get("id").cloned();
        let params = request.get("params");
        if id.is_none() {
            if method == "notifications/initialized"
                && phase == Phase::Initializing
                && no_params(params)
            {
                phase = Phase::Ready;
            }
            continue;
        }
        let id = id.expect("present request id");
        let response = if method == "ping" && no_params(params) {
            success(id, json!({}))
        } else if method == "initialize" {
            if phase != Phase::New {
                error(id, -32600, "Session already initialized")
            } else if !valid_initialize(params) {
                error(id, -32602, "Invalid initialize parameters")
            } else {
                phase = Phase::Initializing;
                success(
                    id,
                    json!({
                        "protocolVersion": PROTOCOL_VERSION,
                        "capabilities": {"tools": {"listChanged": false}},
                        "serverInfo": {"name": "bluey-jobs-mcp", "version": "1.0.0"}
                    }),
                )
            }
        } else if phase != Phase::Ready {
            error(id, -32002, "Session is not initialized")
        } else if method == "tools/list" {
            if list_params(params) {
                success(id, json!({"tools": [tool_definition()]}))
            } else {
                error(id, -32602, "Invalid list parameters")
            }
        } else if method == "tools/call" {
            match tool_call(params, &mut reader) {
                Ok(result) => success(id, result),
                Err(mut invalid) => {
                    invalid["id"] = id;
                    invalid
                }
            }
        } else {
            error(id, -32601, "Method not found")
        };
        write_message(&mut output, &response)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn request(id: i64, method: &str, params: Value) -> Value {
        json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
    }

    fn lead(
        job_id: &str,
        title: &str,
        company: &str,
        location: &str,
        match_score: i64,
    ) -> DiscoveryLead {
        DiscoveryLead::new(
            job_id.to_string(),
            title.to_string(),
            company.to_string(),
            location.to_string(),
            match_score,
        )
    }

    fn shortlist(track_id: &str, matches: Vec<DiscoveryLead>) -> MatchResult {
        MatchResult::discovery_shortlist(track_id.to_string(), matches, None, false)
    }

    fn object_keys(value: &Value) -> Vec<String> {
        let mut keys = value
            .as_object()
            .expect("object")
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        keys.sort();
        keys
    }

    fn run_records(records: Vec<Value>) -> (Vec<Value>, usize) {
        let input = records
            .into_iter()
            .map(|record| format!("{}\n", record))
            .collect::<String>();
        let mut output = Vec::new();
        let mut calls = 0;
        run(Cursor::new(input), &mut output, |request| {
            calls += 1;
            assert_eq!(request, MatchRequest::Start { limit: 2 });
            Ok(Some(shortlist(
                "track-alpha",
                vec![lead("job-1", "Data Engineer", "Example Corp", "Remote", 91)],
            )))
        })
        .expect("stdio run");
        let messages = String::from_utf8(output)
            .expect("utf8 output")
            .lines()
            .map(|line| serde_json::from_str(line).expect("json-rpc output"))
            .collect();
        (messages, calls)
    }

    #[test]
    fn initialized_session_lists_one_tool_and_calls_authorized_reader() {
        let (messages, calls) = run_records(vec![
            request(
                1,
                "initialize",
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": {"name": "test", "version": "1"}
                }),
            ),
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            request(2, "tools/list", json!({})),
            request(
                3,
                "tools/call",
                json!({
                    "name": TOOL_NAME,
                    "arguments": {"limit": 2}
                }),
            ),
        ]);
        assert_eq!(calls, 1);
        assert_eq!(
            messages
                .iter()
                .map(|message| message["id"].clone())
                .collect::<Vec<_>>(),
            vec![json!(1), json!(2), json!(3)]
        );
        assert_eq!(messages[0]["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(messages[1]["result"]["tools"][0]["name"], TOOL_NAME);
        let result = &messages[2]["result"];
        let structured = &result["structuredContent"];
        assert_eq!(structured["trackId"], "track-alpha");
        assert_eq!(structured["nextCursor"], Value::Null);
        assert_eq!(structured["shortlistTruncated"], false);
        assert_eq!(structured["projection"], DISCOVERY_SHORTLIST_PROJECTION);
        assert_eq!(structured["requiresOriginalVerification"], true);
        assert_eq!(structured["matches"][0]["jobId"], "job-1");
        assert_eq!(
            object_keys(structured),
            vec![
                "matches".to_string(),
                "nextCursor".to_string(),
                "projection".to_string(),
                "requiresOriginalVerification".to_string(),
                "shortlistTruncated".to_string(),
                "trackId".to_string()
            ]
        );
        assert_eq!(
            object_keys(&structured["matches"][0]),
            vec![
                "company".to_string(),
                "jobId".to_string(),
                "location".to_string(),
                "matchScore".to_string(),
                "title".to_string()
            ]
        );
        assert!(structured["matches"][0].get("status").is_none());
        let text = result["content"][0]["text"].as_str().expect("text result");
        assert_eq!(serde_json::from_str::<Value>(text).unwrap(), *structured);
    }

    #[test]
    fn tool_schema_is_closed_and_matches_the_typed_discovery_result() {
        let tool = tool_definition();
        assert_eq!(tool["title"], "Read Jobs discovery leads");
        let description = tool["description"].as_str().expect("description");
        assert!(description.contains("requires original-posting verification"));
        assert!(description.contains("does not establish that a job is open or eligible"));
        assert_eq!(
            tool["inputSchema"],
            json!({
                "type": "object",
                "oneOf": [
                    {"additionalProperties": false,
                     "properties": {"limit": {"type": "integer", "minimum": 1, "maximum": 10}}},
                    {"required": ["cursor"], "additionalProperties": false,
                     "properties": {"cursor": {"type": "string", "minLength": 71,
                     "maxLength": 71, "pattern": "^bjmcp1_[0-9a-f]{64}$"}}}
                ]
            })
        );
        assert_eq!(
            tool["outputSchema"],
            json!({
                "type": "object",
                "required": [
                    "trackId",
                    "projection",
                    "requiresOriginalVerification",
                    "matches",
                    "nextCursor",
                    "shortlistTruncated"
                ],
                "additionalProperties": false,
                "properties": {
                    "trackId": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": 64,
                        "pattern": "^[A-Za-z0-9_.:-]+$"
                    },
                    "projection": {
                        "type": "string",
                        "const": DISCOVERY_SHORTLIST_PROJECTION
                    },
                    "requiresOriginalVerification": {"type": "boolean", "const": true},
                    "nextCursor": {"oneOf": [
                        {"type": "null"},
                        {"type": "string", "minLength": 71, "maxLength": 71, "pattern": "^bjmcp1_[0-9a-f]{64}$"}
                    ]},
                    "shortlistTruncated": {"type": "boolean"},
                    "matches": {
                        "type": "array",
                        "maxItems": 10,
                        "items": {
                            "type": "object",
                            "required": [
                                "jobId",
                                "title",
                                "company",
                                "location",
                                "matchScore"
                            ],
                            "additionalProperties": false,
                            "properties": {
                                "jobId": {
                                    "type": "string",
                                    "minLength": 1,
                                    "maxLength": 128,
                                    "pattern": "^[A-Za-z0-9_.:-]+$"
                                },
                                "title": {
                                    "type": "string",
                                    "minLength": 1,
                                    "maxLength": 1024
                                },
                                "company": {
                                    "type": "string",
                                    "minLength": 1,
                                    "maxLength": 768
                                },
                                "location": {"type": "string", "maxLength": 512},
                                "matchScore": {
                                    "type": "integer",
                                    "minimum": 0,
                                    "maximum": 100
                                }
                            }
                        }
                    }
                }
            })
        );

        let serialized = serde_json::to_value(shortlist(
            "track-alpha",
            vec![lead("job-1", "Engineer", "Example", "", 80)],
        ))
        .unwrap();
        assert_eq!(
            object_keys(&serialized),
            object_keys(&tool["outputSchema"]["properties"])
        );
        assert_eq!(
            object_keys(&serialized["matches"][0]),
            object_keys(&tool["outputSchema"]["properties"]["matches"]["items"]["properties"])
        );
    }

    #[test]
    fn empty_shortlist_still_requires_original_verification_and_matches_text() {
        let params = json!({"name": TOOL_NAME, "arguments": {}});
        let result = tool_call(Some(&params), &mut |request| {
            assert_eq!(request, MatchRequest::Start { limit: 10 });
            Ok(Some(shortlist("track-alpha", Vec::new())))
        })
        .unwrap();
        let structured = &result["structuredContent"];
        assert_eq!(structured["matches"], json!([]));
        assert_eq!(structured["requiresOriginalVerification"], true);
        let text = result["content"][0]["text"].as_str().expect("text result");
        assert_eq!(serde_json::from_str::<Value>(text).unwrap(), *structured);
    }

    #[test]
    fn cursor_request_is_exact_and_response_has_required_pagination_fields() {
        let cursor = format!("bjmcp1_{}", "a".repeat(64));
        let next_cursor = format!("bjmcp1_{}", "b".repeat(64));
        let params = json!({"name": TOOL_NAME, "arguments": {"cursor": cursor.clone()}});
        let result = tool_call(Some(&params), &mut |request| {
            assert_eq!(request, MatchRequest::Continue { cursor: &cursor });
            Ok(Some(MatchResult::discovery_shortlist(
                "reader-derived-track".to_string(),
                vec![lead("job-1", "Engineer", "Example", "", 90)],
                Some(next_cursor.clone()),
                true,
            )))
        })
        .expect("cursor call");
        let structured = &result["structuredContent"];
        assert_eq!(structured["trackId"], "reader-derived-track");
        assert_eq!(structured["nextCursor"], next_cursor);
        assert_eq!(structured["shortlistTruncated"], true);
        let text = result["content"][0]["text"].as_str().expect("text result");
        assert_eq!(serde_json::from_str::<Value>(text).unwrap(), *structured);
    }

    #[test]
    fn legacy_foreign_or_mixed_arguments_never_reach_reader() {
        let cursor = format!("bjmcp1_{}", "a".repeat(64));
        let invalid = [
            json!({"limit": 1, "cursor": cursor}),
            json!({"trackId": "foreign", "limit": 1}),
            json!({"accountId": "foreign"}),
            json!({"grant": "foreign"}),
            json!({"provider": "foreign"}),
            json!({"cursor": format!("bjmcp1_{}", "A".repeat(64))}),
            json!({"cursor": "bjmcp1_short"}),
        ];
        for arguments in invalid {
            let params = json!({"name": TOOL_NAME, "arguments": arguments});
            assert!(
                tool_call(Some(&params), &mut |_| -> Result<Option<MatchResult>, ()> {
                    panic!("invalid input reached reader")
                })
                .is_err()
            );
        }
    }

    #[test]
    fn reader_cannot_return_more_than_ten_rows() {
        let params = json!({"name": TOOL_NAME, "arguments": {}});
        let result = tool_call(Some(&params), &mut |_| {
            Ok(Some(MatchResult::discovery_shortlist(
                "reader-derived-track".to_string(),
                vec![lead("job-1", "Engineer", "Example", "", 90); 11],
                None,
                false,
            )))
        })
        .expect("bounded reader result");
        assert_eq!(result["isError"], true);
        assert!(result.get("structuredContent").is_none());
    }

    #[test]
    fn invalid_reader_lead_is_never_emitted_as_success() {
        let params = json!({"name": TOOL_NAME, "arguments": {}});
        let mut rows = vec![
            lead("", "Engineer", "Example", "", 95),
            lead("job/foreign", "Engineer", "Example", "", 95),
            lead("job-1", "  ", "Example", "", 95),
            lead("job-1", "Engineer", "Bad\u{85}Company", "", 95),
            lead("job-1", "Engineer", "Example", "bad\nlocation", 95),
            lead("job-1", "Engineer", "Example", "", -1),
            lead("job-1", "Engineer", "Example", "", 101),
        ];
        for (field, bytes) in [
            ("job_id", 129),
            ("title", 1025),
            ("company", 769),
            ("location", 513),
        ] {
            let mut row = lead("job-1", "Engineer", "Example", "", 95);
            let excessive = "x".repeat(bytes);
            match field {
                "job_id" => row.job_id = excessive,
                "title" => row.title = excessive,
                "company" => row.company = excessive,
                "location" => row.location = excessive,
                _ => unreachable!(),
            }
            rows.push(row);
        }
        // SQL and transport share UTF-8 byte limits, not a character count.
        let mut utf8 = lead("job-1", "Engineer", "Example", "", 95);
        utf8.title = "é".repeat(513);
        rows.push(utf8);
        for row in rows {
            let mut calls = 0;
            let result = tool_call(Some(&params), &mut |_| {
                calls += 1;
                Ok(Some(MatchResult::discovery_shortlist(
                    "track-1".to_string(),
                    vec![lead("job-ok", "Engineer", "Example", "", 95), row.clone()],
                    None,
                    false,
                )))
            })
            .expect("closed reader result");
            assert_eq!(calls, 1);
            assert_eq!(
                result,
                json!({
                    "isError": true,
                    "content": [{"type": "text", "text": "Career Track unavailable"}]
                })
            );
        }
    }

    #[test]
    fn invalid_or_preinitialized_calls_never_reach_reader() {
        let (messages, calls) = run_records(vec![
            request(
                1,
                "tools/call",
                json!({"name": TOOL_NAME, "arguments": {"limit": 2}}),
            ),
            request(
                2,
                "initialize",
                json!({
                    "protocolVersion": "wrong",
                    "capabilities": {},
                    "clientInfo": {"name": "test", "version": "1"}
                }),
            ),
            request(
                3,
                "initialize",
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": {"name": "test", "version": "1"}
                }),
            ),
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            request(
                4,
                "tools/call",
                json!({"name": TOOL_NAME, "arguments": {"cursor": "bad/track"}}),
            ),
            request(
                5,
                "tools/call",
                json!({"name": "write_application", "arguments": {}}),
            ),
        ]);
        assert_eq!(calls, 0);
        assert_eq!(messages[0]["error"]["code"], -32002);
        assert_eq!(messages[1]["error"]["code"], -32602);
        assert_eq!(messages[3]["error"]["code"], -32602);
        assert_eq!(messages[4]["error"]["code"], -32602);
    }

    #[test]
    fn denied_reader_is_generic_and_has_no_structured_content() {
        let input = format!(
            "{}\n{}\n{}\n",
            request(
                1,
                "initialize",
                json!({"protocolVersion": PROTOCOL_VERSION, "capabilities": {}, "clientInfo": {"name": "test", "version": "1"}})
            ),
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            request(2, "tools/call", json!({"name": TOOL_NAME, "arguments": {}})),
        );
        let mut output = Vec::new();
        run(Cursor::new(input), &mut output, |_| Ok(None)).expect("stdio run");
        let response: Value = serde_json::from_str(
            String::from_utf8(output)
                .expect("utf8")
                .lines()
                .last()
                .expect("call response"),
        )
        .expect("json");
        assert_eq!(response["result"]["isError"], true);
        assert!(response["result"].get("structuredContent").is_none());
        assert_eq!(
            response["result"]["content"][0]["text"],
            "Career Track unavailable"
        );
    }

    #[test]
    fn oversized_frame_is_rejected_without_calling_reader() {
        let input = format!("{}\n", "x".repeat(MAX_FRAME_BYTES + 1));
        let mut output = Vec::new();
        let mut calls = 0;
        run(Cursor::new(input), &mut output, |_| {
            calls += 1;
            Ok(Some(shortlist("track-alpha", Vec::new())))
        })
        .expect("stdio run");
        assert_eq!(calls, 0);
        let response: Value = serde_json::from_slice(&output).expect("json response");
        assert_eq!(response["error"]["code"], -32700);
    }

    #[test]
    fn duplicate_keys_in_requests_or_nested_arguments_are_rejected() {
        for input in [
            r#"{"jsonrpc":"2.0","method":"unknown","method":"ping","id":1}"#,
            r#"{"arguments":{"cursor":"first","curs\u006fr":"second"}}"#,
            r#"{"capabilities":[{"duplicate":null,"duplicate":true}]}"#,
        ] {
            assert!(serde_json::from_str::<StrictJson>(input).is_err());
        }
        let input = b"{\"jsonrpc\":\"2.0\",\"method\":\"unknown\",\"method\":\"ping\",\"id\":1}\n";
        let mut output = Vec::new();
        run(Cursor::new(input), &mut output, |_| {
            panic!("invalid request reached reader")
        })
        .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&output).unwrap()["error"]["code"],
            -32700
        );
    }

    #[test]
    fn maximum_escape_heavy_shortlist_fits_in_one_bounded_response() {
        let params = json!({"name": TOOL_NAME, "arguments": {}});
        let row = DiscoveryLead::new(
            "x".repeat(128),
            "\"".repeat(1024),
            "\\".repeat(768),
            "\"".repeat(512),
            100,
        );
        let data = shortlist(&"x".repeat(64), vec![row; 10]);
        let expected = serde_json::to_value(data.clone()).unwrap();
        let result = tool_call(Some(&params), &mut |_| Ok(Some(data.clone()))).unwrap();
        let mut output = Vec::new();
        write_message(&mut output, &success(json!("\0".repeat(128)), result)).unwrap();
        assert!(output.len() <= MAX_OUTPUT_BYTES);
        let response: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(response["result"]["structuredContent"], expected);
        let text = response["result"]["content"][0]["text"].as_str().unwrap();
        assert_eq!(serde_json::from_str::<Value>(text).unwrap(), expected);
    }
}
