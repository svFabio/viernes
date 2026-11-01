import concurrent.futures
import threading

from viernes.input.push_to_talk import PushToTalkController, PushToTalkState


def test_concurrent_push_to_talk_hammering():
    """Stress test: 20 threads simultaneously sending key/IPC events."""
    starts = 0
    finishes = 0
    cancels = 0
    lock = threading.Lock()

    def on_start():
        nonlocal starts
        with lock:
            starts += 1

    def on_finish():
        nonlocal finishes
        with lock:
            finishes += 1

    def on_cancel():
        nonlocal cancels
        with lock:
            cancels += 1

    controller = PushToTalkController(
        shortcut="Super+V",
        on_start=on_start,
        on_finish=on_finish,
        on_cancel=on_cancel,
        notify_fn=lambda t, m: None,
    )

    def worker(worker_id: int):
        for _ in range(50):
            if worker_id % 3 == 0:
                controller.handle_key_down()
                controller.handle_key_up()
            elif worker_id % 3 == 1:
                controller.handle_key_down()
                controller.handle_escape()
            else:
                controller.handle_ipc_command("press")
                controller.handle_ipc_command("release")

    with concurrent.futures.ThreadPoolExecutor(max_workers=10) as executor:
        futures = [executor.submit(worker, i) for i in range(10)]
        for f in concurrent.futures.as_completed(futures):
            f.result()

    # The controller must never deadlock or end in an unknown state
    assert controller.state in (PushToTalkState.IDLE, PushToTalkState.PROCESSING, PushToTalkState.LISTENING)
    assert starts > 0
