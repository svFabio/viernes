import logging
import os
from typing import Any

import sounddevice as sd

logger = logging.getLogger(__name__)


def list_audio_devices() -> list[dict[str, Any]]:
    """Returns a list of all audio devices detected by the system."""
    try:
        raw_devices = sd.query_devices()
        default_input = sd.default.device[0]
        default_output = sd.default.device[1]
    except (sd.PortAudioError, OSError, ValueError, TypeError) as exc:
        logger.warning("Could not query sound devices: %s", exc)
        return []

    devices = []
    # raw_devices can be a DeviceList or list of dicts
    for idx, dev in enumerate(raw_devices):
        dev_dict = dict(dev)
        dev_dict["index"] = idx
        dev_dict["is_default_input"] = idx == default_input
        dev_dict["is_default_output"] = idx == default_output
        devices.append(dev_dict)
    return devices


def print_audio_devices() -> None:
    """Prints detected microphones and speakers to standard output."""
    devices = list_audio_devices()
    inputs = [d for d in devices if d.get("max_input_channels", 0) > 0]
    outputs = [d for d in devices if d.get("max_output_channels", 0) > 0]

    print("\nAvailable Audio Input Devices (Microphones):")
    if not inputs:
        print("  (No input devices found)")
    for dev in inputs:
        default_marker = " * [DEFAULT]" if dev.get("is_default_input") else ""
        print(
            f"  [{dev['index']}] {dev['name']}{default_marker} ({dev['max_input_channels']} channels, {int(dev.get('default_samplerate', 0))}Hz)"
        )

    print("\nAvailable Audio Output Devices (Speakers / Headphones):")
    if not outputs:
        print("  (No output devices found)")
    for dev in outputs:
        default_marker = " * [DEFAULT]" if dev.get("is_default_output") else ""
        print(
            f"  [{dev['index']}] {dev['name']}{default_marker} ({dev['max_output_channels']} channels, {int(dev.get('default_samplerate', 0))}Hz)"
        )
    print()


def resolve_audio_device(
    identifier: int | str | None = None,
    kind: str = "input",
    devices: list[dict[str, Any]] | None = None,
) -> int | None:
    """Resolves an audio device index by index number, partial name, or environment variable.

    Args:
        identifier: Device index (int), partial name substring (str), or None.
        kind: Device kind ('input' or 'output').
        devices: Optional device list for testing.

    Returns:
        int: Resolved device index, or None to use OS default.
    """
    if identifier is None:
        env_var = "AUDIO_INPUT_DEVICE" if kind == "input" else "AUDIO_OUTPUT_DEVICE"
        identifier = os.getenv(env_var, None)

    if identifier is None:
        return None

    ident_str = str(identifier).strip()
    if not ident_str or ident_str.lower() in ("default", "none", "auto"):
        return None

    dev_list = devices if devices is not None else list_audio_devices()
    channel_key = "max_input_channels" if kind == "input" else "max_output_channels"
    matching_kind_devices = [d for d in dev_list if d.get(channel_key, 0) > 0]

    # 1. Try resolving as numerical index
    if ident_str.isdigit() or (ident_str.startswith("-") and ident_str[1:].isdigit()):
        idx = int(ident_str)
        for d in matching_kind_devices:
            if d.get("index") == idx:
                logger.debug("Resolved %s audio device by index %d: %s", kind, idx, d.get("name"))
                return idx

        logger.warning(
            "Audio %s device index '%s' not found or has no %s channels. Falling back to default.",
            kind,
            ident_str,
            kind,
        )
        return None

    # 2. Try resolving as partial case-insensitive name match
    query = ident_str.lower()
    for d in matching_kind_devices:
        name = str(d.get("name", "")).lower()
        if query in name:
            logger.info(
                "Resolved %s audio device by partial name '%s' -> [%d] %s",
                kind,
                ident_str,
                d.get("index"),
                d.get("name"),
            )
            return d.get("index")

    logger.warning(
        "Audio %s device '%s' not found among available devices. Falling back to system default.",
        kind,
        ident_str,
    )
    return None
