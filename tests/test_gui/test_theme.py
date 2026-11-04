import re
import pytest
import customtkinter as ctk

from viernes.gui.theme import Colors, Radius, Spacing, TypographyManager, create_card


HEX_REGEX = re.compile(r"^#[0-9a-fA-F]{6}$")


def test_color_tokens_valid_hex_pairs() -> None:
    """Ensures all semantic color tokens are valid light/dark 6-digit hex tuples."""
    for field_name in dir(Colors):
        if field_name.startswith("_"):
            continue
        val = getattr(Colors, field_name)
        if isinstance(val, tuple) and len(val) == 2:
            light, dark = val
            assert HEX_REGEX.match(light), f"Invalid light hex for {field_name}: {light}"
            assert HEX_REGEX.match(dark), f"Invalid dark hex for {field_name}: {dark}"


def test_radius_and_spacing_tokens() -> None:
    """Verifies geometric radius and spacing scale consistency."""
    assert Radius.CARD >= 8
    assert Radius.BUTTON >= 4
    assert Radius.BADGE >= Radius.BUTTON

    assert Spacing.XS < Spacing.SM < Spacing.MD < Spacing.LG < Spacing.XL


def test_typography_manager_font_presets() -> None:
    """Verifies typography presets instantiate with Montserrat without errors."""
    try:
        app = ctk.CTk()
        app.withdraw()

        fonts = [
            TypographyManager.display(),
            TypographyManager.title(),
            TypographyManager.card_title(),
            TypographyManager.subheading(),
            TypographyManager.body(),
            TypographyManager.body_bold(),
            TypographyManager.caption(),
            TypographyManager.caption_bold(),
            TypographyManager.badge(),
            TypographyManager.mono(),
        ]

        for font in fonts:
            assert isinstance(font, ctk.CTkFont)
            assert font.cget("size") > 0

        # Primary fonts must target Montserrat
        assert TypographyManager.display().cget("family") == "Montserrat"
        assert TypographyManager.body().cget("family") == "Montserrat"

        app.destroy()
    except Exception as exc:
        if "no display name" in str(exc).lower():
            pytest.skip("Skipping font test because display is not available.")
        raise


def test_create_card_factory() -> None:
    """Verifies create_card initializes a framed container with default design tokens."""
    try:
        app = ctk.CTk()
        app.withdraw()

        card = create_card(app)
        assert isinstance(card, ctk.CTkFrame)
        assert card.cget("corner_radius") == Radius.CARD
        assert card.cget("border_width") == 1

        app.destroy()
    except Exception as exc:
        if "no display name" in str(exc).lower():
            pytest.skip("Skipping widget factory test because display is not available.")
        raise
