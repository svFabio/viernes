import asyncio
from collections.abc import AsyncIterator, Callable
import logging
from pathlib import Path
import shutil
import subprocess

from viernes.brain.agent import Agent

logger = logging.getLogger(__name__)


class AntigravityClient(Agent):
    """Client integrating with the Antigravity runtime via CLI headless IPC or SDK."""

    def __init__(
        self,
        cli_bin: str = "agy",
        system_prompt: str | None = None,
        timeout: float = 10.0,
        working_directory: Path | None = None,
        custom_invoker: Callable[[str, float], str] | None = None,
        custom_streamer: Callable[[str, float], AsyncIterator[str]] | None = None,
    ) -> None:
        self.cli_bin = cli_bin
        self.system_prompt = system_prompt
        self.timeout = timeout
        self.working_directory = working_directory
        self._custom_invoker = custom_invoker
        self._custom_streamer = custom_streamer

    def ask(self, prompt: str) -> str:
        """Processes user input synchronously via Antigravity runtime or custom invoker."""
        if self._custom_invoker is not None:
            return self._custom_invoker(prompt, self.timeout)

        executable = shutil.which(self.cli_bin)
        if not executable:
            logger.info("CLI '%s' not found in PATH; operating in fallback mode.", self.cli_bin)
            return f"Antigravity simulated response for: {prompt}"

        cmd = [executable, "--headless"]
        if self.system_prompt:
            cmd.extend(["--system-prompt", self.system_prompt])
        cmd.append(prompt)

        try:
            proc = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=self.timeout,
                check=False,
                cwd=self.working_directory,
            )
            if proc.returncode != 0:
                raise RuntimeError(f"Antigravity CLI failed (exit {proc.returncode}): {proc.stderr.strip()}")
            return proc.stdout.strip()
        except subprocess.TimeoutExpired as err:
            raise TimeoutError(f"Antigravity process timed out after {self.timeout}s") from err

    async def ask_stream(self, prompt: str) -> AsyncIterator[str]:
        """Processes user input asynchronously and streams response tokens/chunks."""
        if self._custom_streamer is not None:
            async for chunk in self._custom_streamer(prompt, self.timeout):
                yield chunk
            return

        executable = shutil.which(self.cli_bin)
        if not executable:
            logger.info("CLI '%s' not found; yielding simulated response chunks.", self.cli_bin)
            yield f"Antigravity simulated response for: {prompt}"
            return

        cmd = [executable, "--headless", "--stream"]
        if self.system_prompt:
            cmd.extend(["--system-prompt", self.system_prompt])
        cmd.append(prompt)

        proc = await asyncio.create_subprocess_exec(
            *cmd,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            cwd=self.working_directory,
        )

        assert proc.stdout is not None
        while True:
            line = await proc.stdout.readline()
            if not line:
                break
            yield line.decode("utf-8")

        await proc.wait()
        if proc.returncode != 0 and proc.stderr:
            err = await proc.stderr.read()
            raise RuntimeError(f"Antigravity streaming process failed: {err.decode('utf-8').strip()}")
