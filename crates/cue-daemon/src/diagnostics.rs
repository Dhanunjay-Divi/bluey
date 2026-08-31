//! Bounded, metadata-only diagnostics for end-to-end desktop latency.
//!
//! Producers only call `try_send`; disk persistence runs in one background
//! worker. The schema intentionally has no free-form payload field, so a hot
//! path cannot accidentally enqueue questions, answers, transcripts, paths,
//! URLs, tokens, or provider response bodies.

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::cloud::sync::{append_privacy_safe_diagnostic_events_for_scope, SessionAuditScope};

const DIAGNOSTIC_SCHEMA_VERSION: u32 = 2;
const ORDINARY_QUEUE_CAPACITY: usize = 2_048;
const TERMINAL_QUEUE_CAPACITY: usize = 256;
const MAX_BATCH_EVENTS: usize = 128;
const TERMINAL_DRAIN_BURST: usize = 8;
const FLUSH_INTERVAL: Duration = Duration::from_secs(1);
const LOSS_SUMMARY_INTERVAL_TICKS: u8 = 5;
const DIAGNOSTIC_TOMBSTONE_DIR: &str = "diagnostic-session-tombstones";
const SUPPORT_DIAGNOSTIC_CONSENT_DIR: &str = "support-diagnostic-consent";
const SUPPORT_DIAGNOSTIC_EPOCH_MARKER: &str = ".consent-epoch.json";
const SESSION_AUDIT_EVENTS_DIR: &str = "session-audit-events";
const SESSION_AUDIT_DIR: &str = "session-audit";
const SESSION_AUDIT_UPLOADED_DIR: &str = "session-audit-uploaded";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DiagnosticPriority {
    Ordinary,
    Terminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DiagnosticComponent {
    NativeOverlay,
    Daemon,
    Audio,
    Stt,
    Rag,
    Model,
    Persistence,
    Support,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DiagnosticOutcome {
    Started,
    Succeeded,
    Failed,
    TimedOut,
    Dropped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DiagnosticEventKind {
    OverlayUserAction,
    OverlayLifecycle,
    AnswerRequestAccepted,
    AnswerContextPrepared,
    AnswerCardCreated,
    AnswerStatusPresented,
    AnswerReplayStarted,
    AnswerRouteCompleted,
    AnswerFirstText,
    AnswerCompleted,
    AnswerFailed,
    AnswerSlowStart,
    NativeFirstTextRendered,
    NativeFinalRendered,
    TranscriptSettled,
    TranscriptBufferConsumed,
    AudioStartRequested,
    AudioCaptureReady,
    AudioStopRequested,
    AudioCaptureStopped,
    AudioFirstChunk,
    SttConnected,
    SttFirstPartial,
    SttFirstFinal,
    RagQueryCompleted,
    ModelAttemptStarted,
    ModelConnected,
    ModelFirstEvent,
    ModelFirstText,
    ModelAttemptCompleted,
    PersistenceCompleted,
    ContextWatchObserved,
    DiagnosticEventsDropped,
}

impl DiagnosticEventKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::OverlayUserAction => "overlay_user_action",
            Self::OverlayLifecycle => "overlay_lifecycle",
            Self::AnswerRequestAccepted => "answer_request_accepted",
            Self::AnswerContextPrepared => "answer_context_prepared",
            Self::AnswerCardCreated => "answer_card_created",
            Self::AnswerStatusPresented => "answer_status_presented",
            Self::AnswerReplayStarted => "answer_replay_started",
            Self::AnswerRouteCompleted => "answer_route_completed",
            Self::AnswerFirstText => "answer_first_text",
            Self::AnswerCompleted => "answer_completed",
            Self::AnswerFailed => "answer_failed",
            Self::AnswerSlowStart => "answer_slow_start",
            Self::NativeFirstTextRendered => "native_first_text_rendered",
            Self::NativeFinalRendered => "native_final_rendered",
            Self::TranscriptSettled => "transcript_settled",
            Self::TranscriptBufferConsumed => "transcript_buffer_consumed",
            Self::AudioStartRequested => "audio_start_requested",
            Self::AudioCaptureReady => "audio_capture_ready",
            Self::AudioStopRequested => "audio_stop_requested",
            Self::AudioCaptureStopped => "audio_capture_stopped",
            Self::AudioFirstChunk => "audio_first_chunk",
            Self::SttConnected => "stt_connected",
            Self::SttFirstPartial => "stt_first_partial",
            Self::SttFirstFinal => "stt_first_final",
            Self::RagQueryCompleted => "rag_query_completed",
            Self::ModelAttemptStarted => "model_attempt_started",
            Self::ModelConnected => "model_connected",
            Self::ModelFirstEvent => "model_first_event",
            Self::ModelFirstText => "model_first_text",
            Self::ModelAttemptCompleted => "model_attempt_completed",
            Self::PersistenceCompleted => "persistence_completed",
            Self::ContextWatchObserved => "context_watch_observed",
            Self::DiagnosticEventsDropped => "diagnostic_events_dropped",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DiagnosticEvent {
    schema_version: u32,
    event_name: &'static str,
    component: DiagnosticComponent,
    outcome: DiagnosticOutcome,
    created_at_ms: u64,
    monotonic_offset_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    interaction_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    audio_run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    card_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    route: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    queue_wait_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    queue_depth: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    queue_high_water: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    attempt: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sequence: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    streaming: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    input_chars: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output_chars: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    context_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    document_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    screenshot_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    transcript_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    memory_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    question_intent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    artifact_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dropped_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    coalesced_count: Option<u64>,
}

impl DiagnosticEvent {
    pub(crate) fn new(
        event: DiagnosticEventKind,
        component: DiagnosticComponent,
        outcome: DiagnosticOutcome,
    ) -> Self {
        Self {
            schema_version: DIAGNOSTIC_SCHEMA_VERSION,
            event_name: event.as_str(),
            component,
            outcome,
            created_at_ms: epoch_ms(),
            monotonic_offset_ms: 0,
            interaction_id: None,
            trace_id: None,
            request_id: None,
            audio_run_id: None,
            card_id: None,
            provider: None,
            model: None,
            route: None,
            action: None,
            error_category: None,
            duration_ms: None,
            queue_wait_ms: None,
            queue_depth: None,
            queue_high_water: None,
            attempt: None,
            generation: None,
            sequence: None,
            streaming: None,
            count: None,
            bytes: None,
            input_chars: None,
            output_chars: None,
            context_count: None,
            document_count: None,
            screenshot_count: None,
            transcript_count: None,
            memory_count: None,
            source_count: None,
            question_intent: None,
            artifact_type: None,
            dropped_count: None,
            coalesced_count: None,
        }
    }

    pub(crate) fn interaction_id(mut self, value: Option<Uuid>) -> Self {
        self.interaction_id = value.map(|value| value.hyphenated().to_string());
        self
    }

    pub(crate) fn trace_id(mut self, value: Option<&str>) -> Self {
        self.trace_id = value
            .and_then(|value| Uuid::parse_str(value).ok())
            .map(|value| value.hyphenated().to_string());
        self
    }

    pub(crate) fn request_id(mut self, value: Uuid) -> Self {
        self.request_id = Some(value.hyphenated().to_string());
        self
    }

    pub(crate) fn audio_run_id(mut self, value: Option<Uuid>) -> Self {
        self.audio_run_id = value.map(|value| value.hyphenated().to_string());
        self
    }

    pub(crate) fn card_id(mut self, value: Uuid) -> Self {
        self.card_id = Some(value.hyphenated().to_string());
        self
    }

    pub(crate) fn provider(mut self, value: &str) -> Self {
        self.provider = safe_provider_label(value);
        self
    }

    pub(crate) fn model(mut self, value: &str) -> Self {
        self.model = safe_model_label(value);
        self
    }

    pub(crate) fn route(mut self, value: &str) -> Self {
        self.route = safe_provider_label(value);
        self
    }

    pub(crate) fn action(mut self, value: &str) -> Self {
        self.action = closed_label(value, DIAGNOSTIC_ACTIONS);
        self
    }

    pub(crate) fn error_category(mut self, value: &str) -> Self {
        self.error_category = closed_label(value, DIAGNOSTIC_ERROR_CATEGORIES);
        self
    }

    pub(crate) fn duration_ms(mut self, value: Option<u64>) -> Self {
        self.duration_ms = value;
        self
    }

    pub(crate) fn attempt(mut self, value: usize) -> Self {
        self.attempt = Some(value as u64);
        self
    }

    pub(crate) fn generation(mut self, value: u64) -> Self {
        self.generation = Some(value);
        self
    }

    pub(crate) fn sequence(mut self, value: u64) -> Self {
        self.sequence = Some(value);
        self
    }

    pub(crate) fn streaming(mut self, value: bool) -> Self {
        self.streaming = Some(value);
        self
    }

    pub(crate) fn count(mut self, value: usize) -> Self {
        self.count = Some(value as u64);
        self
    }

    pub(crate) fn bytes(mut self, value: usize) -> Self {
        self.bytes = Some(value as u64);
        self
    }

    pub(crate) fn input_chars(mut self, value: usize) -> Self {
        self.input_chars = Some(value as u64);
        self
    }

    pub(crate) fn output_chars(mut self, value: usize) -> Self {
        self.output_chars = Some(value as u64);
        self
    }

    pub(crate) fn context_count(mut self, value: usize) -> Self {
        self.context_count = Some(value as u64);
        self
    }

    pub(crate) fn context_shape(
        mut self,
        documents: usize,
        screenshots: usize,
        transcripts: usize,
        memory: usize,
    ) -> Self {
        self.document_count = Some(documents as u64);
        self.screenshot_count = Some(screenshots as u64);
        self.transcript_count = Some(transcripts as u64);
        self.memory_count = Some(memory as u64);
        self
    }

    pub(crate) fn source_count(mut self, value: usize) -> Self {
        self.source_count = Some(value as u64);
        self
    }

    pub(crate) fn question_intent(mut self, value: &str) -> Self {
        self.question_intent = closed_label(value, DIAGNOSTIC_QUESTION_INTENTS);
        self
    }

    pub(crate) fn artifact_type(mut self, value: &str) -> Self {
        self.artifact_type = closed_label(value, DIAGNOSTIC_ARTIFACT_TYPES);
        self
    }

    fn dropped_count(mut self, value: u64) -> Self {
        self.dropped_count = Some(value);
        self
    }
}

#[derive(Debug)]
struct QueuedDiagnostic {
    scope: SessionAuditScope,
    consent_epoch: Option<u64>,
    event: DiagnosticEvent,
    queued_at: Instant,
}

#[derive(Debug, Default)]
struct DiagnosticCounters {
    ordinary_dropped: AtomicU64,
    terminal_dropped: AtomicU64,
    ordinary_high_water: AtomicU64,
    terminal_high_water: AtomicU64,
}

#[derive(Clone)]
struct DiagnosticBus {
    ordinary: Option<mpsc::Sender<QueuedDiagnostic>>,
    terminal: Option<mpsc::Sender<QueuedDiagnostic>>,
    started_at: Instant,
    data_dir: Option<Arc<PathBuf>>,
    counters: Arc<DiagnosticCounters>,
}

impl DiagnosticBus {
    #[cfg(test)]
    fn disabled() -> Self {
        Self {
            ordinary: None,
            terminal: None,
            started_at: Instant::now(),
            data_dir: None,
            counters: Arc::new(DiagnosticCounters::default()),
        }
    }

    fn emit(
        &self,
        scope: SessionAuditScope,
        mut event: DiagnosticEvent,
        priority: DiagnosticPriority,
    ) {
        event.monotonic_offset_ms = elapsed_ms(self.started_at);
        let consent_epoch = self
            .data_dir
            .as_deref()
            .and_then(|data_dir| diagnostic_scope_epoch(data_dir, &scope).ok().flatten());
        let (sender, dropped, high_water) = match priority {
            DiagnosticPriority::Ordinary => (
                self.ordinary.as_ref(),
                &self.counters.ordinary_dropped,
                &self.counters.ordinary_high_water,
            ),
            DiagnosticPriority::Terminal => (
                self.terminal.as_ref(),
                &self.counters.terminal_dropped,
                &self.counters.terminal_high_water,
            ),
        };
        let Some(sender) = sender else {
            return;
        };
        let depth = sender.max_capacity().saturating_sub(sender.capacity()) as u64;
        high_water.fetch_max(depth.saturating_add(1), Ordering::Relaxed);
        if sender
            .try_send(QueuedDiagnostic {
                scope,
                consent_epoch,
                event,
                queued_at: Instant::now(),
            })
            .is_err()
        {
            dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

pub(crate) struct DiagnosticRuntime {
    bus: DiagnosticBus,
    data_dir: Option<PathBuf>,
    shutdown: Mutex<Option<oneshot::Sender<()>>>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl DiagnosticRuntime {
    pub(crate) fn start(data_dir: std::path::PathBuf) -> Self {
        let (ordinary_tx, ordinary_rx) = mpsc::channel(ORDINARY_QUEUE_CAPACITY);
        let (terminal_tx, terminal_rx) = mpsc::channel(TERMINAL_QUEUE_CAPACITY);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let counters = Arc::new(DiagnosticCounters::default());
        let bus = DiagnosticBus {
            ordinary: Some(ordinary_tx),
            terminal: Some(terminal_tx),
            started_at: Instant::now(),
            data_dir: Some(Arc::new(data_dir.clone())),
            counters: Arc::clone(&counters),
        };
        let worker = tokio::spawn(diagnostic_writer_loop(
            data_dir.clone(),
            ordinary_rx,
            terminal_rx,
            shutdown_rx,
            counters,
        ));
        Self {
            bus,
            data_dir: Some(data_dir),
            shutdown: Mutex::new(Some(shutdown_tx)),
            worker: Mutex::new(Some(worker)),
        }
    }

    #[cfg(test)]
    pub(crate) fn disabled() -> Self {
        Self {
            bus: DiagnosticBus::disabled(),
            data_dir: None,
            shutdown: Mutex::new(None),
            worker: Mutex::new(None),
        }
    }

    pub(crate) fn emit(
        &self,
        scope: SessionAuditScope,
        event: DiagnosticEvent,
        priority: DiagnosticPriority,
    ) {
        self.bus.emit(scope, event, priority);
    }

    /// Durably fence metadata-only diagnostics for an account-scoped cloud
    /// tombstone and remove all already-persisted audit artifacts.
    ///
    /// The persistence lock orders this against an in-flight writer flush.
    /// Queued events are filtered by the durable marker when they later drain.
    pub(crate) fn purge_cloud_tombstoned_session_for_owner(
        &self,
        owner_account_id: &str,
        session_id: Uuid,
    ) -> Result<()> {
        let Some(data_dir) = self.data_dir.as_deref() else {
            return Ok(());
        };
        purge_cloud_tombstoned_session_diagnostics(data_dir, owner_account_id, session_id)
    }

    pub(crate) async fn shutdown(&self) {
        if let Some(shutdown) = self.shutdown.lock().await.take() {
            let _ = shutdown.send(());
        }
        if let Some(worker) = self.worker.lock().await.take() {
            let _ = tokio::time::timeout(Duration::from_secs(2), worker).await;
        }
    }
}

async fn diagnostic_writer_loop(
    data_dir: std::path::PathBuf,
    mut ordinary: mpsc::Receiver<QueuedDiagnostic>,
    mut terminal: mpsc::Receiver<QueuedDiagnostic>,
    mut shutdown: oneshot::Receiver<()>,
    counters: Arc<DiagnosticCounters>,
) {
    let mut batch = Vec::with_capacity(MAX_BATCH_EVENTS);
    let mut last_scope: Option<SessionAuditScope> = None;
    let mut flush_tick = tokio::time::interval(FLUSH_INTERVAL);
    let mut ticks_since_loss_summary = 0_u8;
    flush_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        let mut flush_now = false;
        tokio::select! {
            biased;
            _ = &mut shutdown => {
                while let Ok(item) = terminal.try_recv() {
                    last_scope = Some(item.scope.clone());
                    batch.push(item);
                }
                while let Ok(item) = ordinary.try_recv() {
                    last_scope = Some(item.scope.clone());
                    batch.push(item);
                }
                append_loss_summary(&data_dir, &mut batch, last_scope.as_ref(), &counters);
                flush_diagnostic_batch(&data_dir, &mut batch).await;
                return;
            }
            item = terminal.recv() => {
                if let Some(item) = item {
                    last_scope = Some(item.scope.clone());
                    batch.push(item);
                }
            }
            item = ordinary.recv() => {
                if let Some(item) = item {
                    last_scope = Some(item.scope.clone());
                    batch.push(item);
                }
            }
            _ = flush_tick.tick() => {
                ticks_since_loss_summary = ticks_since_loss_summary.saturating_add(1);
                if ticks_since_loss_summary >= LOSS_SUMMARY_INTERVAL_TICKS {
                    append_loss_summary(&data_dir, &mut batch, last_scope.as_ref(), &counters);
                    ticks_since_loss_summary = 0;
                }
                flush_now = true;
            }
        }

        for _ in 1..TERMINAL_DRAIN_BURST {
            let Ok(item) = terminal.try_recv() else {
                break;
            };
            last_scope = Some(item.scope.clone());
            batch.push(item);
            if batch.len() >= MAX_BATCH_EVENTS {
                break;
            }
        }
        if batch.len() < MAX_BATCH_EVENTS {
            if let Ok(item) = ordinary.try_recv() {
                last_scope = Some(item.scope.clone());
                batch.push(item);
            }
        }

        if batch.len() >= MAX_BATCH_EVENTS || flush_now {
            flush_diagnostic_batch(&data_dir, &mut batch).await;
        }
    }
}

fn append_loss_summary(
    data_dir: &Path,
    batch: &mut Vec<QueuedDiagnostic>,
    scope: Option<&SessionAuditScope>,
    counters: &DiagnosticCounters,
) {
    let ordinary = counters.ordinary_dropped.swap(0, Ordering::AcqRel);
    let terminal = counters.terminal_dropped.swap(0, Ordering::AcqRel);
    let dropped = ordinary.saturating_add(terminal);
    if dropped == 0 {
        return;
    }
    let Some(scope) = scope else {
        tracing::warn!(
            dropped_count = dropped,
            "diagnostic events were dropped before a session was available"
        );
        return;
    };
    let high_water = counters
        .ordinary_high_water
        .load(Ordering::Relaxed)
        .max(counters.terminal_high_water.load(Ordering::Relaxed));
    let mut event = DiagnosticEvent::new(
        DiagnosticEventKind::DiagnosticEventsDropped,
        DiagnosticComponent::Support,
        DiagnosticOutcome::Dropped,
    )
    .dropped_count(dropped);
    event.queue_high_water = Some(high_water);
    event.count = Some(dropped);
    batch.push(QueuedDiagnostic {
        scope: scope.clone(),
        consent_epoch: diagnostic_scope_epoch(data_dir, scope).ok().flatten(),
        event,
        queued_at: Instant::now(),
    });
}

async fn flush_diagnostic_batch(data_dir: &std::path::Path, batch: &mut Vec<QueuedDiagnostic>) {
    if batch.is_empty() {
        return;
    }
    let data_dir = data_dir.to_path_buf();
    let records = std::mem::take(batch);
    let failed = tokio::task::spawn_blocking(move || {
        let mut failed = 0_u64;
        let mut groups =
            HashMap::<(SessionAuditScope, Option<u64>), Vec<(String, serde_json::Value)>>::new();
        for mut queued in records {
            queued.event.queue_wait_ms = Some(elapsed_ms(queued.queued_at));
            let payload = match serde_json::to_value(&queued.event) {
                Ok(payload) => payload,
                Err(_) => {
                    failed = failed.saturating_add(1);
                    continue;
                }
            };
            groups
                .entry((queued.scope, queued.consent_epoch))
                .or_default()
                .push((queued.event.event_name.to_string(), payload));
        }
        for ((scope, consent_epoch), events) in groups {
            let _persistence_guard = diagnostic_persistence_lock()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if diagnostic_scope_is_tombstoned(&data_dir, &scope) {
                continue;
            }
            if !diagnostic_epoch_allows_persistence(&data_dir, &scope, consent_epoch) {
                continue;
            }
            if let Some(consent_epoch) = consent_epoch {
                if persist_support_diagnostic_scope_epoch(&data_dir, &scope, consent_epoch).is_err()
                {
                    failed = failed.saturating_add(events.len() as u64);
                    continue;
                }
            }
            if append_privacy_safe_diagnostic_events_for_scope(&data_dir, &scope, &events).is_err()
            {
                failed = failed.saturating_add(events.len() as u64);
            }
        }
        failed
    })
    .await
    .unwrap_or(1);
    if failed > 0 {
        tracing::warn!(
            failed_count = failed,
            "metadata-only diagnostic persistence was incomplete"
        );
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SupportDiagnosticConsentEpoch {
    schema_version: u32,
    epoch: u64,
    revoked_at_ms: u64,
    #[serde(default)]
    observed_server_revision: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SupportDiagnosticScopeEpoch {
    schema_version: u32,
    epoch: u64,
}

fn diagnostic_epoch_cache() -> &'static StdMutex<HashMap<(PathBuf, String), u64>> {
    static CACHE: OnceLock<StdMutex<HashMap<(PathBuf, String), u64>>> = OnceLock::new();
    CACHE.get_or_init(|| StdMutex::new(HashMap::new()))
}

fn diagnostic_scope_epoch(data_dir: &Path, scope: &SessionAuditScope) -> Result<Option<u64>> {
    scope
        .owner_account_id
        .as_deref()
        .map(|owner| current_support_diagnostic_epoch(data_dir, owner))
        .transpose()
}

fn current_support_diagnostic_epoch(data_dir: &Path, owner_account_id: &str) -> Result<u64> {
    let scope_key = diagnostic_account_scope_key(owner_account_id)?;
    let cache_key = (data_dir.to_path_buf(), scope_key.clone());
    let mut cache = diagnostic_epoch_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(epoch) = cache.get(&cache_key) {
        return Ok(*epoch);
    }
    let epoch = read_support_diagnostic_epoch(data_dir, &scope_key)?.epoch;
    cache.insert(cache_key, epoch);
    Ok(epoch)
}

fn read_support_diagnostic_epoch(
    data_dir: &Path,
    scope_key: &str,
) -> Result<SupportDiagnosticConsentEpoch> {
    let path = support_diagnostic_epoch_path(data_dir, scope_key);
    match fs::read(&path) {
        Ok(bytes) => {
            let state: SupportDiagnosticConsentEpoch =
                serde_json::from_slice(&bytes).context("parse support diagnostic consent epoch")?;
            anyhow::ensure!(
                state.schema_version == 1,
                "unsupported support diagnostic consent epoch"
            );
            Ok(state)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(SupportDiagnosticConsentEpoch {
                schema_version: 1,
                epoch: 0,
                revoked_at_ms: 0,
                observed_server_revision: None,
            })
        }
        Err(error) => Err(error).context("read support diagnostic consent epoch"),
    }
}

/// Advance the durable account-scoped consent epoch before revocation is
/// acknowledged. Old on-disk events are purged and queued events from the
/// previous epoch are rejected when the writer eventually drains.
pub(crate) fn fence_support_diagnostics_for_owner(
    data_dir: &Path,
    owner_account_id: &str,
) -> Result<()> {
    let scope_key = diagnostic_account_scope_key(owner_account_id)?;
    let _persistence_guard = diagnostic_persistence_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut state = read_support_diagnostic_epoch(data_dir, &scope_key)?;
    state.epoch = state.epoch.saturating_add(1);
    state.revoked_at_ms = epoch_ms();
    let bytes = serde_json::to_vec(&state).context("serialize support diagnostic consent epoch")?;
    crate::storage::write_private_atomic_bytes(
        &support_diagnostic_epoch_path(data_dir, &scope_key),
        &bytes,
    )?;
    diagnostic_epoch_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert((data_dir.to_path_buf(), scope_key.clone()), state.epoch);

    for root in [
        SESSION_AUDIT_EVENTS_DIR,
        SESSION_AUDIT_DIR,
        SESSION_AUDIT_UPLOADED_DIR,
    ] {
        remove_private_tree_if_present(&data_dir.join(root).join(&scope_key))?;
    }
    Ok(())
}

/// Bind local event epochs to the append-only server consent revision. A
/// server-side revoke/regrant that happened while this desktop was offline
/// changes the revision and therefore purges every event from the earlier
/// grant before upload authority is considered again.
pub(crate) fn align_support_diagnostic_server_revision(
    data_dir: &Path,
    owner_account_id: &str,
    server_revision: i64,
) -> Result<()> {
    anyhow::ensure!(
        server_revision > 0,
        "invalid support diagnostic consent revision"
    );
    let scope_key = diagnostic_account_scope_key(owner_account_id)?;
    let _persistence_guard = diagnostic_persistence_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut state = read_support_diagnostic_epoch(data_dir, &scope_key)?;
    if state.observed_server_revision == Some(server_revision) {
        return Ok(());
    }
    state.epoch = state.epoch.saturating_add(1);
    state.revoked_at_ms = epoch_ms();
    state.observed_server_revision = Some(server_revision);
    let bytes = serde_json::to_vec(&state).context("serialize support diagnostic consent epoch")?;
    crate::storage::write_private_atomic_bytes(
        &support_diagnostic_epoch_path(data_dir, &scope_key),
        &bytes,
    )?;
    diagnostic_epoch_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert((data_dir.to_path_buf(), scope_key.clone()), state.epoch);
    for root in [
        SESSION_AUDIT_EVENTS_DIR,
        SESSION_AUDIT_DIR,
        SESSION_AUDIT_UPLOADED_DIR,
    ] {
        remove_private_tree_if_present(&data_dir.join(root).join(&scope_key))?;
    }
    Ok(())
}

/// Fail closed for legacy/unscoped directories and for every event directory
/// written before the latest account revocation.
pub(crate) fn support_diagnostic_scope_is_uploadable(
    data_dir: &Path,
    scope: &SessionAuditScope,
) -> bool {
    let Some(owner) = scope.owner_account_id.as_deref() else {
        return false;
    };
    let Ok(current_epoch) = current_support_diagnostic_epoch(data_dir, owner) else {
        return false;
    };
    let marker = support_diagnostic_scope_epoch_path(data_dir, scope);
    fs::read(marker)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<SupportDiagnosticScopeEpoch>(&bytes).ok())
        .is_some_and(|state| state.schema_version == 1 && state.epoch == current_epoch)
}

fn diagnostic_epoch_allows_persistence(
    data_dir: &Path,
    scope: &SessionAuditScope,
    queued_epoch: Option<u64>,
) -> bool {
    let Some(owner) = scope.owner_account_id.as_deref() else {
        return true;
    };
    let Some(queued_epoch) = queued_epoch else {
        return false;
    };
    current_support_diagnostic_epoch(data_dir, owner)
        .map(|current| current == queued_epoch)
        .unwrap_or(false)
}

fn persist_support_diagnostic_scope_epoch(
    data_dir: &Path,
    scope: &SessionAuditScope,
    epoch: u64,
) -> Result<()> {
    let marker = support_diagnostic_scope_epoch_path(data_dir, scope);
    let bytes = serde_json::to_vec(&SupportDiagnosticScopeEpoch {
        schema_version: 1,
        epoch,
    })?;
    crate::storage::write_private_atomic_bytes(&marker, &bytes)
}

fn support_diagnostic_scope_epoch_path(data_dir: &Path, scope: &SessionAuditScope) -> PathBuf {
    data_dir
        .join(SESSION_AUDIT_EVENTS_DIR)
        .join(
            scope
                .owner_account_id
                .as_deref()
                .and_then(|owner| diagnostic_account_scope_key(owner).ok())
                .unwrap_or_else(|| "local-unowned".to_string()),
        )
        .join(scope.session_id.to_string())
        .join(SUPPORT_DIAGNOSTIC_EPOCH_MARKER)
}

fn support_diagnostic_epoch_path(data_dir: &Path, scope_key: &str) -> PathBuf {
    data_dir
        .join(SUPPORT_DIAGNOSTIC_CONSENT_DIR)
        .join(format!("{scope_key}.json"))
}

fn diagnostic_account_scope_key(owner_account_id: &str) -> Result<String> {
    let owner = validate_diagnostic_owner_account_id(owner_account_id)?;
    let mut hasher = Sha256::new();
    hasher.update(b"bluey-cloud-account-scope-v1\0");
    hasher.update((owner.len() as u64).to_be_bytes());
    hasher.update(owner.as_bytes());
    Ok(format!("account-{:x}", hasher.finalize()))
}

fn remove_private_tree_if_present(path: &Path) -> Result<()> {
    match fs::remove_dir_all(path) {
        Ok(()) => {
            if let Some(parent) = path.parent() {
                #[cfg(unix)]
                fs::File::open(parent)
                    .and_then(|directory| directory.sync_all())
                    .context("sync support diagnostic purge directory")?;
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).context("purge support diagnostic account scope"),
    }
}

fn purge_cloud_tombstoned_session_diagnostics(
    data_dir: &Path,
    owner_account_id: &str,
    session_id: Uuid,
) -> Result<()> {
    let owner_account_id = validate_diagnostic_owner_account_id(owner_account_id)?;
    let _persistence_guard = diagnostic_persistence_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    persist_diagnostic_tombstone(data_dir, owner_account_id, session_id)?;
    crate::cloud::sync::purge_session_audit_state(data_dir, Some(owner_account_id), session_id)
}

fn diagnostic_persistence_lock() -> &'static StdMutex<()> {
    static LOCK: OnceLock<StdMutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| StdMutex::new(()))
}

fn diagnostic_scope_is_tombstoned(data_dir: &Path, scope: &SessionAuditScope) -> bool {
    scope
        .owner_account_id
        .as_deref()
        .and_then(|owner_account_id| {
            diagnostic_tombstone_path(data_dir, owner_account_id, scope.session_id).ok()
        })
        .is_some_and(|path| path.is_file())
}

fn persist_diagnostic_tombstone(
    data_dir: &Path,
    owner_account_id: &str,
    session_id: Uuid,
) -> Result<()> {
    let path = diagnostic_tombstone_path(data_dir, owner_account_id, session_id)?;
    let parent = path
        .parent()
        .context("diagnostic tombstone path has no parent")?;
    cue_core::app_paths::create_private_dir(parent)?;
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&path)
        .context("create diagnostic deletion tombstone")?;
    if file.metadata()?.len() == 0 {
        file.write_all(b"deleted\n")
            .context("write diagnostic deletion tombstone")?;
    }
    file.sync_all()
        .context("sync diagnostic deletion tombstone")?;
    #[cfg(unix)]
    std::fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .context("sync diagnostic tombstone directory")?;
    Ok(())
}

fn diagnostic_tombstone_path(
    data_dir: &Path,
    owner_account_id: &str,
    session_id: Uuid,
) -> Result<PathBuf> {
    let owner_account_id = validate_diagnostic_owner_account_id(owner_account_id)?;
    let owner_hash = hex::encode(Sha256::digest(owner_account_id.as_bytes()));
    Ok(data_dir
        .join(DIAGNOSTIC_TOMBSTONE_DIR)
        .join(owner_hash)
        .join(format!("{session_id}.tombstone")))
}

fn validate_diagnostic_owner_account_id(owner_account_id: &str) -> Result<&str> {
    let trimmed = owner_account_id.trim();
    if trimmed.is_empty() || trimmed != owner_account_id {
        anyhow::bail!("diagnostic deletion account owner is invalid");
    }
    Ok(trimmed)
}

const DIAGNOSTIC_ACTIONS: &[&str] = &[
    "active_page_capture_requested",
    "analyze_screen_requested",
    "ask_answer_sent",
    "ask_answer_skipped",
    "ask_requested",
    "attach_files_requested",
    "attach_requested",
    "autosend_answer_sent",
    "autosend_answer_skipped",
    "capture_start_requested",
    "capture_stop_requested",
    "close_requested",
    "context_list_requested",
    "hidden",
    "instructions_requested",
    "instructions_updated",
    "meeting_banner_action",
    "opacity_updated",
    "paste_text_requested",
    "ready",
    "recap_requested",
    "recording_start_requested",
    "recording_stop_requested",
    "remove_context_requested",
    "session_continue_requested",
    "session_delete_requested",
    "session_drawer_opened",
    "session_drawer_sessions_rendered",
    "session_list_requested",
    "session_new_requested",
    "session_open_requested",
    "session_rename_requested",
    "shortcuts_coachmark_dismissed",
    "shortcuts_coachmark_shown",
    "shortcuts_overlay_opened",
    "shown",
    "sign_in_requested",
    "theme_changed",
    "transcript_buffer_consumed",
    "transcript_buffer_skip_consumed",
    "transcript_clear_requested",
    "transcript_context_cleared",
];

const DIAGNOSTIC_ERROR_CATEGORIES: &[&str] = &[
    "authentication",
    "billing",
    "cancelled",
    "capacity",
    "dropped",
    "failed",
    "internal",
    "network",
    "none",
    "ok",
    "rate_limit",
    "response_db_write_failed",
    "runtime_setup",
    "runtime_unavailable",
    "safety",
    "start_canceled",
    "timed_out",
    "timeout",
    "unknown",
];

const DIAGNOSTIC_QUESTION_INTENTS: &[&str] = &[
    "code_explanation",
    "code_or_debug",
    "explanation",
    "general",
    "quick_explanation",
    "short_query",
    "system_design",
];

const DIAGNOSTIC_ARTIFACT_TYPES: &[&str] = &[
    "code",
    "document",
    "none",
    "screen",
    "structured",
    "system_design",
];

fn closed_label(value: &str, allowed: &[&str]) -> Option<String> {
    let normalized = value.trim().to_ascii_lowercase();
    allowed.contains(&normalized.as_str()).then_some(normalized)
}

fn safe_provider_label(value: &str) -> Option<String> {
    let normalized = value.trim().to_ascii_lowercase();
    let category = if normalized.contains("bluey") {
        "bluey_managed"
    } else if normalized.contains("openai") || normalized.starts_with("gpt") {
        "openai"
    } else if normalized.contains("anthropic") || normalized.contains("claude") {
        "anthropic"
    } else if normalized.contains("google") || normalized.contains("gemini") {
        "google"
    } else if normalized.contains("deepgram") {
        "deepgram"
    } else if normalized.contains("assemblyai") {
        "assemblyai"
    } else if normalized.contains("groq") {
        "groq"
    } else if normalized.contains("local") {
        "local"
    } else if normalized.is_empty() {
        return None;
    } else {
        "other"
    };
    Some(category.to_string())
}

fn safe_model_label(value: &str) -> Option<String> {
    let normalized = value.trim().to_ascii_lowercase();
    let family = if normalized.starts_with("gpt-") {
        "gpt"
    } else if ["o1", "o3", "o4"]
        .iter()
        .any(|prefix| normalized.starts_with(prefix))
    {
        "openai_reasoning"
    } else if normalized.starts_with("claude-") {
        "claude"
    } else if normalized.starts_with("gemini-") {
        "gemini"
    } else if normalized.starts_with("nova-") {
        "nova"
    } else if normalized.starts_with("whisper-") {
        "whisper"
    } else if normalized.starts_with("bluey-") {
        "bluey"
    } else if normalized.starts_with("llama-") {
        "llama"
    } else if normalized.starts_with("mistral-") {
        "mistral"
    } else if normalized.starts_with("qwen-") {
        "qwen"
    } else if normalized.starts_with("kimi-") {
        "kimi"
    } else if normalized.is_empty() {
        return None;
    } else {
        "other"
    };
    Some(family.to_string())
}

fn elapsed_ms(started_at: Instant) -> u64 {
    started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}

fn epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_schema_rejects_free_form_identifiers_and_labels() {
        let interaction_id = Uuid::new_v4();
        let event = DiagnosticEvent::new(
            DiagnosticEventKind::AnswerRequestAccepted,
            DiagnosticComponent::Daemon,
            DiagnosticOutcome::Started,
        )
        .interaction_id(Some(interaction_id))
        .trace_id(Some("not a uuid"))
        .provider("managed")
        .model("model with private free form text");
        let value = serde_json::to_value(event).unwrap();
        assert_eq!(
            value["interaction_id"],
            interaction_id.hyphenated().to_string()
        );
        assert!(value.get("trace_id").is_none());
        assert_eq!(value["provider"], "other");
        assert_eq!(value["model"], "other");
        assert!(value.get("payload").is_none());
        assert!(value.get("message").is_none());
    }

    #[tokio::test]
    async fn bounded_bus_persists_metadata_without_content_fields() {
        let root =
            std::env::temp_dir().join(format!("bluey-diagnostic-bus-{}", Uuid::new_v4().simple()));
        cue_core::app_paths::create_private_dir(&root).unwrap();
        let runtime = DiagnosticRuntime::start(root.clone());
        let scope = SessionAuditScope {
            session_id: Uuid::new_v4(),
            owner_account_id: Some("acct_test".to_string()),
        };
        runtime.emit(
            scope,
            DiagnosticEvent::new(
                DiagnosticEventKind::AnswerCompleted,
                DiagnosticComponent::Daemon,
                DiagnosticOutcome::Succeeded,
            )
            .output_chars(420)
            .duration_ms(Some(125)),
            DiagnosticPriority::Terminal,
        );
        runtime.shutdown().await;
        let text = collect_jsonl_text(&root);
        assert!(text.contains("answer_completed"));
        assert!(text.contains("output_chars"));
        assert!(!text.contains("question"));
        assert!(!text.contains("transcript"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn cloud_tombstone_purges_diagnostics_and_blocks_late_queued_events() {
        let root = std::env::temp_dir().join(format!(
            "bluey-diagnostic-tombstone-{}",
            Uuid::new_v4().simple()
        ));
        cue_core::app_paths::create_private_dir(&root).unwrap();
        let session_id = Uuid::new_v4();
        let owner = "acct-diagnostic-delete";
        let scope = SessionAuditScope {
            session_id,
            owner_account_id: Some(owner.to_string()),
        };
        append_privacy_safe_diagnostic_events_for_scope(
            &root,
            &scope,
            &[("existing".into(), serde_json::json!({"count": 1}))],
        )
        .unwrap();
        assert!(!collect_jsonl_text(&root).is_empty());

        let runtime = DiagnosticRuntime::start(root.clone());
        runtime
            .purge_cloud_tombstoned_session_for_owner(owner, session_id)
            .unwrap();
        runtime.emit(
            scope.clone(),
            DiagnosticEvent::new(
                DiagnosticEventKind::AnswerCompleted,
                DiagnosticComponent::Daemon,
                DiagnosticOutcome::Succeeded,
            ),
            DiagnosticPriority::Terminal,
        );
        runtime.shutdown().await;

        assert!(collect_jsonl_text(&root).is_empty());
        let owner_marker = diagnostic_tombstone_path(&root, owner, session_id).unwrap();
        assert!(owner_marker.is_file());
        assert!(!owner_marker.to_string_lossy().contains(owner));
        let other_owner_scope = SessionAuditScope {
            session_id,
            owner_account_id: Some("acct-other".to_string()),
        };
        assert!(!diagnostic_scope_is_tombstoned(&root, &other_owner_scope));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn revocation_epoch_purges_old_events_and_accepts_only_new_epoch_events() {
        let root = std::env::temp_dir().join(format!(
            "bluey-diagnostic-consent-epoch-{}",
            Uuid::new_v4().simple()
        ));
        cue_core::app_paths::create_private_dir(&root).unwrap();
        let owner = "acct-consent-epoch";
        let old_scope = SessionAuditScope {
            session_id: Uuid::new_v4(),
            owner_account_id: Some(owner.to_string()),
        };
        let runtime = DiagnosticRuntime::start(root.clone());
        runtime.emit(
            old_scope.clone(),
            DiagnosticEvent::new(
                DiagnosticEventKind::AnswerCompleted,
                DiagnosticComponent::Daemon,
                DiagnosticOutcome::Succeeded,
            )
            .count(1),
            DiagnosticPriority::Terminal,
        );

        fence_support_diagnostics_for_owner(&root, owner).unwrap();
        let new_scope = SessionAuditScope {
            session_id: Uuid::new_v4(),
            owner_account_id: Some(owner.to_string()),
        };
        runtime.emit(
            new_scope.clone(),
            DiagnosticEvent::new(
                DiagnosticEventKind::AnswerCompleted,
                DiagnosticComponent::Daemon,
                DiagnosticOutcome::Succeeded,
            )
            .count(2),
            DiagnosticPriority::Terminal,
        );
        runtime.shutdown().await;

        let text = collect_jsonl_text(&root);
        assert!(!text.contains("\"count\":1"));
        assert!(text.contains("\"count\":2"));
        assert!(!support_diagnostic_scope_is_uploadable(&root, &old_scope));
        assert!(support_diagnostic_scope_is_uploadable(&root, &new_scope));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn revocation_epoch_is_strictly_account_scoped() {
        let root = std::env::temp_dir().join(format!(
            "bluey-diagnostic-consent-account-{}",
            Uuid::new_v4().simple()
        ));
        cue_core::app_paths::create_private_dir(&root).unwrap();
        let scope_a = SessionAuditScope {
            session_id: Uuid::new_v4(),
            owner_account_id: Some("acct-a".to_string()),
        };
        let scope_b = SessionAuditScope {
            session_id: Uuid::new_v4(),
            owner_account_id: Some("acct-b".to_string()),
        };
        let runtime = DiagnosticRuntime::start(root.clone());
        runtime.emit(
            scope_b.clone(),
            DiagnosticEvent::new(
                DiagnosticEventKind::AnswerCompleted,
                DiagnosticComponent::Daemon,
                DiagnosticOutcome::Succeeded,
            ),
            DiagnosticPriority::Terminal,
        );
        fence_support_diagnostics_for_owner(&root, "acct-a").unwrap();
        runtime.shutdown().await;

        assert!(!support_diagnostic_scope_is_uploadable(&root, &scope_a));
        assert!(support_diagnostic_scope_is_uploadable(&root, &scope_b));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn server_regrant_revision_cannot_upload_events_from_the_prior_grant() {
        let root = std::env::temp_dir().join(format!(
            "bluey-diagnostic-server-regrant-{}",
            Uuid::new_v4().simple()
        ));
        cue_core::app_paths::create_private_dir(&root).unwrap();
        let owner = "acct-server-regrant";
        align_support_diagnostic_server_revision(&root, owner, 1).unwrap();
        let old_scope = SessionAuditScope {
            session_id: Uuid::new_v4(),
            owner_account_id: Some(owner.to_string()),
        };
        let runtime = DiagnosticRuntime::start(root.clone());
        runtime.emit(
            old_scope.clone(),
            DiagnosticEvent::new(
                DiagnosticEventKind::AnswerCompleted,
                DiagnosticComponent::Daemon,
                DiagnosticOutcome::Succeeded,
            ),
            DiagnosticPriority::Terminal,
        );
        runtime.shutdown().await;
        assert!(support_diagnostic_scope_is_uploadable(&root, &old_scope));

        align_support_diagnostic_server_revision(&root, owner, 1).unwrap();
        assert!(support_diagnostic_scope_is_uploadable(&root, &old_scope));
        align_support_diagnostic_server_revision(&root, owner, 3).unwrap();
        assert!(!support_diagnostic_scope_is_uploadable(&root, &old_scope));
        assert!(collect_jsonl_text(&root).is_empty());

        let new_scope = SessionAuditScope {
            session_id: Uuid::new_v4(),
            owner_account_id: Some(owner.to_string()),
        };
        let runtime = DiagnosticRuntime::start(root.clone());
        runtime.emit(
            new_scope.clone(),
            DiagnosticEvent::new(
                DiagnosticEventKind::AnswerCompleted,
                DiagnosticComponent::Daemon,
                DiagnosticOutcome::Succeeded,
            ),
            DiagnosticPriority::Terminal,
        );
        runtime.shutdown().await;
        assert!(support_diagnostic_scope_is_uploadable(&root, &new_scope));
        std::fs::remove_dir_all(root).unwrap();
    }

    fn collect_jsonl_text(root: &std::path::Path) -> String {
        let mut text = String::new();
        collect_jsonl_text_recursive(&root.join("session-audit-events"), &mut text);
        text
    }

    fn collect_jsonl_text_recursive(path: &Path, text: &mut String) {
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_jsonl_text_recursive(&path, text);
            } else if path.file_name().and_then(|name| name.to_str()) == Some("events.jsonl") {
                if let Ok(value) = std::fs::read_to_string(path) {
                    text.push_str(&value);
                }
            }
        }
    }
}
