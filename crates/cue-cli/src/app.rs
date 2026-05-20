use std::env;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::process::CommandExt;
#[cfg(windows)]
use std::os::windows::process::CommandExt;

use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use cue_core::app_paths::AppPaths;
use cue_core::ipc::{DaemonRequest, DaemonResponse, DEFAULT_DAEMON_ADDR};
use cue_core::{
    load_account, load_settings, save_account, save_settings, AccountConfig, ActionItem,
    AiProviderId, AiProviderKind, AiRuntimeStatus, AnswerRequest, AnswerResponse,
    AudioPipelineStatus, CardKind, CloudSyncStatus, ContextArtifact, CueCard, CueSettings,
    MeetingRecap, MeetingRecord, MemoryHit, OverlayPosition, ProviderRoute, ProviderSelector,
    Speaker,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
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
    Login(LoginArgs),
    /// Show account and cloud link status.
    Account,
    /// List or inspect saved local sessions.
    Sessions(SessionsArgs),
    /// Show or update terminal-first Bluey settings.
    Settings(SettingsArgs),
    /// Show your Bluey balance, last-7-days usage, and tier projection.
    Usage,
    /// Show your current Bluey balance and 1-year credit-validity reminder.
    /// (Per-batch expiration listing is not yet available; coming in a
    /// future release.)
    Credits,
    /// Log out of Bluey: clear keyring tokens.
    Logout,
    /// Open the Stripe Customer Portal in your browser to manage card / cancel auto top-up / view invoices.
    Portal,
    /// Export your Bluey account data as a JSON file (GDPR).
    Export,
    /// Permanently delete your Bluey account (interactive confirmation; --force to skip prompt).
    DeleteAccount {
        #[arg(long)]
        force: bool,
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
    /// Optional title for immediately creating a session. Without this, Bluey opens the launcher.
    #[arg(long)]
    title: Option<String>,
}

#[derive(Debug, Args)]
struct LoginArgs {
    /// Bluey API URL. Defaults to BLUEY_CLOUD_API_URL, CUE_CLOUD_API_URL, PINKY_API, or https://bluey.sh.
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
        Commands::Logout => bluey_logout_cmd().await,
        Commands::Portal => bluey_portal_cmd().await,
        Commands::Export => bluey_export_cmd().await,
        Commands::DeleteAccount { force } => bluey_delete_account_cmd(force).await,
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
        Commands::Cloud { command } => {
            let request_msg = match command {
                CloudCommands::Status => DaemonRequest::CloudStatus,
                CloudCommands::Sync => DaemonRequest::CloudSyncNow,
            };
            let response = request(request_msg).await?;
            print_response(response)
        }
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
    let paths = AppPaths::discover()?;
    let settings = load_settings(&paths)?;
    ensure_daemon_quiet(false).await?;

    if args.title.is_some() {
        match request(DaemonRequest::MeetingStart { title: args.title }).await? {
            DaemonResponse::Text { text } if text.contains("already active") => {}
            DaemonResponse::Text { .. } | DaemonResponse::Recap { .. } | DaemonResponse::Ok => {}
            DaemonResponse::Error { message } => bail!("daemon error: {message}"),
            other => {
                print_response(other)?;
            }
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
    // The native overlay orders the branded pill front when the child process
    // starts. Do not send OverlayShow here: in the current protocol it expands
    // the full feed, while `bluey on` should launch pill-first.
    let boot = request(DaemonRequest::OverlayBoot {
        title: "Bluey online".to_string(),
        lines: vec![
            "daemon link established".to_string(),
            "private overlay ready".to_string(),
            "click the pill, then choose New or Continue".to_string(),
            "attach files with Attach".to_string(),
            "screen context waits for Analyse consent".to_string(),
            "audio transcripts appear as source-labeled cards".to_string(),
            "answers appear as overlay cards".to_string(),
        ],
    })
    .await;

    match boot {
        Ok(DaemonResponse::Ok) => {
            println!("Bluey is on.");
            Ok(())
        }
        Ok(other) => print_response(other),
        Err(error) => {
            println!("Bluey is on, but the overlay is not reachable yet: {error:#}");
            Ok(())
        }
    }
}

async fn cue_off() -> Result<()> {
    match request(DaemonRequest::Shutdown).await {
        Ok(DaemonResponse::Ok) => {
            println!("Bluey is off.");
            Ok(())
        }
        Ok(response) => print_response(response),
        Err(error) => {
            let paths = AppPaths::discover()?;
            if paths.state_file.exists() {
                tokio::fs::remove_file(&paths.state_file).await.with_context(|| {
                    format!(
                        "Bluey daemon was not reachable ({error:#}), and failed to remove stale state file {}",
                        paths.state_file.display()
                    )
                })?;
                println!("Bluey is off.");
                return Ok(());
            }
            println!("Bluey is off.");
            Ok(())
        }
    }
}

async fn cue_login(args: LoginArgs) -> Result<()> {
    let paths = AppPaths::discover()?;
    paths.ensure()?;

    let api_url = if args.local && args.api_url.is_none() {
        "http://127.0.0.1:8787".to_string()
    } else {
        args.api_url
            .clone()
            .or_else(|| env::var("BLUEY_CLOUD_API_URL").ok())
            .or_else(|| env::var("CUE_CLOUD_API_URL").ok())
            .or_else(|| env::var("PINKY_API").ok())
            .or_else(read_pinky_api_from_auth)
            .unwrap_or_else(|| "https://bluey.sh".to_string())
    };

    let env_token = env::var("BLUEY_CLOUD_TOKEN")
        .ok()
        .or_else(|| env::var("BLUEY_API_TOKEN").ok())
        .or_else(|| env::var("CUE_CLOUD_TOKEN").ok())
        .or_else(|| env::var("CUE_API_TOKEN").ok())
        .or_else(|| env::var("PINKY_CUE_TOKEN").ok());
    let token = args.token.or(env_token);

    let account = if args.local || args.no_browser || token.is_some() {
        let mut account = AccountConfig::local();
        account.provider = if args.local {
            "local".to_string()
        } else if read_pinky_api_from_auth().is_some() {
            "pinky".to_string()
        } else {
            "bluey".to_string()
        };
        account.api_url = api_url;
        account.user_id = args.user;
        account.workspace_id = args.workspace;
        account.access_token = token;
        account.refresh_token = args.refresh_token;
        account
    } else {
        browser_login(&api_url, args.user, args.workspace).await?
    };

    save_account(&paths, &account)?;

    // Codex Stage 8 S8.1 (round 3): save tokens to the cue-cloud-client
    // keyring store whenever access_token exists so `bluey usage` and
    // `bluey credits` (which read from keyring) can find them after
    // any successful `bluey login` path. Token-only / env-token /
    // browser-without-refresh logins all produce access-only sessions;
    // cue-cloud-client treats refresh as optional/defaultable so an
    // empty string is safe. Best-effort: keyring failure prints a
    // warning but does not fail login.
    if let Some(access) = account.access_token.clone() {
        let refresh = account.refresh_token.clone().unwrap_or_default();
        match cue_cloud_client::CloudClient::with_default_keyring() {
            Ok(client) => {
                let email = account.user_id.clone();
                if let Err(e) = client.save_tokens(cue_cloud_client::Tokens {
                    access,
                    refresh,
                    email,
                }) {
                    eprintln!(
                        "warning: could not save tokens to keyring: {e}\n\
                         (legacy AccountConfig path still works; `bluey usage` may report not-logged-in until keyring is available)"
                    );
                }
            }
            Err(e) => {
                eprintln!(
                    "warning: cloud client keyring unavailable: {e}\n\
                     (legacy AccountConfig path still works; `bluey usage` may report not-logged-in until keyring is available)"
                );
            }
        }
    }

    println!(
        "Bluey account linked: {} ({})",
        account.provider, account.api_url
    );
    if account.token_configured() {
        println!("Cloud token saved for this user profile.");
    } else {
        println!("Local account linked. Add a token later with `bluey login --token ...` when the Bluey cloud endpoint is ready.");
    }
    Ok(())
}

async fn print_account() -> Result<()> {
    let paths = AppPaths::discover()?;
    let account = load_account(&paths)?;
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
                if account.token_configured() {
                    "configured"
                } else {
                    "not configured"
                }
            );
        }
        None => {
            println!("Account: local");
            println!("Run `bluey login` to link Bluey to cloud later.");
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
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .context("failed to start local login callback listener")?;
    let port = listener.local_addr()?.port();
    let state = format!("bluey-{}-{}", std::process::id(), epoch_ms()?);
    let callback = format!("http://127.0.0.1:{port}/callback");
    let login_url = format!(
        "{}/login?callback={}&state={}&product=bluey",
        api_url.trim_end_matches('/'),
        percent_encode(&callback),
        percent_encode(&state)
    );

    println!("Opening browser login...");
    println!("{login_url}");
    let _ = open_browser(&login_url);

    let token_result = tokio::time::timeout(Duration::from_secs(300), async {
        loop {
            let (stream, _) = listener.accept().await?;
            if let Some(result) = handle_login_callback(stream, &state).await? {
                return Ok::<_, anyhow::Error>(result);
            }
        }
    })
    .await
    .context("login timed out after 5 minutes")??;

    let mut account = AccountConfig::local();
    account.provider = "bluey".to_string();
    account.api_url = api_url.to_string();
    account.user_id = user_id;
    account.workspace_id = workspace_id;
    account.access_token = Some(token_result.0);
    account.refresh_token = token_result.1;
    Ok(account)
}

async fn handle_login_callback(
    mut stream: TcpStream,
    expected_state: &str,
) -> Result<Option<(String, Option<String>)>> {
    let mut reader = BufReader::new(&mut stream);
    let mut first_line = String::new();
    reader.read_line(&mut first_line).await?;
    let path = first_line.split_whitespace().nth(1).unwrap_or_default();
    let params = parse_query(path);
    let token = params
        .iter()
        .find(|(key, _)| key == "token")
        .map(|(_, value)| value.clone());
    let refresh_token = params
        .iter()
        .find(|(key, _)| key == "refreshToken" || key == "refresh_token")
        .map(|(_, value)| value.clone());
    let state = params
        .iter()
        .find(|(key, _)| key == "state")
        .map(|(_, value)| value.as_str())
        .unwrap_or_default();

    let (status, body, result) = if state != expected_state {
        (
            "403 Forbidden",
            "Bluey login rejected: state mismatch.",
            None,
        )
    } else if let Some(token) = token {
        (
            "200 OK",
            "Bluey login complete. You can close this tab and return to the terminal.",
            Some((token, refresh_token)),
        )
    } else {
        (
            "400 Bad Request",
            "Bluey login failed: missing token.",
            None,
        )
    };

    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.flush().await?;
    Ok(result)
}

fn read_pinky_api_from_auth() -> Option<String> {
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("USERPROFILE").map(PathBuf::from))?;
    let auth_path = home.join(".pinky").join("auth.json");
    let bytes = std::fs::read(auth_path).ok()?;
    let json: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    json.get("api")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
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
    println!("Cloud sync: {}", on_off(settings.cloud_sync_enabled));
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

    command
        .arg(url)
        .status()
        .context("failed to open browser")?;
    Ok(())
}

fn parse_query(path: &str) -> Vec<(String, String)> {
    let Some((_, query)) = path.split_once('?') else {
        return Vec::new();
    };
    query
        .split('&')
        .filter_map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            Some((percent_decode(key)?, percent_decode(value)?))
        })
        .collect()
}

fn percent_encode(value: &str) -> String {
    let mut output = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                output.push(byte as char)
            }
            _ => output.push_str(&format!("%{byte:02X}")),
        }
    }
    output
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                output.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok()?;
                output.push(u8::from_str_radix(hex, 16).ok()?);
                index += 3;
            }
            byte => {
                output.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(output).ok()
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

    if args.foreground {
        let daemon_args = daemon_launch_args(&args);
        let status = Command::new(resolve_daemon_bin()?)
            .args(daemon_args)
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
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    configure_detached_daemon(&mut command);

    let child = command.spawn().context("failed to start Bluey daemon")?;
    wait_for_daemon_ready(Duration::from_secs(5)).await?;
    if !args.quiet {
        println!("Bluey daemon started with pid {}.", child.id());
    }
    Ok(())
}

async fn wait_for_daemon_ready(timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    let mut last_error = None;

    while Instant::now() < deadline {
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
    let bluey_sibling = exe.with_file_name(format!("bluey-daemon{}", env::consts::EXE_SUFFIX));
    if bluey_sibling.exists() {
        return Ok(bluey_sibling);
    }

    let sibling = exe.with_file_name(format!("cue-daemon{}", env::consts::EXE_SUFFIX));
    if sibling.exists() {
        return Ok(sibling);
    }

    Err(anyhow!(
        "could not find bluey-daemon next to bluey; build with `cargo build` or set BLUEY_DAEMON_BIN"
    ))
}

async fn request(message: DaemonRequest) -> Result<DaemonResponse> {
    let addr = env_value_any("BLUEY_DAEMON_ADDR", "CUE_DAEMON_ADDR")
        .unwrap_or_else(|| DEFAULT_DAEMON_ADDR.to_string());
    let stream = TcpStream::connect(&addr)
        .await
        .with_context(|| format!("failed to connect to Bluey daemon at {addr}"))?;
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    let line = serde_json::to_string(&message)?;
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

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
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

async fn bluey_usage_cmd() -> Result<()> {
    let client = match cue_cloud_client::CloudClient::with_default_keyring() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("bluey: cloud client init failed: {e}");
            eprintln!("Run `bluey login` first if you have not already.");
            return Ok(());
        }
    };
    if client.current_tokens().is_none() {
        eprintln!("bluey: not logged in. Run `bluey login` first.");
        return Ok(());
    }
    if let Err(e) = crate::bluey_cmds::show_usage(&client).await {
        eprintln!("bluey: usage failed: {e}");
    }
    Ok(())
}

async fn bluey_credits_cmd() -> Result<()> {
    let client = match cue_cloud_client::CloudClient::with_default_keyring() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("bluey: cloud client init failed: {e}");
            return Ok(());
        }
    };
    if client.current_tokens().is_none() {
        eprintln!("bluey: not logged in. Run `bluey login` first.");
        return Ok(());
    }
    if let Err(e) = crate::bluey_cmds::show_credits(&client).await {
        eprintln!("bluey: credits failed: {e}");
    }
    Ok(())
}

async fn bluey_logout_cmd() -> Result<()> {
    let client = match cue_cloud_client::CloudClient::with_default_keyring() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("bluey: cloud client init failed: {e}");
            return Ok(());
        }
    };
    if let Err(e) = crate::bluey_cmds::logout(&client).await {
        eprintln!("bluey: logout failed: {e}");
    }
    Ok(())
}

async fn bluey_portal_cmd() -> Result<()> {
    let client = match cue_cloud_client::CloudClient::with_default_keyring() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("bluey: cloud client init failed: {e}");
            return Ok(());
        }
    };
    if client.current_tokens().is_none() {
        eprintln!("bluey: not logged in. Run `bluey login` first.");
        return Ok(());
    }
    if let Err(e) = crate::bluey_cmds::portal(&client).await {
        eprintln!("bluey: portal failed: {e}");
    }
    Ok(())
}

async fn bluey_export_cmd() -> Result<()> {
    let client = match cue_cloud_client::CloudClient::with_default_keyring() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("bluey: cloud client init failed: {e}");
            return Ok(());
        }
    };
    if client.current_tokens().is_none() {
        eprintln!("bluey: not logged in. Run `bluey login` first.");
        return Ok(());
    }
    if let Err(e) = crate::bluey_cmds::export_data(&client).await {
        eprintln!("bluey: export failed: {e}");
    }
    Ok(())
}

async fn bluey_delete_account_cmd(force: bool) -> Result<()> {
    let client = match cue_cloud_client::CloudClient::with_default_keyring() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("bluey: cloud client init failed: {e}");
            return Ok(());
        }
    };
    if client.current_tokens().is_none() {
        eprintln!("bluey: not logged in. Run `bluey login` first.");
        return Ok(());
    }
    if let Err(e) = crate::bluey_cmds::delete_account(&client, force).await {
        eprintln!("bluey: delete-account failed: {e}");
    }
    Ok(())
}
