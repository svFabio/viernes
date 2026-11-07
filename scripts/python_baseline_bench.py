#!/usr/bin/env python3
"""Measures legacy Python baseline execution time and peak RSS for Viernes core audio & state pipeline."""

import time
import resource
import math
import numpy as np


def run_legacy_benchmark():
    start = time.perf_counter()

    # 1. DSP Resampling and VAD RMS computation
    # Synthetic stereo 96,000 samples
    samples = np.sin(np.arange(96000, dtype=np.float32) * 0.05) * 0.8
    stereo = np.column_stack((samples, samples))
    mono = np.mean(stereo, axis=1)

    # Linear resample from 48000 to 16000
    indices = np.linspace(0, len(mono) - 1, int(len(mono) / 3))
    resampled = np.interp(indices, np.arange(len(mono)), mono)

    # VAD RMS across 100 iterations of 480-sample chunks
    threshold = 0.02
    for _ in range(100):
        for i in range(0, len(resampled) - 480, 480):
            chunk = resampled[i : i + 480]
            rms = math.sqrt(np.mean(chunk**2))
            _ = rms >= threshold

    # 2. String Tokenizer
    tokens = [
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
    ]
    buffer = ""
    for _ in range(200):
        for tok in tokens:
            buffer += tok
            if any(p in buffer for p in [".", "!", "?"]):
                sentences = buffer.split(".")
                buffer = sentences[-1]

    # 3. State transitions
    state = "Idle"
    for _ in range(1000):
        state = "Listening"
        state = "Thinking"
        state = "Speaking"
        state = "Idle"
    assert state == "Idle"

    elapsed_ms = (time.perf_counter() - start) * 1000.0
    # Peak RSS in MB (ru_maxrss is in KB on Linux)
    usage = resource.getrusage(resource.RUSAGE_SELF)
    rss_mb = usage.ru_maxrss / 1024.0

    print(f"PYTHON_WALL_MS={elapsed_ms:.2f}")
    print(f"PYTHON_PEAK_RSS_MB={rss_mb:.2f}")


if __name__ == "__main__":
    run_legacy_benchmark()
