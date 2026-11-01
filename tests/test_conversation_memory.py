from viernes.brain.memory import ConversationMemory


def test_memory_records_and_retrieves_turns():
    mem = ConversationMemory(max_turns=3)
    mem.add_turn("Hola", "Hola, ¿en qué te ayudo?")
    messages = mem.get_messages()

    assert len(messages) == 2
    assert messages[0].role == "user"
    assert messages[0].content == "Hola"
    assert messages[1].role == "assistant"
    assert messages[1].content == "Hola, ¿en qué te ayudo?"


def test_memory_sliding_window_trims_older_turns():
    mem = ConversationMemory(max_turns=2)
    mem.add_turn("Turno 1", "Resp 1")
    mem.add_turn("Turno 2", "Resp 2")
    mem.add_turn("Turno 3", "Resp 3")

    # Max 2 turns * 2 = 4 messages
    messages = mem.get_messages()
    assert len(messages) == 4
    assert messages[0].content == "Turno 2"
    assert messages[1].content == "Resp 2"
    assert messages[2].content == "Turno 3"
    assert messages[3].content == "Resp 3"


def test_memory_context_prompt_formatting():
    mem = ConversationMemory()
    mem.add_turn("¿Cómo está el clima?", "Soleado y despejado.")
    context = mem.format_context_prompt("¿Y mañana?")

    assert "Historial de conversación reciente:" in context
    assert "Usuario: ¿Cómo está el clima?" in context
    assert "Viernes: Soleado y despejado." in context
    assert "Usuario actual: ¿Y mañana?" in context


def test_memory_clear():
    mem = ConversationMemory()
    mem.add_turn("Prueba", "Respuesta")
    assert len(mem.get_messages()) == 2
    mem.clear()
    assert len(mem.get_messages()) == 0
