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
from viernes.tts.base import TTSProvider
from viernes.tts.elevenlabs_provider import ElevenLabsProvider
from viernes.tts.piper_provider import PiperProvider
from viernes.tts.resilient import ResilientTTS

logging.basicConfig(level=logging.INFO, format="%(asctime)s [%(levelname)s] %(message)s")
logger = logging.getLogger("viernes")


def build_tts_engine(temp_dir: Path) -> ResilientTTS:
    primary: TTSProvider | None = None
    if settings.elevenlabs_api_keys:
        try:
            from viernes.tts.key_pool import KeyPoolElevenLabsProvider

            primary = KeyPoolElevenLabsProvider(
                api_keys=settings.elevenlabs_api_keys,
                voice_id=settings.elevenlabs_voice_id,
                model_id=settings.elevenlabs_model_id,
                temp_dir=temp_dir,
            )
        except (ValueError, RuntimeError) as exc:
            logger.warning("Could not initialize ElevenLabs Key Pool: %s", exc)
    elif settings.elevenlabs_api_key:
        try:
            primary = ElevenLabsProvider(
                api_key=settings.elevenlabs_api_key,
                voice_id=settings.elevenlabs_voice_id,
                model_id=settings.elevenlabs_model_id,
            )
        except (ValueError, RuntimeError) as exc:
            logger.warning("Could not initialize ElevenLabs: %s", exc)

    from viernes.tts.edge_provider import EdgeTTSProvider

    fallback: TTSProvider
    if settings.piper_model_path and Path(settings.piper_model_path).exists():
        fallback = PiperProvider(
            model_path=settings.piper_model_path,
            piper_bin=settings.piper_bin,
        )
    else:
        fallback = EdgeTTSProvider(
            voice=settings.edge_tts_voice,
            temp_dir=temp_dir,
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


def run_listener_loop(
    pipeline: StreamingPipeline,
    barge_in: BargeInController,
    temp_dir: Path,
    continuous: bool = False,
) -> None:
    import os
    import threading
    import time
    from viernes.audio.recorder import AudioRecorder
    from viernes.input.ipc import PushToTalkIPCServer
    from viernes.input.push_to_talk import PushToTalkController
    from viernes.stt.whisper_provider import WhisperSTTProvider

    logger.info("Initializing Speech Recognition (Whisper model: base)...")
    whisper = WhisperSTTProvider(model_size="base", language="es")
    recorder = AudioRecorder(temp_dir=temp_dir)

    is_turn_active = False
    turn_lock = threading.Lock()

    # Create a persistent asyncio event loop to keep the resident brain process alive
    worker_loop = asyncio.new_event_loop()
    loop_thread = threading.Thread(target=worker_loop.run_forever, daemon=True)
    loop_thread.start()

    if isinstance(pipeline.agent, AntigravityAgent):
        logger.info("Pre-warming resident Antigravity process in background loop...")
        asyncio.run_coroutine_threadsafe(
            pipeline.agent._ensure_resident_proc(), worker_loop
        )

    def handle_speech_capture() -> None:
        nonlocal is_turn_active
        with turn_lock:
            if is_turn_active:
                return
            is_turn_active = True

        try:
            logger.info("Microphone is active. Speak now...")
            wav_path = recorder.record_utterance(timeout_s=20.0)
            if wav_path and wav_path.is_file():
                logger.info("Transcribing speech with Whisper...")
                prompt = whisper.transcribe(wav_path)
                logger.info("Recognized prompt: '%s'", prompt)
                if prompt and prompt.strip():
                    fut = asyncio.run_coroutine_threadsafe(
                        run_pipeline_turn(prompt, pipeline, barge_in), worker_loop
                    )
                    fut.result()
        except Exception as exc:
            logger.error("Error during speech turn: %s", exc)
        finally:
            with turn_lock:
                is_turn_active = False

    if continuous:
        logger.info("Viernes is listening CONTINUOUSLY on your microphone. Speak whenever you want.")
        try:
            while True:
                handle_speech_capture()
        except (KeyboardInterrupt, SystemExit):
            logger.info("Stopping continuous listener...")
        return

    def on_ptt_start() -> None:
        logger.info("Push-to-Talk triggered: capturing audio.")
        barge_in.handle_user_speech_detected()
        threading.Thread(target=handle_speech_capture, daemon=True).start()

    def on_ptt_finish() -> None:
        logger.debug("Push-to-Talk released.")

    def on_ptt_cancel() -> None:
        logger.info("Push-to-Talk cancelled.")
        recorder.close()

    controller = PushToTalkController(
        on_start=on_ptt_start,
        on_finish=on_ptt_finish,
        on_cancel=on_ptt_cancel,
    )

    socket_path = Path(os.getenv("VIERNES_PTT_SOCKET", "/tmp/viernes_ptt.sock"))
    logger.info("Starting Viernes Daemon. IPC socket: %s", socket_path)

    with PushToTalkIPCServer(controller=controller, socket_path=socket_path):
        logger.info("Viernes is ACTIVE and listening for PTT commands ('viernes --ptt press' or global hotkey).")
        try:
            while True:
                time.sleep(1)
        except (KeyboardInterrupt, SystemExit):
            logger.info("Stopping Viernes daemon gracefully...")


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
    parser.add_argument("--continuous", action="store_true", help="Continuously listen on microphone with VAD")
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

    # Build streaming pipeline with sequential playback and barge-in controller
    from viernes.audio.fillers import FillerAudioCache
    import threading

    filler_cache = FillerAudioCache()
    threading.Thread(
        target=lambda: filler_cache.prewarm(lambda text, out: tts.synthesize(text, out)),
        daemon=True,
    ).start()

    playback_queue = SequentialPlaybackQueue(lambda p: player.play(p, blocking=True))
    pipeline = StreamingPipeline(
        agent=brain,
        tts_synthesizer=lambda text, out: tts.synthesize(text, out),
        playback_queue=playback_queue,
        temp_dir=settings.temp_dir,
        filler_cache=filler_cache,
    )
    barge_in = BargeInController(
        playback_queue=playback_queue,
        streaming_pipeline=pipeline,
    )

    if text_to_speak:
        asyncio.run(run_pipeline_turn(text_to_speak, pipeline, barge_in))
    else:
        run_listener_loop(
            pipeline=pipeline,
            barge_in=barge_in,
            temp_dir=settings.temp_dir,
            continuous=args.continuous,
        )


if __name__ == "__main__":
    main()
