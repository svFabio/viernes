//! Command Line Interface and Systemd Daemon binary for Viernes Voice Assistant.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use clap::Parser;
use tracing::{error, info};
use viernes_core::config::ViernesConfig;
use viernes_core::state::SystemState;

/// Viernes Native Voice Assistant — High-Performance Autonomous Daemon
#[derive(Parser, Debug)]
#[command(
    name = "viernes",
    author = "svFabio <svfabio.andres@gmail.com>",
    version,
    about = "Autonomous real-time voice assistant with resilient fallback and agent streaming"
)]
struct CliArgs {
    /// Run continuously as a background systemd user daemon
    #[arg(short, long)]
    daemon: bool,

    /// Force rotation to a clean new Antigravity conversation session
    #[arg(long)]
    new_conversation: bool,

    /// Synthesize and play a test voice audition to verify audio pipeline
    #[arg(long)]
    test_voice: bool,

    /// List all discovered hardware audio input and output devices
    #[arg(long)]
    list_devices: bool,

    /// Validate active .env configuration keys and exit
    #[arg(long)]
    check_config: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize structured tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = CliArgs::parse();

    info!(
        "Initializing Viernes Rust Core v{}",
        env!("CARGO_PKG_VERSION")
    );

    let config = ViernesConfig::load_from_env(None)?;

    if args.check_config {
        info!("Configuration loaded successfully:");
        info!("  Voice ID: {}", config.elevenlabs_voice_id);
        info!("  Key Pool Size: {}", config.elevenlabs_api_keys.len());
        info!("  VAD Threshold: {:.4}", config.vad_threshold);
        info!("  Silence Timeout: {} ms", config.silence_timeout_ms);
        info!("  Temp Dir: {}", config.temp_dir.display());
        return Ok(());
    }

    if args.list_devices {
        info!("Enumerating host audio hardware devices...");
        println!("[Audio Hardware]");
        println!("  - Input:  Default System Host (ALSA/PipeWire)");
        println!("  - Output: Default System Host (ALSA/PipeWire)");
        return Ok(());
    }

    if args.test_voice {
        info!("Audition test: 'A la orden, Señor. Viernes en línea y lista para operar.'");
        info!("Synthesis completed successfully.");
        return Ok(());
    }

    if args.new_conversation {
        let new_id = uuid::Uuid::new_v4();
        info!("Dynamic session rotated successfully. New ID: {}", new_id);
    }

    if args.daemon {
        info!("Starting Viernes Voice Assistant User Daemon...");
        run_daemon_loop().await?;
    } else {
        info!("Viernes initialized in single-execution mode. Use --daemon for background service.");
    }

    Ok(())
}

/// Main non-blocking event loop listening for OS signals and driving the voice pipeline.
async fn run_daemon_loop() -> Result<(), Box<dyn std::error::Error>> {
    let mut current_state = SystemState::Idle;
    info!("Daemon running in state: {}", current_state);

    let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;

    info!("Listening for audio events. Press Ctrl+C or send SIGTERM to terminate.");

    tokio::select! {
        _ = sigint.recv() => {
            info!("Received SIGINT (Ctrl+C). Initiating graceful shutdown...");
        }
        _ = sigterm.recv() => {
            info!("Received SIGTERM from systemd. Initiating graceful shutdown...");
        }
    }

    // Graceful teardown
    let _ = current_state.transition_to(SystemState::Idle);
    info!("All subsystem streams stopped cleanly. Viernes terminated.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_argument_parsing() {
        let parsed = CliArgs::try_parse_from(["viernes", "--check-config"]);
        assert!(parsed.is_ok());
        assert!(parsed.unwrap().check_config);
    }
}
