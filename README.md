# Viernes (09-viernes)

Modular voice assistant architecture designed with decoupled Perception, Brain, and Actuation layers.

## Core Features

- **Resilient TTS (Circuit Breaker / Fallback Pattern):**
  Uses **ElevenLabs** as the primary high-fidelity voice provider. If API quota limits or HTTP 429 errors occur, the system automatically and seamlessly fails over to a local neural TTS provider (**Piper**) without crashing.
- **Audio Dispatcher:**
  Plays generated audio directly via system daemons (`mpv`, `pw-play`, `paplay`).
- **Decoupled Architecture:**
  Clean separation between audio input, reasoning orchestrators (Antigravity/LLM), and output synthesis.

## Architecture

```
src/viernes/
├── config.py             # Environment configuration
├── audio/
│   └── player.py         # Subprocess audio output dispatcher
├── brain/
│   └── agent.py          # Reasoning & tool execution contract
├── tts/
│   ├── base.py           # TTSProvider interface
│   ├── elevenlabs_provider.py # Primary cloud TTS
│   ├── piper_provider.py # Offline neural TTS
│   └── resilient.py      # Circuit-breaker / fallback orchestrator
└── main.py               # CLI entrypoint
```

## Setup & Usage

### 1. Requirements
- Python >= 3.12 (managed via `uv`)
- `mpv`, `pipewire`, or `pulseaudio`

### 2. Install dependencies
```bash
uv sync
```

### 3. Configure credentials
Copy the sample environment file and set your keys:
```bash
cp .env.example .env
```

### 4. Run tests
```bash
uv run pytest
```

### 5. Run Viernes
```bash
uv run python -m viernes.main "Hello, system is online."
```
