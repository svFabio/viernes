<p align="center">
  <h1 align="center">Viernes</h1>
  <p align="center"><strong>Low-latency, intelligent voice assistant for deterministic Linux workflows.</strong></p>
  <p align="center">
    <img src="https://img.shields.io/badge/RUST-NATIVE-dc2626?style=for-the-badge&labelColor=181515" alt="Rust Native" />
    <img src="https://img.shields.io/badge/VERSION-v0.1.0-dc2626?style=for-the-badge&labelColor=181515" alt="Version" />
    <img src="https://img.shields.io/badge/SAFETY-FORBID(UNSAFE)-dc2626?style=for-the-badge&labelColor=181515" alt="Safety" />
    <img src="https://img.shields.io/badge/LATENCY-%3C150ms-dc2626?style=for-the-badge&labelColor=181515" alt="Latency" />
    <img src="https://img.shields.io/badge/LICENSE-MIT-dc2626?style=for-the-badge&labelColor=181515" alt="License" />
  </p>
  <p align="center">
    <a href="#quickstart">Quickstart</a> ·
    <a href="#architecture">Architecture</a> ·
    <a href="#terminal-dashboard-tui">TUI Dashboard</a> ·
    <a href="#crates-breakdown">Crates</a> ·
    <a href="#verification--testing">Verification</a>
  </p>
  <p align="center">
    <em>Your terminal can run an assistant. Your workspace should help you lead it.</em>
  </p>
</p>

---

## Overview

**Viernes** is an executive-grade personal voice assistant built for Linux (Wayland / Hyprland). Engineered as a high-performance **modular Rust workspace** with `#![forbid(unsafe_code)]`, Viernes provides zero-lag Push-to-Talk audio capture, instant conversational fillers, deterministic state machine arbitration, and streaming synthesis powered by resident LLM brains.

```
                    ┌─────────────────────────┐
                    │    Hyprland Desktop     │
                    │   Hotkey: SUPER + V     │
                    └────────────┬────────────┘
                                 │ IPC Socket (/tmp/viernes_ptt.sock)
                                 ▼
┌──────────────────┐    ┌─────────────────┐    ┌──────────────────┐
│  Hardware Audio  │───>│  Deterministic  │───>│     Resident     │
│   Capture 16kHz  │    │  State Machine  │    │ Antigravity Brain│
└──────────────────┘    └────────┬────────┘    └────────┬─────────┘
                                 │                      │ Token Stream
                                 ▼                      ▼
                        ┌─────────────────┐    ┌──────────────────┐
                        │   Rodio Audio   │<───│  Resilient TTS   │
                        │    Playback     │    │  (ElevenLabs /   │
                        └─────────────────┘    │   Edge / Piper)  │
                                               └──────────────────┘
```

---

## Key Features

- **⚡ Sub-150ms Conversational Fillers:** Pre-warmed audio cache emits contextual verbal cues (*"Entendido, Señor..."*, *"Revisando..."*) before LLM inference finishes, eliminating perceived dead air.
- **🦀 100% Safe Native Rust Architecture:** Strict `#![forbid(unsafe_code)]` across all 5 workspace crates with zero memory leaks, lock contention safety, and zero buffer underruns.
- **🎙️ Push-to-Talk (PTT) & Acoustic Barge-in:** Hardware-arbitrated audio loop listening on `/tmp/viernes_ptt.sock` with atomic turn epoch cancellation and automatic pause protection.
- **📊 Interactive Ratatui Dashboard (TUI):** Full-screen terminal dashboard displaying real-time 64-band FFT frequency spectrum, active state machine transitions, credentials manager, and audio routing.
- **🛡️ Resilient TTS with Key Rotation & Cooldown:** Multi-account ElevenLabs key pool with HTTP 429 backoff and automatic 60-second cooldown recovery, failing over to Edge TTS or local Piper.
- **🧠 Resident Subprocess Brain:** Continuous background Antigravity streaming process with persistent single-session pointer across daemon restarts and early-clause sentence tokenization.

---

## Architecture

The project is structured as a decoupled, modular Rust workspace:

| Crate | Responsibility | Key Modules |
|-------|----------------|-------------|
| **[`viernes-core`](crates/viernes-core)** | Domain primitives, events, state machine, runtime config | `StateMachine`, `SystemEvent`, `ViernesConfig` |
| **[`viernes-audio`](crates/viernes-audio)** | Hardware capture, resampling, VAD, PTT IPC listener, STT | `CpalAudioCapture`, `EnergyVad`, `LocalWhisperTranscriber` |
| **[`viernes-brain`](crates/viernes-brain)** | Resident LLM client, tokenizer, session management | `AntigravityClient`, `SentenceTokenizer`, `SessionPointerManager` |
| **[`viernes-tts`](crates/viernes-tts)** | Streaming audio player, multi-provider synthesis, key pool | `StreamingAudioPlayer`, `KeyPool`, `ResilientTtsEngine` |
| **[`viernes-cli`](crates/viernes-cli)** | CLI entrypoints, user daemon loop, interactive Ratatui TUI | `TuiApp`, `run_daemon`, `views::*` |

---

## Quickstart

### 1. Requirements

- **Linux x86_64** (Wayland / PipeWire recommended)
- **Rust toolchain** >= 1.85 (`cargo`, `rustc`)
- **System build tools**: `cmake`, `build-essential`, `pkg-config`, `libasound2-dev`, `libssl-dev`
- **Whisper GGML Model**: `ggml-tiny.bin` (placed in `~/.config/viernes/models/` or `models/`)

### 2. Build & Install

```bash
# Clone repository
git clone https://github.com/svFabio/viernes.git
cd viernes

# Compile optimized release binary
cargo build --release

# Install binary atomically to local PATH
install -m 755 target/release/viernes ~/.local/bin/viernes
```

### 3. Desktop Hotkey Setup (Hyprland)

Add the Push-to-Talk hotkey to `~/.config/hypr/bindings.conf` or `bindings.lua`:

```ini
# Hold Super + V to talk, release to transcribe & respond
bind = SUPER, V, exec, viernes --ptt press
bindr = SUPER, V, exec, viernes --ptt release
```

### 4. Background Service (systemd)

Install and enable the background user daemon:

```bash
# Link user unit file and reload systemd daemon
mkdir -p ~/.config/systemd/user
ln -sf $(pwd)/viernes.service ~/.config/systemd/user/viernes.service
systemctl --user daemon-reload

# Start and enable on login
systemctl --user enable --now viernes.service

# Check service status and streaming logs
systemctl --user status viernes.service
journalctl --user -u viernes.service -f
```

---

## Terminal Dashboard (TUI)

Launch the interactive dashboard at any time without disturbing the background daemon:

```bash
viernes --tui
```

```
┌─ Viernes Terminal Dashboard v0.1.0 ────────────────────────────────────────┐
│ [1: Monitor]  [2: Voz & Claves]  [3: Frases Relleno]  [4: Audio]  [5: Servicio] │
├────────────────────────────────────────────────────────────────────────────┤
│  Espectro de Voz (Capturando audio)                                        │
│   ▂▃▅▆▇██▇▆▅▃▂ ▂▃▅▆▇██▇▆▅▃▂ ▂▃▅▆▇██▇▆▅▃▂ ▂▃▅▆▇██▇▆▅▃▂ ▂▃▅▆▇██▇▆▅▃▂       │
│  20Hz                           1kHz                         20kHz         │
├────────────────────────────────────────────────────────────────────────────┤
│  Transcripción & Historial Conversacional                                  │
│  [Sistema] 19:10 : Viernes Terminal Dashboard inicializado. Listo.         │
│  [Usuario] 19:10 : Viernes, ¿cuál es el estado del sistema?                │
│  [Viernes] 19:10 : Todo el pipeline opera en verde, Señor.                │
├────────────────────────────────────────────────────────────────────────────┤
│  [ ESPACIO / ENTER : PTT ]  [ 1-5 : Pestañas ]  [ Q : Salir ]              │
└────────────────────────────────────────────────────────────────────────────┘
```

- **Tab 1 (Monitor):** Live frequency spectrum visualizer and continuous conversation feed.
- **Tab 2 (Voz & Claves):** ElevenLabs API key rotation management and voice IDs.
- **Tab 3 (Frases Relleno):** Dynamic phrase configuration and category navigation.
- **Tab 4 (Audio):** Hardware capture and output device enumerator with live VU metering.
- **Tab 5 (Servicio):** Native `systemd` daemon health monitor, memory usage, and log inspector.

---

## Verification & Testing

Every commit is verified against deterministic automated test suites:

```bash
# Run 100% of native Rust unit, integration, and property tests (104+ tests)
cargo test --workspace

# Run Rust linter and format validation
cargo clippy --workspace -- -D warnings
cargo fmt --check
```

---

## License

Distributed under the [MIT License](LICENSE). Built with safety and speed in mind.
