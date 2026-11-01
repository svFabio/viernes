import os
import shutil
import subprocess
import uuid
from pathlib import Path

from viernes.tts.base import TTSProvider


class PiperProvider(TTSProvider):
    """Local Piper neural TTS provider."""

    def __init__(
        self,
        model_path: str | Path = "",
        piper_bin: str = "piper",
        temp_dir: Path | str | None = None,
    ):
        self.model_path = str(model_path) if model_path else ""
        self.piper_bin = piper_bin
        self.temp_dir = Path(temp_dir or os.getenv("VIERNES_TEMP_DIR", "/tmp/viernes"))

    def synthesize(self, text: str, output_path: Path | None = None) -> Path:
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
        _stdout, stderr = process.communicate(input=text.strip())
        if process.returncode != 0:
            raise RuntimeError(f"Piper execution failed: {stderr.strip()}")

        return output_path.resolve()
