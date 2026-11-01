import os
import shutil
import subprocess
import uuid
from pathlib import Path

from viernes.tts.base import TTSProvider


class PiperProvider(TTSProvider):
    """Local Piper neural TTS provider."""

    DEFAULT_TIMEOUT_SECONDS = 15.0

    def __init__(
        self,
        model_path: str | Path | None = None,
        piper_bin: str | None = None,
        temp_dir: Path | str | None = None,
        timeout: float = DEFAULT_TIMEOUT_SECONDS,
    ):
        model = model_path if model_path is not None else os.getenv("PIPER_MODEL_PATH", "")
        self.model_path = str(model) if model else ""
        self.piper_bin = piper_bin or os.getenv("PIPER_BIN", "piper")
        self.timeout = timeout
        resolved = temp_dir if temp_dir is not None else os.getenv("VIERNES_TEMP_DIR")
        self.temp_dir = Path(resolved or "/tmp/viernes")

    def synthesize(self, text: str, output_path: Path | None = None) -> Path:
        """Synthesize text into WAV audio using Piper neural TTS."""
        if not text or not text.strip():
            raise ValueError("Text cannot be empty or whitespace only.")

        if output_path is None:
            self.temp_dir.mkdir(parents=True, exist_ok=True)
            output_path = self.temp_dir / f"piper_{uuid.uuid4().hex[:8]}.wav"
        else:
            output_path = Path(output_path)
            output_path.parent.mkdir(parents=True, exist_ok=True)

        piper_executable = shutil.which(self.piper_bin)
        if not piper_executable:
            raise FileNotFoundError(
                f"Piper binary '{self.piper_bin}' not found in PATH. "
                "Install piper or configure PIPER_BIN in your environment."
            )

        cmd = [piper_executable]
        if self.model_path:
            cmd.extend(["--model", self.model_path])
        cmd.extend(["--output_file", str(output_path)])

        process = subprocess.Popen(
            cmd,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )

        try:
            _stdout, stderr = process.communicate(input=text.strip(), timeout=self.timeout)
        except subprocess.TimeoutExpired as exc:
            process.kill()
            process.wait()
            raise TimeoutError(f"Piper execution timed out after {self.timeout}s") from exc

        if process.returncode != 0:
            raise RuntimeError(f"Piper execution failed: {stderr.strip()}")

        return output_path.resolve()

    def __repr__(self) -> str:
        return f"PiperProvider(model_path='{self.model_path}', piper_bin='{self.piper_bin}')"
