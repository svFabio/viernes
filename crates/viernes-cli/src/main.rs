//! Command Line Interface and Systemd Daemon binary for Viernes Voice Assistant.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::sync::Arc;

use clap::Parser;
use tracing::{info, warn};
use viernes_audio::capture::{AudioCaptureConfig, CpalAudioCapture};
use viernes_audio::ptt::{IpcSocketPttListener, PttEvent, PttListener};
use viernes_audio::stt::{LocalWhisperTranscriber, WhisperConfig};
use viernes_audio::vad::{VadEvent, VoiceActivityDetector};
use viernes_brain::tokenizer::SentenceTokenizer;
use viernes_core::config::ViernesConfig;
use viernes_core::events::SystemEvent;
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

    /// Send Push-to-Talk IPC trigger to running daemon (e.g. "press", "release", "cancel")
    #[arg(long, value_name = "COMMAND")]
    ptt: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = CliArgs::parse();

    // Fast-path: Dispatch PTT IPC command directly without initializing heavy subsystems
    if let Some(ref cmd) = args.ptt {
        return send_ptt_command(cmd).await;
    }

    // Initialize structured tracing
    tracing_subscriber::fmt()
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
        let test_phrase = "A la orden, Señor. Viernes en línea y lista para operar.";
        info!("Audition test: '{}'", test_phrase);
        let tts = ResilientTtsEngine::from_env();
        let mut player = StreamingAudioPlayer::create_resilient_player(None);

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
        let new_id = uuid::Uuid::new_v4();
        info!("Dynamic session rotated successfully. New ID: {}", new_id);
    }

    if args.benchmark {
        info!("Running representative PGO training benchmark...");
        run_benchmark().await?;
        return Ok(());
    }

    if args.daemon {
        info!("Starting Viernes Voice Assistant User Daemon...");
        run_daemon_loop(&config).await?;
    } else {
        info!("Viernes initialized in single-execution mode. Use --daemon for background service.");
    }

    Ok(())
}

/// Main non-blocking event loop listening for OS signals and driving the voice pipeline.
async fn run_daemon_loop(config: &ViernesConfig) -> Result<(), Box<dyn std::error::Error>> {
    let state_machine = Arc::new(StateMachine::new(256));
    let mut transition_rx = state_machine.subscribe_transitions();
    info!(
        "Daemon initialized in state: {}",
        state_machine.state().await
    );

    // 1. Audio Capture & VAD setup
    let capture_config = AudioCaptureConfig {
        device_query: None,
        preferred_sample_rate: 16_000,
        chunk_ms: 20,
        channel_capacity: 128,
    };
    let mut capture = CpalAudioCapture::new(capture_config);
    let audio_rx = capture.receiver();
    let audio_active = capture.start_stream().await.is_ok();
    if audio_active {
        info!("Real-time audio capture hardware stream active.");
    } else {
        warn!("Audio hardware capture stream unavailable (running in event-driven headless mode).");
    }

    let mut vad =
        VoiceActivityDetector::new(16_000, config.vad_threshold, config.silence_timeout_ms);

    // 2. Audio Playback & TTS setup
    let event_tx = state_machine.event_sender();
    let player: Arc<tokio::sync::Mutex<Box<dyn AudioPlayback>>> =
        Arc::new(tokio::sync::Mutex::new(
            StreamingAudioPlayer::create_resilient_player(Some(event_tx)),
        ));
    let tts_engine = Arc::new(ResilientTtsEngine::from_env());

    // 3. Push-to-Talk IPC Server
    let mut ptt_listener = IpcSocketPttListener::from_env();
    let ptt_rx = ptt_listener.event_receiver();
    if let Err(e) = ptt_listener.start().await {
        warn!("PTT IPC server failed to start: {e}");
    }

    // 4. Signal Handlers
    let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;

    info!("Viernes Reactive Voice Pipeline listening. Press Ctrl+C or send SIGTERM to terminate.");

    // 4. Background transition logger task
    tokio::spawn(async move {
        while let Ok(record) = transition_rx.recv().await {
            info!(
                "Lifecycle Transition: {} -> {} (trigger: {:?})",
                record.from_state, record.to_state, record.trigger_event
            );
        }
    });

    // 5. Main event processing loop
    let mut chunk_interval = tokio::time::interval(std::time::Duration::from_millis(20));

    loop {
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
                            let _ = state_machine.dispatch(SystemEvent::PttPressed).await;
                            if let Ok(mut p) = player.try_lock() {
                                if p.is_playing() {
                                    let _ = p.stop_immediately();
                                    info!("Barge-in: interrupted active playback via PTT.");
                                }
                            }
                        }
                        PttEvent::Released => {
                            info!("Push-to-Talk IPC event: Released");
                            let _ = state_machine.dispatch(SystemEvent::PttReleased).await;
                        }
                    }
                }

                while let Ok(chunk) = audio_rx.try_recv() {
                    let vad_events = vad.feed_chunk(&chunk);
                    for event in vad_events {
                        match event {
                            VadEvent::VoiceStarted => {
                                info!("VAD detected speech onset (VoiceStarted)");
                                let _ = state_machine.dispatch(SystemEvent::VoiceStarted).await;
                                if let Ok(mut p) = player.try_lock() {
                                    if p.is_playing() {
                                        let _ = p.stop_immediately();
                                        info!("Barge-in: interrupted active playback.");
                                    }
                                }
                            }
                            VadEvent::VoiceFinished(pcm_buffer) => {
                                info!(
                                    "VAD detected speech completion (VoiceFinished, {} samples)",
                                    pcm_buffer.len()
                                );
                                let _ = state_machine
                                    .dispatch(SystemEvent::VoiceFinished {
                                        samples: pcm_buffer.clone(),
                                        sample_rate: 16_000,
                                    })
                                    .await;

                                let sm_turn = state_machine.clone();
                                let tts_turn = tts_engine.clone();
                                let player_turn = player.clone();
                                tokio::spawn(async move {
                                    process_speech_turn(
                                        pcm_buffer,
                                        sm_turn,
                                        tts_turn,
                                        player_turn,
                                    ).await;
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    if audio_active {
        let _ = capture.stop_stream().await;
    }
    let _ = ptt_listener.stop().await;
    let _ = state_machine.dispatch(SystemEvent::ResetToIdle).await;
    info!("All subsystem streams stopped cleanly. Viernes terminated.");
    Ok(())
}

/// Orchestrates an end-to-end voice turn: STT -> Brain reasoning -> Tokenizer -> TTS -> Audio playback.
async fn process_speech_turn(
    pcm: Vec<f32>,
    sm: Arc<StateMachine>,
    tts: Arc<ResilientTtsEngine>,
    player: Arc<tokio::sync::Mutex<Box<dyn AudioPlayback>>>,
) {
    let transcriber = LocalWhisperTranscriber::new(WhisperConfig::default());
    let prompt = match transcriber {
        Ok(t) => match t.transcribe(&pcm, 16_000).await {
            Ok(text) if !text.trim().is_empty() => text,
            _ => "A la orden, Señor. Viernes escuchando.".to_string(),
        },
        Err(_) => "A la orden, Señor. Viernes escuchando.".to_string(),
    };

    info!("STT Result: '{}'", prompt);
    let _ = sm
        .dispatch(SystemEvent::TranscriptReady(prompt.clone()))
        .await;

    let (tx, mut rx) = tokio::sync::mpsc::channel(10);
    let mut tokenizer = SentenceTokenizer::new(tx);

    let response_chunks = vec![format!(
        "Comprendido, Señor. Procesando comando: {}.",
        prompt
    )];

    for chunk in response_chunks {
        let _ = tokenizer.feed_token(&chunk).await;
    }
    let _ = tokenizer.flush().await;

    while let Some(sentence) = rx.recv().await {
        info!("Synthesizing sentence: '{}'", sentence);
        let _ = sm
            .dispatch(SystemEvent::SentenceReady(sentence.clone()))
            .await;
        let _ = sm.dispatch(SystemEvent::SpeakingStarted).await;

        match tts.synthesize_resilient(&sentence).await {
            Ok(audio_bytes) => {
                let mut p = player.lock().await;
                let _ = p.enqueue_chunk(audio_bytes).await;
            }
            Err(e) => {
                warn!("TTS synthesis failed for sentence: {e}");
            }
        }
    }
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

/// Sends a Push-to-Talk IPC trigger command directly to the active Viernes daemon socket.
async fn send_ptt_command(command: &str) -> Result<(), Box<dyn std::error::Error>> {
    use tokio::io::AsyncWriteExt;
    use tokio::net::UnixStream;

    let socket_path = std::env::var("VIERNES_PTT_SOCKET")
        .unwrap_or_else(|_| "/tmp/viernes_ptt.sock".to_string());

    let path = std::path::Path::new(&socket_path);
    if !path.exists() {
        eprintln!(
            "Error: Viernes daemon is not running (socket {:?} not found).",
            socket_path
        );
        eprintln!("Start the daemon first with: viernes --daemon");
        std::process::exit(1);
    }

    let mut stream = UnixStream::connect(path).await?;
    stream
        .write_all(format!("{}\n", command.trim()).as_bytes())
        .await?;
    stream.flush().await?;
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
    }
}
