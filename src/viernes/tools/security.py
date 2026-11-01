import re
from typing import Any

FORBIDDEN_COMMANDS = {
    "bash",
    "sh",
    "zsh",
    "dash",
    "ksh",
    "eval",
    "exec",
    "rm",
    "sudo",
    "su",
    "chmod",
    "chown",
    "dd",
    "mkfs",
}

SHELL_INJECTION_PATTERN = re.compile(r"[;&|`$<>\n\r]")


class SecurityViolationError(PermissionError):
    """Raised when an unauthorized or potentially dangerous shell execution is detected."""


def inspect_for_shell_injection(value: Any) -> None:
    """Recursively inspects parameter values to block shell metacharacters and forbidden binaries."""
    if isinstance(value, str):
        # Check metacharacters
        if SHELL_INJECTION_PATTERN.search(value):
            raise SecurityViolationError(
                f"Dangerous shell characters detected in input argument: '{value}'"
            )

        # Check tokenized command prefixes
        tokens = value.lower().split()
        for token in tokens:
            if token in FORBIDDEN_COMMANDS:
                raise SecurityViolationError(
                    f"Forbidden shell command '{token}' detected in argument."
                )

    elif isinstance(value, dict):
        for k, v in value.items():
            inspect_for_shell_injection(k)
            inspect_for_shell_injection(v)
    elif isinstance(value, (list, tuple, set)):
        for item in value:
            inspect_for_shell_injection(item)
