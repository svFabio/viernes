import subprocess
import sys
from pathlib import Path

from viernes.pipeline.barge_in import (
    AcousticEchoFilter,
    BargeInController,
    SystemState,
)
from viernes.pipeline.playback_queue import SequentialPlaybackQueue


def test_acoustic_echo_cancellation() -> None:
    filter_gate = AcousticEchoFilter(echo_margin_ratio=1.2, min_voice_energy_threshold=0.1)

    # 1. No VAD -> always False
    assert not filter_gate.is_user_interruption(mic_energy=0.8, speaker_energy=0.0, vad_active=False)

    # 2. Below minimum threshold -> False
    assert not filter_gate.is_user_interruption(mic_energy=0.05, speaker_energy=0.0, vad_active=True)

    # 3. Speaker reflection (mic = 0.5, speaker = 0.5) -> False (echo filtered)
    assert not filter_gate.is_user_interruption(mic_energy=0.5, speaker_energy=0.5, vad_active=True)

    # 4. Loud human speech over speaker (mic = 0.9, speaker = 0.5 -> 0.9 > 0.5 * 1.2 = 0.6) -> True!
    assert filter_gate.is_user_interruption(mic_energy=0.9, speaker_energy=0.5, vad_active=True)


def test_barge_in_kills_player_clears_queue_and_transitions_recording(tmp_path: Path) -> None:
    # Spawn dummy long-running sleeper process
    proc = subprocess.Popen(
        [sys.executable, "-c", "import time; time.sleep(10)"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    assert proc.poll() is None  # Running

    state_transitions: list[SystemState] = []
    queue = SequentialPlaybackQueue(lambda p: None)

    # Put dummy files in queue
    for i in range(5):
        queue._queue.put_nowait(tmp_path / f"{i}.mp3")
    assert queue._queue.qsize() == 5

    controller = BargeInController(
        playback_queue=queue,
        active_process_supplier=lambda: proc,
        on_state_transition=lambda s: state_transitions.append(s),
    )

    interrupted = controller.handle_voice_detected(mic_energy=0.8, speaker_energy=0.2, vad_active=True)

    assert interrupted is True
    assert proc.poll() is not None  # Process was terminated
    assert queue._queue.qsize() == 0  # Queue drained
    assert controller.current_state == SystemState.RECORDING
    assert state_transitions == [SystemState.RECORDING]


def test_barge_in_repeated_interruption_stress() -> None:
    """Stress test: 20 rapid consecutive interruptions without leaking zombie processes or locks."""
    processes: list[subprocess.Popen] = []
    current_proc: subprocess.Popen | None = None

    def get_proc():
        return current_proc

    controller = BargeInController(
        active_process_supplier=get_proc,
    )

    for _ in range(20):
        # Spawn sleeper
        current_proc = subprocess.Popen(
            [sys.executable, "-c", "import time; time.sleep(5)"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        processes.append(current_proc)
        assert current_proc.poll() is None

        # Immediate interrupt
        interrupted = controller.handle_voice_detected(mic_energy=0.9, speaker_energy=0.1, vad_active=True)
        assert interrupted is True
        assert current_proc.poll() is not None  # Successfully terminated immediately

    # Verify all processes exited and none are left zombie
    for p in processes:
        assert p.poll() is not None
