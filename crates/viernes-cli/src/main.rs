//! Command Line Interface and Systemd Daemon binary for Viernes Voice Assistant.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod tui;

use std::sync::Arc;

use clap::Parser;
use tracing::{debug, info, warn};
use viernes_audio::capture::{AudioCaptureConfig, CpalAudioCapture};
use viernes_audio::ducking::AudioDucker;
use viernes_audio::ptt::{IpcSocketPttListener, PttEvent, PttListener};
use viernes_audio::stt::{LocalWhisperTranscriber, WhisperConfig};
use viernes_audio::vad::{VadEvent, VadState, VoiceActivityDetector};
use viernes_brain::client::{AntigravityClient, BrainEvent};
use viernes_brain::confirmation::{ConfirmationManager, RiskClassifier};
use viernes_brain::fillers::FillerCacheManager;
use viernes_brain::session::SessionPointerManager;
use viernes_brain::tokenizer::SentenceTokenizer;
use viernes_core::config::ViernesConfig;
use viernes_core::events::SystemEvent;
use viernes_core::state::{DaemonLiveStatus, SystemState};
use viernes_core::state_machine::StateMachine;
use viernes_core::traits::{AudioCapture, AudioPlayback, SpeechToText};
use viernes_tts::player::StreamingAudioPlayer;
use viernes_tts::resilient::ResilientTtsEngine;

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

    /// Continuously listen on microphone with VAD (default: Push-to-Talk only)
    #[arg(short, long)]
    continuous: bool,

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

    /// Run synthetic workload benchmark for PGO training and performance analysis
    #[arg(long)]
    benchmark: bool,

    /// Send Push-to-Talk IPC trigger to running daemon (e.g. "press", "release", "cancel", "reload")
    #[arg(long, value_name = "COMMAND")]
    ptt: Option<String>,

    /// Launch interactive terminal user interface (TUI) dashboard
    #[arg(long)]
    tui: bool,
}

/// Silences verbose C-level stderr output (such as ALSA/libasound diagnostic messages)
/// by redirecting fd 2 to a private log file during interactive TUI execution,
/// preserving Rust backtraces and journald diagnostics in daemon/CLI mode.
#[cfg(target_os = "linux")]
fn silence_c_stderr() {
    use std::os::unix::fs::OpenOptionsExt;

    let alsa_log_path = viernes_core::default_alsa_log_path();
    let log_file = (|| -> std::io::Result<std::fs::File> {
        if let Some(parent) = alsa_log_path.parent() {
            if !parent.as_os_str().is_empty() {
                viernes_core::ensure_private_dir(parent)?;
            }
        }
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(&alsa_log_path)
    })()
    .or_else(|_| std::fs::File::create("/dev/null"));

    if let Ok(file) = log_file {
        use std::os::unix::io::AsRawFd;
        let _ = nix::unistd::dup2(file.as_raw_fd(), 2);
    }
}

#[cfg(not(target_os = "linux"))]
fn silence_c_stderr() {}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = CliArgs::parse();

    // Fast-path: Dispatch PTT IPC command directly without initializing heavy subsystems
    if let Some(ref cmd) = args.ptt {
        return send_ptt_command(cmd).await;
    }

    // Launch interactive TUI (silencing verbose C-level stderr to avoid corrupting the screen)
    if args.tui {
        silence_c_stderr();
        let config = ViernesConfig::load_from_env(None).unwrap_or_default();
        return tui::run_tui(config).await;
    }

    // Initialize structured tracing to stdout
    tracing_subscriber::fmt()
        .with_writer(std::io::stdout)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!(
        "Initializing Viernes Rust Core v{}",
        env!("CARGO_PKG_VERSION")
    );

    let config = ViernesConfig::load_from_env(None)?;

    if args.check_config {
        info!("Configuration loaded successfully:");
        info!("  Voice ID: {}", config.elevenlabs_voice_id);
        info!(
            "  Model ID: {}",
            config.elevenlabs_model_id.as_deref().unwrap_or("default")
        );
        info!("  Key Pool Size: {}", config.elevenlabs_api_keys.len());
        info!("  VAD Threshold: {:.4}", config.vad_threshold);
        info!("  Silence Timeout: {} ms", config.silence_timeout_ms);
        info!("  Temp Dir: {}", config.temp_dir.display());
        return Ok(());
    }

    if args.list_devices {
        info!("Enumerating host audio hardware devices...");
        use viernes_audio::devices::{CpalDeviceEnumerator, DeviceEnumerator};

        let enumerator = CpalDeviceEnumerator::new();

        println!("[Audio Hardware — Input Devices (Microphones)]");
        match enumerator.list_input_devices() {
            Ok(inputs) if inputs.is_empty() => {
                println!("  (No audio input devices discovered)");
            }
            Ok(inputs) => {
                for dev in inputs {
                    println!("  {dev}");
                }
            }
            Err(e) => {
                println!("  Error querying input devices: {e}");
            }
        }

        println!("\n[Audio Hardware — Output Devices (Speakers/Headphones)]");
        match enumerator.list_output_devices() {
            Ok(outputs) if outputs.is_empty() => {
                println!("  (No audio output devices discovered)");
            }
            Ok(outputs) => {
                for dev in outputs {
                    println!("  {dev}");
                }
            }
            Err(e) => {
                println!("  Error querying output devices: {e}");
            }
        }
        return Ok(());
    }

    if args.test_voice {
        let test_phrase = "A la orden, Señor. Viernes en línea y lista para operar.";
        info!("Audition test: '{}'", test_phrase);
        let tts = ResilientTtsEngine::from_env();
        let initial_volume = (config.viernes_volume_percent as f32) / 100.0;
        let mut player = StreamingAudioPlayer::create_resilient_player_with_options(
            None,
            initial_volume,
            config.audio_output_device.as_deref(),
        );

        info!("Synthesizing test audio via TTS engine...");
        let audio_bytes = tts.synthesize_resilient(test_phrase).await?;
        info!("Playing audio ({} bytes)...", audio_bytes.len());
        player.enqueue_chunk(audio_bytes).await?;

        // Allow playback thread to receive and start playback
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        while player.is_playing() {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        info!("Synthesis and playback completed successfully.");
        return Ok(());
    }

    if args.new_conversation {
        let mgr = SessionPointerManager::new(None::<&str>)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        let new_id = mgr.rotate_session().await?;
        info!("Dynamic session rotated successfully. New ID: {}", new_id);
        println!("Viernes conversation session rotated to: {}", new_id);
        let socket_path = resolve_ptt_socket_path();
        if let Err(err) = send_ipc_ptt(&socket_path, "reload").await {
            if err.kind() != std::io::ErrorKind::NotFound
                && err.kind() != std::io::ErrorKind::ConnectionRefused
            {
                warn!("Failed to notify running daemon to reload session: {err}");
            }
        }
        return Ok(());
    }

    if args.benchmark {
        info!("Running representative PGO training benchmark...");
        run_benchmark().await?;
        return Ok(());
    }

    if args.daemon {
        info!("Starting Viernes Voice Assistant User Daemon...");
        run_daemon_loop(&config, args.continuous).await?;
    } else {
        info!("Viernes initialized in single-execution mode. Use --daemon for background service.");
    }

    Ok(())
}

/// Coordinates monotonic turn epochs and race-free asynchronous cancellation notifications.
#[derive(Clone)]
struct TurnCoordinator {
    epoch: Arc<std::sync::atomic::AtomicU64>,
    cancel_notify: Arc<tokio::sync::Notify>,
    epoch_tx: tokio::sync::watch::Sender<u64>,
}

impl TurnCoordinator {
    fn new() -> Self {
        let (epoch_tx, _) = tokio::sync::watch::channel(0u64);
        Self {
            epoch: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            cancel_notify: Arc::new(tokio::sync::Notify::new()),
            epoch_tx,
        }
    }

    /// Increments the turn epoch, notifies all active waiters, and returns the new epoch.
    fn bump(&self) -> u64 {
        let next = self.epoch.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        let _ = self.epoch_tx.send(next);
        self.cancel_notify.notify_waiters();
        next
    }

    fn current_epoch(&self) -> u64 {
        self.epoch.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn is_current(&self, turn_id: u64) -> bool {
        self.current_epoch() == turn_id
    }

    /// Resolves immediately if `epoch` has already advanced past `turn_id`,
    /// or as soon as `bump()` advances it.
    async fn wait_cancelled(&self, turn_id: u64) {
        let mut rx = self.epoch_tx.subscribe();
        if *rx.borrow_and_update() != turn_id {
            return;
        }
        tokio::select! {
            _ = rx.wait_for(|&v| v != turn_id) => {}
            _ = async {
                loop {
                    self.cancel_notify.notified().await;
                    if self.current_epoch() != turn_id {
                        break;
                    }
                }
            } => {}
        }
    }
}

/// Aborts all tracked per-turn spawned subtasks when dropped or explicitly cleared.
struct TaskAbortGuard(Vec<tokio::task::JoinHandle<()>>);

impl TaskAbortGuard {
    fn new() -> Self {
        Self(Vec::new())
    }

    fn push(&mut self, handle: tokio::task::JoinHandle<()>) {
        self.0.retain(|h| !h.is_finished());
        self.0.push(handle);
    }

    fn abort_all(&mut self) {
        for handle in self.0.drain(..) {
            handle.abort();
        }
    }
}

impl Drop for TaskAbortGuard {
    fn drop(&mut self) {
        self.abort_all();
    }
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

async fn update_and_persist_status<F>(status: &tokio::sync::Mutex<DaemonLiveStatus>, mutate: F)
where
    F: FnOnce(&mut DaemonLiveStatus),
{
    let snapshot = {
        let mut st = status.lock().await;
        mutate(&mut st);
        st.timestamp_ms = now_unix_ms();
        st.clone()
    };
    let _ = snapshot.write_to_disk();
}

fn spawn_restore_ducking(ducker: &Arc<AudioDucker>) {
    if ducker.is_ducked() {
        let ducker_clone = ducker.clone();
        tokio::spawn(async move {
            ducker_clone.restore().await;
        });
    }
}

fn build_agy_args(session_id: Option<&str>) -> Vec<String> {
    let agy_model =
        std::env::var("VIERNES_AGY_MODEL").unwrap_or_else(|_| "gemini-3.8-flash-low".to_string());
    let mut agy_args = vec![
        "--model".to_string(),
        agy_model,
        "--effort".to_string(),
        "low".to_string(),
        "--dangerously-skip-permissions".to_string(),
        "--disable-slash-commands".to_string(),
        "--input-format".to_string(),
        "stream-json".to_string(),
        "--output-format".to_string(),
        "stream-json".to_string(),
    ];
    if let Some(id) = session_id {
        let trimmed = id.trim();
        if !trimmed.is_empty() {
            agy_args.push("--conversation".to_string());
            agy_args.push(trimmed.to_string());
        }
    }
    agy_args
}

fn spawn_antigravity_client(
    session_id: Option<&str>,
) -> Result<AntigravityClient, viernes_brain::error::BrainError> {
    let agy_bin = std::env::var("VIERNES_AGY_BIN").unwrap_or_else(|_| "agy".to_string());
    let args = build_agy_args(session_id);
    AntigravityClient::spawn(&agy_bin, &args)
}

/// Transcribes a spoken confirmation response, classifies affirmative/negative intent,
/// resolves the pending confirmation oneshot, and dispatches `ConfirmationReceived`.
async fn route_confirmation_response<S: SpeechToText + ?Sized>(
    pcm: &[f32],
    stt: &S,
    pending_confirmation: &tokio::sync::Mutex<Option<tokio::sync::oneshot::Sender<bool>>>,
    sm: &StateMachine,
) -> bool {
    let prompt = match stt.transcribe(pcm, 16_000).await {
        Ok(text) => text.trim().to_string(),
        Err(e) => {
            warn!("Confirmation STT transcription error: {e}");
            String::new()
        }
    };
    let confirmed = RiskClassifier::default()
        .parse_confirmation_response(&prompt)
        .unwrap_or(false);
    info!(
        transcript = %prompt,
        confirmed,
        "Processed voice confirmation response"
    );

    let tx_opt = {
        let mut guard = pending_confirmation.lock().await;
        guard.take()
    };
    if let Some(tx) = tx_opt {
        let _ = tx.send(confirmed);
    }
    let _ = sm
        .dispatch(SystemEvent::ConfirmationReceived { confirmed })
        .await;
    confirmed
}

/// Main non-blocking event loop listening for OS signals and driving the voice pipeline.
async fn run_daemon_loop(
    config: &ViernesConfig,
    continuous: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut active_config = config.clone();
    let state_machine = Arc::new(StateMachine::new(256));
    let mut transition_rx = state_machine.subscribe_transitions();
    info!(
        "Daemon initialized in state: {} (mode: {})",
        state_machine.state().await,
        if continuous {
            "Continuous VAD"
        } else {
            "Push-to-Talk"
        }
    );

    // 1. Audio Capture & VAD setup
    let capture_config = AudioCaptureConfig {
        device_query: active_config.audio_input_device.clone(),
        preferred_sample_rate: 16_000,
        chunk_ms: 20,
        channel_capacity: 128,
    };
    let mut capture = CpalAudioCapture::new(capture_config);
    capture.set_recording_active(false);
    let audio_rx = capture.receiver();
    let audio_active = capture.start_stream().await.is_ok();
    if audio_active {
        info!("Real-time audio capture hardware stream active.");
    } else {
        warn!("Audio hardware capture stream unavailable (running in event-driven headless mode).");
    }

    let mut vad = VoiceActivityDetector::new(
        16_000,
        active_config.vad_threshold,
        active_config.silence_timeout_ms,
    );

    // 2. Audio Playback, Resident STT, TTS & Orphaned Ducking Recovery setup
    let event_tx = state_machine.event_sender();
    let initial_volume = (active_config.viernes_volume_percent as f32) / 100.0;
    let player: Arc<tokio::sync::Mutex<Box<dyn AudioPlayback>>> = Arc::new(
        tokio::sync::Mutex::new(StreamingAudioPlayer::create_resilient_player_with_options(
            Some(event_tx),
            initial_volume,
            active_config.audio_output_device.as_deref(),
        )),
    );
    let tts_engine = Arc::new(tokio::sync::RwLock::new(Arc::new(
        ResilientTtsEngine::from_env(),
    )));
    let transcriber = Arc::new(
        LocalWhisperTranscriber::new(WhisperConfig::default())
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?,
    );
    let ducker = Arc::new(AudioDucker::from_config(&active_config));
    let recovered_ducking = ducker.recover_orphaned_ducking().await;
    if recovered_ducking > 0 {
        info!(
            recovered_ducking,
            "Recovered orphaned media stream ducking from previous session"
        );
    }

    // 3. Fillers Manager & Pre-warming (in-memory audio cache)
    let fillers_mgr = Arc::new(
        FillerCacheManager::new(None::<&str>)
            .or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
                let dynamic_path =
                    std::path::PathBuf::from(home).join(".config/viernes/fillers.json");
                FillerCacheManager::new(Some(dynamic_path))
            })
            .or_else(|_| FillerCacheManager::new(Some("fillers.json")))
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?,
    );
    let home_dir = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let fillers_cache_dir =
        std::path::PathBuf::from(home_dir).join(".config/viernes/fillers_cache");
    let loaded_fillers = fillers_mgr.prewarm_audio_cache(&fillers_cache_dir).await;
    info!(
        loaded_fillers,
        "Conversational Fillers engine active with in-memory RAM audio cache"
    );

    // 4. Session Pointer Manager & Resident Antigravity Brain Client
    let initial_session_mgr = SessionPointerManager::new(None::<&str>)
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
    let active_session_id = initial_session_mgr.current_session_id().await;
    info!("Active conversation session ID: {}", active_session_id);
    let session_mgr = Arc::new(tokio::sync::RwLock::new(Arc::new(initial_session_mgr)));

    let agy_bin = std::env::var("VIERNES_AGY_BIN").unwrap_or_else(|_| "agy".to_string());
    let initial_sid = (!active_session_id.is_empty()).then_some(active_session_id.as_str());
    let brain_client: Arc<tokio::sync::Mutex<Option<AntigravityClient>>> =
        match spawn_antigravity_client(initial_sid) {
            Ok(client) => {
                info!("Resident Antigravity brain spawned successfully ({agy_bin})");
                Arc::new(tokio::sync::Mutex::new(Some(client)))
            }
            Err(e) => {
                warn!("Resident Antigravity client unavailable ({agy_bin}): {e}");
                Arc::new(tokio::sync::Mutex::new(None))
            }
        };

    // 5. Push-to-Talk IPC Server
    let mut ptt_listener = IpcSocketPttListener::from_env();
    let ptt_rx = ptt_listener.event_receiver();
    if let Err(e) = ptt_listener.start().await {
        warn!("PTT IPC server failed to start: {e}");
    }

    // 6. Live Status Tracker for CLI & TUI
    let live_status = Arc::new(tokio::sync::Mutex::new(DaemonLiveStatus {
        state: SystemState::Idle,
        last_user_prompt: String::new(),
        last_assistant_text: String::new(),
        turn_counter: 0,
        timestamp_ms: now_unix_ms(),
    }));
    {
        let initial_snapshot = live_status.lock().await.clone();
        let _ = initial_snapshot.write_to_disk();
    }

    // 7. Signal Handlers
    let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;

    info!("Viernes Reactive Voice Pipeline listening. Press Ctrl+C or send SIGTERM to terminate.");

    // 8. Background transition logger and live status sync task
    let status_tracker = live_status.clone();
    let ducker_transition = ducker.clone();
    let transition_handle = tokio::spawn(async move {
        while let Ok(record) = transition_rx.recv().await {
            info!(
                "Lifecycle Transition: {} -> {} (trigger: {:?})",
                record.from_state, record.to_state, record.trigger_event
            );
            let snapshot = {
                let mut st = status_tracker.lock().await;
                st.state = record.to_state;
                st.timestamp_ms = now_unix_ms();
                st.clone()
            };
            let _ = snapshot.write_to_disk();

            if record.to_state == SystemState::Idle || record.to_state == SystemState::Listening {
                ducker_transition.restore().await;
            }
        }
    });

    // 8b. Background audio playback event listener for ducking restoration
    let mut playback_event_rx = state_machine.subscribe_events();
    let ducker_events = ducker.clone();
    let player_events = player.clone();
    let playback_events_handle = tokio::spawn(async move {
        while let Ok(event) = playback_event_rx.recv().await {
            match event {
                SystemEvent::FillerPlaybackFinished => {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    let is_playing = if let Ok(p) = player_events.try_lock() {
                        p.is_playing()
                    } else {
                        false
                    };
                    if !is_playing {
                        debug!(
                            "Filler finished and response not yet audible; restoring media volume"
                        );
                        ducker_events.restore().await;
                    }
                }
                SystemEvent::ResetToIdle | SystemEvent::BargeIn => {
                    ducker_events.restore().await;
                }
                _ => {}
            }
        }
    });

    // 9. Temporary Storage Manager & Initial Stale Audio Cleanup
    let temp_storage = Arc::new(viernes_tts::TempStorageManager::new(
        active_config.temp_dir.clone(),
    ));
    if let Err(e) = temp_storage.purge_excess_and_expired() {
        warn!("Initial temp storage cleanup failed: {e}");
    }

    // 10. Turn Coordinator, Confirmation Channel & Reload Lock
    let turn_coord = TurnCoordinator::new();
    let pending_confirmation: Arc<tokio::sync::Mutex<Option<tokio::sync::oneshot::Sender<bool>>>> =
        Arc::new(tokio::sync::Mutex::new(None));
    let reload_lock = Arc::new(tokio::sync::Mutex::new(()));
    let mut active_turn_handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();

    // 11. Main event processing loop with Acoustic Echo Suppression & Continuous Barge-in
    let mut chunk_interval = tokio::time::interval(std::time::Duration::from_millis(20));
    let mut ptt_active = false;
    let mut ptt_accumulated: Vec<f32> = Vec::new();
    let mut playback_cooldown_until = std::time::Instant::now();
    let mut barge_in_consecutive_frames = 0;

    loop {
        active_turn_handles.retain(|h| !h.is_finished());

        tokio::select! {
            _ = sigint.recv() => {
                info!("Received SIGINT (Ctrl+C). Initiating graceful shutdown...");
                break;
            }
            _ = sigterm.recv() => {
                info!("Received SIGTERM from systemd. Initiating graceful shutdown...");
                break;
            }
            _ = chunk_interval.tick() => {
                while let Ok(ptt_event) = ptt_rx.try_recv() {
                    match ptt_event {
                        PttEvent::Pressed => {
                            info!("Push-to-Talk IPC event: Pressed");
                            ptt_active = true;
                            ptt_accumulated.clear();
                            let awaiting_confirm =
                                state_machine.state().await == SystemState::AwaitingConfirmation;
                            if !awaiting_confirm {
                                turn_coord.bump();
                                let _ = state_machine.dispatch(SystemEvent::PttPressed).await;
                            }
                            let interrupted_playback = {
                                let mut p = player.lock().await;
                                if p.is_playing() {
                                    let _ = p.stop_immediately();
                                    true
                                } else {
                                    false
                                }
                            };
                            if interrupted_playback {
                                info!("Barge-in: interrupted active playback via PTT.");
                            }
                            if interrupted_playback || ducker.is_ducked() {
                                spawn_restore_ducking(&ducker);
                            }
                            vad.set_silence_timeout_enabled(false);
                            vad.start_speech_capture();
                        }
                        PttEvent::Released => {
                            info!("Push-to-Talk IPC event: Released");
                            ptt_active = false;
                            let awaiting_confirm =
                                state_machine.state().await == SystemState::AwaitingConfirmation;
                            if !awaiting_confirm {
                                let _ = state_machine.dispatch(SystemEvent::PttReleased).await;
                            }
                            vad.set_silence_timeout_enabled(continuous);
                            let pcm_buffer = vad
                                .force_finish()
                                .unwrap_or_else(|| std::mem::take(&mut ptt_accumulated));
                            ptt_accumulated.clear();

                            // Discard micro-clicks (< 2400 samples / 150ms @ 16kHz)
                            if pcm_buffer.len() < 2400 {
                                info!(
                                    "PTT audio too short ({} samples < 2400 / 150ms); discarding micro-click",
                                    pcm_buffer.len()
                                );
                                if !awaiting_confirm {
                                    let _ = state_machine.dispatch(SystemEvent::ResetToIdle).await;
                                    spawn_restore_ducking(&ducker);
                                }
                                continue;
                            }

                            if awaiting_confirm {
                                info!(
                                    "PTT confirmation speech finished ({} samples)",
                                    pcm_buffer.len()
                                );
                                let stt_c = transcriber.clone();
                                let pending_c = pending_confirmation.clone();
                                let sm_c = state_machine.clone();
                                tokio::spawn(async move {
                                    route_confirmation_response(&pcm_buffer, &*stt_c, &pending_c, &sm_c)
                                        .await;
                                });
                                continue;
                            }

                            info!(
                                "PTT speech finished instantly ({} samples)",
                                pcm_buffer.len()
                            );
                            let _ = state_machine
                                .dispatch(SystemEvent::VoiceFinished {
                                    samples: Vec::new(),
                                    sample_rate: 16_000,
                                })
                                .await;

                            let turn_id = turn_coord.bump();
                            let ctx = SpeechTurnContext {
                                sm: state_machine.clone(),
                                stt: transcriber.clone(),
                                tts: tts_engine.clone(),
                                player: player.clone(),
                                ducker: ducker.clone(),
                                fillers: fillers_mgr.clone(),
                                brain: brain_client.clone(),
                                status: live_status.clone(),
                                session_mgr: session_mgr.clone(),
                                turn_coord: turn_coord.clone(),
                                pending_confirmation: pending_confirmation.clone(),
                                turn_id,
                            };
                            active_turn_handles.push(tokio::spawn(async move {
                                process_speech_turn(pcm_buffer, ctx).await;
                            }));
                        }
                        PttEvent::Cancel => {
                            info!("Push-to-Talk IPC event: Cancel");
                            ptt_active = false;
                            ptt_accumulated.clear();
                            barge_in_consecutive_frames = 0;
                            if let Some(tx) = pending_confirmation.lock().await.take() {
                                let _ = tx.send(false);
                            }
                            turn_coord.bump();
                            {
                                let mut p = player.lock().await;
                                if p.is_playing() {
                                    let _ = p.stop_immediately();
                                }
                            }
                            vad.reset();
                            vad.set_silence_timeout_enabled(continuous);
                            spawn_restore_ducking(&ducker);
                            let _ = state_machine.dispatch(SystemEvent::ResetToIdle).await;
                        }
                        PttEvent::Reload => {
                            info!("Push-to-Talk IPC event: Reload");
                            match ViernesConfig::load_from_env(None) {
                                Ok(new_cfg) => {
                                    ducker.set_enabled(new_cfg.audio_ducking_enabled);
                                    ducker.set_target_volume_percent(
                                        new_cfg.audio_ducking_volume_percent,
                                    );
                                    if !ptt_active && vad.state() == VadState::Silence {
                                        let timeout_enabled = vad.is_silence_timeout_enabled();
                                        vad = VoiceActivityDetector::new(
                                            16_000,
                                            new_cfg.vad_threshold,
                                            new_cfg.silence_timeout_ms,
                                        );
                                        vad.set_silence_timeout_enabled(timeout_enabled);
                                    } else {
                                        vad.set_threshold(new_cfg.vad_threshold);
                                    }
                                    active_config = new_cfg;
                                }
                                Err(e) => {
                                    warn!("Failed to reload .env configuration: {e}");
                                }
                            }

                            let new_vol = (active_config.viernes_volume_percent as f32) / 100.0;
                            let fillers_r = fillers_mgr.clone();
                            let cache_dir_r = fillers_cache_dir.clone();
                            let tts_r = tts_engine.clone();
                            let player_r = player.clone();
                            let session_r = session_mgr.clone();
                            let brain_r = brain_client.clone();
                            let turn_r = turn_coord.clone();
                            let reload_lock_r = reload_lock.clone();

                            tokio::spawn(async move {
                                let _guard = reload_lock_r.lock().await;
                                if let Err(e) = fillers_r.reload().await {
                                    warn!("Failed to reload conversational fillers: {e}");
                                }
                                let loaded = fillers_r.prewarm_audio_cache(&cache_dir_r).await;
                                info!(loaded, "Reloaded conversational fillers audio cache");

                                let fresh_tts = Arc::new(ResilientTtsEngine::from_env());
                                *tts_r.write().await = fresh_tts;

                                {
                                    let mut p = player_r.lock().await;
                                    let _ = p.set_volume(new_vol);
                                }

                                match SessionPointerManager::new(None::<&str>) {
                                    Ok(disk_mgr) => {
                                        let disk_id = disk_mgr.current_session_id().await;
                                        let current_id = {
                                            let mgr = session_r.read().await;
                                            mgr.current_session_id().await
                                        };
                                        if disk_id != current_id {
                                            info!(
                                                old_session = %current_id,
                                                new_session = %disk_id,
                                                "Session pointer changed on disk; respawning Antigravity brain"
                                            );
                                            turn_r.bump();
                                            *session_r.write().await = Arc::new(disk_mgr);
                                            let mut brain_guard = brain_r.lock().await;
                                            if let Some(ref mut client) = *brain_guard {
                                                let _ = client.terminate().await;
                                            }
                                            let sid = (!disk_id.is_empty()).then_some(disk_id.as_str());
                                            *brain_guard = spawn_antigravity_client(sid).ok();
                                        }
                                    }
                                    Err(e) => {
                                        warn!("Failed to reload session.json from disk: {e}");
                                    }
                                }
                            });
                        }
                    }
                }

                while let Ok(chunk) = audio_rx.try_recv() {
                    let is_playing = if let Ok(p) = player.try_lock() {
                        p.is_playing()
                    } else {
                        true
                    };

                    if is_playing {
                        // Acoustic Barge-in: Monitor microphone for speech interrupting the speaker
                        let rms = viernes_audio::vad::compute_rms(&chunk);
                        let barge_in_threshold = (active_config.vad_threshold * 1.8).max(0.04);

                        if continuous && rms >= barge_in_threshold {
                            barge_in_consecutive_frames += 1;
                            if barge_in_consecutive_frames >= 2 {
                                info!(
                                    "Acoustic Barge-in triggered (RMS {:.4} >= {:.4}); interrupting assistant playback",
                                    rms, barge_in_threshold
                                );
                                barge_in_consecutive_frames = 0;
                                let awaiting_confirm = state_machine.state().await
                                    == SystemState::AwaitingConfirmation;
                                if !awaiting_confirm {
                                    turn_coord.bump();
                                }
                                if let Ok(mut p) = player.try_lock() {
                                    if p.is_playing() {
                                        let _ = p.stop_immediately();
                                    }
                                }
                                spawn_restore_ducking(&ducker);
                                if !awaiting_confirm {
                                    let _ = state_machine.dispatch(SystemEvent::BargeIn).await;
                                }
                                vad.start_speech_capture();
                                playback_cooldown_until = std::time::Instant::now() + std::time::Duration::from_millis(150);
                            }
                        } else {
                            barge_in_consecutive_frames = 0;
                            // Keep VAD pre-roll clean from speaker playback while below barge-in threshold
                            vad.reset();
                        }
                        continue;
                    }

                    barge_in_consecutive_frames = 0;

                    if std::time::Instant::now() < playback_cooldown_until {
                        // Reverberation grace window: discard room echo
                        vad.reset();
                        continue;
                    }

                    // In PTT mode, only accumulate audio into VAD while user is holding PTT
                    if !continuous && !ptt_active {
                        vad.reset();
                        continue;
                    }

                    if !continuous && ptt_active {
                        ptt_accumulated.extend_from_slice(&chunk);
                    }

                    let vad_events = vad.feed_chunk(&chunk);
                    for event in vad_events {
                        match event {
                            VadEvent::VoiceStarted => {
                                info!("VAD detected speech onset (VoiceStarted)");
                                let awaiting_confirm = state_machine.state().await
                                    == SystemState::AwaitingConfirmation;
                                if !awaiting_confirm {
                                    turn_coord.bump();
                                    let _ = state_machine.dispatch(SystemEvent::VoiceStarted).await;
                                }
                                let interrupted = if let Ok(mut p) = player.try_lock() {
                                    if p.is_playing() {
                                        let _ = p.stop_immediately();
                                        true
                                    } else {
                                        false
                                    }
                                } else {
                                    false
                                };
                                if interrupted {
                                    info!("Barge-in: interrupted active playback.");
                                    spawn_restore_ducking(&ducker);
                                }
                            }
                            VadEvent::VoiceFinished(pcm_buffer) => {
                                if continuous {
                                    let awaiting_confirm = state_machine.state().await
                                        == SystemState::AwaitingConfirmation;
                                    // Discard micro-clicks (< 2400 samples / 150ms @ 16kHz)
                                    if pcm_buffer.len() < 2400 {
                                        info!(
                                            "Continuous speech segment too short ({} samples < 2400 / 150ms); discarding",
                                            pcm_buffer.len()
                                        );
                                        if !awaiting_confirm {
                                            let _ = state_machine
                                                .dispatch(SystemEvent::ResetToIdle)
                                                .await;
                                            spawn_restore_ducking(&ducker);
                                        }
                                        continue;
                                    }

                                    if awaiting_confirm {
                                        info!(
                                            "VAD confirmation speech finished ({} samples)",
                                            pcm_buffer.len()
                                        );
                                        let stt_c = transcriber.clone();
                                        let pending_c = pending_confirmation.clone();
                                        let sm_c = state_machine.clone();
                                        tokio::spawn(async move {
                                            route_confirmation_response(
                                                &pcm_buffer,
                                                &*stt_c,
                                                &pending_c,
                                                &sm_c,
                                            )
                                            .await;
                                        });
                                        continue;
                                    }

                                    info!(
                                        "VAD detected speech completion (VoiceFinished, {} samples)",
                                        pcm_buffer.len()
                                    );
                                    let _ = state_machine
                                        .dispatch(SystemEvent::VoiceFinished {
                                            samples: Vec::new(),
                                            sample_rate: 16_000,
                                        })
                                        .await;

                                    let turn_id = turn_coord.bump();
                                    let ctx = SpeechTurnContext {
                                        sm: state_machine.clone(),
                                        stt: transcriber.clone(),
                                        tts: tts_engine.clone(),
                                        player: player.clone(),
                                        ducker: ducker.clone(),
                                        fillers: fillers_mgr.clone(),
                                        brain: brain_client.clone(),
                                        status: live_status.clone(),
                                        session_mgr: session_mgr.clone(),
                                        turn_coord: turn_coord.clone(),
                                        pending_confirmation: pending_confirmation.clone(),
                                        turn_id,
                                    };
                                    active_turn_handles.push(tokio::spawn(async move {
                                        process_speech_turn(pcm_buffer, ctx).await;
                                    }));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    turn_coord.bump();
    if let Some(tx) = pending_confirmation.lock().await.take() {
        let _ = tx.send(false);
    }
    for handle in active_turn_handles.drain(..) {
        handle.abort();
    }
    if audio_active {
        let _ = capture.stop_stream().await;
    }
    let _ = ptt_listener.stop().await;
    {
        let mut brain_guard = brain_client.lock().await;
        if let Some(ref mut client) = *brain_guard {
            let _ = client.terminate().await;
        }
        *brain_guard = None;
    }
    transition_handle.abort();
    playback_events_handle.abort();
    let _ = temp_storage.clean_all();
    ducker.restore().await;
    let _ = state_machine.dispatch(SystemEvent::ResetToIdle).await;
    info!("All subsystem streams stopped cleanly. Viernes terminated.");
    Ok(())
}

/// Context and shared handles bundled for an asynchronous speech turn execution.
struct SpeechTurnContext {
    sm: Arc<StateMachine>,
    stt: Arc<LocalWhisperTranscriber>,
    tts: Arc<tokio::sync::RwLock<Arc<ResilientTtsEngine>>>,
    player: Arc<tokio::sync::Mutex<Box<dyn AudioPlayback>>>,
    ducker: Arc<AudioDucker>,
    fillers: Arc<FillerCacheManager>,
    brain: Arc<tokio::sync::Mutex<Option<AntigravityClient>>>,
    status: Arc<tokio::sync::Mutex<DaemonLiveStatus>>,
    session_mgr: Arc<tokio::sync::RwLock<Arc<SessionPointerManager>>>,
    turn_coord: TurnCoordinator,
    pending_confirmation: Arc<tokio::sync::Mutex<Option<tokio::sync::oneshot::Sender<bool>>>>,
    turn_id: u64,
}

async fn wait_player_drain(
    player: &tokio::sync::Mutex<Box<dyn AudioPlayback>>,
    turn_coord: &TurnCoordinator,
    turn_id: u64,
) -> bool {
    tokio::time::sleep(tokio::time::Duration::from_millis(60)).await;
    loop {
        if !turn_coord.is_current(turn_id) {
            return false;
        }
        let playing = {
            let guard = player.lock().await;
            guard.is_playing()
        };
        if !playing {
            return true;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }
}

/// Speaks a short standalone utterance (e.g. rotation ack or cancellation notice) and drains playback.
async fn speak_turn_message(ctx: &SpeechTurnContext, text: &str, user_prompt: Option<String>) {
    let tts_snapshot = ctx.tts.read().await.clone();
    let synth_res = tokio::select! {
        biased;
        _ = ctx.turn_coord.wait_cancelled(ctx.turn_id) => {
            ctx.ducker.restore().await;
            return;
        }
        res = tts_snapshot.synthesize_resilient(text) => res,
    };

    if !ctx.turn_coord.is_current(ctx.turn_id) {
        ctx.ducker.restore().await;
        return;
    }

    match synth_res {
        Ok(audio) => {
            update_and_persist_status(&ctx.status, |st| {
                if let Some(prompt) = user_prompt {
                    st.last_user_prompt = prompt;
                    st.turn_counter += 1;
                }
                st.last_assistant_text = text.to_string();
            })
            .await;

            let _ = ctx
                .sm
                .dispatch(SystemEvent::SentenceReady(text.to_string()))
                .await;
            let _ = ctx.sm.dispatch(SystemEvent::SpeakingStarted).await;

            ctx.ducker.duck().await;
            {
                let mut p = ctx.player.lock().await;
                if !ctx.turn_coord.is_current(ctx.turn_id) {
                    drop(p);
                    ctx.ducker.restore().await;
                    return;
                }
                let _ = p.enqueue_chunk(audio).await;
            }

            if wait_player_drain(&ctx.player, &ctx.turn_coord, ctx.turn_id).await {
                let _ = ctx.sm.dispatch(SystemEvent::PlaybackFinished).await;
            }
            ctx.ducker.restore().await;
        }
        Err(e) => {
            warn!("TTS synthesis failed for standalone turn message: {e}");
            if ctx.turn_coord.is_current(ctx.turn_id) {
                let _ = ctx.sm.dispatch(SystemEvent::ResetToIdle).await;
            }
            ctx.ducker.restore().await;
        }
    }
}

/// Orchestrates an end-to-end voice turn: STT -> Instant Filler -> Brain streaming -> Tokenizer -> TTS.
async fn process_speech_turn(pcm: Vec<f32>, ctx: SpeechTurnContext) {
    ctx.sm.reset_barge_in();

    if !ctx.turn_coord.is_current(ctx.turn_id) {
        return;
    }

    if pcm.len() < 2400 {
        info!(
            "Discarding micro-click audio buffer ({} samples < 2400 / 150ms)",
            pcm.len()
        );
        let _ = ctx.sm.dispatch(SystemEvent::ResetToIdle).await;
        ctx.ducker.restore().await;
        return;
    }

    let prompt = match ctx.stt.transcribe(&pcm, 16_000).await {
        Ok(text) => text.trim().to_string(),
        Err(e) => {
            warn!("STT transcription error: {e}");
            String::new()
        }
    };

    if !ctx.turn_coord.is_current(ctx.turn_id) {
        return;
    }

    if prompt.is_empty() {
        info!("No speech recognized in audio buffer. Resetting to Idle.");
        let _ = ctx.sm.dispatch(SystemEvent::ResetToIdle).await;
        ctx.ducker.restore().await;
        return;
    }

    info!("STT Result: '{}'", prompt);

    let active_session_mgr = ctx.session_mgr.read().await.clone();

    // 1. Explicit Session Rotation Intent Detection
    if active_session_mgr.detects_rotation_intent(&prompt) {
        info!(
            "Explicit conversation rotation intent detected: '{}'",
            prompt
        );
        let new_session_id = match active_session_mgr.rotate_session().await {
            Ok(id) => Some(id),
            Err(e) => {
                warn!("Failed to rotate session: {e}");
                None
            }
        };

        // Terminate active Antigravity process and spawn fresh session with --conversation
        {
            let mut brain_guard = tokio::select! {
                biased;
                _ = ctx.turn_coord.wait_cancelled(ctx.turn_id) => return,
                guard = ctx.brain.lock() => guard,
            };
            if !ctx.turn_coord.is_current(ctx.turn_id) {
                return;
            }
            if let Some(ref mut client) = *brain_guard {
                let _ = client.terminate().await;
            }
            let sid = new_session_id.as_deref().filter(|id| !id.trim().is_empty());
            *brain_guard = spawn_antigravity_client(sid).ok();
        }

        let ack =
            "A la orden, Señor. Archivo la sesión actual e inicio una nueva conversación limpia.";
        speak_turn_message(&ctx, ack, Some(prompt)).await;
        return;
    }

    let _ = active_session_mgr.increment_turn_count().await;

    update_and_persist_status(&ctx.status, |st| {
        st.last_user_prompt = prompt.clone();
        st.last_assistant_text.clear();
        st.turn_counter += 1;
    })
    .await;

    let _ = ctx
        .sm
        .dispatch(SystemEvent::TranscriptReady(prompt.clone()))
        .await;

    let mut spawned_tasks = TaskAbortGuard::new();

    // Strict Filler policy: At most 2 fillers per turn (1 immediate acknowledgment + at most 1 tool filler if delayed)
    let mut total_fillers_played = 0;
    let mut last_filler_time = std::time::Instant::now();
    let response_started = Arc::new(std::sync::atomic::AtomicBool::new(false));

    // 1. Instantly play conversational filler phrase to eliminate perceived latency (< 150ms)
    if let Some(filler) = ctx.fillers.select_immediate().await {
        info!("Dispatching instant conversational filler: '{}'", filler);
        total_fillers_played += 1;
        last_filler_time = std::time::Instant::now();

        if let Some(audio) = ctx.fillers.get_cached_audio(&filler).await {
            info!(
                "Enqueuing instant filler directly from RAM (< 1ms): '{}'",
                filler
            );
            if ctx.turn_coord.is_current(ctx.turn_id) {
                ctx.ducker.duck().await;
                let mut p = ctx.player.lock().await;
                if ctx.turn_coord.is_current(ctx.turn_id) {
                    let _ = p.enqueue_filler(audio).await;
                }
            }
        } else {
            let tts_filler = ctx.tts.clone();
            let player_filler = ctx.player.clone();
            let ducker_filler = ctx.ducker.clone();
            let coord_filler = ctx.turn_coord.clone();
            let turn_id = ctx.turn_id;
            spawned_tasks.push(tokio::spawn(async move {
                if !coord_filler.is_current(turn_id) {
                    return;
                }
                let tts_snapshot = tts_filler.read().await.clone();
                let synth_res = tokio::select! {
                    biased;
                    _ = coord_filler.wait_cancelled(turn_id) => return,
                    res = tts_snapshot.synthesize_resilient(&filler) => res,
                };
                if let Ok(audio) = synth_res {
                    if coord_filler.is_current(turn_id) {
                        ducker_filler.duck().await;
                        let mut p = player_filler.lock().await;
                        if coord_filler.is_current(turn_id) {
                            let _ = p.enqueue_filler(audio).await;
                        }
                    }
                }
            }));
        }
    }

    // 2. Setup streaming SentenceTokenizer
    let (tx, mut rx) = tokio::sync::mpsc::channel(16);
    let mut tokenizer = SentenceTokenizer::new(tx);

    let tts_worker = ctx.tts.clone();
    let player_worker = ctx.player.clone();
    let ducker_worker = ctx.ducker.clone();
    let sm_worker = ctx.sm.clone();
    let status_worker = ctx.status.clone();
    let resp_started_worker = response_started.clone();
    let coord_worker = ctx.turn_coord.clone();
    let turn_id = ctx.turn_id;
    let mut synthesis_worker = Some(tokio::spawn(async move {
        loop {
            let sentence_opt = tokio::select! {
                biased;
                _ = coord_worker.wait_cancelled(turn_id) => {
                    debug!("Barge-in: aborting sentence synthesis worker");
                    break;
                }
                msg = rx.recv() => msg,
            };
            let Some(sentence) = sentence_opt else {
                break;
            };
            if !coord_worker.is_current(turn_id) {
                debug!("Barge-in: aborting sentence synthesis worker");
                break;
            }
            let trimmed = sentence.trim();
            if trimmed.is_empty() {
                continue;
            }
            info!("Synthesizing sentence: '{}'", trimmed);
            let tts_snapshot = tts_worker.read().await.clone();
            let synth_res = tokio::select! {
                biased;
                _ = coord_worker.wait_cancelled(turn_id) => {
                    debug!("Barge-in: cancelling in-flight TTS synthesis");
                    break;
                }
                res = tts_snapshot.synthesize_resilient(trimmed) => res,
            };

            match synth_res {
                Ok(audio_bytes) => {
                    if !coord_worker.is_current(turn_id) {
                        debug!("Barge-in: discarding synthesized audio for cancelled turn");
                        break;
                    }
                    resp_started_worker.store(true, std::sync::atomic::Ordering::SeqCst);
                    update_and_persist_status(&status_worker, |st| {
                        if st.last_assistant_text.is_empty() {
                            st.last_assistant_text = trimmed.to_string();
                        } else {
                            st.last_assistant_text.push(' ');
                            st.last_assistant_text.push_str(trimmed);
                        }
                    })
                    .await;
                    let _ = sm_worker
                        .dispatch(SystemEvent::SentenceReady(trimmed.to_string()))
                        .await;
                    let _ = sm_worker.dispatch(SystemEvent::SpeakingStarted).await;

                    ducker_worker.duck().await;
                    let mut p = player_worker.lock().await;
                    if !coord_worker.is_current(turn_id) {
                        break;
                    }
                    let _ = p.enqueue_chunk(audio_bytes).await;
                }
                Err(e) => {
                    warn!("TTS synthesis failed for sentence: {e}");
                }
            }
        }
    }));

    // 3. Stream from resident Antigravity Brain
    let risk_classifier = RiskClassifier::default();
    let mut brain_guard = tokio::select! {
        biased;
        _ = ctx.turn_coord.wait_cancelled(ctx.turn_id) => {
            if let Some(handle) = synthesis_worker.take() {
                handle.abort();
            }
            spawned_tasks.abort_all();
            ctx.ducker.restore().await;
            return;
        }
        guard = ctx.brain.lock() => guard,
    };

    let mut response_received = false;
    let mut cancelled = !ctx.turn_coord.is_current(ctx.turn_id);
    let mut aborted_by_confirmation = false;

    if !cancelled {
        if let Some(ref mut client) = *brain_guard {
            // Drain any residual stale lines from previous turns to prevent pipeline lag/desync
            client.drain_residual_lines().await;

            if !ctx.turn_coord.is_current(ctx.turn_id) {
                cancelled = true;
            } else if let Err(e) = client.send_prompt(&prompt).await {
                warn!("Failed to send prompt to resident Antigravity client: {e}");
            } else {
                let mut stream_failed = false;
                loop {
                    if !ctx.turn_coord.is_current(ctx.turn_id) {
                        debug!("Barge-in: aborting token stream from brain for obsolete turn");
                        cancelled = true;
                        break;
                    }
                    let ev_res = tokio::select! {
                        biased;
                        _ = ctx.turn_coord.wait_cancelled(ctx.turn_id) => {
                            debug!("Barge-in: turn cancelled while awaiting Antigravity brain event");
                            cancelled = true;
                            break;
                        }
                        res = client.next_event() => res,
                    };
                    match ev_res {
                        Ok(Some(BrainEvent::TextChunk { content })) => {
                            response_received = true;
                            response_started.store(true, std::sync::atomic::Ordering::SeqCst);
                            if let Some(real_id) = client.conversation_id() {
                                let _ = active_session_mgr.update_conversation_id(real_id).await;
                            }
                            let _ = tokenizer.feed_token(&content).await;
                        }
                        Ok(Some(BrainEvent::ToolCall { name, args })) => {
                            info!("Brain active tool call: '{}'", name);
                            if let Some(real_id) = client.conversation_id() {
                                let _ = active_session_mgr.update_conversation_id(real_id).await;
                            }

                            // Extract command payload from tool args or name for risk evaluation
                            let command_payload =
                                if let Some(cmd) = args.get("command").and_then(|v| v.as_str()) {
                                    cmd.to_string()
                                } else if let Some(cmd) = args.get("cmd").and_then(|v| v.as_str()) {
                                    cmd.to_string()
                                } else if let Some(s) = args.as_str() {
                                    s.to_string()
                                } else if !args.is_null() {
                                    args.to_string()
                                } else {
                                    name.clone()
                                };

                            if risk_classifier.is_destructive(&command_payload)
                                || risk_classifier.is_destructive(&name)
                            {
                                warn!(
                                    tool = %name,
                                    payload = %command_payload,
                                    "Destructive tool action detected; requesting user voice confirmation"
                                );
                                let _ = ctx
                                    .sm
                                    .dispatch(SystemEvent::AwaitingConfirmationRequested {
                                        action: format!("{name}: {command_payload}"),
                                    })
                                    .await;

                                let (confirm_tx, confirm_rx) =
                                    tokio::sync::oneshot::channel::<bool>();
                                {
                                    let mut guard = ctx.pending_confirmation.lock().await;
                                    *guard = Some(confirm_tx);
                                }
                                let (str_tx, str_rx) = tokio::sync::oneshot::channel::<String>();
                                let forward_handle = tokio::spawn(async move {
                                    if let Ok(confirmed) = confirm_rx.await {
                                        let decision_text =
                                            if confirmed { "confirmado" } else { "cancelar" };
                                        let _ = str_tx.send(decision_text.to_string());
                                    }
                                });

                                let warn_msg = format!(
                                    "Atención Señor: Se ha solicitado una acción de riesgo en {}. ¿Desea que proceda?",
                                    name
                                );
                                let tts_snapshot = ctx.tts.read().await.clone();
                                let warn_synth = tokio::select! {
                                    biased;
                                    _ = ctx.turn_coord.wait_cancelled(ctx.turn_id) => {
                                        forward_handle.abort();
                                        let _ = ctx.pending_confirmation.lock().await.take();
                                        cancelled = true;
                                        break;
                                    }
                                    res = tts_snapshot.synthesize_resilient(&warn_msg) => res,
                                };
                                if !ctx.turn_coord.is_current(ctx.turn_id) {
                                    forward_handle.abort();
                                    let _ = ctx.pending_confirmation.lock().await.take();
                                    cancelled = true;
                                    break;
                                }
                                if let Ok(audio) = warn_synth {
                                    ctx.ducker.duck().await;
                                    let mut p = ctx.player.lock().await;
                                    if ctx.turn_coord.is_current(ctx.turn_id) {
                                        let _ = p.enqueue_chunk(audio).await;
                                    }
                                }

                                let confirm_mgr = ConfirmationManager::new();
                                let confirm_res = tokio::select! {
                                    biased;
                                    _ = ctx.turn_coord.wait_cancelled(ctx.turn_id) => {
                                        forward_handle.abort();
                                        let _ = ctx.pending_confirmation.lock().await.take();
                                        cancelled = true;
                                        break;
                                    }
                                    res = confirm_mgr.await_confirmation(str_rx) => res,
                                };
                                forward_handle.abort();
                                let _ = ctx.pending_confirmation.lock().await.take();

                                match confirm_res {
                                    Ok(true) => {
                                        if ctx.sm.state().await == SystemState::AwaitingConfirmation
                                        {
                                            let _ = ctx
                                                .sm
                                                .dispatch(SystemEvent::ConfirmationReceived {
                                                    confirmed: true,
                                                })
                                                .await;
                                        }
                                    }
                                    Ok(false) | Err(_) => {
                                        warn!(
                                            tool = %name,
                                            "Destructive action rejected or timed out; aborting brain stream"
                                        );
                                        aborted_by_confirmation = true;
                                        break;
                                    }
                                }
                            } else if total_fillers_played < 2
                                && !response_started.load(std::sync::atomic::Ordering::SeqCst)
                                && last_filler_time.elapsed()
                                    >= std::time::Duration::from_millis(3500)
                            {
                                if let Some(tool_filler) =
                                    ctx.fillers.select_tool_filler(&name).await
                                {
                                    info!(
                                        "Dispatching tool conversational filler ({}/2): '{}'",
                                        total_fillers_played + 1,
                                        tool_filler
                                    );
                                    total_fillers_played += 1;
                                    last_filler_time = std::time::Instant::now();
                                    let tts_tool = ctx.tts.clone();
                                    let player_tool = ctx.player.clone();
                                    let ducker_tool = ctx.ducker.clone();
                                    let fillers_ref = ctx.fillers.clone();
                                    let resp_started_tool = response_started.clone();
                                    let coord_tool = ctx.turn_coord.clone();
                                    let turn_id = ctx.turn_id;
                                    spawned_tasks.push(tokio::spawn(async move {
                                        if !coord_tool.is_current(turn_id) {
                                            return;
                                        }
                                        let audio_opt = if let Some(cached) =
                                            fillers_ref.get_cached_audio(&tool_filler).await
                                        {
                                            debug!("Dispatching pre-warmed tool filler audio directly from RAM");
                                            Some(cached)
                                        } else {
                                            let tts_snapshot = tts_tool.read().await.clone();
                                            tokio::select! {
                                                biased;
                                                _ = coord_tool.wait_cancelled(turn_id) => return,
                                                res = tts_snapshot.synthesize_resilient(&tool_filler) => res.ok(),
                                            }
                                        };

                                        if let Some(audio) = audio_opt {
                                            if resp_started_tool
                                                .load(std::sync::atomic::Ordering::SeqCst)
                                                || !coord_tool.is_current(turn_id)
                                            {
                                                tracing::debug!("Discarding tool filler because assistant response already arrived or turn cancelled");
                                                return;
                                            }
                                            ducker_tool.duck().await;
                                            let mut p = player_tool.lock().await;
                                            if resp_started_tool
                                                .load(std::sync::atomic::Ordering::SeqCst)
                                                || !coord_tool.is_current(turn_id)
                                            {
                                                tracing::debug!("Discarding tool filler because assistant response already enqueued or turn cancelled");
                                                return;
                                            }
                                            let _ = p.enqueue_filler(audio).await;
                                        }
                                    }));
                                }
                            }
                        }
                        Ok(Some(BrainEvent::Done)) | Ok(None) => {
                            if let Some(real_id) = client.conversation_id() {
                                let _ = active_session_mgr.update_conversation_id(real_id).await;
                            }
                            break;
                        }
                        Err(err) => {
                            warn!("Antigravity stream error/timeout: {err}; restarting resident process to prevent pipeline desync");
                            stream_failed = true;
                            break;
                        }
                        _ => {}
                    }
                }

                if stream_failed || cancelled || aborted_by_confirmation {
                    let _ = client.terminate().await;
                    let current_id = active_session_mgr.current_session_id().await;
                    let sid = (!current_id.is_empty()).then_some(current_id.as_str());
                    *brain_guard = spawn_antigravity_client(sid).ok();
                }
            }
        }
    }
    drop(brain_guard);

    if cancelled || !ctx.turn_coord.is_current(ctx.turn_id) {
        if let Some(handle) = synthesis_worker.take() {
            handle.abort();
        }
        spawned_tasks.abort_all();
        ctx.ducker.restore().await;
        return;
    }

    if aborted_by_confirmation {
        if let Some(handle) = synthesis_worker.take() {
            handle.abort();
        }
        spawned_tasks.abort_all();
        if ctx.sm.state().await == SystemState::AwaitingConfirmation {
            let _ = ctx
                .sm
                .dispatch(SystemEvent::ConfirmationReceived { confirmed: false })
                .await;
        }
        speak_turn_message(&ctx, "Acción cancelada, Señor.", None).await;
        return;
    }

    if !response_received && ctx.turn_coord.is_current(ctx.turn_id) {
        warn!("No tokens received from Antigravity; emitting direct fallback.");
        let fallback =
            "Mil disculpas, Señor. El servicio en la nube demoró en responder. Por favor intente nuevamente."
                .to_string();
        let _ = tokenizer.feed_token(&fallback).await;
    }

    let _ = tokenizer.flush().await;
    drop(tokenizer);
    if let Some(mut handle) = synthesis_worker.take() {
        tokio::select! {
            _ = ctx.turn_coord.wait_cancelled(ctx.turn_id) => {
                handle.abort();
                spawned_tasks.abort_all();
                ctx.ducker.restore().await;
                return;
            }
            _ = &mut handle => {}
        }
    }

    // Proactive Context Warning: If turn count reaches threshold, suggest rotating cleanly
    if active_session_mgr.should_suggest_rotation().await && ctx.turn_coord.is_current(ctx.turn_id)
    {
        let suggestion = "Por cierto, Señor, esta conversación acumula bastante contexto. Si desea que iniciemos una nueva sesión limpia, solo pídamelo cuando guste.";
        info!("Proactively suggesting session rotation due to high turn count");
        let tts_snapshot = ctx.tts.read().await.clone();
        let suggest_res = tokio::select! {
            biased;
            _ = ctx.turn_coord.wait_cancelled(ctx.turn_id) => {
                spawned_tasks.abort_all();
                ctx.ducker.restore().await;
                return;
            }
            res = tts_snapshot.synthesize_resilient(suggestion) => res,
        };
        if let Ok(audio) = suggest_res {
            if ctx.turn_coord.is_current(ctx.turn_id) {
                let _ = ctx
                    .sm
                    .dispatch(SystemEvent::SentenceReady(suggestion.to_string()))
                    .await;
                let _ = ctx.sm.dispatch(SystemEvent::SpeakingStarted).await;
                ctx.ducker.duck().await;
                let mut p = ctx.player.lock().await;
                if ctx.turn_coord.is_current(ctx.turn_id) {
                    let _ = p.enqueue_chunk(audio).await;
                }
            }
        }
    }

    if wait_player_drain(&ctx.player, &ctx.turn_coord, ctx.turn_id).await {
        let _ = ctx.sm.dispatch(SystemEvent::PlaybackFinished).await;
    } else {
        debug!("Barge-in detected during audio drain: exiting turn immediately");
    }
    ctx.ducker.restore().await;
}

/// Executes representative workloads across VAD, DSP resampling, sentence tokenization, and state machine transitions.
async fn run_benchmark() -> Result<(), Box<dyn std::error::Error>> {
    let start = std::time::Instant::now();

    // 1. DSP Resampling and VAD hot loop
    let synthetic_stereo: Vec<f32> = (0..96_000).map(|i| (i as f32 * 0.05).sin() * 0.8).collect();
    let mono = viernes_audio::resample::downmix_to_mono(&synthetic_stereo, 2);
    let resampled = viernes_audio::resample::resample_linear(&mono, 48_000, 16_000);

    let mut vad = viernes_audio::vad::VoiceActivityDetector::new(16_000, 0.02, 800);
    for _ in 0..100 {
        for chunk in resampled.chunks(480) {
            let _ = vad.feed_chunk(chunk);
        }
    }

    // 2. Sentence Tokenizer
    let (tx, mut rx) = tokio::sync::mpsc::channel(100);
    let drain_handle = tokio::spawn(async move { while (rx.recv().await).is_some() {} });
    let mut tokenizer = viernes_brain::tokenizer::SentenceTokenizer::new(tx);
    let tokens = [
        "A ",
        "la ",
        "orden, ",
        "Señor. ",
        "Viernes ",
        "está ",
        "en ",
        "línea! ",
        "¿Cuál ",
        "es ",
        "la ",
        "siguiente ",
        "instrucción? ",
        "Procediendo ",
        "ahora.",
    ];
    for _ in 0..200 {
        for &tok in &tokens {
            tokenizer.feed_token(tok).await?;
        }
        tokenizer.flush().await?;
    }
    drop(tokenizer);
    let _ = drain_handle.await;

    // 3. Central State Machine Transitions
    let sm = viernes_core::state_machine::StateMachine::new(100);
    for _ in 0..1000 {
        let _ = sm
            .dispatch(viernes_core::events::SystemEvent::VoiceStarted)
            .await;
        let _ = sm
            .dispatch(viernes_core::events::SystemEvent::TranscriptReady(
                "Ok".to_string(),
            ))
            .await;
        let _ = sm
            .dispatch(viernes_core::events::SystemEvent::SpeakingStarted)
            .await;
        let _ = sm
            .dispatch(viernes_core::events::SystemEvent::PlaybackFinished)
            .await;
    }

    let elapsed = start.elapsed();
    info!(
        "Benchmark completed successfully in {:.2} ms",
        elapsed.as_secs_f64() * 1000.0
    );
    println!("BENCHMARK_RESULT_MS={:.2}", elapsed.as_secs_f64() * 1000.0);
    Ok(())
}

/// Resolves the Push-to-Talk IPC socket path, honoring `VIERNES_PTT_SOCKET` or `default_ptt_socket_path()`.
fn resolve_ptt_socket_path() -> std::path::PathBuf {
    std::env::var("VIERNES_PTT_SOCKET")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(viernes_core::default_ptt_socket_path)
}

/// Sends a Push-to-Talk IPC command to the specified Unix domain socket path.
async fn send_ipc_ptt(socket_path: &std::path::Path, command: &str) -> std::io::Result<()> {
    use tokio::io::AsyncWriteExt;
    use tokio::net::UnixStream;

    let mut stream = UnixStream::connect(socket_path).await?;
    stream
        .write_all(format!("{}\n", command.trim()).as_bytes())
        .await?;
    stream.flush().await?;
    Ok(())
}

/// Sends a Push-to-Talk IPC trigger command directly to the active Viernes daemon socket.
async fn send_ptt_command(command: &str) -> Result<(), Box<dyn std::error::Error>> {
    let socket_path = resolve_ptt_socket_path();

    if !socket_path.exists() {
        eprintln!(
            "Error: Viernes daemon is not running (socket {:?} not found).",
            socket_path
        );
        eprintln!("Start the daemon first with: viernes --daemon");
        std::process::exit(1);
    }

    send_ipc_ptt(&socket_path, command).await?;
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

        let parsed_ptt = CliArgs::try_parse_from(["viernes", "--ptt", "press"]);
        assert!(parsed_ptt.is_ok());
        assert_eq!(parsed_ptt.unwrap().ptt.as_deref(), Some("press"));

        let parsed_cancel = CliArgs::try_parse_from(["viernes", "--ptt", "cancel"]);
        assert!(parsed_cancel.is_ok());
        assert_eq!(parsed_cancel.unwrap().ptt.as_deref(), Some("cancel"));

        let parsed_reload = CliArgs::try_parse_from(["viernes", "--ptt", "reload"]);
        assert!(parsed_reload.is_ok());
        assert_eq!(parsed_reload.unwrap().ptt.as_deref(), Some("reload"));

        let parsed_new_conv = CliArgs::try_parse_from(["viernes", "--new-conversation"]);
        assert!(parsed_new_conv.is_ok());
        assert!(parsed_new_conv.unwrap().new_conversation);

        let parsed_tui = CliArgs::try_parse_from(["viernes", "--tui"]);
        assert!(parsed_tui.is_ok());
        assert!(parsed_tui.unwrap().tui);
    }

    #[test]
    fn test_build_agy_args_conversation_flag() {
        let with_session = build_agy_args(Some("session-abc-123"));
        assert!(with_session
            .windows(2)
            .any(|w| w[0] == "--conversation" && w[1] == "session-abc-123"));

        let without_session = build_agy_args(None);
        assert!(!without_session.iter().any(|a| a == "--conversation"));

        let empty_session = build_agy_args(Some("   "));
        assert!(!empty_session.iter().any(|a| a == "--conversation"));
    }

    #[tokio::test]
    async fn test_turn_coordinator_epoch_cancellation() {
        let coord = TurnCoordinator::new();
        let turn_1 = coord.bump();
        assert!(coord.is_current(turn_1));

        // Bumping the epoch cancels turn_1 immediately for both waiting and future callers
        let waiter_coord = coord.clone();
        let waiter = tokio::spawn(async move {
            waiter_coord.wait_cancelled(turn_1).await;
        });

        tokio::task::yield_now().await;
        let turn_2 = coord.bump();
        assert!(!coord.is_current(turn_1));
        assert!(coord.is_current(turn_2));

        tokio::time::timeout(std::time::Duration::from_millis(200), waiter)
            .await
            .expect("wait_cancelled should wake when epoch bumps")
            .unwrap();

        // Already-cancelled epoch resolves immediately without blocking
        tokio::time::timeout(
            std::time::Duration::from_millis(50),
            coord.wait_cancelled(turn_1),
        )
        .await
        .expect("already-obsolete turn should resolve immediately");
    }

    #[tokio::test]
    async fn test_confirmation_response_routing() {
        use viernes_core::traits::FakeSpeechToText;

        // 1. Affirmative confirmation: routes `true` and transitions AwaitingConfirmation -> Thinking
        let sm = StateMachine::new(16);
        sm.dispatch(SystemEvent::VoiceStarted).await.unwrap();
        sm.dispatch(SystemEvent::PttReleased).await.unwrap();
        sm.dispatch(SystemEvent::AwaitingConfirmationRequested {
            action: "rm -rf ./tmp".into(),
        })
        .await
        .unwrap();
        assert_eq!(sm.state().await, SystemState::AwaitingConfirmation);

        let pending = tokio::sync::Mutex::new(None);
        let (tx, rx) = tokio::sync::oneshot::channel();
        *pending.lock().await = Some(tx);

        let stt_yes = FakeSpeechToText {
            default_transcript: "sí, procede".into(),
        };
        let decision = route_confirmation_response(&[0.1; 3200], &stt_yes, &pending, &sm).await;
        assert!(decision);
        assert!(rx.await.unwrap());
        assert_eq!(sm.state().await, SystemState::Thinking);

        // 2. Negative confirmation: routes `false` and transitions AwaitingConfirmation -> Idle
        sm.dispatch(SystemEvent::AwaitingConfirmationRequested {
            action: "git reset --hard".into(),
        })
        .await
        .unwrap();
        assert_eq!(sm.state().await, SystemState::AwaitingConfirmation);

        let (tx, rx) = tokio::sync::oneshot::channel();
        *pending.lock().await = Some(tx);

        let stt_no = FakeSpeechToText {
            default_transcript: "no, cancela".into(),
        };
        let decision = route_confirmation_response(&[0.1; 3200], &stt_no, &pending, &sm).await;
        assert!(!decision);
        assert!(!rx.await.unwrap());
        assert_eq!(sm.state().await, SystemState::Idle);

        // 3. Ambiguous confirmation: defaults safely to `false` and transitions -> Idle
        sm.dispatch(SystemEvent::VoiceStarted).await.unwrap();
        sm.dispatch(SystemEvent::PttReleased).await.unwrap();
        sm.dispatch(SystemEvent::AwaitingConfirmationRequested {
            action: "mkfs.ext4 /dev/sda1".into(),
        })
        .await
        .unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel();
        *pending.lock().await = Some(tx);

        let stt_ambiguous = FakeSpeechToText {
            default_transcript: "tal vez más tarde".into(),
        };
        let decision =
            route_confirmation_response(&[0.1; 3200], &stt_ambiguous, &pending, &sm).await;
        assert!(!decision);
        assert!(!rx.await.unwrap());
        assert_eq!(sm.state().await, SystemState::Idle);
    }

    #[tokio::test]
    async fn test_send_ipc_ptt_roundtrip() {
        use viernes_audio::ptt::IpcSocketPttListener;

        let dir = tempfile::tempdir().unwrap();
        let socket_path = dir.path().join("ptt.sock");
        let mut listener = IpcSocketPttListener::new(&socket_path);
        listener.start().await.unwrap();
        let rx = listener.event_receiver();

        send_ipc_ptt(&socket_path, "cancel").await.unwrap();
        send_ipc_ptt(&socket_path, "reload").await.unwrap();

        let ev1 = tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                if let Ok(ev) = rx.try_recv() {
                    return ev;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let ev2 = tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                if let Ok(ev) = rx.try_recv() {
                    return ev;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();

        assert_eq!(ev1, PttEvent::Cancel);
        assert_eq!(ev2, PttEvent::Reload);

        listener.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_playback_ducking_lifecycle_events() {
        use viernes_tts::SilentAudioSink;

        let ducker = Arc::new(AudioDucker::new_dry_run(true, 40));
        let player: Arc<tokio::sync::Mutex<Box<dyn AudioPlayback>>> =
            Arc::new(tokio::sync::Mutex::new(Box::new(SilentAudioSink::new())));
        let sm = StateMachine::new(16);
        let mut event_rx = sm.subscribe_events();
        let mut transition_rx = sm.subscribe_transitions();

        let ducker_trans = ducker.clone();
        let trans_listener = tokio::spawn(async move {
            while let Ok(record) = transition_rx.recv().await {
                if record.to_state == SystemState::Idle || record.to_state == SystemState::Listening
                {
                    ducker_trans.restore().await;
                }
            }
        });

        let ducker_events = ducker.clone();
        let player_events = player.clone();
        let listener = tokio::spawn(async move {
            while let Ok(event) = event_rx.recv().await {
                match event {
                    SystemEvent::FillerPlaybackFinished => {
                        let is_playing = if let Ok(p) = player_events.try_lock() {
                            p.is_playing()
                        } else {
                            false
                        };
                        if !is_playing {
                            ducker_events.restore().await;
                        }
                    }
                    SystemEvent::ResetToIdle | SystemEvent::BargeIn => {
                        ducker_events.restore().await;
                    }
                    _ => {}
                }
            }
        });

        // 1. Initial idle state: music is NOT ducked
        assert!(!ducker.is_ducked());

        // 2. Filler starts playing: duck music
        ducker.duck().await;
        assert!(ducker.is_ducked());

        // 3. Filler finishes playing and assistant response is not yet ready: music restores
        let _ = sm.dispatch(SystemEvent::FillerPlaybackFinished).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
        assert!(!ducker.is_ducked());

        // 4. Assistant response audio begins playing: transition to Speaking and duck music
        let _ = sm.dispatch(SystemEvent::SpeakingStarted).await;
        ducker.duck().await;
        assert!(ducker.is_ducked());

        // 5. Assistant response playback finishes: transition to Idle and music restores
        let _ = sm.dispatch(SystemEvent::PlaybackFinished).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
        assert!(!ducker.is_ducked());

        listener.abort();
        trans_listener.abort();
    }
}
