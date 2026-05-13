use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_epoch_ms_string() -> String {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    elapsed.as_millis().to_string()
}
