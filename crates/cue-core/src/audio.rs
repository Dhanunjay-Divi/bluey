use serde::{Deserialize, Serialize};

use crate::clock;

pub const DEFAULT_CAPTURE_SAMPLE_RATE_HZ: u32 = 48_000;
pub const DEFAULT_STT_SAMPLE_RATE_HZ: u32 = 16_000;
pub const DEFAULT_CHUNK_DURATION_MS: u32 = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioSourceKind {
    System,
    Microphone,
}

impl AudioSourceKind {
    pub fn default_label(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Microphone => "microphone",
        }
    }
}

impl std::fmt::Display for AudioSourceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.default_label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioBackend {
    ScreenCaptureKit,
    CoreAudio,
    Wasapi,
    Cpal,
    Mock,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioRuntimeMode {
    Idle,
    Native,
    SimulatedDevelopment,
    Unavailable,
}

impl Default for AudioRuntimeMode {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioPlatformCapability {
    pub platform: String,
    pub native_capture_available: bool,
    pub simulated_capture_available: bool,
    pub native_backend: Option<AudioBackend>,
    pub note: String,
}

impl AudioPlatformCapability {
    pub fn current() -> Self {
        Self {
            platform: std::env::consts::OS.to_string(),
            native_capture_available: false,
            simulated_capture_available: true,
            native_backend: Some(default_system_backend()),
            note: "Native audio capture is not linked in this build; development PCM simulation is available.".to_string(),
        }
    }

    pub fn native_available(backend: Option<AudioBackend>, note: impl Into<String>) -> Self {
        Self {
            native_capture_available: true,
            native_backend: backend,
            note: note.into(),
            ..Self::current()
        }
    }
}

impl Default for AudioBackend {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioDeviceRole {
    Input,
    Output,
    Loopback,
    Virtual,
    Aggregate,
}

impl AudioSourceKind {
    pub fn default_device_role(self) -> AudioDeviceRole {
        match self {
            Self::System => AudioDeviceRole::Loopback,
            Self::Microphone => AudioDeviceRole::Input,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioDeviceDescriptor {
    pub id: String,
    pub name: String,
    pub source: AudioSourceKind,
    pub role: AudioDeviceRole,
    pub backend: AudioBackend,
    pub is_default: bool,
    pub is_available: bool,
    pub manufacturer: Option<String>,
    pub sample_rate_hz: Option<u32>,
    pub channel_count: Option<u16>,
}

impl AudioDeviceDescriptor {
    pub fn new(
        source: AudioSourceKind,
        backend: AudioBackend,
        id: impl Into<String>,
        name: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            source,
            role: source.default_device_role(),
            backend,
            is_default: false,
            is_available: true,
            manufacturer: None,
            sample_rate_hz: None,
            channel_count: None,
        }
    }

    pub fn default_system(backend: AudioBackend) -> Self {
        Self {
            is_default: true,
            ..Self::new(
                AudioSourceKind::System,
                backend,
                "default_system",
                "Default system audio",
            )
        }
    }

    pub fn default_microphone(backend: AudioBackend) -> Self {
        Self {
            is_default: true,
            ..Self::new(
                AudioSourceKind::Microphone,
                backend,
                "default_microphone",
                "Default microphone",
            )
        }
    }

    pub fn with_role(mut self, role: AudioDeviceRole) -> Self {
        self.role = role;
        self
    }

    pub fn with_manufacturer(mut self, manufacturer: impl Into<String>) -> Self {
        self.manufacturer = Some(manufacturer.into());
        self
    }

    pub fn with_format(mut self, sample_rate_hz: u32, channel_count: u16) -> Self {
        self.sample_rate_hz = Some(sample_rate_hz);
        self.channel_count = Some(channel_count);
        self
    }

    pub fn unavailable(mut self) -> Self {
        self.is_available = false;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioSampleFormat {
    F32,
    I16,
    I24,
    I32,
    U8,
}

impl Default for AudioSampleFormat {
    fn default() -> Self {
        Self::F32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioStreamFormat {
    pub sample_rate_hz: u32,
    pub channel_count: u16,
    pub sample_format: AudioSampleFormat,
}

impl AudioStreamFormat {
    pub fn new(sample_rate_hz: u32, channel_count: u16, sample_format: AudioSampleFormat) -> Self {
        Self {
            sample_rate_hz,
            channel_count,
            sample_format,
        }
    }

    pub fn capture_mono() -> Self {
        Self::new(DEFAULT_CAPTURE_SAMPLE_RATE_HZ, 1, AudioSampleFormat::F32)
    }

    pub fn capture_stereo() -> Self {
        Self::new(DEFAULT_CAPTURE_SAMPLE_RATE_HZ, 2, AudioSampleFormat::F32)
    }

    pub fn stt_mono() -> Self {
        Self::new(DEFAULT_STT_SAMPLE_RATE_HZ, 1, AudioSampleFormat::F32)
    }

    pub fn estimated_frame_count(self, duration_ms: u32) -> u64 {
        self.sample_rate_hz as u64 * duration_ms as u64 / 1_000
    }
}

impl Default for AudioStreamFormat {
    fn default() -> Self {
        Self::capture_mono()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioSourceConfig {
    pub source: AudioSourceKind,
    pub enabled: bool,
    pub device_id: Option<String>,
    pub fallback_to_default: bool,
    pub preferred_format: AudioStreamFormat,
}

impl AudioSourceConfig {
    pub fn system_default() -> Self {
        Self {
            source: AudioSourceKind::System,
            enabled: true,
            device_id: None,
            fallback_to_default: true,
            preferred_format: AudioStreamFormat::capture_stereo(),
        }
    }

    pub fn microphone_default() -> Self {
        Self {
            source: AudioSourceKind::Microphone,
            enabled: true,
            device_id: None,
            fallback_to_default: true,
            preferred_format: AudioStreamFormat::capture_mono(),
        }
    }

    pub fn disabled(source: AudioSourceKind) -> Self {
        let mut config = match source {
            AudioSourceKind::System => Self::system_default(),
            AudioSourceKind::Microphone => Self::microphone_default(),
        };
        config.enabled = false;
        config
    }

    pub fn with_device_id(mut self, device_id: impl Into<String>) -> Self {
        self.device_id = Some(device_id.into());
        self
    }

    pub fn with_preferred_format(mut self, preferred_format: AudioStreamFormat) -> Self {
        self.preferred_format = preferred_format;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioMixStrategy {
    SeparateTracks,
    MixedMono,
    MixedStereo,
}

impl Default for AudioMixStrategy {
    fn default() -> Self {
        Self::SeparateTracks
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioCaptureConfig {
    pub system: AudioSourceConfig,
    pub microphone: AudioSourceConfig,
    pub target_format: AudioStreamFormat,
    pub chunk_duration_ms: u32,
    pub mix_strategy: AudioMixStrategy,
    pub echo_cancellation: bool,
    pub noise_suppression: bool,
    pub automatic_gain_control: bool,
}

impl AudioCaptureConfig {
    pub fn dual_default() -> Self {
        Self::default()
    }

    pub fn from_enabled_sources(enable_system: bool, enable_microphone: bool) -> Self {
        let mut config = Self::default();
        config.system.enabled = enable_system;
        config.microphone.enabled = enable_microphone;
        config
    }

    pub fn microphone_only() -> Self {
        Self {
            system: AudioSourceConfig::disabled(AudioSourceKind::System),
            ..Self::default()
        }
    }

    pub fn system_only() -> Self {
        Self {
            microphone: AudioSourceConfig::disabled(AudioSourceKind::Microphone),
            ..Self::default()
        }
    }

    pub fn enabled_sources(&self) -> Vec<AudioSourceKind> {
        let mut sources = Vec::new();
        if self.system.enabled {
            sources.push(AudioSourceKind::System);
        }
        if self.microphone.enabled {
            sources.push(AudioSourceKind::Microphone);
        }
        sources
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioPipelineStatus {
    pub config: AudioCaptureConfig,
    pub plan: AudioCapturePlan,
    pub capture: AudioCaptureStatus,
    pub devices: Vec<AudioDeviceDescriptor>,
    pub session_id: Option<String>,
    pub runtime_mode: AudioRuntimeMode,
    pub platform: AudioPlatformCapability,
    pub stt_provider: Option<String>,
    pub backend_ready: bool,
    pub transcript_segments_emitted: u64,
    pub note: Option<String>,
    pub updated_at: String,
}

impl AudioPipelineStatus {
    pub fn idle() -> Self {
        let config = AudioCaptureConfig::default();
        let plan = AudioCapturePlan::from_config(config.clone());
        Self {
            config,
            plan,
            capture: AudioCaptureStatus::idle(),
            devices: default_planned_devices(),
            session_id: None,
            runtime_mode: AudioRuntimeMode::Idle,
            platform: AudioPlatformCapability::current(),
            stt_provider: None,
            backend_ready: false,
            transcript_segments_emitted: 0,
            note: Some("Native audio capture is not started.".to_string()),
            updated_at: clock::now_epoch_ms_string(),
        }
    }

    pub fn planned(config: AudioCaptureConfig) -> Self {
        let plan = AudioCapturePlan::from_config(config.clone());
        Self {
            capture: AudioCaptureStatus::from_plan(&plan),
            config,
            plan,
            devices: default_planned_devices(),
            session_id: None,
            runtime_mode: AudioRuntimeMode::Unavailable,
            platform: AudioPlatformCapability::current(),
            stt_provider: None,
            backend_ready: false,
            transcript_segments_emitted: 0,
            note: Some(
                "Audio session is planned. Native capture is not linked in this build.".to_string(),
            ),
            updated_at: clock::now_epoch_ms_string(),
        }
    }

    pub fn simulated(session_id: impl Into<String>, config: AudioCaptureConfig) -> Self {
        let session_id = session_id.into();
        let devices = simulated_devices(&config);
        let mut plan = AudioCapturePlan::from_config(config.clone());
        if let Some(device) = devices
            .iter()
            .find(|device| device.source == AudioSourceKind::System)
            .cloned()
        {
            plan.system = plan.system.with_selected_device(device);
        }
        if let Some(device) = devices
            .iter()
            .find(|device| device.source == AudioSourceKind::Microphone)
            .cloned()
        {
            plan.microphone = plan.microphone.with_selected_device(device);
        }

        Self {
            config,
            capture: AudioCaptureStatus::capturing(&plan),
            plan,
            devices,
            session_id: Some(session_id),
            runtime_mode: AudioRuntimeMode::SimulatedDevelopment,
            platform: AudioPlatformCapability::current(),
            stt_provider: Some("simulated-dev-stt".to_string()),
            backend_ready: true,
            transcript_segments_emitted: 0,
            note: Some(
                "Using development PCM simulation; no native system or microphone audio is captured."
                    .to_string(),
            ),
            updated_at: clock::now_epoch_ms_string(),
        }
    }

    pub fn native(
        session_id: impl Into<String>,
        config: AudioCaptureConfig,
        devices: Vec<AudioDeviceDescriptor>,
        stt_provider: impl Into<String>,
        note: impl Into<String>,
    ) -> Self {
        let session_id = session_id.into();
        let mut plan = AudioCapturePlan::from_config(config.clone());
        if let Some(device) = devices
            .iter()
            .find(|device| device.source == AudioSourceKind::System)
            .cloned()
        {
            plan.system = plan.system.with_selected_device(device);
        }
        if let Some(device) = devices
            .iter()
            .find(|device| device.source == AudioSourceKind::Microphone)
            .cloned()
        {
            plan.microphone = plan.microphone.with_selected_device(device);
        }

        let backend = devices.first().map(|device| device.backend);
        Self {
            config,
            capture: AudioCaptureStatus::capturing(&plan),
            plan,
            devices,
            session_id: Some(session_id),
            runtime_mode: AudioRuntimeMode::Native,
            platform: AudioPlatformCapability::native_available(backend, note.into()),
            stt_provider: Some(stt_provider.into()),
            backend_ready: true,
            transcript_segments_emitted: 0,
            note: Some(
                "Using real chunked audio capture and live speech-to-text. Audio files are transient and removed after transcription."
                    .to_string(),
            ),
            updated_at: clock::now_epoch_ms_string(),
        }
    }

    pub fn stopped(mut self) -> Self {
        self.capture = AudioCaptureStatus::idle();
        self.capture.state = AudioCaptureState::Stopped;
        self.session_id = None;
        self.runtime_mode = AudioRuntimeMode::Idle;
        self.backend_ready = false;
        self.note = Some("Audio capture stopped.".to_string());
        self.updated_at = clock::now_epoch_ms_string();
        self
    }

    pub fn active_source_count(&self) -> usize {
        self.config.enabled_sources().len()
    }

    pub fn record_chunk(&mut self, chunk: &AudioChunkMetadata) {
        match chunk.source {
            AudioSourceKind::System => self.capture.system.record_chunk(chunk.sequence),
            AudioSourceKind::Microphone => self.capture.microphone.record_chunk(chunk.sequence),
        }
        self.capture.state = AudioCaptureState::Capturing;
        self.capture.updated_at = clock::now_epoch_ms_string();
        self.updated_at = self.capture.updated_at.clone();
    }

    pub fn record_stt_segment(&mut self) {
        self.transcript_segments_emitted = self.transcript_segments_emitted.saturating_add(1);
        self.updated_at = clock::now_epoch_ms_string();
    }

    pub fn record_drop(&mut self, source: AudioSourceKind, message: impl Into<String>) {
        let message = message.into();
        match source {
            AudioSourceKind::System => self.capture.system.record_drop(message.clone()),
            AudioSourceKind::Microphone => self.capture.microphone.record_drop(message.clone()),
        }
        self.capture.last_error = Some(message);
        self.capture.updated_at = clock::now_epoch_ms_string();
        self.updated_at = self.capture.updated_at.clone();
    }
}

pub fn default_planned_devices() -> Vec<AudioDeviceDescriptor> {
    vec![
        AudioDeviceDescriptor::default_system(default_system_backend()),
        AudioDeviceDescriptor::default_microphone(default_microphone_backend()),
    ]
}

pub fn simulated_devices(config: &AudioCaptureConfig) -> Vec<AudioDeviceDescriptor> {
    let mut devices = Vec::new();
    if config.system.enabled {
        devices.push(
            AudioDeviceDescriptor::default_system(AudioBackend::Mock)
                .with_format(DEFAULT_STT_SAMPLE_RATE_HZ, 1)
                .with_manufacturer("Bluey development runtime"),
        );
    }
    if config.microphone.enabled {
        devices.push(
            AudioDeviceDescriptor::default_microphone(AudioBackend::Mock)
                .with_format(DEFAULT_STT_SAMPLE_RATE_HZ, 1)
                .with_manufacturer("Bluey development runtime"),
        );
    }
    devices
}

#[cfg(target_os = "macos")]
fn default_system_backend() -> AudioBackend {
    AudioBackend::ScreenCaptureKit
}

#[cfg(target_os = "windows")]
fn default_system_backend() -> AudioBackend {
    AudioBackend::Wasapi
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn default_system_backend() -> AudioBackend {
    AudioBackend::Unknown
}

#[cfg(target_os = "macos")]
fn default_microphone_backend() -> AudioBackend {
    AudioBackend::CoreAudio
}

#[cfg(target_os = "windows")]
fn default_microphone_backend() -> AudioBackend {
    AudioBackend::Cpal
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn default_microphone_backend() -> AudioBackend {
    AudioBackend::Unknown
}

impl Default for AudioCaptureConfig {
    fn default() -> Self {
        Self {
            system: AudioSourceConfig::system_default(),
            microphone: AudioSourceConfig::microphone_default(),
            target_format: AudioStreamFormat::stt_mono(),
            chunk_duration_ms: DEFAULT_CHUNK_DURATION_MS,
            mix_strategy: AudioMixStrategy::SeparateTracks,
            echo_cancellation: false,
            noise_suppression: true,
            automatic_gain_control: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioSourcePlanStatus {
    Disabled,
    AwaitingDevice,
    Ready,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioSourcePlan {
    pub source: AudioSourceKind,
    pub enabled: bool,
    pub requested_device_id: Option<String>,
    pub fallback_to_default: bool,
    pub selected_device: Option<AudioDeviceDescriptor>,
    pub format: AudioStreamFormat,
    pub status: AudioSourcePlanStatus,
}

impl AudioSourcePlan {
    pub fn from_config(config: &AudioSourceConfig) -> Self {
        Self {
            source: config.source,
            enabled: config.enabled,
            requested_device_id: config.device_id.clone(),
            fallback_to_default: config.fallback_to_default,
            selected_device: None,
            format: config.preferred_format,
            status: if config.enabled {
                AudioSourcePlanStatus::AwaitingDevice
            } else {
                AudioSourcePlanStatus::Disabled
            },
        }
    }

    pub fn with_selected_device(mut self, device: AudioDeviceDescriptor) -> Self {
        if let Some(sample_rate_hz) = device.sample_rate_hz {
            self.format.sample_rate_hz = sample_rate_hz;
        }
        if let Some(channel_count) = device.channel_count {
            self.format.channel_count = channel_count;
        }

        self.status = if !self.enabled {
            AudioSourcePlanStatus::Disabled
        } else if device.is_available {
            AudioSourcePlanStatus::Ready
        } else {
            AudioSourcePlanStatus::Unavailable
        };
        self.selected_device = Some(device);
        self
    }

    pub fn selected_device_id(&self) -> Option<&str> {
        self.selected_device
            .as_ref()
            .map(|device| device.id.as_str())
            .or(self.requested_device_id.as_deref())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioCapturePlan {
    pub created_at: String,
    pub system: AudioSourcePlan,
    pub microphone: AudioSourcePlan,
    pub target_format: AudioStreamFormat,
    pub chunk_duration_ms: u32,
    pub mix_strategy: AudioMixStrategy,
}

impl AudioCapturePlan {
    pub fn from_config(config: AudioCaptureConfig) -> Self {
        Self {
            created_at: clock::now_epoch_ms_string(),
            system: AudioSourcePlan::from_config(&config.system),
            microphone: AudioSourcePlan::from_config(&config.microphone),
            target_format: config.target_format,
            chunk_duration_ms: config.chunk_duration_ms,
            mix_strategy: config.mix_strategy,
        }
    }

    pub fn enabled_sources(&self) -> Vec<AudioSourceKind> {
        let mut sources = Vec::new();
        if self.system.enabled {
            sources.push(AudioSourceKind::System);
        }
        if self.microphone.enabled {
            sources.push(AudioSourceKind::Microphone);
        }
        sources
    }

    pub fn is_dual_capture(&self) -> bool {
        self.system.enabled && self.microphone.enabled
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioCaptureState {
    Idle,
    Planning,
    Starting,
    Capturing,
    Paused,
    Stopping,
    Stopped,
    Failed,
}

impl Default for AudioCaptureState {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioSourceState {
    Disabled,
    AwaitingDevice,
    Ready,
    Capturing,
    Muted,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioSourceStatus {
    pub source: AudioSourceKind,
    pub state: AudioSourceState,
    pub device_id: Option<String>,
    pub chunks_captured: u64,
    pub chunks_dropped: u64,
    pub last_sequence: Option<u64>,
    pub last_audio_at: Option<String>,
    pub last_error: Option<String>,
}

impl AudioSourceStatus {
    pub fn disabled(source: AudioSourceKind) -> Self {
        Self {
            source,
            state: AudioSourceState::Disabled,
            device_id: None,
            chunks_captured: 0,
            chunks_dropped: 0,
            last_sequence: None,
            last_audio_at: None,
            last_error: None,
        }
    }

    pub fn awaiting_device(source: AudioSourceKind, device_id: Option<String>) -> Self {
        Self {
            source,
            state: AudioSourceState::AwaitingDevice,
            device_id,
            chunks_captured: 0,
            chunks_dropped: 0,
            last_sequence: None,
            last_audio_at: None,
            last_error: None,
        }
    }

    pub fn from_plan(plan: &AudioSourcePlan) -> Self {
        let device_id = plan.selected_device_id().map(str::to_string);
        match plan.status {
            AudioSourcePlanStatus::Disabled => Self::disabled(plan.source),
            AudioSourcePlanStatus::AwaitingDevice => Self::awaiting_device(plan.source, device_id),
            AudioSourcePlanStatus::Ready => Self {
                source: plan.source,
                state: AudioSourceState::Ready,
                device_id,
                chunks_captured: 0,
                chunks_dropped: 0,
                last_sequence: None,
                last_audio_at: None,
                last_error: None,
            },
            AudioSourcePlanStatus::Unavailable => Self {
                source: plan.source,
                state: AudioSourceState::Failed,
                device_id,
                chunks_captured: 0,
                chunks_dropped: 0,
                last_sequence: None,
                last_audio_at: None,
                last_error: Some("audio source unavailable".to_string()),
            },
        }
    }

    pub fn record_chunk(&mut self, sequence: u64) {
        self.state = AudioSourceState::Capturing;
        self.chunks_captured = self.chunks_captured.saturating_add(1);
        self.last_sequence = Some(sequence);
        self.last_audio_at = Some(clock::now_epoch_ms_string());
        self.last_error = None;
    }

    pub fn record_drop(&mut self, message: impl Into<String>) {
        self.chunks_dropped = self.chunks_dropped.saturating_add(1);
        self.last_error = Some(message.into());
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioCaptureStatus {
    pub state: AudioCaptureState,
    /// Set when a capture error is classified as permission denial.
    pub permission_denied_source: Option<AudioSourceKind>,
    pub started_at: Option<String>,
    pub updated_at: String,
    pub system: AudioSourceStatus,
    pub microphone: AudioSourceStatus,
    pub last_error: Option<String>,
}

impl AudioCaptureStatus {
    pub fn idle() -> Self {
        Self {
            state: AudioCaptureState::Idle,
            permission_denied_source: None,
            started_at: None,
            updated_at: clock::now_epoch_ms_string(),
            system: AudioSourceStatus::disabled(AudioSourceKind::System),
            microphone: AudioSourceStatus::disabled(AudioSourceKind::Microphone),
            last_error: None,
        }
    }

    pub fn from_plan(plan: &AudioCapturePlan) -> Self {
        Self {
            state: AudioCaptureState::Planning,
            permission_denied_source: None,
            started_at: None,
            updated_at: clock::now_epoch_ms_string(),
            system: AudioSourceStatus::from_plan(&plan.system),
            microphone: AudioSourceStatus::from_plan(&plan.microphone),
            last_error: None,
        }
    }

    pub fn capturing(plan: &AudioCapturePlan) -> Self {
        let now = clock::now_epoch_ms_string();
        let mut status = Self::from_plan(plan);
        status.state = AudioCaptureState::Capturing;
        status.started_at = Some(now.clone());
        status.updated_at = now;
        if status.system.state == AudioSourceState::Ready {
            status.system.state = AudioSourceState::Capturing;
        }
        if status.microphone.state == AudioSourceState::Ready {
            status.microphone.state = AudioSourceState::Capturing;
        }
        status
    }

    pub fn failed(message: impl Into<String>) -> Self {
        let mut status = Self::idle();
        status.state = AudioCaptureState::Failed;
        status.last_error = Some(message.into());
        status
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self.state,
            AudioCaptureState::Starting
                | AudioCaptureState::Capturing
                | AudioCaptureState::Paused
                | AudioCaptureState::Stopping
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioTimeRange {
    pub start_ms: u64,
    pub duration_ms: u32,
}

impl AudioTimeRange {
    pub fn new(start_ms: u64, duration_ms: u32) -> Self {
        Self {
            start_ms,
            duration_ms,
        }
    }

    pub fn end_ms(self) -> u64 {
        self.start_ms.saturating_add(self.duration_ms as u64)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioChunkMetadata {
    pub source: AudioSourceKind,
    pub stream_id: String,
    pub sequence: u64,
    pub captured_at: String,
    pub time: AudioTimeRange,
    pub format: AudioStreamFormat,
    pub frame_count: u64,
    pub byte_len: u64,
    pub rms_dbfs: Option<f32>,
    pub peak_dbfs: Option<f32>,
    pub speech_probability: Option<f32>,
}

impl AudioChunkMetadata {
    pub fn new(
        source: AudioSourceKind,
        stream_id: impl Into<String>,
        sequence: u64,
        start_ms: u64,
        duration_ms: u32,
        format: AudioStreamFormat,
        byte_len: u64,
    ) -> Self {
        Self {
            source,
            stream_id: stream_id.into(),
            sequence,
            captured_at: clock::now_epoch_ms_string(),
            time: AudioTimeRange::new(start_ms, duration_ms),
            format,
            frame_count: format.estimated_frame_count(duration_ms),
            byte_len,
            rms_dbfs: None,
            peak_dbfs: None,
            speech_probability: None,
        }
    }

    pub fn with_levels(mut self, rms_dbfs: f32, peak_dbfs: f32) -> Self {
        self.rms_dbfs = Some(rms_dbfs);
        self.peak_dbfs = Some(peak_dbfs);
        self
    }

    pub fn with_speech_probability(mut self, speech_probability: f32) -> Self {
        self.speech_probability = Some(speech_probability);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SttSegmentMetadata {
    pub provider_segment_id: Option<String>,
    pub source: Option<AudioSourceKind>,
    pub speaker_label: Option<String>,
    pub text: String,
    pub language: Option<String>,
    pub confidence: Option<f32>,
    pub time: AudioTimeRange,
    pub source_sequence_start: Option<u64>,
    pub source_sequence_end: Option<u64>,
    pub is_final: bool,
    pub created_at: String,
}

impl SttSegmentMetadata {
    pub fn new(text: impl Into<String>, start_ms: u64, duration_ms: u32, is_final: bool) -> Self {
        Self {
            provider_segment_id: None,
            source: None,
            speaker_label: None,
            text: text.into(),
            language: None,
            confidence: None,
            time: AudioTimeRange::new(start_ms, duration_ms),
            source_sequence_start: None,
            source_sequence_end: None,
            is_final,
            created_at: clock::now_epoch_ms_string(),
        }
    }

    pub fn with_provider_segment_id(mut self, provider_segment_id: impl Into<String>) -> Self {
        self.provider_segment_id = Some(provider_segment_id.into());
        self
    }

    pub fn with_source(mut self, source: AudioSourceKind) -> Self {
        self.source = Some(source);
        self
    }

    pub fn with_speaker_label(mut self, speaker_label: impl Into<String>) -> Self {
        self.speaker_label = Some(speaker_label.into());
        self
    }

    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = Some(confidence);
        self
    }

    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.language = Some(language.into());
        self
    }

    pub fn with_source_sequence_range(mut self, start: u64, end: u64) -> Self {
        self.source_sequence_start = Some(start);
        self.source_sequence_end = Some(end);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AudioEvent {
    CapturePlanned {
        plan: AudioCapturePlan,
    },
    CaptureStarted {
        status: AudioCaptureStatus,
    },
    CaptureStopped {
        status: AudioCaptureStatus,
    },
    SourceStatusChanged {
        source: AudioSourceKind,
        status: AudioSourceStatus,
    },
    DeviceListChanged {
        devices: Vec<AudioDeviceDescriptor>,
    },
    ChunkCaptured {
        chunk: AudioChunkMetadata,
    },
    ChunkDropped {
        source: AudioSourceKind,
        sequence: u64,
        reason: String,
    },
    SttSegment {
        segment: SttSegmentMetadata,
    },
    PermissionDenied {
        source: AudioSourceKind,
    },
    Error {
        source: Option<AudioSourceKind>,
        message: String,
    },
}

impl AudioEvent {
    pub fn error(source: Option<AudioSourceKind>, message: impl Into<String>) -> Self {
        Self::Error {
            source,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimulatedPcmChunk {
    pub metadata: AudioChunkMetadata,
    pub pcm_f32: Vec<f32>,
}

impl SimulatedPcmChunk {
    pub fn new(
        source: AudioSourceKind,
        stream_id: impl Into<String>,
        sequence: u64,
        duration_ms: u32,
        format: AudioStreamFormat,
    ) -> Self {
        let frame_count = format.estimated_frame_count(duration_ms);
        let sample_count = frame_count.saturating_mul(format.channel_count as u64) as usize;
        let amplitude = match source {
            AudioSourceKind::System => 0.18,
            AudioSourceKind::Microphone => 0.12,
        };
        let pcm_f32 = (0..sample_count)
            .map(|sample| {
                let phase = (sample as f32 / 16.0) % std::f32::consts::TAU;
                phase.sin() * amplitude
            })
            .collect::<Vec<_>>();
        let byte_len = (pcm_f32.len() * std::mem::size_of::<f32>()) as u64;
        let start_ms = sequence.saturating_sub(1) * duration_ms as u64;
        let metadata = AudioChunkMetadata::new(
            source,
            stream_id,
            sequence,
            start_ms,
            duration_ms,
            format,
            byte_len,
        )
        .with_levels(-24.0, -12.0)
        .with_speech_probability(0.86);

        Self { metadata, pcm_f32 }
    }

    pub fn transcript_segment(&self) -> SttSegmentMetadata {
        let label = match self.metadata.source {
            AudioSourceKind::System => "system",
            AudioSourceKind::Microphone => "microphone",
        };
        SttSegmentMetadata::new(
            format!(
                "[dev audio:{label}] simulated speech chunk {}",
                self.metadata.sequence
            ),
            self.metadata.time.start_ms,
            self.metadata.time.duration_ms,
            true,
        )
        .with_provider_segment_id(format!("{label}-{}", self.metadata.sequence))
        .with_source(self.metadata.source)
        .with_speaker_label(label)
        .with_confidence(0.99)
        .with_language("en")
        .with_source_sequence_range(self.metadata.sequence, self.metadata.sequence)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_plans_dual_capture() {
        let config = AudioCaptureConfig::default();
        assert_eq!(
            config.enabled_sources(),
            vec![AudioSourceKind::System, AudioSourceKind::Microphone]
        );
        assert_eq!(config.target_format, AudioStreamFormat::stt_mono());

        let plan = AudioCapturePlan::from_config(config);
        assert!(plan.is_dual_capture());
        assert_eq!(plan.system.status, AudioSourcePlanStatus::AwaitingDevice);
        assert_eq!(
            plan.microphone.status,
            AudioSourcePlanStatus::AwaitingDevice
        );
    }

    #[test]
    fn selected_device_marks_source_ready_and_updates_format() {
        let config = AudioSourceConfig::microphone_default().with_device_id("mic-1");
        let device = AudioDeviceDescriptor::default_microphone(AudioBackend::CoreAudio)
            .with_format(44_100, 2);

        let plan = AudioSourcePlan::from_config(&config).with_selected_device(device);

        assert_eq!(plan.status, AudioSourcePlanStatus::Ready);
        assert_eq!(plan.selected_device_id(), Some("default_microphone"));
        assert_eq!(plan.format.sample_rate_hz, 44_100);
        assert_eq!(plan.format.channel_count, 2);
    }

    #[test]
    fn chunk_metadata_tracks_timing_and_frames() {
        let chunk = AudioChunkMetadata::new(
            AudioSourceKind::System,
            "system-track",
            7,
            2_000,
            500,
            AudioStreamFormat::capture_stereo(),
            96_000,
        )
        .with_levels(-18.0, -3.0)
        .with_speech_probability(0.72);

        assert_eq!(chunk.time.end_ms(), 2_500);
        assert_eq!(chunk.frame_count, 24_000);
        assert_eq!(chunk.rms_dbfs, Some(-18.0));
        assert_eq!(chunk.speech_probability, Some(0.72));
    }

    #[test]
    fn capturing_status_preserves_disabled_sources() {
        let mut plan = AudioCapturePlan::from_config(AudioCaptureConfig::microphone_only());
        plan.microphone =
            plan.microphone
                .with_selected_device(AudioDeviceDescriptor::default_microphone(
                    AudioBackend::CoreAudio,
                ));

        let status = AudioCaptureStatus::capturing(&plan);

        assert!(status.is_active());
        assert_eq!(status.system.state, AudioSourceState::Disabled);
        assert_eq!(status.microphone.state, AudioSourceState::Capturing);
    }

    #[test]
    fn audio_event_serializes_with_snake_case_tag() {
        let chunk = AudioChunkMetadata::new(
            AudioSourceKind::System,
            "system-track",
            1,
            0,
            DEFAULT_CHUNK_DURATION_MS,
            AudioStreamFormat::stt_mono(),
            64_000,
        );
        let event = AudioEvent::ChunkCaptured { chunk };

        let value = serde_json::to_value(event).expect("event serializes");
        assert_eq!(value["type"], "chunk_captured");
        assert_eq!(value["chunk"]["source"], "system");
        assert_eq!(value["chunk"]["format"]["sample_rate_hz"], 16_000);
    }

    #[test]
    fn pipeline_status_is_honest_about_pending_backend() {
        let status = AudioPipelineStatus::planned(AudioCaptureConfig::default());

        assert_eq!(status.capture.state, AudioCaptureState::Planning);
        assert!(!status.backend_ready);
        assert_eq!(status.runtime_mode, AudioRuntimeMode::Unavailable);
        assert!(!status.platform.native_capture_available);
        assert!(status.platform.simulated_capture_available);
        assert_eq!(status.active_source_count(), 2);
        assert_eq!(status.devices.len(), 2);
    }

    #[test]
    fn simulated_status_marks_enabled_sources_capturing_without_native_claims() {
        let status = AudioPipelineStatus::simulated("audio-test", AudioCaptureConfig::default());

        assert_eq!(status.runtime_mode, AudioRuntimeMode::SimulatedDevelopment);
        assert!(status.backend_ready);
        assert!(!status.platform.native_capture_available);
        assert_eq!(status.stt_provider.as_deref(), Some("simulated-dev-stt"));
        assert_eq!(status.capture.system.state, AudioSourceState::Capturing);
        assert_eq!(status.capture.microphone.state, AudioSourceState::Capturing);
        assert!(status
            .note
            .as_deref()
            .unwrap_or_default()
            .contains("no native"));
    }

    #[test]
    fn simulated_pcm_chunk_emits_labeled_transcript_metadata() {
        let chunk = SimulatedPcmChunk::new(
            AudioSourceKind::Microphone,
            "microphone-dev",
            3,
            250,
            AudioStreamFormat::stt_mono(),
        );
        let segment = chunk.transcript_segment();

        assert_eq!(chunk.metadata.frame_count, 4_000);
        assert_eq!(segment.source, Some(AudioSourceKind::Microphone));
        assert_eq!(segment.speaker_label.as_deref(), Some("microphone"));
        assert!(segment.text.contains("[dev audio:microphone]"));
        assert_eq!(segment.source_sequence_start, Some(3));
        assert_eq!(segment.source_sequence_end, Some(3));
    }

    #[test]
    fn capture_status_permission_denied_source_defaults_to_none() {
        let status = AudioCaptureStatus::idle();
        assert_eq!(status.permission_denied_source, None);
    }

    #[test]
    fn capture_status_permission_denied_source_can_be_set() {
        let mut status = AudioCaptureStatus::idle();
        status.permission_denied_source = Some(AudioSourceKind::Microphone);
        assert_eq!(status.permission_denied_source, Some(AudioSourceKind::Microphone));

        // Verify it serializes correctly
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["permission_denied_source"], "microphone");
    }
}
