"""Conversation turn memory and context retention."""

from dataclasses import dataclass


@dataclass
class Message:
    role: str
    content: str


class ConversationMemory:
    """Retains recent conversation turns to resolve anaphoric references."""

    def __init__(self, max_turns: int = 5) -> None:
        self.max_turns = max_turns
        self._history: list[Message] = []

    def add_user_message(self, content: str) -> None:
        self._history.append(Message(role="user", content=content))
        self._trim()

    def add_assistant_message(self, content: str) -> None:
        self._history.append(Message(role="assistant", content=content))
        self._trim()

    def add_turn(self, user_content: str, assistant_content: str) -> None:
        self.add_user_message(user_content)
        self.add_assistant_message(assistant_content)

    def get_messages(self) -> list[Message]:
        return list(self._history)

    def format_context_prompt(self, current_prompt: str) -> str:
        """Formats prior turns into a continuous context block for the reasoning engine."""
        if not self._history:
            return current_prompt

        lines: list[str] = ["Historial de conversación reciente:"]
        for msg in self._history:
            speaker = "Usuario" if msg.role == "user" else "Viernes"
            lines.append(f"{speaker}: {msg.content}")
        lines.append(f"Usuario actual: {current_prompt}")
        return "\n".join(lines)

    def clear(self) -> None:
        self._history.clear()

    def _trim(self) -> None:
        max_messages = self.max_turns * 2
        if len(self._history) > max_messages:
            self._history = self._history[-max_messages:]
