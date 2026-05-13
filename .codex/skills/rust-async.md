# Skill: Rust Async Patterns

## Tokio Runtime

Cue uses `tokio` with the `full` feature. The daemon runs a multi-threaded runtime.

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::init();
    // ...
}
```

## Channels

### mpsc — Multiple producers, single consumer (task queues)
```rust
let (tx, mut rx) = tokio::sync::mpsc::channel::<AudioChunk>(128);

// Producer
tx.send(chunk).await?;

// Consumer
while let Some(chunk) = rx.recv().await {
    process(chunk).await;
}
```

### broadcast — One producer, many consumers (events)
```rust
let (tx, _) = tokio::sync::broadcast::channel::<TranscriptEvent>(64);
let mut rx = tx.subscribe();

// Emit
tx.send(event)?;

// Receive (each subscriber gets every message)
let event = rx.recv().await?;
```

## CancellationToken

```rust
use tokio_util::sync::CancellationToken;

let token = CancellationToken::new();
let child_token = token.child_token();

tokio::spawn(async move {
    tokio::select! {
        _ = child_token.cancelled() => { /* cleanup */ }
        result = do_work() => { /* handle result */ }
    }
});

// Cancel from parent
token.cancel();
```

## Arc<Mutex<T>> vs Arc<RwLock<T>>

```rust
// Use Mutex for exclusive access (write-heavy)
let state = Arc::new(tokio::sync::Mutex::new(AppConfig::default()));

// Use RwLock for read-heavy shared state
let cache = Arc::new(tokio::sync::RwLock::new(HashMap::new()));
let val = cache.read().await.get("key").cloned();
```

## spawn_blocking for CPU-bound work

```rust
// Never block the async runtime
let result = tokio::task::spawn_blocking(move || {
    expensive_computation(&data)
}).await?;
```

## Graceful Shutdown

```rust
let token = CancellationToken::new();

// Ctrl+C handler
let shutdown_token = token.clone();
tokio::spawn(async move {
    tokio::signal::ctrl_c().await.ok();
    shutdown_token.cancel();
});

// Main loop respects cancellation
loop {
    tokio::select! {
        _ = token.cancelled() => break,
        msg = rx.recv() => { /* process */ }
    }
}
```
