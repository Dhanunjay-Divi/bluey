//! Local JSONL log rotation for Bluey desktop processes.
//!
//! Daemon and dashboard use this module so support tooling can look in one
//! predictable directory and get the same standard fields on every event.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde_json::{Map, Number, Value};
use tracing::{Event, Subscriber};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::field::Visit;
use tracing_subscriber::fmt::format::Writer;
#[cfg(debug_assertions)]
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::EnvFilter;

const DEFAULT_RETAINED_LOG_FILES: usize = 7;

#[derive(Debug)]
pub struct LocalLogGuard {
    _file_guard: Option<WorkerGuard>,
}

impl LocalLogGuard {
    fn file(file_guard: WorkerGuard) -> Self {
        Self {
            _file_guard: Some(file_guard),
        }
    }

    fn stderr_only() -> Self {
        Self { _file_guard: None }
    }
}

pub fn init_local_json_logging(component: &'static str, default_filter: &str) -> LocalLogGuard {
    let log_dir = local_log_dir();
    if let Err(error) = fs::create_dir_all(&log_dir) {
        eprintln!(
            "bluey: failed to create log directory {}: {error}",
            log_dir.display()
        );
        init_stderr_json_logging(component, default_filter);
        return LocalLogGuard::stderr_only();
    }

    let prefix = log_file_prefix(component);
    if let Err(error) = retain_recent_log_files(&log_dir, &prefix, DEFAULT_RETAINED_LOG_FILES) {
        eprintln!(
            "bluey: failed to prune old log files in {}: {error}",
            log_dir.display()
        );
    }

    let appender = match tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix(&prefix)
        .filename_suffix("log")
        .max_log_files(DEFAULT_RETAINED_LOG_FILES)
        .build(&log_dir)
    {
        Ok(appender) => appender,
        Err(error) => {
            eprintln!(
                "bluey: failed to initialize file log appender in {}: {error}",
                log_dir.display()
            );
            init_stderr_json_logging(component, default_filter);
            return LocalLogGuard::stderr_only();
        }
    };

    let (file_writer, file_guard) = tracing_appender::non_blocking(appender);
    #[cfg(debug_assertions)]
    {
        if let Err(error) = tracing_subscriber::fmt()
            .event_format(StandardJsonEventFormat::new(component))
            .with_writer(file_writer.and(std::io::stderr))
            .with_env_filter(env_filter(default_filter))
            .try_init()
        {
            eprintln!("bluey: tracing subscriber already initialized or unavailable: {error}");
        }
    }

    #[cfg(not(debug_assertions))]
    {
        if let Err(error) = tracing_subscriber::fmt()
            .event_format(StandardJsonEventFormat::new(component))
            .with_writer(file_writer)
            .with_env_filter(env_filter(default_filter))
            .try_init()
        {
            eprintln!("bluey: tracing subscriber already initialized or unavailable: {error}");
        }
    }

    tracing::info!(log_dir = %log_dir.display(), "local log rotation initialized");

    LocalLogGuard::file(file_guard)
}

pub fn local_log_dir() -> PathBuf {
    if let Some(path) = path_override_any("BLUEY_LOG_DIR", "CUE_LOG_DIR") {
        return path;
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join("Library/Logs/Bluey");
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(".local/state/bluey/log");
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(base) = dirs::data_local_dir() {
            return base.join("Bluey").join("logs");
        }
    }

    dirs::data_local_dir()
        .or_else(dirs::data_dir)
        .unwrap_or_else(std::env::temp_dir)
        .join("bluey")
        .join("logs")
}

pub fn log_file_prefix(component: &str) -> String {
    let prefix = match component {
        "cue-daemon" | "bluey-daemon" | "Terminal" | "Terminal.exe" => "daemon",
        "cue-dashboard" | "bluey-dashboard" => "dashboard",
        other => other
            .trim_start_matches("cue-")
            .trim_start_matches("bluey-"),
    };
    let sanitized: String = prefix
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    format!("{}-log", sanitized.trim_matches('-'))
}

pub fn retain_recent_log_files(log_dir: &Path, prefix: &str, keep: usize) -> Result<()> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(log_dir).with_context(|| format!("read_dir {}", log_dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with(prefix) && name.ends_with(".log") {
            entries.push(path);
        }
    }

    if entries.len() <= keep {
        return Ok(());
    }

    entries.sort();
    let remove_count = entries.len().saturating_sub(keep);
    for path in entries.into_iter().take(remove_count) {
        fs::remove_file(&path).with_context(|| format!("remove {}", path.display()))?;
    }
    Ok(())
}

fn init_stderr_json_logging(component: &'static str, default_filter: &str) {
    if let Err(error) = tracing_subscriber::fmt()
        .event_format(StandardJsonEventFormat::new(component))
        .with_writer(std::io::stderr)
        .with_env_filter(env_filter(default_filter))
        .try_init()
    {
        eprintln!("bluey: tracing subscriber already initialized or unavailable: {error}");
    }
}

fn env_filter(default_filter: &str) -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter))
}

fn path_override(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn path_override_any(primary: &str, legacy: &str) -> Option<PathBuf> {
    path_override(primary).or_else(|| path_override(legacy))
}

#[derive(Clone, Debug)]
struct StandardJsonEventFormat {
    component: &'static str,
    version: &'static str,
    platform: String,
}

impl StandardJsonEventFormat {
    fn new(component: &'static str) -> Self {
        Self {
            component,
            version: env!("CARGO_PKG_VERSION"),
            platform: crate::platform(),
        }
    }
}

impl<S, N> FormatEvent<S, N> for StandardJsonEventFormat
where
    S: Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        _ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let mut fields = Map::new();
        fields.insert("ts_ms".to_string(), Value::Number(now_ms().into()));
        fields.insert(
            "level".to_string(),
            Value::String(event.metadata().level().as_str().to_ascii_lowercase()),
        );
        fields.insert(
            "target".to_string(),
            Value::String(event.metadata().target().to_string()),
        );
        fields.insert(
            "component".to_string(),
            Value::String(self.component.to_string()),
        );
        fields.insert(
            "version".to_string(),
            Value::String(self.version.to_string()),
        );
        fields.insert("platform".to_string(), Value::String(self.platform.clone()));

        let mut visitor = JsonFieldVisitor { fields };
        event.record(&mut visitor);

        let line = Value::Object(visitor.fields).to_string();
        writer.write_str(&line)?;
        writer.write_char('\n')
    }
}

struct JsonFieldVisitor {
    fields: Map<String, Value>,
}

impl JsonFieldVisitor {
    fn insert(&mut self, field: &tracing::field::Field, value: Value) {
        self.fields.insert(field.name().to_string(), value);
    }
}

impl Visit for JsonFieldVisitor {
    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.insert(field, Value::Bool(value));
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.insert(field, Value::Number(value.into()));
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.insert(field, Value::Number(value.into()));
    }

    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        let value = Number::from_f64(value)
            .map(Value::Number)
            .unwrap_or(Value::Null);
        self.insert(field, value);
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.insert(field, Value::String(value.to_string()));
    }

    fn record_error(
        &mut self,
        field: &tracing::field::Field,
        value: &(dyn std::error::Error + 'static),
    ) {
        self.insert(field, Value::String(value.to_string()));
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn fmt::Debug) {
        self.insert(field, Value::String(format!("{value:?}")));
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_file_prefix_matches_support_tool_filters() {
        assert_eq!(log_file_prefix("cue-daemon"), "daemon-log");
        assert_eq!(log_file_prefix("bluey-daemon"), "daemon-log");
        assert_eq!(log_file_prefix("Terminal"), "daemon-log");
        assert_eq!(log_file_prefix("Terminal.exe"), "daemon-log");
        assert_eq!(log_file_prefix("cue-dashboard"), "dashboard-log");
        assert_eq!(log_file_prefix("cue-cloud-client"), "cloud-client-log");
    }

    #[test]
    fn retention_keeps_newest_matching_logs_only() {
        let base =
            std::env::temp_dir().join(format!("bluey-log-retention-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&base).expect("temp log dir");
        for day in 1..=9 {
            fs::write(
                base.join(format!("daemon-log.2026-05-{day:02}.log")),
                "line\n",
            )
            .expect("write daemon log");
        }
        fs::write(base.join("dashboard-log.2026-05-01.log"), "line\n")
            .expect("write dashboard log");

        retain_recent_log_files(&base, "daemon-log", 7).expect("retain daemon logs");

        assert!(!base.join("daemon-log.2026-05-01.log").exists());
        assert!(!base.join("daemon-log.2026-05-02.log").exists());
        assert!(base.join("daemon-log.2026-05-03.log").exists());
        assert!(base.join("daemon-log.2026-05-09.log").exists());
        assert!(base.join("dashboard-log.2026-05-01.log").exists());

        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn local_json_logging_writes_standard_fields() {
        let base = std::env::temp_dir().join(format!("bluey-log-smoke-{}", uuid::Uuid::new_v4()));
        let previous_rust_log = std::env::var_os("RUST_LOG");
        std::env::set_var("RUST_LOG", "trace");
        std::env::set_var("BLUEY_LOG_DIR", &base);
        let guard = init_local_json_logging("cue-core-test", "trace");
        std::env::remove_var("BLUEY_LOG_DIR");
        if let Some(value) = previous_rust_log {
            std::env::set_var("RUST_LOG", value);
        } else {
            std::env::remove_var("RUST_LOG");
        }

        assert!(tracing::enabled!(tracing::Level::INFO));
        tracing::info!(answer = 42_u64, "phase two smoke");
        drop(guard);
        std::thread::sleep(std::time::Duration::from_millis(100));

        let entries: Vec<_> = fs::read_dir(&base)
            .expect("read smoke log dir")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.is_file())
            .collect();
        assert_eq!(entries.len(), 1, "{entries:?}");
        let content = fs::read_to_string(&entries[0]).expect("read smoke log");
        assert!(content.contains("\"component\":\"cue-core-test\""));
        assert!(content.contains(&format!("\"version\":\"{}\"", env!("CARGO_PKG_VERSION"))));
        assert!(content.contains("\"platform\":"));
        assert!(content.contains("\"answer\":42"));
        assert!(content.contains("\"message\":\"phase two smoke\""));

        let _ = fs::remove_dir_all(base);
    }
}
