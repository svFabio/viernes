import argparse
import asyncio
import logging
import sys
from pathlib import Path

from viernes.audio.player import AudioPlayer
from viernes.brain.agent import Agent, EchoAgent
from viernes.brain.antigravity_client import AntigravityAgent
from viernes.brain.mock import MockAgent
from viernes.config import settings
from viernes.pipeline.barge_in import BargeInController
from viernes.pipeline.playback_queue import SequentialPlaybackQueue
from viernes.pipeline.streaming_pipeline import StreamingPipeline
from viernes.tools.registry import DEFAULT_ALLOWLIST
from viernes.tts.elevenlabs_provider import ElevenLabsProvider
from viernes.tts.piper_provider import PiperProvider
from viernes.tts.resilient import ResilientTTS

logging.basicConfig(level=logging.INFO, format="%(asctime)s [%(levelname)s] %(message)s")
logger = logging.getLogger("viernes")


def build_tts_engine(temp_dir: Path) -> ResilientTTS:
    primary = None
    if settings.elevenlabs_api_key:
        try:
            primary = ElevenLabsProvider(
                api_key=settings.elevenlabs_api_key,
                voice_id=settings.elevenlabs_voice_id,
            )
        except (ValueError, RuntimeError) as exc:
            logger.warning("Could not initialize ElevenLabs: %s", exc)

    fallback = PiperProvider(
        model_path=settings.piper_model_path,
        piper_bin=settings.piper_bin,
    )

    return ResilientTTS(
        primary=primary or fallback,
        fallback=fallback,
        on_fallback_activated=lambda exc: logger.warning("TTS fallback activated: %s", exc),
    )


def build_brain_agent(use_mock: bool = False, use_echo: bool = False) -> Agent:
    if use_echo:
        return EchoAgent()
    if use_mock:
        return MockAgent(
            default_response="Hola, soy Viernes. Todos los sistemas están funcionando con normalidad."
        )
    return AntigravityAgent()


async def run_pipeline_turn(
    prompt: str,
    pipeline: StreamingPipeline,
    barge_in: BargeInController | None = None,
) -> None:
    logger.info("Processing user command: '%s'", prompt)
    metrics = await pipeline.execute(prompt)
    logger.info(
        "Turn completed: %d sentences synthesized. First audio latency: %.3fs. Total: %.3fs.",
        metrics.sentences_count,
        metrics.first_audio_latency_seconds,
        metrics.total_pipeline_duration_seconds,
    )


def main() -> None:
    parser = argparse.ArgumentParser(description="Viernes: Modular Voice Assistant")
    parser.add_argument("prompt", nargs="*", help="Direct text prompt to speak or ask")
    parser.add_argument("--test-tts", action="store_true", help="Test TTS synthesis directly")
    parser.add_argument("--mock-brain", action="store_true", help="Use MockAgent for offline execution")
    parser.add_argument("--echo-brain", action="store_true", help="Use EchoAgent for simple roundtrip tests")
    parser.add_argument("--list-tools", action="store_true", help="List security-allowed MCP tools")
    parser.add_argument("--list-audio-devices", action="store_true", help="List detected microphones and speakers")
    parser.add_argument("--ptt", choices=["press", "release", "cancel"], help="Send Push-to-Talk IPC command")
    parser.add_argument("--gui", action="store_true", help="Launch the Viernes Control Center GUI")
    args = parser.parse_args()

    if args.gui:
        from viernes.gui.app import main as gui_main
        gui_main()
        return

    if args.list_audio_devices:
        from viernes.audio.devices import print_audio_devices
        print_audio_devices()
        return

    if args.ptt:
        from viernes.input.ipc import send_ptt_ipc_command
        success = send_ptt_ipc_command(args.ptt)
        if not success:
            logger.error("Failed to deliver Push-to-Talk IPC command '%s'", args.ptt)
            sys.exit(1)
        return

    if args.list_tools:
        print("Allowed MCP Tools in Viernes Security Allowlist:")
        for tool in sorted(DEFAULT_ALLOWLIST):
            print(f" - {tool}")
        return

    settings.temp_dir.mkdir(parents=True, exist_ok=True)
    player = AudioPlayer()
    tts = build_tts_engine(settings.temp_dir)
    brain = build_brain_agent(use_mock=args.mock_brain, use_echo=args.echo_brain)

    text_to_speak = " ".join(args.prompt) if args.prompt else ""

    if args.test_tts:
        msg = text_to_speak or "Hola, soy Viernes. ¿En qué te puedo ayudar?"
        output_file = settings.temp_dir / "response.mp3"
        logger.info("Synthesizing test audio: '%s'", msg)
        try:
            synthesized_path = tts.synthesize(msg, output_file)
            logger.info("Playing test audio: %s", synthesized_path)
            player.play(synthesized_path)
        except (RuntimeError, OSError) as exc:
            logger.error("Synthesis failed: %s", exc)
            sys.exit(1)
        return

    if text_to_speak:
        # Build streaming pipeline with sequential playback and barge-in controller
        playback_queue = SequentialPlaybackQueue(lambda p: player.play(p, blocking=True))
        pipeline = StreamingPipeline(
            agent=brain,
            tts_synthesizer=lambda text, out: tts.synthesize(text, out),
            playback_queue=playback_queue,
            temp_dir=settings.temp_dir,
        )
        barge_in = BargeInController(
            playback_queue=playback_queue,
            streaming_pipeline=pipeline,
        )

        asyncio.run(run_pipeline_turn(text_to_speak, pipeline, barge_in))
    else:
        logger.info("Viernes ready. Provide a prompt to ask or use --test-tts.")


if __name__ == "__main__":
    main()
