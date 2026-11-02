import asyncio
import logging
import shutil
import subprocess
from collections.abc import AsyncIterator, Callable

from viernes.brain.agent import Agent
from viernes.brain.memory import ConversationMemory
from viernes.brain.prompt import VIERNES_SYSTEM_PROMPT

logger = logging.getLogger(__name__)

DEFAULT_TIMEOUT_SECONDS = 25.0
FALLBACK_DEGRADED_RESPONSE = (
    "Disculpa, tardé demasiado en pensar la respuesta. Por favor intenta de nuevo."
)


class AntigravityAgent(Agent):
    """Client integration with Antigravity runtime via CLI headless IPC or SDK."""

    def __init__(
        self,
        cli_bin: str = "agy",
        system_prompt: str = VIERNES_SYSTEM_PROMPT,
        timeout: float = DEFAULT_TIMEOUT_SECONDS,
        memory: ConversationMemory | None = None,
        custom_invoker: Callable[[str, float], str] | None = None,
        custom_streamer: Callable[[str, float], AsyncIterator[str]] | None = None,
    ) -> None:
        self.cli_bin = cli_bin
        self.system_prompt = system_prompt
        self.timeout = timeout
        self.memory = memory or ConversationMemory()
        self._custom_invoker = custom_invoker
        self._custom_streamer = custom_streamer

    def ask(self, prompt: str) -> str:
        """Synchronously processes prompt with timeout and graceful degradation."""
        contextualized_prompt = self.memory.format_context_prompt(prompt)

        try:
            if self._custom_invoker is not None:
                response = self._custom_invoker(contextualized_prompt, self.timeout)
            else:
                response = self._invoke_cli(contextualized_prompt, self.timeout)

            self.memory.add_turn(prompt, response)
            return response
        except TimeoutError:
            logger.warning(
                "Antigravity reasoning timed out after %s seconds. Degraded response applied.",
                self.timeout,
            )
            return FALLBACK_DEGRADED_RESPONSE
        except (RuntimeError, OSError, subprocess.SubprocessError) as exc:
            logger.error("Antigravity execution failed: %s. Returning fallback.", exc)
            return "Lo siento, tuve un problema al procesar tu solicitud."

    async def ask_stream(self, prompt: str) -> AsyncIterator[str]:
        """Asynchronously streams response tokens with timeout and graceful degradation."""
        contextualized_prompt = self.memory.format_context_prompt(prompt)
        full_response_chunks: list[str] = []

        try:
            async with asyncio.timeout(self.timeout):
                if self._custom_streamer is not None:
                    async for chunk in self._custom_streamer(
                        contextualized_prompt, self.timeout
                    ):
                        full_response_chunks.append(chunk)
                        yield chunk
                else:
                    async for chunk in self._stream_cli(contextualized_prompt):
                        full_response_chunks.append(chunk)
                        yield chunk

            full_response = "".join(full_response_chunks).strip()
            if full_response:
                self.memory.add_turn(prompt, full_response)
        except TimeoutError:
            logger.warning(
                "Antigravity streaming timed out after %s seconds. Emitting fallback.",
                self.timeout,
            )
            yield FALLBACK_DEGRADED_RESPONSE
        except (RuntimeError, OSError, subprocess.SubprocessError) as exc:
            logger.error("Antigravity stream failed: %s", exc)
            yield "Lo siento, ocurrió un error en la conexión."

    def _invoke_cli(self, prompt: str, timeout: float) -> str:
        executable = shutil.which(self.cli_bin)
        if not executable:
            logger.info("CLI %s not found in PATH; falling back to simulated engine.", self.cli_bin)
            return f"Respuesta simulada para: {prompt}"

        full_prompt = f"{self.system_prompt}\n\n{prompt}"
        cmd = [executable, "--output-format", "text", "-p", full_prompt]
        try:
            proc = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=timeout,
                check=False,
            )
            if proc.returncode != 0:
                raise RuntimeError(f"CLI error: {proc.stderr.strip()}")
            return proc.stdout.strip()
        except subprocess.TimeoutExpired as err:
            raise TimeoutError("Antigravity CLI process timed out") from err

    async def _stream_cli(self, prompt: str) -> AsyncIterator[str]:
        executable = shutil.which(self.cli_bin)
        if not executable:
            yield f"Respuesta simulada para: {prompt}"
            return

        full_prompt = f"{self.system_prompt}\n\n{prompt}"
        cmd = [executable, "--output-format", "text", "-p", full_prompt]
        proc = await asyncio.create_subprocess_exec(
            *cmd,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )

        assert proc.stdout is not None
        while True:
            line = await proc.stdout.readline()
            if not line:
                break
            yield line.decode("utf-8")

        await proc.wait()
