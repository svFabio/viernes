import asyncio
import json
import logging
import os
import shutil
import subprocess
from collections.abc import AsyncIterator, Callable
from pathlib import Path

from viernes.brain.agent import Agent
from viernes.brain.memory import ConversationMemory
from viernes.brain.prompt import VIERNES_SYSTEM_PROMPT

logger = logging.getLogger(__name__)

DEFAULT_TIMEOUT_SECONDS = float(os.getenv("VIERNES_BRAIN_TIMEOUT", "120.0"))
FALLBACK_DEGRADED_RESPONSE = "Señor, la operación tomó más tiempo del límite esperado para responder. Por favor indíqueme si desea que lo intente nuevamente."


class AntigravityAgent(Agent):
    """Client integration with Antigravity runtime via resident CLI NDJSON stream IPC."""

    def __init__(
        self,
        cli_bin: str = "agy",
        model: str = "gemini-3.8-flash-low",
        effort: str = "low",
        system_prompt: str = VIERNES_SYSTEM_PROMPT,
        timeout: float = DEFAULT_TIMEOUT_SECONDS,
        memory: ConversationMemory | None = None,
        custom_invoker: Callable[[str, float], str] | None = None,
        custom_streamer: Callable[[str, float], AsyncIterator[str]] | None = None,
        on_tool_activity: Callable[[str], None] | None = None,
    ) -> None:
        self.cli_bin = cli_bin
        self.model = model
        self.effort = effort
        self.system_prompt = system_prompt
        self.timeout = timeout
        self.memory = memory or ConversationMemory()
        self._custom_invoker = custom_invoker
        self._custom_streamer = custom_streamer
        self.on_tool_activity = on_tool_activity
        self._resident_proc: asyncio.subprocess.Process | None = None
        self._resident_loop: asyncio.AbstractEventLoop | None = None
        self._proc_lock: asyncio.Lock | None = None

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
        """Asynchronously streams response tokens via resident channel with graceful degradation."""
        contextualized_prompt = self.memory.format_context_prompt(prompt)
        full_response_chunks: list[str] = []

        try:
            async with asyncio.timeout(self.timeout):
                if self._custom_streamer is not None:
                    async for chunk in self._custom_streamer(contextualized_prompt, self.timeout):
                        full_response_chunks.append(chunk)
                        yield chunk
                else:
                    async for chunk in self._stream_resident(contextualized_prompt):
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
            self._kill_resident_proc()
            yield FALLBACK_DEGRADED_RESPONSE
        except (RuntimeError, OSError, subprocess.SubprocessError) as exc:
            logger.error("Antigravity stream failed: %s", exc)
            self._kill_resident_proc()
            yield "Lo siento, ocurrió un error en la conexión."

    async def _ensure_resident_proc(self) -> asyncio.subprocess.Process:
        """Starts or returns the persistent background agy subprocess in stream-json mode."""
        current_loop = asyncio.get_running_loop()
        if (
            self._resident_proc is not None
            and self._resident_proc.returncode is None
            and self._resident_loop == current_loop
        ):
            return self._resident_proc

        if self._resident_proc is not None:
            self._kill_resident_proc()

        executable = shutil.which(self.cli_bin)
        if not executable:
            raise RuntimeError(f"CLI binary '{self.cli_bin}' not found in PATH")

        cmd = [
            executable,
            "--model",
            self.model,
            "--effort",
            self.effort,
            "--dangerously-skip-permissions",
            "--disable-slash-commands",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
        ]
        logger.info("Spawning resident Antigravity CLI process: %s", " ".join(cmd))
        proc = await asyncio.create_subprocess_exec(
            *cmd,
            stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
        )
        assert proc.stdout is not None
        # Consume the startup 'init' event
        init_line = await proc.stdout.readline()
        logger.debug("Resident CLI initialized: %s", init_line.decode("utf-8").strip()[:100])
        self._resident_proc = proc
        self._resident_loop = current_loop
        self._proc_lock = asyncio.Lock()
        return self._resident_proc

    def _kill_resident_proc(self) -> None:
        """Kills the persistent process to recover from timeouts or errors."""
        if self._resident_proc is not None:
            try:
                self._resident_proc.kill()
            except ProcessLookupError:
                pass
            self._resident_proc = None
            self._resident_loop = None

    async def close(self) -> None:
        """Gracefully closes the resident subprocess."""
        if self._resident_proc is not None:
            try:
                if self._resident_proc.stdin is not None:
                    self._resident_proc.stdin.close()
                self._resident_proc.terminate()
                await self._resident_proc.wait()
            except Exception as exc:
                logger.debug("Error while closing resident proc: %s", exc)
            finally:
                self._resident_proc = None

    async def _stream_resident(self, prompt: str) -> AsyncIterator[str]:
        """Sends prompt to resident process and yields streaming text_delta tokens."""
        executable = shutil.which(self.cli_bin)
        if not executable:
            yield f"Respuesta simulada para: {prompt}"
            return

        if self._proc_lock is None:
            self._proc_lock = asyncio.Lock()

        async with self._proc_lock:
            proc = await self._ensure_resident_proc()
            assert proc.stdin is not None
            assert proc.stdout is not None

            full_prompt = f"{self.system_prompt}\n\n{prompt}"
            payload = {
                "event": "user",
                "message": {
                    "content": full_prompt,
                },
            }
            raw_msg = json.dumps(payload) + "\n"
            proc.stdin.write(raw_msg.encode("utf-8"))
            await proc.stdin.drain()

            while True:
                line_bytes = await proc.stdout.readline()
                if not line_bytes:
                    logger.warning("Resident agy closed stdout; terminating process")
                    self._kill_resident_proc()
                    break

                line_str = line_bytes.decode("utf-8").strip()
                if not line_str:
                    continue

                try:
                    data = json.loads(line_str)
                except json.JSONDecodeError:
                    continue

                event_type = data.get("event")

                if event_type == "step_update":
                    step_update = data.get("step_update", {})
                    step_type = step_update.get("step_type")
                    state = step_update.get("state")

                    # Notify on tool invocation so pipeline can play context-aware fillers
                    if step_type == "tool" and state == "ACTIVE":
                        tool_name = step_update.get("tool_name") or "tool"
                        logger.info("Antigravity tool active: %s", tool_name)
                        if self.on_tool_activity:
                            try:
                                self.on_tool_activity(tool_name)
                            except Exception as exc:
                                logger.error("Error in on_tool_activity callback: %s", exc)

                    delta = step_update.get("text_delta")
                    if delta:
                        yield delta

                elif event_type == "result":
                    res = data.get("result", {})
                    logger.debug("Antigravity turn completed: status=%s", res.get("status"))
                    break

    def _invoke_cli(self, prompt: str, timeout: float) -> str:
        """One-off CLI invocation fallback for synchronous execution."""
        executable = shutil.which(self.cli_bin)
        if not executable:
            logger.info("CLI %s not found in PATH; falling back to simulated engine.", self.cli_bin)
            return f"Respuesta simulada para: {prompt}"

        full_prompt = f"{self.system_prompt}\n\n{prompt}"
        cmd = [
            executable,
            "--model",
            self.model,
            "--effort",
            self.effort,
            "--disable-slash-commands",
            "--output-format",
            "text",
            "-p",
            full_prompt,
        ]
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
