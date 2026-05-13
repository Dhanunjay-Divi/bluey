# Skill: Prompt Engineering

## XML-Tagged Composition

Cue uses XML tags to structure prompts for clarity and parseability.

```xml
<system>
You are a meeting copilot. Analyze the transcript and provide actionable insights.

<constraints>
- Respond in the same language as the transcript
- Keep suggestions under 3 sentences
- Never fabricate information not in the transcript
</constraints>

<output_format>
Respond with a JSON object: { "summary": "...", "action_items": [...], "key_decisions": [...] }
</output_format>
</system>

<context>
<meeting_type>standup</meeting_type>
<participants>Alice (PM), Bob (Eng), Carol (Design)</participants>
<prior_context>{rag_results}</prior_context>
</context>

<transcript>
{current_transcript}
</transcript>
```

## Skill Templates (Hot-Reloadable)

```
prompts/
├── system-base.txt          # Core persona + constraints
├── meeting-summary.txt      # Summarization prompt
├── action-items.txt         # Extract action items
├── coding-help.txt          # Technical assistance
└── patch-mode.txt           # Diff generation
```

Each template uses `{VARIABLE}` placeholders filled at runtime:
```rust
let prompt = template
    .replace("{TRANSCRIPT}", &transcript)
    .replace("{CONTEXT}", &rag_context)
    .replace("{LANGUAGE}", &detected_language);
```

## Patch Mode (PATCH/KEEP/MODIFY/ADD/REMOVE)

For suggesting code or document changes:

```xml
<patch_instructions>
Output changes in patch format. For each section of the document:
- KEEP: unchanged content (show first/last line only)
- MODIFY: show before → after
- ADD: new content with location marker
- REMOVE: content to delete

Format:
[KEEP] lines 1-15
[MODIFY] line 16
  - old: "Weekly sync with team"
  + new: "Daily standup (15 min max)"
[ADD] after line 20
  + "Action: Bob to review PR by EOD"
[KEEP] lines 21-end
</patch_instructions>
```

## Token Budget Management

```rust
const MAX_CONTEXT_TOKENS: usize = 100_000;
const RESERVED_FOR_RESPONSE: usize = 4_096;
const AVAILABLE_FOR_INPUT: usize = MAX_CONTEXT_TOKENS - RESERVED_FOR_RESPONSE;

fn build_prompt(transcript: &str, rag: &[Chunk], system: &str) -> Vec<Message> {
    let system_tokens = count_tokens(system);
    let mut budget = AVAILABLE_FOR_INPUT - system_tokens;

    // Priority: recent transcript > RAG > older transcript
    let transcript_tokens = count_tokens(transcript);
    let transcript_alloc = budget.min(transcript_tokens);
    budget -= transcript_alloc;

    let rag_text = rag.iter()
        .take_while(|c| { budget -= c.tokens; budget > 0 })
        .map(|c| &c.text)
        .collect::<Vec<_>>()
        .join("\n");

    // Assemble messages...
}
```
