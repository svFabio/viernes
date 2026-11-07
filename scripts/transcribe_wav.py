#!/usr/bin/env python3
"""CLI utility for executing local Faster-Whisper inference from Rust."""

import sys
from pathlib import Path
from viernes.stt.whisper_provider import WhisperSTTProvider

def main():
    if len(sys.argv) < 2:
        sys.exit(1)

    wav_path = Path(sys.argv[1])
    if not wav_path.exists():
        sys.exit(2)

    provider = WhisperSTTProvider(model_size="base", language="es")
    transcript = provider.transcribe(wav_path)
    print(transcript)

if __name__ == "__main__":
    main()
