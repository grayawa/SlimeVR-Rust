use clap::{Parser, Subcommand};
use slimevr_server::{
    log_level::LogLevel,
    protocol,
    receiver::{normalize_mac, ReceiverConfig},
    recording,
    runtime::{self, ListenOptions},
    solve,
};
use std::{error::Error, net::SocketAddr, path::PathBuf, time::Duration};

#[derive(Parser)]
#[command(
    version,
    about = "SlimeVR Rust UDP receiver and deterministic pose core"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

// Parsed once at startup; keeping named CLI arguments together avoids extra indirection.
#[allow(clippy::large_enum_variant)]
#[derive(Subcommand)]
enum Command {
    #[command(hide = true)]
    FlashWorker {
        #[arg(long)]
        port: String,
        #[arg(long)]
        vid: u16,
        #[arg(long)]
        pid: u16,
        #[arg(long)]
        manifest: PathBuf,
    },
    /// Receive fused poses, emit JSON diagnostics, optionally record a replay journal.
    Listen {
        /// Override server.trackerPort in vrconfig.yml.
        #[arg(long)]
        bind: Option<SocketAddr>,
        /// Explicit device admission; may be repeated. Unknown devices are denied by default.
        #[arg(long="allow-device", value_parser=normalize_mac)]
        allowed_macs: Vec<String>,
        #[arg(long)]
        accept_new_devices: bool,
        #[arg(long)]
        no_discovery: bool,
        /// Overrides automatically detected IPv4 subnet broadcasts.
        #[arg(long = "discovery-target")]
        discovery_targets: Vec<SocketAddr>,
        #[arg(long)]
        record: Option<PathBuf>,
        /// Include high-frequency sample and telemetry events.
        #[arg(long)]
        events: bool,
        /// Minimum live diagnostic severity (or SLIMEVR_LOG_LEVEL; default info).
        #[arg(long, value_enum)]
        log_level: Option<LogLevel>,
        #[arg(long, default_value_t=1000, value_parser=clap::value_parser!(u64).range(1..))]
        summary_ms: u64,
        /// Exit after this many seconds (useful for captures and integration tests).
        #[arg(long, value_parser=clap::value_parser!(u64).range(1..))]
        run_for: Option<u64>,
        /// Run the pose algorithm on every received sample and explicit solve tick.
        #[arg(long)]
        pose_config: Option<PathBuf>,
        #[arg(long,default_value_t=4,value_parser=clap::value_parser!(u64).range(1..=1000))]
        pose_ms: u64,
        #[arg(long,default_value_t=20,value_parser=clap::value_parser!(u64).range(1..=1000))]
        pose_output_ms: u64,
        /// Enable the SolarXR WebSocket API (GUI default: 127.0.0.1:21110).
        #[arg(long)]
        api_bind: Option<SocketAddr>,
        /// SlimeVR YAML configuration; API mode defaults to the original vrconfig.yml location.
        #[arg(long = "config", alias = "state")]
        state: Option<PathBuf>,
        /// Owned desktop child: finalize recordings when the parent's stdin pipe closes.
        #[arg(long)]
        shutdown_on_stdin_eof: bool,
        /// Disable the SteamVR driver bridge (API mode enables it by default).
        #[arg(long)]
        no_steamvr: bool,
        /// Override the SteamVR Unix socket or Windows named pipe endpoint.
        #[arg(long, conflicts_with = "no_steamvr")]
        steamvr_endpoint: Option<PathBuf>,
        /// Optional original SlimeVR bindings provider executable.
        #[arg(long, conflicts_with = "no_bindings_provider")]
        bindings_provider: Option<PathBuf>,
        #[arg(long)]
        no_bindings_provider: bool,
        /// Bundled SlimeVR OpenVR driver directory (auto-detected when omitted).
        #[arg(long)]
        steamvr_driver: Option<PathBuf>,
        /// SteamVR runtime directory (defaults to the OpenVR paths configuration).
        #[arg(long)]
        steamvr_runtime: Option<PathBuf>,
        /// Local SteamVR management HTTP endpoint.
        #[arg(long)]
        steamvr_http: Option<String>,
        #[arg(long)]
        no_driver_install: bool,
        #[arg(long)]
        no_steamvr_restart: bool,
        /// Additional serial ports (including adapters not recognized by USB IDs).
        #[arg(long = "serial-port")]
        serial_ports: Vec<String>,
    },
    /// Replay locally and check exact receiver reply bytes; never opens a socket.
    Replay {
        file: PathBuf,
        #[arg(long)]
        events: bool,
    },
    /// Decode one hexadecimal UDP datagram without registering a device.
    Decode { hex: String },
    /// Solve a server-space algorithm scene with explicit samples, head poses, resets and ticks.
    Solve {
        file: PathBuf,
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        frames: bool,
    },
    /// Fit bone lengths from a calibrated algorithm scene containing HMD positions.
    #[command(name = "autobone", alias = "auto-bone")]
    AutoBone {
        file: PathBuf,
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        settings: Option<PathBuf>,
        #[arg(long)]
        target_height: Option<f32>,
        /// Save an accepted result as a new complete pose configuration.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Solve an existing UDP journal; preserve every recorded sample and tick, verify replies.
    SolveRecording {
        file: PathBuf,
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        frames: bool,
    },
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(error) = run().await {
        slimevr_server::logging::diagnostic(
            LogLevel::Error,
            &serde_json::json!({"type":"backend_fatal_error", "message":error.to_string()}),
        );
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn Error>> {
    match Cli::parse().command {
        Command::FlashWorker {
            port,
            vid,
            pid,
            manifest,
        } => slimevr_server::firmware::flash_worker(&port, vid, pid, &manifest)?,
        Command::Listen {
            bind,
            allowed_macs,
            accept_new_devices,
            no_discovery,
            discovery_targets,
            record,
            events,
            summary_ms,
            log_level,
            run_for,
            pose_config,
            pose_ms,
            pose_output_ms,
            api_bind,
            state,
            shutdown_on_stdin_eof,
            no_steamvr,
            steamvr_endpoint,
            bindings_provider,
            no_bindings_provider,
            steamvr_driver,
            steamvr_runtime,
            steamvr_http,
            no_driver_install,
            no_steamvr_restart,
            serial_ports,
        } => {
            runtime::listen(ListenOptions {
                bind: bind.unwrap_or_else(|| "0.0.0.0:6969".parse().unwrap()),
                bind_explicit: bind.is_some(),
                config: ReceiverConfig {
                    allowed_macs,
                    accept_new_devices,
                    ..Default::default()
                },
                record,
                log_level: LogLevel::resolve(log_level, events)?,
                summary_ms,
                run_for: run_for.map(Duration::from_secs),
                discovery: !no_discovery,
                discovery_targets,
                pose_config: pose_config.as_deref().map(solve::load_config).transpose()?,
                pose_ms,
                pose_output_ms,
                api_bind,
                state,
                shutdown_on_stdin_eof,
                no_steamvr,
                steamvr_endpoint,
                bindings_provider,
                no_bindings_provider,
                steamvr_driver,
                steamvr_runtime,
                steamvr_http,
                no_driver_install,
                no_steamvr_restart,
                serial_ports,
            })
            .await?;
        }
        Command::Replay { file, events } => {
            // Propagate stdout failures instead of panicking inside the replay callback.
            let mut output_error = None;
            let result = recording::replay(&file, |e| {
                if output_error.is_none() && (events || !e.kind.is_noisy()) {
                    if let Err(error) = runtime::print_json(e) {
                        output_error = Some(error);
                    }
                }
            })?;
            if let Some(error) = output_error {
                return Err(error);
            }
            runtime::print_json(&result.receiver.snapshot(result.at_ms))?;
            runtime::print_json(
                &serde_json::json!({"type":"replay_complete", "records":result.records, "verified_replies":result.verified_replies}),
            )?;
        }
        Command::Decode { hex } => {
            runtime::print_json(&protocol::parse(&recording::decode_hex(&hex)?)?)?;
        }
        Command::Solve {
            file,
            config,
            frames,
        } => solve::scene(&file, &config, frames)?,
        Command::AutoBone {
            file,
            config,
            settings,
            target_height,
            output,
        } => solve::autobone(
            &file,
            &config,
            settings.as_deref(),
            target_height,
            output.as_deref(),
        )?,
        Command::SolveRecording {
            file,
            config,
            frames,
        } => solve::journal(&file, &config, frames)?,
    }
    Ok(())
}
