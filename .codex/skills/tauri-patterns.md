# Skill: Tauri 2 Patterns

## Commands (Request/Response)

```rust
// Backend: define command in cue-daemon
#[tauri::command]
async fn get_meeting_summary(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<MeetingSummary, String> {
    state.db.get_summary(&meeting_id)
        .await
        .map_err(|e| e.to_string())
}

// Register in builder
tauri::Builder::default()
    .invoke_handler(tauri::generate_handler![get_meeting_summary])
```

```typescript
// Frontend: invoke command
import { invoke } from "@tauri-apps/api/core";

const summary = await invoke<MeetingSummary>("get_meeting_summary", {
  meetingId: "abc-123",
});
```

## Events (Streaming/Push)

```rust
// Backend: emit event
app_handle.emit("transcript-chunk", TranscriptChunk {
    text: chunk.text,
    speaker: chunk.speaker,
    timestamp: chunk.ts,
})?;
```

```typescript
// Frontend: listen for events
import { listen } from "@tauri-apps/api/event";

const unlisten = await listen<TranscriptChunk>("transcript-chunk", (event) => {
  setTranscript((prev) => [...prev, event.payload]);
});
// Clean up on unmount
return () => { unlisten(); };
```

## Plugin Integration

```rust
// Using tauri-plugin-store for persistent config
use tauri_plugin_store::StoreExt;

let store = app.store("config.json")?;
store.set("api_key_provider", "anthropic");
store.save()?;
```

## Window Management (Overlay)

```rust
// Create overlay window
let overlay = tauri::WebviewWindowBuilder::new(
    &app,
    "overlay",
    tauri::WebviewUrl::App("overlay.html".into()),
)
.transparent(true)
.decorations(false)
.always_on_top(true)
.skip_taskbar(true)
.build()?;
```

## State Management

```rust
// Shared state via Tauri managed state
struct AppState {
    db: Arc<Database>,
    llm_router: Arc<LlmRouter>,
    cancel_token: CancellationToken,
}

tauri::Builder::default()
    .manage(AppState { db, llm_router, cancel_token })
```
