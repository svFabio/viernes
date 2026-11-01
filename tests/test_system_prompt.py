from viernes.brain.prompt import VIERNES_SYSTEM_PROMPT, get_default_system_prompt


def test_system_prompt_content_and_guidelines():
    prompt = get_default_system_prompt()
    assert prompt == VIERNES_SYSTEM_PROMPT

    # Check key voice persona instructions
    assert "Sos Viernes" in prompt
    assert "concisa" in prompt
    assert "Markdown" in prompt
    assert "emojis" in prompt


def test_system_prompt_does_not_contain_markdown_decorations():
    prompt = get_default_system_prompt()
    # Prompt text itself should not be decorated with bold/italics
    assert "**" not in prompt
    assert "```" not in prompt
