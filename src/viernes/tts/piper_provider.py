import shutil
import subprocess
from pathlib import Path
from viernes.tts.base import TTSProvider


class PiperProvider(TTSProvider):
    """Local Piper neural TTS provider."""

    def __init__(self, model_path: str = "", piper_bin: str = "piper"):
        self.model_path = model_path
        self.piper_bin = piper_bin

    def synthesize(self, text: str, output_path: Path) -> Path:
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
        stdout, stderr = process.communicate(input=text)
        if process.returncode != 0:
            raise RuntimeError(f"Piper execution failed: {stderr}")

        return output_path
