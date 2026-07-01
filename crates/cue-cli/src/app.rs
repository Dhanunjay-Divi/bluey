use std::env;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Instant as StdInstant, SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::process::CommandExt;
#[cfg(windows)]
use std::os::windows::process::CommandExt;

use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use cue_core::app_paths::AppPaths;
use cue_core::ipc::{DaemonRequest, DaemonResponse, DEFAULT_DAEMON_ADDR};
use cue_core::{
    load_account, load_settings, new_trace_id, save_settings, trace_id_from_env, AccountConfig,
    ActionItem, AiProviderId, AiProviderKind, AiRuntimeStatus, AnswerRequest, AnswerResponse,
    AudioPipelineStatus, CardKind, CloudSyncStatus, ContextArtifact, CueCard, CueSettings,
    MeetingRecap, MeetingRecord, MemoryHit, OverlayPosition, ProviderRoute, ProviderSelector,
    Speaker, BLUEY_TRACE_ID_ENV,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::time::{sleep, Duration, Instant};

#[derive(Debug, Parser)]
#[command(
    name = "bluey",
    version,
    about = "Bluey AI copilot - stay present, work quietly"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Start Bluey and open an interactive meeting session.
    #[command(hide = true)]
    Run(RunArgs),
    /// Turn Bluey on with the overlay-first product flow.
    On(OnArgs),
    /// Turn Bluey off.
    Off,
    /// Sign in or link Bluey to a cloud account.
    #[command(hide = true)]
    Login(LoginArgs),
    /// Show account and cloud link status.
    #[command(hide = true)]
    Account,
    /// List or inspect saved local sessions.
    #[command(hide = true)]
    Sessions(SessionsArgs),
    /// Show or update terminal-first Bluey settings.
    #[command(hide = true)]
    Settings(SettingsArgs),
    /// Show your Bluey balance, last-7-days usage, and tier projection.
    #[command(hide = true)]
    Usage,
    /// Show your current Bluey balance and 1-year credit-validity reminder.
    /// (Per-batch expiration listing is not yet available; coming in a
    /// future release.)
    #[command(hide = true)]
    Credits,
    /// Check for and install a Bluey desktop update.
    #[command(hide = true)]
    Update(UpdateArgs),
    /// Log out of Bluey: clear local account tokens.
    #[command(hide = true)]
    Logout,
    /// Open the Bluey billing page in your browser to manage credits and provider-backed billing.
    #[command(hide = true)]
    Portal,
    /// Export your Bluey account data as a JSON file (GDPR).
    #[command(hide = true)]
    Export,
    /// Permanently delete your Bluey account (interactive confirmation; --force to skip prompt).
    #[command(hide = true)]
    DeleteAccount {
        #[arg(long)]
        force: bool,
    },
    /// Print a redacted self-diagnosis snapshot for support tickets.
    #[command(hide = true)]
    Doctor {
        /// Emit JSON instead of human-readable text. Schema version 1.
        /// Useful for support automation. Sensitive fields are still
        /// hashed/redacted exactly as the human-readable output is.
        #[arg(long)]
        json: bool,
    },
    /// Manage local Bluey logs.
    #[command(hide = true)]
    Logs {
        #[command(subcommand)]
        command: LogsCommands,
    },
    /// Bundle bluey doctor + logs export into a single zip for support tickets.
    #[command(hide = true)]
    Support {
        /// Disable redaction. By default the bundle strips bearer tokens,
        /// magic-link URLs, billing IDs, provider keys, emails, IPv4
        /// addresses, and /Users/<name>/ paths.
        #[arg(long = "no-redact", default_value_t = false)]
        no_redact: bool,
        /// Include log files modified within the last N days.
        #[arg(long, default_value_t = 7)]
        days: u32,
        /// Output zip path. Defaults to ~/Bluey-support-YYYYMMDD-(redacted|raw).zip
        #[arg(long, short)]
        output: Option<std::path::PathBuf>,
    },
    /// Start the Bluey daemon.
    #[command(hide = true)]
    Start(StartArgs),
    /// Stop the Bluey daemon.
    #[command(hide = true)]
    Stop,
    /// Show daemon status.
    #[command(hide = true)]
    Status,
    /// Control the private overlay.
    #[command(hide = true)]
    Overlay {
        #[command(subcommand)]
        command: OverlayCommands,
    },
    /// Control a meeting session.
    #[command(hide = true)]
    Meeting {
        #[command(subcommand)]
        command: MeetingCommands,
    },
    /// Feed a transcript line into Bluey's meeting engine.
    #[command(hide = true)]
    Listen(ListenArgs),
    /// Ask Bluey a manual question using current session context.
    #[command(hide = true)]
    Ask(AskArgs),
    /// Recap the active or most recent meeting.
    #[command(hide = true)]
    Recap,
    /// List detected action items.
    #[command(hide = true)]
    ActionItems,
    /// Attach or list meeting context files.
    #[command(hide = true)]
    Context {
        #[command(subcommand)]
        command: ContextCommands,
    },
    /// Set or view session answer instructions.
    #[command(hide = true)]
    Instructions {
        #[command(subcommand)]
        command: InstructionsCommands,
    },
    /// Search meeting memory across history and attached context.
    #[command(hide = true)]
    Memory {
        #[command(subcommand)]
        command: MemoryCommands,
    },
    /// Inspect or arm dual system/microphone audio capture.
    #[command(hide = true)]
    Audio {
        #[command(subcommand)]
        command: AudioCommands,
    },
    /// Inspect managed AI routing and answer pipeline readiness.
    #[command(hide = true)]
    Ai {
        #[command(subcommand)]
        command: AiCommands,
    },
    /// Inspect or trigger secure cloud/RAG sync.
    #[command(hide = true)]
    Cloud {
        #[command(subcommand)]
        command: CloudCommands,
    },
    /// Show configured AI provider environment.
    #[command(hide = true)]
    Providers,
    /// Push a local demo card through the daemon/overlay path.
    #[command(hide = true)]
    DevCard(DevCardArgs),
}

#[derive(Debug, Args)]
struct StartArgs {
    /// Run daemon in the current terminal instead of detaching it.
    #[arg(long)]
    foreground: bool,
    /// Do not spawn the native overlay sidecar.
    #[arg(long)]
    no_overlay: bool,
    /// Suppress startup output. Used by `bluey on`.
    #[arg(long, hide = true)]
    quiet: bool,
}

#[derive(Debug, Args)]
struct RunArgs {
    /// Meeting title for a new session.
    #[arg(long)]
    title: Option<String>,
    /// Do not spawn the native overlay sidecar.
    #[arg(long)]
    no_overlay: bool,
    /// Default speaker for unprefixed transcript lines.
    #[arg(long, value_enum, default_value_t = SpeakerArg::System)]
    speaker: SpeakerArg,
}

#[derive(Debug, Args)]
struct OnArgs {
    /// Optional title for the fresh session that `bluey on` starts.
    #[arg(long)]
    title: Option<String>,
}

#[derive(Debug, Args)]
struct UpdateArgs {
    /// Only report whether an update is available.
    #[arg(long)]
    check_only: bool,
    /// Install without the 5-second Esc countdown.
    #[arg(long)]
    yes: bool,
    /// Allow update checks from local target/debug builds.
    #[arg(long)]
    force: bool,
}

#[derive(Debug, Args)]
struct LoginArgs {
    /// Bluey API URL. Defaults to BLUEY_CLOUD_API_URL, CUE_CLOUD_API_URL, or https://bluey.sh.
    #[arg(long)]
    api_url: Option<String>,
    /// Access token for non-browser/dev login. Prefer browser login for real accounts.
    #[arg(long)]
    token: Option<String>,
    /// Refresh token for non-browser/dev login.
    #[arg(long)]
    refresh_token: Option<String>,
    /// Link local mode without a cloud token.
    #[arg(long)]
    local: bool,
    /// Do not open a browser; only save provided token/env/local account values.
    #[arg(long)]
    no_browser: bool,
    /// Workspace id for cloud/RAG scope.
    #[arg(long, default_value = "default")]
    workspace: String,
    /// User id stored with this device.
    #[arg(long, default_value = "local-user")]
    user: String,
}

#[derive(Debug, Args)]
struct SessionsArgs {
    /// Show a single session by id prefix.
    #[arg(long)]
    show: Option<String>,
    /// Print raw JSON.
    #[arg(long)]
    json: bool,
    /// Maximum sessions to list.
    #[arg(long, default_value_t = 20)]
    limit: usize,
}

#[derive(Debug, Args)]
struct SettingsArgs {
    /// Reset settings to defaults.
    #[arg(long)]
    reset: bool,
    /// Set default answer style/instructions.
    #[arg(long)]
    answer_style: Option<String>,
    /// Set default model label used by product UI.
    #[arg(long)]
    model: Option<String>,
    /// Set default answer mode.
    #[arg(long)]
    mode: Option<String>,
    /// Set overlay opacity as percent or 0.0-1.0.
    #[arg(long)]
    opacity: Option<f32>,
    /// Enable or disable cloud sync preference.
    #[arg(long)]
    cloud_sync: Option<bool>,
    /// Local/cloud retention target in days.
    #[arg(long)]
    retention_days: Option<u32>,
}

#[derive(Debug, clap::Subcommand)]
enum LogsCommands {
    /// Bundle local Bluey logs into a redacted zip for support.
    Export {
        /// Disable redaction. By default the export strips bearer tokens,
        /// magic-link URLs, billing IDs, provider keys, emails, and IPv4
        /// addresses. Use --no-redact only when you control where the zip
        /// is going.
        #[arg(long = "no-redact", default_value_t = false)]
        no_redact: bool,
        /// Include log files modified within the last N days.
        #[arg(long, default_value_t = 7)]
        days: u32,
        /// Output zip path. Defaults to ~/Bluey-logs-YYYYMMDD-(redacted|raw).zip
        #[arg(long, short)]
        output: Option<std::path::PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
enum OverlayCommands {
    Show,
    Hide,
    Toggle,
    Clear,
    Opacity { value: f32 },
    Position { position: OverlayPositionArg },
}

#[derive(Debug, Subcommand)]
enum MeetingCommands {
    Start(MeetingStartArgs),
    End,
}

#[derive(Debug, Subcommand)]
enum ContextCommands {
    Add(ContextAddArgs),
    Capture(ContextCaptureArgs),
    Page,
    List,
    Watch {
        #[command(subcommand)]
        command: ContextWatchCommands,
    },
}

#[derive(Debug, Args)]
struct ContextAddArgs {
    path: PathBuf,
    #[arg(long)]
    title: Option<String>,
    #[arg(long)]
    note: Option<String>,
}

#[derive(Debug, Args)]
struct ContextCaptureArgs {
    /// Capture the whole screen instead of using the OS region/window picker.
    #[arg(long)]
    full_screen: bool,
    /// Attach without asking for confirmation after capture.
    #[arg(long)]
    yes: bool,
    /// Do not open the captured image for local preview before confirmation.
    #[arg(long)]
    no_preview: bool,
    /// Optional title for the captured context item.
    #[arg(long)]
    title: Option<String>,
    /// Optional note stored with the captured context item.
    #[arg(long)]
    note: Option<String>,
}

#[derive(Debug, Subcommand)]
enum ContextWatchCommands {
    Start(ContextWatchStartArgs),
    Stop,
}

#[derive(Debug, Args)]
struct ContextWatchStartArgs {
    /// Seconds between approved background captures while the eye mode is on.
    #[arg(long, default_value_t = 12)]
    interval: u64,
}

#[derive(Debug, Subcommand)]
enum InstructionsCommands {
    Set(InstructionsSetArgs),
    Show,
    Clear,
}

#[derive(Debug, Subcommand)]
enum MemoryCommands {
    Search(MemorySearchArgs),
}

#[derive(Debug, Subcommand)]
enum AudioCommands {
    Status,
    Start(AudioStartArgs),
    Stop,
}

#[derive(Debug, Args)]
struct AudioStartArgs {
    /// Disable system audio capture planning for this session.
    #[arg(long)]
    no_system: bool,
    /// Disable microphone capture planning for this session.
    #[arg(long)]
    no_microphone: bool,
}

#[derive(Debug, Subcommand)]
enum AiCommands {
    Status,
}

#[derive(Debug, Subcommand)]
enum CloudCommands {
    Status,
    Sync,
    /// List cloud-synced sessions for this account.
    Sessions {
        #[arg(long, default_value_t = 20)]
        limit: i64,
        #[arg(long)]
        json: bool,
    },
    /// Show a cloud-synced session bundle by id.
    Show {
        id: String,
        #[arg(long)]
        json: bool,
    },
    /// Query account-scoped cloud RAG memory.
    Rag {
        query: Vec<String>,
        #[arg(long, default_value_t = 8)]
        limit: i64,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Args)]
struct MemorySearchArgs {
    query: Vec<String>,
    #[arg(long, default_value_t = 8)]
    limit: usize,
}

#[derive(Debug, Args)]
struct InstructionsSetArgs {
    text: Vec<String>,
}

#[derive(Debug, Args)]
struct MeetingStartArgs {
    #[arg(long)]
    title: Option<String>,
}

#[derive(Debug, Args)]
struct ListenArgs {
    #[arg(long, value_enum, default_value_t = SpeakerArg::System)]
    speaker: SpeakerArg,
    #[arg(long, default_value_t = true)]
    final_segment: bool,
    text: Vec<String>,
}

#[derive(Debug, Args)]
struct AskArgs {
    /// Provider route to request. Remote providers require their env key and endpoint.
    #[arg(long)]
    provider: Option<String>,
    /// Model id to request for the selected provider route.
    #[arg(long)]
    model: Option<String>,
    /// Ask the daemon to include answer delta events in the IPC response.
    #[arg(long)]
    stream: bool,
    /// Print provider/runtime metadata after the answer.
    #[arg(long)]
    metadata: bool,
    question: Vec<String>,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum OverlayPositionArg {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Center,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum SpeakerArg {
    System,
    User,
    Other,
    Unknown,
}

impl From<SpeakerArg> for Speaker {
    fn from(value: SpeakerArg) -> Self {
        match value {
            SpeakerArg::System => Self::System,
            SpeakerArg::User => Self::User,
            SpeakerArg::Other => Self::Other,
            SpeakerArg::Unknown => Self::Unknown,
        }
    }
}

impl From<OverlayPositionArg> for OverlayPosition {
    fn from(value: OverlayPositionArg) -> Self {
        match value {
            OverlayPositionArg::TopLeft => Self::TopLeft,
            OverlayPositionArg::TopRight => Self::TopRight,
            OverlayPositionArg::BottomLeft => Self::BottomLeft,
            OverlayPositionArg::BottomRight => Self::BottomRight,
            OverlayPositionArg::Center => Self::Center,
        }
    }
}

#[derive(Debug, Args)]
struct DevCardArgs {
    #[arg(long, default_value = "Bluey is awake")]
    title: String,
    #[arg(
        long,
        default_value = "Overlay protocol is connected and ready for meeting intelligence."
    )]
    body: String,
}

#[tokio::main]
pub async fn cli_main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run(args) => run(args).await,
        Commands::On(args) => cue_on(args).await,
        Commands::Off => cue_off().await,
        Commands::Login(args) => cue_login(args).await,
        Commands::Account => print_account().await,
        Commands::Sessions(args) => print_sessions(args),
        Commands::Settings(args) => cue_settings(args),
        Commands::Usage => bluey_usage_cmd().await,
        Commands::Credits => bluey_credits_cmd().await,
        Commands::Update(args) => {
            crate::update::manual_update(args.check_only, args.yes, args.force).await
        }
        Commands::Logout => bluey_logout_cmd().await,
        Commands::Portal => bluey_portal_cmd().await,
        Commands::Export => bluey_export_cmd().await,
        Commands::DeleteAccount { force } => bluey_delete_account_cmd(force).await,
        Commands::Doctor { json } => {
            if json {
                crate::doctor::run_json()?;
            } else {
                crate::doctor::run()?;
            }
            Ok(())
        }
        Commands::Logs { command } => match command {
            LogsCommands::Export {
                no_redact,
                days,
                output,
            } => {
                crate::logs::export(crate::logs::LogsExportArgs {
                    redact: !no_redact,
                    days,
                    output,
                })?;
                Ok(())
            }
        },
        Commands::Support {
            no_redact,
            days,
            output,
        } => {
            crate::support::bundle(crate::support::SupportArgs {
                redact: !no_redact,
                days,
                output,
            })?;
            Ok(())
        }
        Commands::Start(args) => start(args).await,
        Commands::Stop => {
            let response = request(DaemonRequest::Shutdown).await?;
            print_response(response)
        }
        Commands::Status => match request(DaemonRequest::Status).await {
            Ok(response) => print_response(response),
            Err(error) => {
                let paths = AppPaths::discover()?;
                if paths.state_file.exists() {
                    let state = std::fs::read_to_string(&paths.state_file)?;
                    println!("{state}");
                    println!("daemon IPC is not reachable: {error}");
                    Ok(())
                } else {
                    bail!("Bluey daemon is not running");
                }
            }
        },
        Commands::Overlay { command } => {
            let request_msg = match command {
                OverlayCommands::Show => DaemonRequest::OverlayShow,
                OverlayCommands::Hide => DaemonRequest::OverlayHide,
                OverlayCommands::Toggle => DaemonRequest::OverlayToggle,
                OverlayCommands::Clear => DaemonRequest::OverlayClear,
                OverlayCommands::Opacity { value } => DaemonRequest::OverlaySetOpacity {
                    opacity: normalize_opacity(value),
                },
                OverlayCommands::Position { position } => DaemonRequest::OverlaySetPosition {
                    position: position.into(),
                },
            };
            let response = request(request_msg).await?;
            print_response(response)
        }
        Commands::Meeting { command } => {
            let request_msg = match command {
                MeetingCommands::Start(args) => DaemonRequest::MeetingStart { title: args.title },
                MeetingCommands::End => DaemonRequest::MeetingEnd,
            };
            let response = request(request_msg).await?;
            print_response(response)
        }
        Commands::Listen(args) => {
            let text = args.text.join(" ");
            if text.trim().is_empty() {
                bail!("provide transcript text, for example: bluey listen \"What is the status?\"");
            }
            let response = request(DaemonRequest::TranscriptAdd {
                speaker: args.speaker.into(),
                text,
                is_final: args.final_segment,
            })
            .await?;
            print_response(response)
        }
        Commands::Ask(args) => {
            let question = args.question.join(" ");
            if question.trim().is_empty() {
                bail!("provide a question, for example: bluey ask \"what are the action items?\"");
            }
            let request_msg = DaemonRequest::Answer {
                request: answer_request_from_args(question, &args),
            };
            match request(request_msg).await? {
                DaemonResponse::Answer { response, events } => {
                    print_answer_response(response, args.metadata, events.len());
                    Ok(())
                }
                response => print_response(response),
            }
        }
        Commands::Recap => {
            let response = request(DaemonRequest::Recap).await?;
            print_response(response)
        }
        Commands::ActionItems => {
            let response = request(DaemonRequest::ActionItems).await?;
            print_response(response)
        }
        Commands::Context { command } => match command {
            ContextCommands::Add(args) => {
                let response = request(DaemonRequest::ContextAdd {
                    path: args.path.display().to_string(),
                    title: args.title,
                    note: args.note,
                })
                .await?;
                print_response(response)
            }
            ContextCommands::Capture(args) => capture_context(args).await,
            ContextCommands::Page => {
                let response = request(DaemonRequest::ActivePageCapture).await?;
                print_response(response)
            }
            ContextCommands::List => {
                let response = request(DaemonRequest::ContextList).await?;
                print_response(response)
            }
            ContextCommands::Watch { command } => {
                let request_msg = match command {
                    ContextWatchCommands::Start(args) => DaemonRequest::ScreenCaptureStart {
                        interval_secs: Some(args.interval),
                    },
                    ContextWatchCommands::Stop => DaemonRequest::ScreenCaptureStop,
                };
                let response = request(request_msg).await?;
                print_response(response)
            }
        },
        Commands::Instructions { command } => {
            let request_msg = match command {
                InstructionsCommands::Set(args) => {
                    let text = args.text.join(" ");
                    if text.trim().is_empty() {
                        bail!("provide instructions, for example: bluey instructions set \"answer briefly\"");
                    }
                    DaemonRequest::InstructionsSet { text }
                }
                InstructionsCommands::Show => DaemonRequest::InstructionsGet,
                InstructionsCommands::Clear => DaemonRequest::InstructionsClear,
            };
            let response = request(request_msg).await?;
            print_response(response)
        }
        Commands::Memory { command } => match command {
            MemoryCommands::Search(args) => {
                let query = args.query.join(" ");
                if query.trim().is_empty() {
                    bail!("provide a memory query, for example: bluey memory search architecture risk");
                }
                let response = request(DaemonRequest::MemorySearch {
                    query,
                    limit: args.limit,
                })
                .await?;
                print_response(response)
            }
        },
        Commands::Audio { command } => {
            let request_msg = match command {
                AudioCommands::Status => DaemonRequest::AudioStatus,
                AudioCommands::Start(args) => DaemonRequest::AudioStart {
                    enable_system: !args.no_system,
                    enable_microphone: !args.no_microphone,
                    mic_device_id: None,
                },
                AudioCommands::Stop => DaemonRequest::AudioStop,
            };
            let response = request(request_msg).await?;
            print_response(response)
        }
        Commands::Ai { command } => {
            let request_msg = match command {
                AiCommands::Status => DaemonRequest::AiStatus,
            };
            let response = request(request_msg).await?;
            print_response(response)
        }
        Commands::Cloud { command } => match command {
            CloudCommands::Status => print_response(request(DaemonRequest::CloudStatus).await?),
            CloudCommands::Sync => print_response(request(DaemonRequest::CloudSyncNow).await?),
            CloudCommands::Sessions { limit, json } => print_cloud_sessions(limit, json).await,
            CloudCommands::Show { id, json } => print_cloud_session(&id, json).await,
            CloudCommands::Rag { query, limit, json } => {
                print_cloud_rag(&query.join(" "), limit, json).await
            }
        },
        Commands::Providers => {
            print_provider_status();
            Ok(())
        }
        Commands::DevCard(args) => {
            let card =
                CueCard::new(CardKind::System, args.title, args.body).with_source("bluey dev-card");
            let response = request(DaemonRequest::PushCard { card }).await?;
            print_response(response)
        }
    }
}

async fn run(args: RunArgs) -> Result<()> {
    ensure_daemon(args.no_overlay).await?;

    match request(DaemonRequest::MeetingStart { title: args.title }).await? {
        DaemonResponse::Text { text } if text.contains("already active") => {
            println!("Using active meeting.");
        }
        response => print_response(response)?,
    }

    let _ = request(DaemonRequest::OverlayShow).await;
    print_live_help();
    live_loop(args.speaker.into()).await
}

async fn cue_on(args: OnArgs) -> Result<()> {
    crate::update::maybe_update_before_on(args.title.as_deref()).await?;
    ensure_bluey_on_permissions_ready().await?;

    let paths = AppPaths::discover()?;
    let settings = load_settings(&paths)?;
    ensure_daemon_quiet(false).await?;

    // Product flow: every `bluey on` starts a fresh recording. Existing active
    // sessions are archived first; users can restore the latest recording from
    // the overlay's `Latest` control.
    let _ = request(DaemonRequest::MeetingEnd).await;
    match request(DaemonRequest::MeetingStart { title: args.title }).await? {
        DaemonResponse::Text { text } if text.contains("already active") => {}
        DaemonResponse::Text { .. } | DaemonResponse::Recap { .. } | DaemonResponse::Ok => {}
        DaemonResponse::Error { message } => bail!("daemon error: {message}"),
        other => {
            print_response(other)?;
        }
    }

    if let Some(answer_style) = settings
        .answer_style
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        let _ = request(DaemonRequest::InstructionsSet {
            text: answer_style.clone(),
        })
        .await;
    }
    let _ = request(DaemonRequest::OverlaySetOpacity {
        opacity: settings.overlay_opacity,
    })
    .await;
    let account_ready = bluey_account_linked(&paths);
    let auth_state = if account_ready {
        BlueyOnAuthState::Ready
    } else {
        BlueyOnAuthState::SignInAvailable {
            url: bluey_signin_url(),
        }
    };
    // The native overlay orders the branded pill front when the child process
    // starts. Do not send OverlayShow here: in the current protocol it expands
    // the full feed, while `bluey on` should launch pill-first.
    let boot = request(DaemonRequest::OverlayBoot {
        title: bluey_on_boot_title(&auth_state).to_string(),
        lines: bluey_on_boot_lines(&auth_state),
    })
    .await;
    let should_start_login = matches!(auth_state, BlueyOnAuthState::SignInAvailable { .. });

    match boot {
        Ok(DaemonResponse::Ok) => {
            if should_start_login {
                match request(DaemonRequest::CloudLogin).await {
                    Ok(DaemonResponse::Text { text }) => println!("{text}"),
                    Ok(DaemonResponse::Ok) => println!("Opening Bluey sign-in in your browser."),
                    Ok(DaemonResponse::Error { message }) => {
                        println!("Open sign-in manually with `bluey login`: {message}");
                    }
                    Ok(other) => {
                        let _ = print_response(other);
                    }
                    Err(error) => {
                        println!("Open sign-in manually with `bluey login`: {error:#}");
                    }
                }
            }
            match auth_state {
                BlueyOnAuthState::Ready => println!("Bluey is on."),
                BlueyOnAuthState::SignInAvailable { url } => {
                    println!(
                        "Bluey is on. Finish sign-in in the browser, or click the pill to retry."
                    );
                    println!("Login: {url}");
                }
            }
            Ok(())
        }
        Ok(other) => print_response(other),
        Err(error) => {
            println!("Bluey is on, but the overlay is not reachable yet: {error:#}");
            Ok(())
        }
    }
}

async fn ensure_bluey_on_permissions_ready() -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        ensure_macos_bluey_on_permissions_ready().await
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(())
    }
}

#[cfg(target_os = "macos")]
async fn ensure_macos_bluey_on_permissions_ready() -> Result<()> {
    if env_present("BLUEY_SKIP_PERMISSION_PREFLIGHT") {
        return Ok(());
    }

    let mut missing = current_macos_permission_checks()
        .into_iter()
        .filter(|check| !macos_permission_ready(check.status))
        .collect::<Vec<_>>();
    if missing.is_empty() {
        return Ok(());
    }

    println!("Bluey needs a few macOS permissions before the pill opens.");
    println!("Approve the missing items in System Settings; Bluey will continue automatically.");
    println!("Press Ctrl-C to stop waiting.\n");
    print_macos_permission_status(&missing);
    request_macos_permission_prompts(&missing);
    open_macos_permission_panes(&missing);

    let deadline = Instant::now() + Duration::from_secs(120);
    let mut last_print = Instant::now();
    loop {
        sleep(Duration::from_secs(2)).await;
        missing = current_macos_permission_checks()
            .into_iter()
            .filter(|check| !macos_permission_ready(check.status))
            .collect();

        if missing.is_empty() {
            println!("\nAll required macOS permissions are granted. Starting Bluey...");
            return Ok(());
        }

        if Instant::now() >= deadline {
            print_macos_permission_status(&missing);
            bail!(
                "Bluey is waiting on macOS permission approval. Enable the missing item(s), then run `bluey on` again."
            );
        }

        if Instant::now().duration_since(last_print) >= Duration::from_secs(10) {
            print_macos_permission_status(&missing);
            last_print = Instant::now();
        }
    }
}

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy)]
struct MacPermissionCheck {
    name: &'static str,
    settings_section: &'static str,
    status: crate::macos_perms::PermissionStatus,
}

#[cfg(target_os = "macos")]
fn current_macos_permission_checks() -> Vec<MacPermissionCheck> {
    use crate::macos_perms::{accessibility_status, microphone_status, screen_recording_status};

    vec![
        MacPermissionCheck {
            name: "Accessibility",
            settings_section: "Privacy_Accessibility",
            status: accessibility_status(),
        },
        MacPermissionCheck {
            name: "Microphone",
            settings_section: "Privacy_Microphone",
            status: microphone_status(),
        },
        MacPermissionCheck {
            name: "Screen Recording",
            settings_section: "Privacy_ScreenCapture",
            status: screen_recording_status(),
        },
    ]
}

#[cfg(any(target_os = "macos", test))]
fn macos_permission_ready(status: crate::macos_perms::PermissionStatus) -> bool {
    use crate::macos_perms::PermissionStatus;
    matches!(
        status,
        PermissionStatus::Granted | PermissionStatus::NotApplicable | PermissionStatus::Unknown
    )
}

#[cfg(target_os = "macos")]
fn print_macos_permission_status(missing: &[MacPermissionCheck]) {
    println!("Missing macOS permission(s):");
    for check in missing {
        println!("  - {}: {}", check.name, check.status.label());
        if let Some(hint) = check.status.hint(check.name) {
            println!("    {hint}");
        }
    }
}

#[cfg(target_os = "macos")]
fn request_macos_permission_prompts(missing: &[MacPermissionCheck]) {
    use crate::macos_perms::PermissionStatus;

    let needs_microphone = missing
        .iter()
        .any(|check| check.name == "Microphone" && check.status == PermissionStatus::NotDetermined);
    let needs_screen_recording = missing.iter().any(|check| {
        check.name == "Screen Recording" && check.status == PermissionStatus::NotDetermined
    });

    if !needs_microphone && !needs_screen_recording {
        return;
    }

    let Some(helper) = find_macos_audio_permission_helper() else {
        println!(
            "Could not find Bluey's audio helper to trigger permission prompts; opening System Settings instead."
        );
        return;
    };

    if needs_microphone {
        println!("Requesting Microphone permission prompt...");
        run_macos_audio_permission_probe(&helper, "microphone");
    }
    if needs_screen_recording {
        println!("Requesting System Audio / Screen Recording permission prompt...");
        run_macos_audio_permission_probe(&helper, "system");
    }
}

#[cfg(target_os = "macos")]
fn find_macos_audio_permission_helper() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(exe) = env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join("bluey-audio-macos"));
            candidates.push(parent.join("cue-audio-macos"));
        }
    }
    if let Some(home) = env::var_os("HOME") {
        let bluey_bin = PathBuf::from(home).join(".bluey/bin");
        candidates.push(bluey_bin.join("bluey-audio-macos"));
        candidates.push(bluey_bin.join("cue-audio-macos"));
    }
    candidates.push(PathBuf::from("./bluey-audio-macos"));
    candidates.push(PathBuf::from("./cue-audio-macos"));

    candidates.into_iter().find(|path| is_executable_file(path))
}

#[cfg(target_os = "macos")]
fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

#[cfg(target_os = "macos")]
fn run_macos_audio_permission_probe(helper: &Path, source: &str) {
    let child = Command::new(helper)
        .arg("--source")
        .arg(source)
        .arg("--duration-ms")
        .arg("250")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();

    let Ok(mut child) = child else {
        eprintln!(
            "Could not start {} for {} permission prompt.",
            helper.display(),
            source
        );
        return;
    };

    let deadline = StdInstant::now() + std::time::Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if StdInstant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
            Err(_) => break,
        }
    }
}

#[cfg(target_os = "macos")]
fn open_macos_permission_panes(missing: &[MacPermissionCheck]) {
    for check in missing {
        let uri = macos_permission_settings_uri(check.settings_section);
        if let Err(error) = Command::new("open").arg(uri).status() {
            eprintln!(
                "Could not open System Settings for {} automatically: {error}",
                check.name
            );
        }
    }
}

#[cfg(target_os = "macos")]
fn macos_permission_settings_uri(section: &str) -> String {
    format!("x-apple.systempreferences:com.apple.preference.security?{section}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum BlueyOnAuthState {
    Ready,
    SignInAvailable { url: String },
}

fn bluey_account_linked(paths: &AppPaths) -> bool {
    cue_cloud_client::tokens::tokens_available(paths)
        || (legacy_keyring_fallback_enabled()
            && keyring_has_tokens_with_timeout(std::time::Duration::from_secs(1))
                .ok()
                .flatten()
                .unwrap_or(false))
}

fn bluey_signin_url() -> String {
    env::var("BLUEY_SIGNIN_URL").unwrap_or_else(|_| default_bluey_signin_url().to_string())
}

fn default_bluey_signin_url() -> &'static str {
    "https://bluey.sh/login"
}

fn bluey_on_boot_lines(auth_state: &BlueyOnAuthState) -> Vec<String> {
    match auth_state {
        BlueyOnAuthState::Ready => vec![
            "new recording ready".to_string(),
            "listen, attach docs, analyse screen, or ask from the composer".to_string(),
            "previous sessions live in the sidebar".to_string(),
            "answers stream into chat; canvas opens when useful".to_string(),
        ],
        BlueyOnAuthState::SignInAvailable { url } => vec![
            "local recording is ready".to_string(),
            "browser sign-in opens automatically when this desktop is not linked".to_string(),
            "click Sign in from the Bluey window if you need to retry cloud answers, balance, sync, or knowledge base".to_string(),
            format!("login_url: {url}"),
        ],
    }
}

fn bluey_on_boot_title(auth_state: &BlueyOnAuthState) -> &'static str {
    match auth_state {
        BlueyOnAuthState::Ready => "Bluey online",
        BlueyOnAuthState::SignInAvailable { .. } => "Sign in to Bluey",
    }
}

async fn cue_off() -> Result<()> {
    match request(DaemonRequest::Shutdown).await {
        Ok(DaemonResponse::Ok) => {
            if let Err(error) = wait_for_daemon_stopped(Duration::from_secs(8)).await {
                eprintln!("warning: Bluey shutdown is still settling: {error:#}");
            }
            println!("Bluey is off.");
            Ok(())
        }
        Ok(response) => print_response(response),
        Err(error) => {
            let paths = AppPaths::discover()?;
            cleanup_stale_daemon(&paths, true)
                .await
                .with_context(|| format!("Bluey daemon was not reachable ({error:#})"))?;
            println!("Bluey is off.");
            Ok(())
        }
    }
}

async fn cue_login(args: LoginArgs) -> Result<()> {
    let paths = AppPaths::discover()?;
    paths.ensure()?;

    let api_url = resolve_login_api_url(args.local, args.api_url.clone());

    let env_token = env::var("BLUEY_CLOUD_TOKEN")
        .ok()
        .or_else(|| env::var("BLUEY_CLOUD_API_TOKEN").ok())
        .or_else(|| env::var("BLUEY_API_TOKEN").ok())
        .or_else(|| env::var("CUE_CLOUD_TOKEN").ok())
        .or_else(|| env::var("CUE_API_TOKEN").ok());
    let token = args.token.or(env_token);
    let refresh_token = args
        .refresh_token
        .or_else(|| env::var("BLUEY_CLOUD_REFRESH_TOKEN").ok())
        .or_else(|| env::var("CUE_CLOUD_REFRESH_TOKEN").ok());

    let account = if args.local || args.no_browser || token.is_some() {
        let mut account = AccountConfig::local();
        account.provider = login_account_provider(args.local).to_string();
        account.api_url = api_url;
        account.user_id = args.user;
        account.workspace_id = args.workspace;
        account.access_token = token;
        account.refresh_token = refresh_token;
        account
    } else {
        browser_login(&api_url, args.user, args.workspace).await?
    };

    let has_cloud_tokens = account.token_configured();
    cue_cloud_client::save_account_profile_and_tokens(&paths, &account)?;
    if has_cloud_tokens {
        let mut settings = load_settings(&paths)?;
        if !settings.cloud_sync_enabled {
            settings.cloud_sync_enabled = true;
            settings.touch();
            save_settings(&paths, &settings)?;
        }
        let _ = request(DaemonRequest::CloudStatus).await;
    }

    println!(
        "Bluey account linked: {} ({})",
        account.provider, account.api_url
    );
    if has_cloud_tokens {
        println!("Cloud token saved in Bluey's private local account profile.");
        println!("Saved sessions sync automatically in the background while Bluey is on.");
    } else {
        println!("Local account linked. Run `bluey on` later to sign in when the Bluey cloud endpoint is ready.");
    }
    Ok(())
}

async fn print_account() -> Result<()> {
    let paths = AppPaths::discover()?;
    let account = load_account(&paths)?;
    let has_stored_tokens = cue_cloud_client::tokens::tokens_available(&paths);
    let cloud = request(DaemonRequest::CloudStatus).await.ok();

    match account {
        Some(account) => {
            println!("Account: {}", account.provider);
            println!("API: {}", account.api_url);
            println!("Workspace: {}", account.workspace_id);
            println!("User: {}", account.user_id);
            println!("Device: {}", account.device_id);
            println!(
                "Token: {}",
                if has_stored_tokens {
                    "configured"
                } else {
                    "not configured"
                }
            );
        }
        None => {
            println!("Account: local");
            println!("Run `bluey on` to sign in when you are ready.");
        }
    }

    if let Some(DaemonResponse::CloudStatus { status }) = cloud {
        println!("Cloud sync: {:?}", status.sync_state);
        if let Some(error) = status.last_error {
            println!("Cloud note: {error}");
        }
    }

    Ok(())
}

fn print_sessions(args: SessionsArgs) -> Result<()> {
    let meetings = load_local_meetings()?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&meetings)?);
        return Ok(());
    }

    if let Some(id_prefix) = args.show {
        let Some(meeting) = meetings
            .iter()
            .find(|meeting| meeting.id.to_string().starts_with(&id_prefix))
        else {
            bail!("no saved session matches id prefix `{id_prefix}`");
        };
        print_meeting_detail(meeting);
        return Ok(());
    }

    if meetings.is_empty() {
        println!("No saved sessions yet. Run `bluey on` to start one.");
        return Ok(());
    }

    println!("Saved sessions:");
    for meeting in meetings.iter().take(args.limit.max(1)) {
        let active = if meeting.ended_at.is_none() {
            "active"
        } else {
            "saved"
        };
        println!(
            "- {}  {}  {} segment(s), {} context, {} action(s), {} decision(s)",
            &meeting.id.to_string()[..8],
            active,
            meeting.transcript.len(),
            meeting.context.len(),
            meeting.action_items.len(),
            meeting.decisions.len()
        );
        println!("  {}", meeting.title);
    }
    Ok(())
}

fn cue_settings(args: SettingsArgs) -> Result<()> {
    let paths = AppPaths::discover()?;
    let mut settings = if args.reset {
        CueSettings::default()
    } else {
        load_settings(&paths)?
    };

    if let Some(answer_style) = args.answer_style {
        settings.answer_style = if answer_style.trim().is_empty() {
            None
        } else {
            Some(answer_style)
        };
    }
    if let Some(model) = args.model {
        settings.default_model = model;
    }
    if let Some(mode) = args.mode {
        settings.default_mode = mode;
    }
    if let Some(opacity) = args.opacity {
        settings.overlay_opacity = normalize_opacity(opacity);
    }
    if let Some(cloud_sync) = args.cloud_sync {
        settings.cloud_sync_enabled = cloud_sync;
    }
    if let Some(days) = args.retention_days {
        settings.retention_days = days;
    }
    settings.touch();

    save_settings(&paths, &settings)?;
    print_settings(&settings);
    Ok(())
}

async fn browser_login(
    api_url: &str,
    user_id: String,
    workspace_id: String,
) -> Result<AccountConfig> {
    let config = cue_cloud_client::client::ClientConfig {
        base_url: api_url.to_string(),
        trace_id: Some(command_trace_id()),
        ..Default::default()
    };
    let client = cue_cloud_client::CloudClient::new(
        config,
        Arc::new(cue_cloud_client::tokens::MemoryStore::new()),
    )
    .context("failed to initialize Bluey cloud client")?;
    let flow = cue_cloud_client::DeviceFlow::start(&client)
        .await
        .context("failed to start browser login")?;
    let login_url = device_login_url(&flow.verification_uri, &flow.user_code);

    println!("Opening Bluey sign-in...");
    println!("Code: {}", flow.user_code);
    println!("Approve this code only in the Bluey account you want this desktop to use.");
    println!("{login_url}");
    let _ = open_browser(&login_url);
    println!("Waiting for browser approval...");
    println!("After signing in, click Connect desktop on the Bluey page. This terminal will finish automatically.");

    let auth = tokio::time::timeout(Duration::from_secs(DEVICE_LOGIN_TIMEOUT_SECS), async {
        await_browser_device_login(&flow, &client)
            .await
            .map_err(anyhow::Error::from)
    })
    .await
    .context("login timed out after 10 minutes. Re-run `bluey login`, then click Connect desktop in the browser.")??;

    let mut account = AccountConfig::local();
    account.provider = "bluey".to_string();
    account.api_url = api_url.to_string();
    account.user_id = if user_id == "local-user" {
        auth.account.email.clone()
    } else {
        user_id
    };
    account.workspace_id = workspace_id;
    account.access_token = Some(auth.access_token);
    account.refresh_token = Some(auth.refresh_token);
    Ok(account)
}

const DEVICE_LOGIN_TIMEOUT_SECS: u64 = 600;

async fn await_browser_device_login(
    flow: &cue_cloud_client::DeviceFlow,
    client: &cue_cloud_client::CloudClient,
) -> cue_cloud_client::error::Result<cue_cloud_client::AuthResponse> {
    let interval = Duration::from_secs(flow.interval_secs.max(1));
    let mut next_hint = Instant::now() + Duration::from_secs(20);
    loop {
        match flow.poll(client).await? {
            cue_cloud_client::DeviceFlowState::LoggedIn(auth) => return Ok(auth),
            cue_cloud_client::DeviceFlowState::Pending => {
                if Instant::now() >= next_hint {
                    println!("Still waiting. In the browser, click Connect desktop to approve this terminal.");
                    next_hint += Duration::from_secs(20);
                }
                sleep(interval).await;
            }
            cue_cloud_client::DeviceFlowState::Expired => {
                return Err(cue_cloud_client::Error::Other("device_code expired".into()));
            }
        }
    }
}

fn resolve_login_api_url(local: bool, explicit_api_url: Option<String>) -> String {
    resolve_login_api_url_from(
        local,
        explicit_api_url,
        env::var("BLUEY_CLOUD_API_URL").ok(),
        env::var("CUE_CLOUD_API_URL").ok(),
    )
}

fn resolve_login_api_url_from(
    local: bool,
    explicit_api_url: Option<String>,
    bluey_api_url: Option<String>,
    cue_api_url: Option<String>,
) -> String {
    if local && explicit_api_url.is_none() {
        return "http://127.0.0.1:8787".to_string();
    }
    explicit_api_url
        .or(bluey_api_url)
        .or(cue_api_url)
        .unwrap_or_else(|| "https://bluey.sh".to_string())
}

fn login_account_provider(local: bool) -> &'static str {
    if local {
        "local"
    } else {
        "bluey"
    }
}

fn legacy_keyring_fallback_enabled() -> bool {
    truthy_env("BLUEY_LEGACY_KEYRING_FALLBACK")
}

fn truthy_env(name: &str) -> bool {
    env::var(name)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn keyring_has_tokens_with_timeout(timeout: std::time::Duration) -> Result<Option<bool>> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = (|| -> Result<bool> {
            let client = cue_cloud_client::CloudClient::with_default_keyring()
                .context("failed to open keyring token store")?;
            Ok(client.current_tokens().is_some())
        })();
        let _ = tx.send(result);
    });
    match rx.recv_timeout(timeout) {
        Ok(result) => result.map(Some),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Ok(None),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            Err(anyhow!("keyring token check task ended without returning"))
        }
    }
}

fn device_login_url(verification_uri: &str, user_code: &str) -> String {
    let base = verification_uri.trim_end_matches('/');
    let separator = if base.contains('?') { '&' } else { '?' };
    format!("{base}{separator}user_code={user_code}")
}

fn load_local_meetings() -> Result<Vec<MeetingRecord>> {
    let paths = AppPaths::discover()?;
    let mut meetings: Vec<MeetingRecord> = Vec::new();

    let active_path = paths.data_dir.join("active-meeting.json");
    if let Ok(bytes) = std::fs::read(&active_path) {
        meetings.push(
            serde_json::from_slice(&bytes)
                .with_context(|| format!("failed to parse {}", active_path.display()))?,
        );
    }

    let archive_dir = paths.data_dir.join("meetings");
    if archive_dir.exists() {
        for entry in std::fs::read_dir(&archive_dir)
            .with_context(|| format!("failed to read {}", archive_dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let bytes = std::fs::read(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            meetings.push(
                serde_json::from_slice(&bytes)
                    .with_context(|| format!("failed to parse {}", path.display()))?,
            );
        }
    }

    meetings.sort_by(|left, right| right.started_at.cmp(&left.started_at));
    Ok(meetings)
}

fn print_meeting_detail(meeting: &MeetingRecord) {
    println!("Session: {}", meeting.title);
    println!("ID: {}", meeting.id);
    println!("Started: {}", meeting.started_at);
    if let Some(ended_at) = &meeting.ended_at {
        println!("Ended: {ended_at}");
    } else {
        println!("Status: active");
    }
    println!("Transcript segments: {}", meeting.transcript.len());
    println!("Context items: {}", meeting.context.len());
    println!("Action items: {}", meeting.action_items.len());
    println!("Decisions: {}", meeting.decisions.len());

    if let Some(summary) = &meeting.summary {
        println!("\nSummary:\n{summary}");
    }
    if !meeting.action_items.is_empty() {
        println!("\nAction items:");
        for item in &meeting.action_items {
            println!("- {}", item.text);
        }
    }
    if !meeting.decisions.is_empty() {
        println!("\nDecisions:");
        for decision in &meeting.decisions {
            println!("- {}", decision.text);
        }
    }
    if !meeting.context.is_empty() {
        println!("\nContext:");
        for item in &meeting.context {
            println!("- {} ({})", item.title, item.kind);
        }
    }
}

fn print_settings(settings: &CueSettings) {
    println!("Bluey settings:");
    println!("Model: {}", settings.default_model);
    println!("Mode: {}", settings.default_mode);
    println!(
        "Answer style: {}",
        settings.answer_style.as_deref().unwrap_or("default")
    );
    println!("Opacity: {}%", (settings.overlay_opacity * 100.0).round());
    println!("System audio: {}", on_off(settings.audio_system_enabled));
    println!("Microphone: {}", on_off(settings.audio_microphone_enabled));
    println!(
        "Cloud sync: {}",
        if settings.cloud_sync_enabled {
            "automatic"
        } else {
            "off"
        }
    );
    println!("Retention: {} day(s)", settings.retention_days);
}

fn on_off(value: bool) -> &'static str {
    if value {
        "on"
    } else {
        "off"
    }
}

fn open_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("cmd");
        command.arg("/C").arg("start").arg("");
        command
    };
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let mut command = Command::new("xdg-open");

    let status = command
        .arg(url)
        .status()
        .context("failed to open browser")?;
    if !status.success() {
        bail!("browser opener exited with status {status}");
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn powershell_single_quoted(path: &PathBuf) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "''"))
}

async fn ensure_daemon(no_overlay: bool) -> Result<()> {
    ensure_daemon_with_output(no_overlay, false).await
}

async fn ensure_daemon_quiet(no_overlay: bool) -> Result<()> {
    ensure_daemon_with_output(no_overlay, true).await
}

async fn ensure_daemon_with_output(no_overlay: bool, quiet: bool) -> Result<()> {
    if request(DaemonRequest::Ping).await.is_ok() {
        return Ok(());
    }

    let paths = AppPaths::discover()?;
    cleanup_stale_daemon(&paths, quiet).await?;

    start(StartArgs {
        foreground: false,
        no_overlay,
        quiet,
    })
    .await
}

async fn live_loop(default_speaker: Speaker) -> Result<()> {
    let stdin = io::stdin();

    loop {
        print!("bluey> ");
        io::stdout().flush()?;

        let mut line = String::new();
        if stdin.read_line(&mut line)? == 0 {
            println!();
            println!("Live session closed. Bluey daemon is still running.");
            return Ok(());
        }

        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(command) = LiveCommand::parse(line) {
            if !handle_live_command(command).await? {
                return Ok(());
            }
            continue;
        }

        if let Some(question) = line.strip_prefix('?') {
            ask_live(question.trim()).await?;
            continue;
        }

        let (speaker, text) = parse_transcript_line(line, default_speaker);
        if text.is_empty() {
            continue;
        }
        let response = request(DaemonRequest::TranscriptAdd {
            speaker,
            text,
            is_final: true,
        })
        .await?;
        match response {
            DaemonResponse::Text { .. } => println!("captured"),
            other => print_response(other)?,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum LiveCommand<'a> {
    Actions,
    Ai,
    Ask(&'a str),
    Attach(&'a str),
    Audio,
    Capture,
    CaptureOff,
    CaptureOn(u64),
    Clear,
    Context,
    Cloud,
    End,
    Help,
    Hide,
    Instructions(&'a str),
    Opacity(f32),
    Position(OverlayPosition),
    Privacy,
    Quit,
    Recap,
    Show,
    Status,
    Stop,
}

impl<'a> LiveCommand<'a> {
    fn parse(line: &'a str) -> Option<Self> {
        let lower = line.to_ascii_lowercase();
        if let Some(raw) = lower.strip_prefix("/opacity ") {
            return raw
                .trim()
                .parse::<f32>()
                .ok()
                .map(normalize_opacity)
                .map(Self::Opacity);
        }
        if let Some(raw) = lower.strip_prefix("/position ") {
            return parse_overlay_position(raw.trim()).map(Self::Position);
        }
        if let Some(raw) = line.strip_prefix("/attach ") {
            return Some(Self::Attach(raw.trim()));
        }
        if let Some(raw) = line.strip_prefix("/style ") {
            return Some(Self::Instructions(raw.trim()));
        }
        if let Some(raw) = line.strip_prefix("/instructions ") {
            return Some(Self::Instructions(raw.trim()));
        }
        if let Some(raw) = lower.strip_prefix("/capture-on") {
            let interval = raw.trim().parse::<u64>().ok().unwrap_or(12).clamp(3, 300);
            return Some(Self::CaptureOn(interval));
        }

        match lower.as_str() {
            "/actions" | "/action-items" => Some(Self::Actions),
            "/ai" | "/ai-status" => Some(Self::Ai),
            "/audio" | "/audio-status" => Some(Self::Audio),
            "/capture-off" | "/stop-capture" => Some(Self::CaptureOff),
            "/capture" | "/screenshot" => Some(Self::Capture),
            "/clear" => Some(Self::Clear),
            "/context" | "/attachments" => Some(Self::Context),
            "/cloud" | "/cloud-status" => Some(Self::Cloud),
            "/end" => Some(Self::End),
            "/help" | "help" => Some(Self::Help),
            "/hide" => Some(Self::Hide),
            "/privacy" | "/capture-status" | "/screen-capture-status" => Some(Self::Privacy),
            "/quit" | "/exit" => Some(Self::Quit),
            "/recap" => Some(Self::Recap),
            "/show" => Some(Self::Show),
            "/status" => Some(Self::Status),
            "/stop" => Some(Self::Stop),
            _ => line
                .strip_prefix("/ask ")
                .map(|question| Self::Ask(question.trim())),
        }
    }
}

async fn handle_live_command(command: LiveCommand<'_>) -> Result<bool> {
    match command {
        LiveCommand::Actions => {
            print_response(request(DaemonRequest::ActionItems).await?)?;
        }
        LiveCommand::Ai => {
            print_response(request(DaemonRequest::AiStatus).await?)?;
        }
        LiveCommand::Ask(question) => ask_live(question).await?,
        LiveCommand::Attach(path) => {
            let items = add_context(path, None, None).await?;
            print_context_items(items);
        }
        LiveCommand::Audio => {
            print_response(request(DaemonRequest::AudioStatus).await?)?;
        }
        LiveCommand::Capture => {
            capture_context(ContextCaptureArgs {
                full_screen: false,
                yes: false,
                no_preview: false,
                title: None,
                note: Some("User-approved screenshot capture".to_string()),
            })
            .await?;
        }
        LiveCommand::CaptureOn(interval_secs) => {
            print_response(
                request(DaemonRequest::ScreenCaptureStart {
                    interval_secs: Some(interval_secs),
                })
                .await?,
            )?;
        }
        LiveCommand::CaptureOff => {
            print_response(request(DaemonRequest::ScreenCaptureStop).await?)?;
        }
        LiveCommand::Clear => {
            print_live_overlay_response(request(DaemonRequest::OverlayClear).await?)?;
        }
        LiveCommand::Context => {
            print_response(request(DaemonRequest::ContextList).await?)?;
        }
        LiveCommand::Cloud => {
            print_response(request(DaemonRequest::CloudStatus).await?)?;
        }
        LiveCommand::End => {
            print_response(request(DaemonRequest::MeetingEnd).await?)?;
            println!("Meeting archived. Bluey daemon is still running.");
            return Ok(false);
        }
        LiveCommand::Help => print_live_help(),
        LiveCommand::Hide => {
            print_live_overlay_response(request(DaemonRequest::OverlayHide).await?)?;
        }
        LiveCommand::Instructions(text) => {
            if text.trim().is_empty() {
                print_response(request(DaemonRequest::InstructionsGet).await?)?;
            } else {
                print_response(
                    request(DaemonRequest::InstructionsSet {
                        text: text.to_string(),
                    })
                    .await?,
                )?;
            }
        }
        LiveCommand::Opacity(opacity) => {
            let changed = print_live_overlay_response(
                request(DaemonRequest::OverlaySetOpacity { opacity }).await?,
            )?;
            if changed {
                println!(
                    "Overlay opacity set to {}%.",
                    (opacity * 100.0).round() as i32
                );
            }
        }
        LiveCommand::Position(position) => {
            print_live_overlay_response(
                request(DaemonRequest::OverlaySetPosition { position }).await?,
            )?;
        }
        LiveCommand::Privacy => match request(DaemonRequest::Status).await? {
            DaemonResponse::Status { state } => {
                let privacy = match state.overlay_capture_excluded {
                    Some(true) => "enabled",
                    Some(false) => "not enabled",
                    None => "unknown",
                };
                println!("Overlay capture exclusion: {privacy}.");
            }
            other => print_response(other)?,
        },
        LiveCommand::Quit => {
            println!("Live session closed. Bluey daemon is still running.");
            return Ok(false);
        }
        LiveCommand::Recap => {
            print_response(request(DaemonRequest::Recap).await?)?;
        }
        LiveCommand::Show => {
            print_live_overlay_response(request(DaemonRequest::OverlayShow).await?)?;
        }
        LiveCommand::Status => {
            print_response(request(DaemonRequest::Status).await?)?;
        }
        LiveCommand::Stop => {
            let _ = request(DaemonRequest::MeetingEnd).await;
            print_response(request(DaemonRequest::Shutdown).await?)?;
            return Ok(false);
        }
    }

    Ok(true)
}

fn print_live_overlay_response(response: DaemonResponse) -> Result<bool> {
    match response {
        DaemonResponse::Ok => {
            println!("ok");
            Ok(true)
        }
        DaemonResponse::Error { message } if message.contains("overlay process is not running") => {
            println!("Overlay is not running in this session.");
            Ok(false)
        }
        other => {
            print_response(other)?;
            Ok(true)
        }
    }
}

async fn ask_live(question: &str) -> Result<()> {
    if question.is_empty() {
        bail!("ask needs a question, for example: ? what are the action items?");
    }
    print_response(
        request(DaemonRequest::Ask {
            question: question.to_string(),
        })
        .await?,
    )
}

async fn capture_context(args: ContextCaptureArgs) -> Result<()> {
    ensure_daemon(false).await?;
    let capture_path = capture_screen(&args)?;
    let metadata = std::fs::metadata(&capture_path)
        .with_context(|| format!("failed to read capture {}", capture_path.display()))?;
    if metadata.len() == 0 {
        bail!("capture was empty; nothing was attached");
    }

    if !args.no_preview {
        preview_capture(&capture_path);
    }

    if !args.yes && !confirm_attach(&capture_path)? {
        println!("Capture saved but not attached: {}", capture_path.display());
        return Ok(());
    }

    let title = args.title.or_else(|| {
        capture_path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string)
    });
    let note = args
        .note
        .or_else(|| Some("User-approved screenshot capture".to_string()));
    let items = add_context(&capture_path.display().to_string(), title, note).await?;
    print_context_items(items);
    Ok(())
}

fn capture_screen(args: &ContextCaptureArgs) -> Result<PathBuf> {
    let paths = AppPaths::discover()?;
    paths.ensure()?;
    let capture_dir = paths.data_dir.join("captures");
    std::fs::create_dir_all(&capture_dir)
        .with_context(|| format!("failed to create {}", capture_dir.display()))?;
    let capture_path = capture_dir.join(format!("capture-{}.png", epoch_ms()?));

    capture_screen_platform(args, &capture_path)?;
    Ok(capture_path)
}

#[cfg(target_os = "macos")]
fn capture_screen_platform(args: &ContextCaptureArgs, capture_path: &PathBuf) -> Result<()> {
    let mut command = Command::new("screencapture");
    if args.full_screen {
        command.arg("-x");
    } else {
        command.arg("-i");
    }
    let status = command
        .arg(capture_path)
        .status()
        .context("failed to launch macOS screencapture")?;
    if !status.success() {
        bail!("screen capture was cancelled or failed");
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
#[cfg(not(target_os = "windows"))]
fn capture_screen_platform(_args: &ContextCaptureArgs, _capture_path: &PathBuf) -> Result<()> {
    bail!("permissioned screenshot capture is implemented on macOS first; use `bluey context add <path>` on this platform for now")
}

#[cfg(target_os = "windows")]
fn capture_screen_platform(_args: &ContextCaptureArgs, capture_path: &PathBuf) -> Result<()> {
    let escaped_path = powershell_single_quoted(capture_path);
    let script = format!(
        r#"
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$bitmap = New-Object System.Drawing.Bitmap $bounds.Width, $bounds.Height
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
$bitmap.Save({escaped_path}, [System.Drawing.Imaging.ImageFormat]::Png)
$graphics.Dispose()
$bitmap.Dispose()
"#
    );
    let status = Command::new("powershell")
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-Command")
        .arg(script)
        .status()
        .context("failed to launch Windows screen capture")?;
    if !status.success() {
        bail!("Windows screen capture failed or was denied");
    }
    Ok(())
}

fn preview_capture(capture_path: &PathBuf) {
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open").arg(capture_path).status();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("cmd")
            .arg("/C")
            .arg("start")
            .arg("")
            .arg(capture_path)
            .status();
    }
}

fn confirm_attach(capture_path: &PathBuf) -> Result<bool> {
    print!(
        "Attach this capture to the active meeting? {} [y/N] ",
        capture_path.display()
    );
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

fn epoch_ms() -> Result<u128> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before Unix epoch")?
        .as_millis())
}

async fn add_context(
    path: &str,
    title: Option<String>,
    note: Option<String>,
) -> Result<Vec<ContextArtifact>> {
    let response = request(DaemonRequest::ContextAdd {
        path: path.to_string(),
        title,
        note,
    })
    .await?;
    match response {
        DaemonResponse::ContextItems { items } => Ok(items),
        DaemonResponse::Error { message } => bail!("daemon error: {message}"),
        other => {
            print_response(other)?;
            Ok(Vec::new())
        }
    }
}

fn parse_transcript_line(line: &str, default_speaker: Speaker) -> (Speaker, String) {
    for (prefix, speaker) in [
        ("me:", Speaker::User),
        ("user:", Speaker::User),
        ("you:", Speaker::User),
        ("system:", Speaker::System),
        ("speaker:", Speaker::System),
        ("them:", Speaker::System),
        ("other:", Speaker::Other),
    ] {
        if line
            .get(..prefix.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
        {
            return (
                speaker,
                line[prefix.len()..].trim().trim_matches('"').to_string(),
            );
        }
    }

    (default_speaker, line.to_string())
}

fn print_live_help() {
    println!("Bluey live mode");
    println!("Type meeting transcript lines. Prefix with `me:` or `system:` when useful.");
    println!("Ask with `? your question` or `/ask your question`.");
    println!("Commands: /style text, /capture, /capture-on 12, /capture-off, /attach path, /context, /audio, /ai, /cloud, /actions, /recap, /opacity 70, /position center, /privacy, /show, /hide, /clear, /status, /end, /stop, /quit");
}

fn normalize_opacity(value: f32) -> f32 {
    let opacity = if value > 1.0 { value / 100.0 } else { value };
    opacity.clamp(0.05, 1.0)
}

fn parse_overlay_position(value: &str) -> Option<OverlayPosition> {
    match value.replace('-', "_").as_str() {
        "top_left" => Some(OverlayPosition::TopLeft),
        "top_right" => Some(OverlayPosition::TopRight),
        "bottom_left" => Some(OverlayPosition::BottomLeft),
        "bottom_right" => Some(OverlayPosition::BottomRight),
        "center" => Some(OverlayPosition::Center),
        _ => None,
    }
}

async fn start(args: StartArgs) -> Result<()> {
    if request(DaemonRequest::Ping).await.is_ok() {
        if !args.quiet {
            println!("Bluey daemon is already running.");
        }
        return Ok(());
    }

    let paths = AppPaths::discover()?;
    cleanup_stale_daemon(&paths, args.quiet).await?;

    if args.foreground {
        let daemon_args = daemon_launch_args(&args);
        let mut command = Command::new(resolve_daemon_bin()?);
        command
            .args(daemon_args)
            .env(BLUEY_TRACE_ID_ENV, command_trace_id());
        let status = command
            .status()
            .context("failed to run Bluey daemon in foreground")?;
        if !status.success() {
            bail!("Bluey daemon exited with status {status}");
        }
        return Ok(());
    }

    let mut command = Command::new(resolve_daemon_bin()?);
    command.args(daemon_launch_args(&args));
    command
        .env(BLUEY_TRACE_ID_ENV, command_trace_id())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    configure_detached_daemon(&mut command);

    let mut child = command.spawn().context("failed to start Bluey daemon")?;
    wait_for_daemon_ready(Duration::from_secs(20), Some(&mut child)).await?;
    if !args.quiet {
        println!("Bluey daemon started with pid {}.", child.id());
    }
    Ok(())
}

async fn cleanup_stale_daemon(paths: &AppPaths, quiet: bool) -> Result<()> {
    let state_pid = read_daemon_state_pid(&paths.state_file)?;

    let daemon_bin = resolve_daemon_bin().ok();
    let killed = terminate_recorded_daemon_process(state_pid, daemon_bin.as_deref())?;
    if killed > 0 {
        if let Some(pid) = state_pid {
            match wait_for_recorded_daemon_exit(pid, Duration::from_secs(5)).await {
                Ok(true) => {}
                Ok(false) if !quiet => {
                    eprintln!("Bluey daemon shutdown is still settling.");
                }
                Ok(false) => {}
                Err(error) if !quiet => {
                    eprintln!("warning: could not confirm stale Bluey daemon exit: {error:#}");
                }
                Err(_) => {}
            }
        }
    }

    if paths.state_file.exists() {
        tokio::fs::remove_file(&paths.state_file)
            .await
            .with_context(|| format!("failed to remove {}", paths.state_file.display()))?;
    }

    if !quiet && (state_pid.is_some() || killed > 0) {
        eprintln!("Cleaned up stale Bluey daemon state.");
    }
    Ok(())
}

fn read_daemon_state_pid(state_file: &Path) -> Result<Option<u32>> {
    let Ok(contents) = std::fs::read_to_string(state_file) else {
        return Ok(None);
    };
    if contents.trim().is_empty() {
        return Ok(None);
    }
    let json: serde_json::Value = match serde_json::from_str(&contents) {
        Ok(json) => json,
        Err(_) => return Ok(None),
    };
    Ok(json
        .get("pid")
        .and_then(|value| value.as_u64())
        .and_then(|pid| u32::try_from(pid).ok()))
}

#[cfg(unix)]
fn terminate_pid(pid: u32) -> Result<bool> {
    if pid == 0 || pid == std::process::id() {
        return Ok(false);
    }
    let rc = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
    if rc == 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    if error.kind() == io::ErrorKind::NotFound || error.raw_os_error() == Some(libc::ESRCH) {
        return Ok(false);
    }
    Err(error).with_context(|| format!("failed to terminate stale Bluey daemon pid {pid}"))
}

#[cfg(unix)]
fn terminate_recorded_daemon_process(
    state_pid: Option<u32>,
    daemon_bin: Option<&Path>,
) -> Result<usize> {
    let Some(pid) = state_pid else {
        return Ok(0);
    };
    let output = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .output()
        .context("failed to inspect stale daemon process")?;
    if !output.status.success() {
        return Ok(0);
    }

    let command = String::from_utf8_lossy(&output.stdout);
    if !recorded_daemon_command_matches(command.trim(), daemon_bin) {
        return Ok(0);
    }

    terminate_pid(pid).map(usize::from)
}

#[cfg(not(unix))]
fn terminate_recorded_daemon_process(
    _state_pid: Option<u32>,
    _daemon_bin: Option<&Path>,
) -> Result<usize> {
    Ok(0)
}

#[cfg(unix)]
async fn wait_for_recorded_daemon_exit(pid: u32, timeout: Duration) -> Result<bool> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if !pid_is_running(pid)? {
            return Ok(true);
        }
        sleep(Duration::from_millis(100)).await;
    }
    Ok(!pid_is_running(pid)?)
}

#[cfg(not(unix))]
async fn wait_for_recorded_daemon_exit(_pid: u32, _timeout: Duration) -> Result<bool> {
    Ok(true)
}

#[cfg(unix)]
fn pid_is_running(pid: u32) -> Result<bool> {
    if pid == 0 {
        return Ok(false);
    }
    let rc = unsafe { libc::kill(pid as libc::pid_t, 0) };
    if rc == 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    if error.kind() == io::ErrorKind::NotFound || error.raw_os_error() == Some(libc::ESRCH) {
        return Ok(false);
    }
    if error.raw_os_error() == Some(libc::EPERM) {
        return Ok(true);
    }
    Err(error).with_context(|| format!("failed to inspect Bluey daemon pid {pid}"))
}

#[cfg(unix)]
fn recorded_daemon_command_matches(command: &str, daemon_bin: Option<&Path>) -> bool {
    let expected = daemon_bin.and_then(|path| path.canonicalize().ok());
    let Some(exe) = command.split_whitespace().next() else {
        return false;
    };
    if !is_daemon_executable_name(exe) {
        return false;
    }
    if let Some(expected) = expected.as_ref() {
        let Ok(actual) = Path::new(exe).canonicalize() else {
            return false;
        };
        return actual == *expected;
    }
    true
}

#[cfg(unix)]
fn is_daemon_executable_name(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            let name = name.trim_end_matches(env::consts::EXE_SUFFIX);
            name == "bluey-daemon" || name == "cue-daemon"
        })
}

async fn wait_for_daemon_ready(timeout: Duration, mut child: Option<&mut Child>) -> Result<()> {
    let deadline = Instant::now() + timeout;
    let mut last_error = None;

    while Instant::now() < deadline {
        if let Some(child) = child.as_mut() {
            match (*child).try_wait() {
                Ok(Some(status)) => {
                    bail!("Bluey daemon exited before becoming ready: {status}");
                }
                Ok(None) => {}
                Err(error) => {
                    return Err(error).context("failed to inspect Bluey daemon startup process");
                }
            }
        }
        match request(DaemonRequest::Ping).await {
            Ok(DaemonResponse::Pong) => return Ok(()),
            Ok(other) => last_error = Some(anyhow!("unexpected daemon response: {other:?}")),
            Err(error) => last_error = Some(error),
        }
        sleep(Duration::from_millis(100)).await;
    }

    match last_error {
        Some(error) => Err(error).context("Bluey daemon did not become ready"),
        None => bail!("Bluey daemon did not become ready"),
    }
}

async fn wait_for_daemon_stopped(timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;

    while Instant::now() < deadline {
        if request(DaemonRequest::Ping).await.is_err() {
            return Ok(());
        }
        sleep(Duration::from_millis(100)).await;
    }

    bail!("Bluey daemon did not stop within {}s", timeout.as_secs())
}

#[cfg(unix)]
fn configure_detached_daemon(command: &mut Command) {
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }
}

#[cfg(windows)]
fn configure_detached_daemon(command: &mut Command) {
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}

fn daemon_launch_args(args: &StartArgs) -> Vec<String> {
    let mut launch_args = Vec::new();
    if args.no_overlay {
        launch_args.push("--no-overlay".to_string());
    }
    if let Some(addr) = env_value_any("BLUEY_DAEMON_ADDR", "CUE_DAEMON_ADDR") {
        if !addr.trim().is_empty() {
            launch_args.push("--addr".to_string());
            launch_args.push(addr);
        }
    }
    launch_args
}

fn resolve_daemon_bin() -> Result<PathBuf> {
    if let Some(path) = env_value_any("BLUEY_DAEMON_BIN", "CUE_DAEMON_BIN") {
        return Ok(PathBuf::from(path));
    }

    let exe = env::current_exe().context("failed to resolve current executable")?;
    let mut roots = vec![exe.clone()];
    if let Ok(real_exe) = exe.canonicalize() {
        roots.push(real_exe);
    }
    if let Some(install_root) = env::var_os("BLUEY_INSTALL_ROOT") {
        roots.push(PathBuf::from(install_root).join("bin").join("bluey"));
    }
    if let Some(home) = env::var_os("HOME") {
        roots.push(PathBuf::from(home).join(".bluey/bin/bluey"));
    }

    if let Some(path) = resolve_daemon_bin_from_roots(roots) {
        return Ok(path);
    }

    Err(anyhow!(
        "could not find bluey-daemon in the Bluey install; reinstall with `curl -fsSL https://bluey.sh/install.sh | bash` or set BLUEY_DAEMON_BIN"
    ))
}

fn resolve_daemon_bin_from_roots(roots: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    for root in roots {
        let bluey_sibling = root.with_file_name(format!("bluey-daemon{}", env::consts::EXE_SUFFIX));
        if bluey_sibling.exists() {
            return Some(bluey_sibling);
        }

        let sibling = root.with_file_name(format!("cue-daemon{}", env::consts::EXE_SUFFIX));
        if sibling.exists() {
            return Some(sibling);
        }
    }
    None
}

async fn request(message: DaemonRequest) -> Result<DaemonResponse> {
    let addr = env_value_any("BLUEY_DAEMON_ADDR", "CUE_DAEMON_ADDR")
        .unwrap_or_else(|| DEFAULT_DAEMON_ADDR.to_string());
    let stream = TcpStream::connect(&addr)
        .await
        .with_context(|| format!("failed to connect to Bluey daemon at {addr}"))?;
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    let line = serde_json::to_string(&message.with_trace_id(command_trace_id()))?;
    writer.write_all(line.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;

    let mut response = String::new();
    let read = reader.read_line(&mut response).await?;
    if read == 0 {
        bail!("daemon closed connection without a response");
    }
    Ok(serde_json::from_str(response.trim_end())?)
}

fn command_trace_id() -> String {
    static TRACE_ID: once_cell::sync::Lazy<String> =
        once_cell::sync::Lazy::new(|| trace_id_from_env().unwrap_or_else(new_trace_id));
    TRACE_ID.clone()
}

fn env_value_any(primary: &str, legacy: &str) -> Option<String> {
    env::var(primary)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            env::var(legacy)
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
}

fn answer_request_from_args(question: String, args: &AskArgs) -> AnswerRequest {
    let route = args
        .provider
        .as_deref()
        .or(args.model.as_deref().map(|_| "bluey_managed"))
        .map(|provider| ProviderRoute::direct(provider_selector(provider, args.model.as_deref())))
        .unwrap_or_default();
    let request = AnswerRequest::new(question, route);
    if args.stream {
        request.streaming()
    } else {
        request
    }
}

fn provider_selector(provider: &str, model: Option<&str>) -> ProviderSelector {
    let provider_id = AiProviderId::new(provider);
    let provider_kind = match provider.trim().to_ascii_lowercase().as_str() {
        "bluey" | "bluey_managed" | "cue" | "cue_managed" | "managed" => AiProviderKind::CueManaged,
        "openai" => AiProviderKind::OpenAi,
        "anthropic" => AiProviderKind::Anthropic,
        "google" | "gemini" => AiProviderKind::Google,
        "azure" | "azure_openai" => AiProviderKind::AzureOpenAi,
        "mistral" => AiProviderKind::Mistral,
        "groq" => AiProviderKind::Groq,
        "cerebras" => AiProviderKind::Cerebras,
        "cohere" => AiProviderKind::Cohere,
        "deepgram" => AiProviderKind::Deepgram,
        "local" => AiProviderKind::Local,
        _ => AiProviderKind::Custom,
    };

    let selector = ProviderSelector::new(provider_id, provider_kind);
    match model.filter(|value| !value.trim().is_empty()) {
        Some(model) => selector.with_model(model),
        None => selector,
    }
}

fn print_response(response: DaemonResponse) -> Result<()> {
    match response {
        DaemonResponse::Ok => println!("ok"),
        DaemonResponse::Pong => println!("pong"),
        DaemonResponse::Status { state } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&state).expect("state serializes")
            );
        }
        DaemonResponse::Text { text } => println!("{text}"),
        DaemonResponse::Recap { recap } => print_recap(recap),
        DaemonResponse::ActionItems { items } => print_action_items(items),
        DaemonResponse::ContextItems { items } => print_context_items(items),
        DaemonResponse::MemoryHits { hits } => print_memory_hits(hits),
        DaemonResponse::AudioStatus { status } => print_audio_status(status),
        DaemonResponse::AiStatus { status } => print_ai_status(status),
        DaemonResponse::Answer { response, events } => {
            print_answer_response(response, false, events.len())
        }
        DaemonResponse::CloudStatus { status } => print_cloud_status(status),
        DaemonResponse::Error { message } => {
            bail!("daemon error: {message}");
        }
    }
    Ok(())
}

fn print_answer_response(response: AnswerResponse, metadata: bool, event_count: usize) {
    println!("{}", response.answer);
    if !metadata {
        return;
    }

    let model = response
        .metadata
        .provider
        .model
        .as_ref()
        .map(|model| format!(" / {model}"))
        .unwrap_or_default();
    println!();
    println!("Answer runtime");
    println!("- request: {}", response.metadata.request_id);
    println!(
        "- provider: {}{} ({:?})",
        response.metadata.provider.provider_id, model, response.metadata.provider.provider_kind
    );
    if let Some(route) = response.metadata.requested_route.as_ref() {
        let requested_model = route
            .primary
            .provider
            .model
            .as_ref()
            .map(|model| format!(" / {model}"))
            .unwrap_or_default();
        println!(
            "- requested route: {}{} ({:?})",
            route.primary.provider.provider_id,
            requested_model,
            route.primary.provider.provider_kind
        );
    }
    println!(
        "- finish: {}",
        response
            .metadata
            .finish_reason
            .map(|reason| format!("{reason:?}"))
            .unwrap_or_else(|| "unknown".to_string())
    );
    println!(
        "- latency: {}ms",
        response
            .metadata
            .latency_ms
            .map(|latency| latency.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    );
    println!("- events: {event_count}");
    if let Some(usage) = response.metadata.token_usage {
        println!(
            "- tokens: {} input / {} output / {} total",
            usage.input_tokens, usage.output_tokens, usage.total_tokens
        );
    }
    if let Some(cost) = response.metadata.cost_estimate {
        println!(
            "- cost: {:.4} {} estimated={}",
            cost.amount, cost.currency, cost.estimated
        );
    }
    for notice in response.metadata.safety.notices {
        println!("- note: {notice}");
    }
}

fn print_recap(recap: MeetingRecap) {
    println!("Meeting: {}", recap.title);
    println!("Segments: {}", recap.transcript_segments);
    println!();
    println!("{}", recap.summary);

    if !recap.decisions.is_empty() {
        println!();
        println!("Decisions:");
        for decision in recap.decisions {
            println!("- {}", decision.text);
        }
    }

    if !recap.action_items.is_empty() {
        println!();
        println!("Action items:");
        for item in recap.action_items {
            println!("- {}", item.text);
        }
    }

    if !recap.context.is_empty() {
        println!();
        println!("Context:");
        print_context_items(recap.context);
    }

    if let Some(instructions) = recap
        .answer_instructions
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        println!();
        println!("Answer instructions:");
        println!("{instructions}");
    }
}

fn print_action_items(items: Vec<ActionItem>) {
    if items.is_empty() {
        println!("No action items detected yet.");
        return;
    }

    for item in items {
        match item.owner {
            Some(owner) => println!("- [{}] {}", owner, item.text),
            None => println!("- {}", item.text),
        }
    }
}

fn print_context_items(items: Vec<ContextArtifact>) {
    if items.is_empty() {
        println!("No context files attached yet.");
        return;
    }

    for item in items {
        let note = item
            .note
            .as_ref()
            .filter(|note| !note.trim().is_empty())
            .map(|note| format!(" - {note}"))
            .unwrap_or_default();
        println!(
            "- [{}:{}] {}{}",
            item.kind, item.processing_status, item.title, note
        );
        println!("  {}", item.path);
        if let Some(error) = item
            .processing_error
            .as_ref()
            .filter(|error| !error.trim().is_empty())
        {
            println!("  {}", error);
        }
    }
}

fn print_memory_hits(hits: Vec<MemoryHit>) {
    if hits.is_empty() {
        println!("No memory hits found.");
        return;
    }

    for hit in hits {
        println!(
            "- {} [{}] score {}",
            hit.meeting_title, hit.source, hit.score
        );
        println!("  {}", hit.snippet);
    }
}

fn print_audio_status(status: AudioPipelineStatus) {
    println!("Audio pipeline");
    println!("- state: {:?}", status.capture.state);
    println!(
        "- session: {}",
        status.session_id.as_deref().unwrap_or("none")
    );
    println!("- runtime: {:?}", status.runtime_mode);
    println!("- sources: {}", status.active_source_count());
    println!("- runtime ready: {}", yes_no(status.backend_ready));
    println!(
        "- native capture: {}",
        yes_no(status.platform.native_capture_available)
    );
    println!(
        "- development simulator available: {}",
        yes_no(status.platform.simulated_capture_available)
    );
    println!(
        "- STT provider: {}",
        status.stt_provider.as_deref().unwrap_or("not selected")
    );
    println!(
        "- transcript segments emitted: {}",
        status.transcript_segments_emitted
    );
    println!("- chunk duration: {}ms", status.config.chunk_duration_ms);
    println!("- mix strategy: {:?}", status.config.mix_strategy);
    println!("- platform: {}", status.platform.platform);
    println!("- capability: {}", status.platform.note);
    if let Some(note) = status.note.as_ref() {
        println!("- note: {note}");
    }
    println!();
    println!("Sources:");
    println!(
        "- system: {:?} ({}, chunks: {}, last sequence: {})",
        status.capture.system.state,
        if status.config.system.enabled {
            "enabled"
        } else {
            "disabled"
        },
        status.capture.system.chunks_captured,
        status
            .capture
            .system
            .last_sequence
            .map(|sequence| sequence.to_string())
            .unwrap_or_else(|| "none".to_string())
    );
    println!(
        "- microphone: {:?} ({}, chunks: {}, last sequence: {})",
        status.capture.microphone.state,
        if status.config.microphone.enabled {
            "enabled"
        } else {
            "disabled"
        },
        status.capture.microphone.chunks_captured,
        status
            .capture
            .microphone
            .last_sequence
            .map(|sequence| sequence.to_string())
            .unwrap_or_else(|| "none".to_string())
    );
    if !status.devices.is_empty() {
        println!();
        println!("Devices:");
        for device in status.devices {
            println!(
                "- {}: {} via {:?} ({})",
                device.source,
                device.name,
                device.backend,
                if device.is_available {
                    "available"
                } else {
                    "unavailable"
                }
            );
        }
    }
}

fn print_ai_status(status: AiRuntimeStatus) {
    println!("AI routing");
    println!(
        "- primary: {} ({:?})",
        status.route.primary.provider.provider_id, status.route.primary.provider.provider_kind
    );
    println!("- fallbacks: {}", status.route.fallbacks.len());
    println!(
        "- streaming answers: {}",
        yes_no(status.streaming_answers_enabled)
    );
    println!("- vision: {}", yes_no(status.vision_enabled));
    println!("- STT: {}", yes_no(status.stt_enabled));
    println!(
        "- managed cloud required: {}",
        yes_no(status.managed_cloud_required)
    );
    println!();
    println!("Providers:");
    for provider in status.providers {
        let model = provider
            .provider
            .model
            .as_ref()
            .map(|model| format!(" / {model}"))
            .unwrap_or_default();
        let message = provider
            .message
            .as_ref()
            .map(|message| format!(" - {message}"))
            .unwrap_or_default();
        println!(
            "- {}{}: {:?}{}",
            provider.provider.provider_id, model, provider.health, message
        );
    }
}

fn print_cloud_status(status: CloudSyncStatus) {
    println!("Cloud sync");
    println!("- endpoint: {}", status.endpoint.api_url);
    println!("- auth: {:?}", status.auth_state);
    println!("- sync: {:?}", status.sync_state);
    println!("- workspace: {}", status.workspace_id);
    println!("- user: {}", status.user_id);
    println!("- device: {}", status.device_id);
    println!("- cloud RAG: {}", yes_no(status.rag_enabled));
    println!("- pending uploads: {}", status.pending_uploads);
    println!("- pending downloads: {}", status.pending_downloads);
    if let Some(last_synced_at) = status.last_synced_at.as_ref() {
        println!("- last synced: {last_synced_at}");
    }
    if let Some(error) = status.last_error.as_ref() {
        println!("- note: {error}");
    }
}

async fn print_cloud_sessions(limit: i64, json: bool) -> Result<()> {
    let Some(client) = cloud_client_or_message()? else {
        println!("Not signed in. Run `bluey on` to finish setup.");
        return Ok(());
    };
    let sessions = client
        .list_cloud_sessions(Some(limit.clamp(1, 200)))
        .await
        .context("failed to list cloud sessions")?;
    if json {
        println!("{}", serde_json::to_string_pretty(&sessions)?);
        return Ok(());
    }
    if sessions.sessions.is_empty() {
        println!("No cloud sessions yet. Start or answer in Bluey while signed in; saved sessions sync automatically in the background.");
        return Ok(());
    }
    println!("Cloud sessions:");
    for session in sessions.sessions {
        println!(
            "- {}  {}  {} transcript, {} answer(s), {} context",
            short_id(&session.session_id),
            session.status,
            session.transcript_count,
            session.response_count,
            session.context_count
        );
        println!("  {}", session.title);
    }
    Ok(())
}

async fn print_cloud_session(id: &str, json: bool) -> Result<()> {
    let Some(client) = cloud_client_or_message()? else {
        println!("Not signed in. Run `bluey on` to finish setup.");
        return Ok(());
    };
    let bundle = client
        .load_cloud_session(id)
        .await
        .with_context(|| format!("failed to load cloud session {id}"))?;
    if json {
        println!("{}", serde_json::to_string_pretty(&bundle)?);
        return Ok(());
    }
    println!("Session: {}", bundle.session.title);
    println!("ID: {}", bundle.session.session_id);
    println!("Status: {}", bundle.session.status);
    println!(
        "Stored: {} transcript, {} answer(s), {} context",
        bundle.transcript_segments.len(),
        bundle.cue_responses.len(),
        bundle.context_artifacts.len()
    );
    if let Some(instructions) = bundle.session.answer_style.as_deref() {
        println!("\nAnswer style:\n{instructions}");
    }
    if let Some(segment) = bundle.transcript_segments.last() {
        println!(
            "\nLatest transcript:\n{}: {}",
            segment.speaker, segment.text
        );
    }
    if let Some(answer) = bundle.cue_responses.last() {
        println!("\nLatest answer:\n{}", answer.text);
    }
    Ok(())
}

async fn print_cloud_rag(query: &str, limit: i64, json: bool) -> Result<()> {
    let query = query.trim();
    if query.is_empty() {
        bail!("provide a RAG query, for example: bluey cloud rag architecture risk");
    }
    let Some(client) = cloud_client_or_message()? else {
        println!("Not signed in. Run `bluey on` to finish setup.");
        return Ok(());
    };
    let result = client
        .query_rag(&cue_cloud_client::RagQueryRequest {
            query: query.to_string(),
            embedding: None,
            top_k: Some(limit.clamp(1, 20)),
        })
        .await
        .context("failed to query cloud RAG")?;
    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }
    if result.matches.is_empty() {
        println!("No cloud RAG matches yet.");
        return Ok(());
    }
    for hit in result.matches {
        println!(
            "- {}:{}  score {:.3}  session {}",
            hit.source_kind,
            hit.chunk_index,
            hit.score,
            hit.session_id.as_deref().map(short_id).unwrap_or("global")
        );
        println!("  {}", compact_line(&hit.text, 180));
    }
    Ok(())
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn short_id(id: &str) -> &str {
    id.get(..8).unwrap_or(id)
}

fn compact_line(text: &str, max_chars: usize) -> String {
    let clean = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean.chars().count() <= max_chars {
        return clean;
    }
    let mut out = clean.chars().take(max_chars).collect::<String>();
    out.push_str("...");
    out
}

fn print_provider_status() {
    println!("Provider routing status");
    println!("- local deterministic engine: available with `--provider local`");
    for (name, key_env, endpoint_env, default_endpoint) in [
        (
            "Bluey managed",
            "BLUEY_CLOUD_API_TOKEN",
            "BLUEY_CLOUD_API_URL",
            "http://127.0.0.1:8787",
        ),
        (
            "OpenAI",
            "OPENAI_API_KEY",
            "OPENAI_API_URL",
            "https://api.openai.com/v1/responses",
        ),
        (
            "Groq",
            "GROQ_API_KEY",
            "GROQ_API_URL",
            "https://api.groq.com/openai/v1/chat/completions",
        ),
        (
            "Cerebras",
            "CEREBRAS_API_KEY",
            "CEREBRAS_API_URL",
            "https://api.cerebras.ai/v1/chat/completions",
        ),
        (
            "Anthropic",
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_API_URL",
            "https://api.anthropic.com/v1/messages",
        ),
    ] {
        let configured = env_present(key_env);
        let endpoint = env::var(endpoint_env).unwrap_or_else(|_| default_endpoint.to_string());
        println!(
            "- {}: {} ({}), endpoint {}",
            name,
            if configured {
                "configured"
            } else {
                "not configured"
            },
            key_env,
            endpoint
        );
    }
    println!("Remote providers return a clear unavailable error until an HTTP adapter is linked.");
}

fn env_present(name: &str) -> bool {
    env::var_os(name)
        .filter(|value| !value.is_empty())
        .is_some()
}

fn optional_cloud_client() -> Result<Option<cue_cloud_client::CloudClient>> {
    let paths = AppPaths::discover()?;
    let account = load_account(&paths).ok().flatten();
    let base_url = env::var("BLUEY_CLOUD_API_URL")
        .or_else(|_| env::var("CUE_CLOUD_API_URL"))
        .ok()
        .or_else(|| account.as_ref().map(|account| account.api_url.clone()))
        .unwrap_or_else(|| "https://bluey.sh".to_string());
    let config = cue_cloud_client::client::ClientConfig {
        base_url,
        trace_id: Some(command_trace_id()),
        ..Default::default()
    };

    if let Some(access) = cloud_access_token_from_env() {
        let store = cue_cloud_client::tokens::MemoryStore::new();
        cue_cloud_client::TokenStore::save(
            &store,
            &cue_cloud_client::Tokens {
                access,
                refresh: env::var("BLUEY_CLOUD_REFRESH_TOKEN")
                    .or_else(|_| env::var("CUE_CLOUD_REFRESH_TOKEN"))
                    .unwrap_or_default(),
                email: env::var("BLUEY_USER_ID")
                    .or_else(|_| env::var("CUE_USER_ID"))
                    .unwrap_or_else(|_| "env-token".to_string()),
            },
        )?;
        return cue_cloud_client::CloudClient::new(config, Arc::new(store))
            .map(Some)
            .map_err(Into::into);
    }

    let store = cue_cloud_client::SecureAccountStore::new(paths.clone());
    let client = cue_cloud_client::CloudClient::new(config.clone(), Arc::new(store))?;
    if client.current_tokens().is_some() {
        return Ok(Some(client));
    }

    if legacy_keyring_fallback_enabled() {
        let client = cue_cloud_client::CloudClient::new(
            config,
            Arc::new(cue_cloud_client::tokens::KeyringStore::new()),
        )?;
        if client.current_tokens().is_some() {
            return Ok(Some(client));
        }
    }

    Ok(None)
}

fn cloud_client_or_message() -> Result<Option<cue_cloud_client::CloudClient>> {
    match optional_cloud_client() {
        Ok(client) => Ok(client),
        Err(error) => {
            eprintln!("bluey: cloud client init failed: {error}");
            eprintln!("Run `bluey on` to sign in if you have not already.");
            Ok(None)
        }
    }
}

fn cloud_access_token_from_env() -> Option<String> {
    env::var("BLUEY_CLOUD_TOKEN")
        .ok()
        .or_else(|| env::var("BLUEY_CLOUD_API_TOKEN").ok())
        .or_else(|| env::var("BLUEY_API_TOKEN").ok())
        .or_else(|| env::var("CUE_CLOUD_TOKEN").ok())
        .or_else(|| env::var("CUE_API_TOKEN").ok())
        .filter(|token| !token.trim().is_empty())
}

async fn bluey_usage_cmd() -> Result<()> {
    let Some(client) = cloud_client_or_message()? else {
        eprintln!("bluey: not signed in. Run `bluey on` to finish setup.");
        return Ok(());
    };
    if let Err(e) = crate::bluey_cmds::show_usage(&client).await {
        eprintln!("bluey: usage failed: {e}");
    }
    Ok(())
}

async fn bluey_credits_cmd() -> Result<()> {
    let Some(client) = cloud_client_or_message()? else {
        eprintln!("bluey: not signed in. Run `bluey on` to finish setup.");
        return Ok(());
    };
    if let Err(e) = crate::bluey_cmds::show_credits(&client).await {
        eprintln!("bluey: credits failed: {e}");
    }
    Ok(())
}

async fn bluey_logout_cmd() -> Result<()> {
    let paths = AppPaths::discover()?;
    let account_store = cue_cloud_client::SecureAccountStore::new(paths.clone());
    let had_account_tokens = cue_cloud_client::TokenStore::load(&account_store)?.is_some();
    cue_cloud_client::TokenStore::clear(&account_store)?;

    let had_keyring_tokens = if legacy_keyring_fallback_enabled() {
        match clear_keyring_tokens_with_timeout()? {
            Some(had_tokens) => had_tokens,
            None => {
                eprintln!(
                    "bluey: legacy keyring cleanup timed out; local account config was still cleared"
                );
                false
            }
        }
    } else {
        false
    };

    if !had_account_tokens && !had_keyring_tokens {
        println!("Bluey is already logged out.");
        return Ok(());
    }
    let _ = request(DaemonRequest::CloudLogout).await;
    println!("Bluey account logged out.");
    Ok(())
}

fn clear_keyring_tokens_with_timeout() -> Result<Option<bool>> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = (|| -> Result<bool> {
            let client = cue_cloud_client::CloudClient::with_default_keyring()
                .context("failed to open keyring token store")?;
            let had_keyring_tokens = client.current_tokens().is_some();
            client.clear_tokens()?;
            Ok(had_keyring_tokens)
        })();
        let _ = tx.send(result);
    });
    match rx.recv_timeout(std::time::Duration::from_secs(5)) {
        Ok(result) => result.map(Some),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Ok(None),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            Err(anyhow!("keyring cleanup task ended without returning"))
        }
    }
}

async fn bluey_portal_cmd() -> Result<()> {
    let Some(client) = cloud_client_or_message()? else {
        eprintln!("bluey: not signed in. Run `bluey on` to finish setup.");
        return Ok(());
    };
    if let Err(e) = crate::bluey_cmds::portal(&client).await {
        eprintln!("bluey: portal failed: {e}");
    }
    Ok(())
}

async fn bluey_export_cmd() -> Result<()> {
    let Some(client) = cloud_client_or_message()? else {
        eprintln!("bluey: not signed in. Run `bluey on` to finish setup.");
        return Ok(());
    };
    if let Err(e) = crate::bluey_cmds::export_data(&client).await {
        eprintln!("bluey: export failed: {e}");
    }
    Ok(())
}

async fn bluey_delete_account_cmd(force: bool) -> Result<()> {
    let Some(client) = cloud_client_or_message()? else {
        eprintln!("bluey: not signed in. Run `bluey on` to finish setup.");
        return Ok(());
    };
    if let Err(e) = crate::bluey_cmds::delete_account(&client, force).await {
        eprintln!("bluey: delete-account failed: {e}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        bluey_on_boot_lines, bluey_on_boot_title, default_bluey_signin_url, device_login_url,
        login_account_provider, resolve_daemon_bin_from_roots, resolve_login_api_url_from,
        BlueyOnAuthState,
    };
    use std::{fs, path::PathBuf};

    #[test]
    fn bluey_on_boot_lines_offer_browser_signin_when_unlinked() {
        let lines = bluey_on_boot_lines(&BlueyOnAuthState::SignInAvailable {
            url: "https://bluey.sh/login".to_string(),
        });
        assert!(lines.iter().any(|line| line.contains("local recording")));
        assert!(lines.iter().any(|line| line.contains("knowledge base")));
        assert!(lines
            .iter()
            .any(|line| line.contains("opens automatically")));
    }

    #[test]
    fn bluey_on_boot_lines_confirm_managed_ready_when_linked() {
        let lines = bluey_on_boot_lines(&BlueyOnAuthState::Ready);
        assert!(lines.iter().any(|line| line.contains("answers stream")));
        assert!(!lines
            .iter()
            .any(|line| line.contains("separate login command")));
    }

    #[test]
    fn bluey_on_boot_lines_include_link_url_when_unlinked() {
        let lines = bluey_on_boot_lines(&BlueyOnAuthState::SignInAvailable {
            url: "https://bluey.sh/login".to_string(),
        });
        assert!(lines
            .iter()
            .any(|line| line == "login_url: https://bluey.sh/login"));
    }

    #[test]
    fn bluey_on_boot_title_reflects_signed_out_state() {
        assert_eq!(
            bluey_on_boot_title(&BlueyOnAuthState::SignInAvailable {
                url: "https://bluey.sh/login".to_string(),
            }),
            "Sign in to Bluey"
        );
        assert_eq!(
            bluey_on_boot_title(&BlueyOnAuthState::Ready),
            "Bluey online"
        );
    }

    #[test]
    fn bluey_signin_url_defaults_to_login_page() {
        assert_eq!(default_bluey_signin_url(), "https://bluey.sh/login");
    }

    #[test]
    fn bluey_login_defaults_to_bluey_cloud() {
        assert_eq!(
            resolve_login_api_url_from(false, None, None, None),
            "https://bluey.sh"
        );
        assert_eq!(login_account_provider(false), "bluey");
    }

    #[test]
    fn bluey_login_local_mode_stays_local() {
        assert_eq!(
            resolve_login_api_url_from(true, None, None, None),
            "http://127.0.0.1:8787"
        );
        assert_eq!(login_account_provider(true), "local");
    }

    #[test]
    fn bluey_login_explicit_url_wins() {
        assert_eq!(
            resolve_login_api_url_from(
                false,
                Some("https://staging.bluey.sh".to_string()),
                Some("https://ignored.bluey.sh".to_string()),
                Some("https://ignored-cue.bluey.sh".to_string()),
            ),
            "https://staging.bluey.sh"
        );
    }

    #[test]
    fn bluey_login_env_prefers_bluey_over_legacy_cue() {
        assert_eq!(
            resolve_login_api_url_from(
                false,
                None,
                Some("https://cloud.bluey.sh".to_string()),
                Some("https://legacy-cue.example".to_string()),
            ),
            "https://cloud.bluey.sh"
        );
    }

    #[test]
    fn device_login_url_appends_code_to_login_page() {
        assert_eq!(
            device_login_url("https://bluey.sh/login", "ABCD-EFGH"),
            "https://bluey.sh/login?user_code=ABCD-EFGH"
        );
    }

    #[test]
    fn device_login_url_preserves_existing_query() {
        assert_eq!(
            device_login_url("https://bluey.sh/login?source=desktop", "ABCD-EFGH"),
            "https://bluey.sh/login?source=desktop&user_code=ABCD-EFGH"
        );
    }

    #[test]
    fn bluey_on_permission_gate_blocks_only_actionable_states() {
        use crate::macos_perms::PermissionStatus;

        assert!(super::macos_permission_ready(PermissionStatus::Granted));
        assert!(super::macos_permission_ready(
            PermissionStatus::NotApplicable
        ));
        assert!(super::macos_permission_ready(PermissionStatus::Unknown));
        assert!(!super::macos_permission_ready(PermissionStatus::Denied));
        assert!(!super::macos_permission_ready(
            PermissionStatus::NotDetermined
        ));
        assert!(!super::macos_permission_ready(PermissionStatus::Restricted));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_permission_settings_uri_points_to_privacy_section() {
        assert_eq!(
            super::macos_permission_settings_uri("Privacy_Microphone"),
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
        );
    }

    #[test]
    fn resolve_daemon_bin_finds_installed_bluey_daemon_sibling() {
        let base = std::env::temp_dir().join(format!(
            "bluey-daemon-lookup-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        let bin_dir = base.join("bin");
        fs::create_dir_all(&bin_dir).expect("create bin dir");

        let bluey = bin_dir.join(format!("bluey{}", std::env::consts::EXE_SUFFIX));
        let daemon = bin_dir.join(format!("bluey-daemon{}", std::env::consts::EXE_SUFFIX));
        fs::write(&bluey, b"bluey").expect("write bluey");
        fs::write(&daemon, b"bluey-daemon").expect("write daemon");

        assert_eq!(
            resolve_daemon_bin_from_roots(vec![PathBuf::from("/missing/bluey"), bluey]),
            Some(daemon)
        );

        let _ = fs::remove_dir_all(base);
    }

    #[cfg(unix)]
    #[test]
    fn stale_daemon_command_rejects_reused_non_daemon_pid() {
        assert!(!super::recorded_daemon_command_matches(
            "/bin/sleep 999",
            None,
        ));
    }

    #[cfg(unix)]
    #[test]
    fn stale_daemon_command_requires_expected_binary_when_known() {
        let base = std::env::temp_dir().join(format!(
            "bluey-stale-daemon-command-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        fs::create_dir_all(&base).expect("create temp dir");
        let expected = base.join("bluey-daemon");
        let other = base.join("bluey-daemon-other");
        fs::write(&expected, b"daemon").expect("write expected daemon");
        fs::write(&other, b"daemon").expect("write other daemon");

        assert!(super::recorded_daemon_command_matches(
            &format!("{} --foreground", expected.display()),
            Some(&expected),
        ));
        assert!(!super::recorded_daemon_command_matches(
            &format!("{} --foreground", other.display()),
            Some(&expected),
        ));

        let _ = fs::remove_dir_all(base);
    }
}
