import argparse
import logging
import sys
from pathlib import Path
from viernes.config import settings
from viernes.tts.elevenlabs_provider import ElevenLabsProvider
from viernes.tts.piper_provider import PiperProvider
from viernes.tts.resilient import ResilientTTS
from viernes.audio.player import AudioPlayer
from viernes.brain.agent import EchoAgent

logging.basicConfig(level=logging.INFO, format="%(asctime)s [%(levelname)s] %(message)s")
logger = logging.getLogger("viernes")


def main() -> None:
    parser = argparse.ArgumentParser(description="Viernes: Modular Voice Assistant")
    parser.add_argument("prompt", nargs="*", help="Direct text prompt to speak or ask")
    parser.add_argument("--test-tts", action="store_true", help="Test TTS synthesis directly")
    parser.add_argument(
        "--ptt",
        choices=["press", "release", "cancel"],
        help="Send Push-to-Talk IPC trigger command (press, release, cancel) to the daemon",
    )
    args = parser.parse_args()

    if args.ptt:
        from viernes.input.ipc import send_ptt_ipc_command

        success = send_ptt_ipc_command(args.ptt)
        if success:
            logger.info("Push-to-Talk IPC command '%s' sent successfully.", args.ptt)
            return
        else:
            logger.error("Could not send Push-to-Talk IPC command '%s'. Is Viernes running?", args.ptt)
            sys.exit(1)

    settings.temp_dir.mkdir(parents=True, exist_ok=True)
    player = AudioPlayer()

    # Build TTS system
    primary = None
    if settings.elevenlabs_api_key:
        try:
            primary = ElevenLabsProvider(
                api_key=settings.elevenlabs_api_key,
                voice_id=settings.elevenlabs_voice_id,
            )
        except Exception as e:
            logger.warning("Could not initialize ElevenLabs: %s", e)

    fallback = PiperProvider(
        model_path=settings.piper_model_path,
        piper_bin=settings.piper_bin,
    )

    tts = ResilientTTS(
        primary=primary or fallback,
        fallback=fallback,
        on_fallback_activated=lambda exc: logger.warning("Fallback triggered: %s", exc),
    )

    text_to_speak = " ".join(args.prompt) if args.prompt else "Hola, soy Viernes. ¿En qué te puedo ayudar?"

    if args.test_tts or args.prompt:
        output_file = settings.temp_dir / "response.mp3"
        logger.info("Synthesizing: '%s'", text_to_speak)
        try:
            synthesized_path = tts.synthesize(text_to_speak, output_file)
            logger.info("Audio ready at %s. Playing...", synthesized_path)
            player.play(synthesized_path)
        except Exception as exc:
            logger.error("Synthesis failed: %s", exc)
            sys.exit(1)
    else:
        logger.info("Viernes initialized. Ready for commands. Run with --test-tts or pass a message.")


if __name__ == "__main__":
    main()
