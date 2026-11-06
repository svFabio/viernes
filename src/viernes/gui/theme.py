"""Design System & Theme Tokens for Viernes Control Center.

Adheres to modern executive dark-mode aesthetics (Slate / Zinc / Obsidian):
- Zero hardcoded HEX strings in consumer widgets.
- Clean semantic tokens for Backgrounds, Surfaces, Borders, Typography, and Status.
- First-class typography engine leveraging 'Montserrat' with defensive fallback.
- Strictly NO neon or high-saturation electric palettes.
"""

from __future__ import annotations

import logging
from dataclasses import dataclass
from pathlib import Path
from typing import Final, Literal

import customtkinter as ctk

logger = logging.getLogger(__name__)

# Fallback font families in priority order
FONT_FAMILY_PRIMARY: Final[str] = "Montserrat"
FONT_FALLBACKS: Final[tuple[str, ...]] = (
    "Montserrat",
    "Segoe UI",
    "DejaVu Sans",
    "Helvetica Neue",
    "Arial",
    "sans-serif",
)


@dataclass(frozen=True)
class ColorTokens:
    """Design System Semantic Color Tokens (Light, Dark)."""

    # Backgrounds & Canvas (Deep Obsidian / Slate)
    BG_APP: tuple[str, str] = ("#f8fafc", "#0b0f17")
    BG_HEADER: tuple[str, str] = ("#ffffff", "#111827")
    BG_CARD: tuple[str, str] = ("#ffffff", "#16202c")
    BG_CARD_SUBTLE: tuple[str, str] = ("#f8fafc", "#1a2533")
    BG_TABVIEW: tuple[str, str] = ("#f1f5f9", "#0f172a")

    # Hairline Borders & Dividers
    BORDER_DEFAULT: tuple[str, str] = ("#e2e8f0", "#243242")
    BORDER_SUBTLE: tuple[str, str] = ("#f1f5f9", "#1e293b")
    BORDER_MUTED: tuple[str, str] = ("#cbd5e1", "#283548")
    BORDER_FOCUS: tuple[str, str] = ("#94a3b8", "#3b82f6")

    # Typography / Text Tokens
    TEXT_PRIMARY: tuple[str, str] = ("#0f172a", "#f8fafc")
    TEXT_SECONDARY: tuple[str, str] = ("#475569", "#94a3b8")
    TEXT_MUTED: tuple[str, str] = ("#64748b", "#64748b")
    TEXT_INVERTED: tuple[str, str] = ("#ffffff", "#0b0f17")

    # Primary Action (Refined Royal / Steel Slate — Non-Neon)
    PRIMARY: tuple[str, str] = ("#2563eb", "#3b82f6")
    PRIMARY_HOVER: tuple[str, str] = ("#1d4ed8", "#2563eb")
    PRIMARY_CONTAINER: tuple[str, str] = ("#eff6ff", "#1e293b")

    # Secondary / Neutral Actions
    NEUTRAL_BTN: tuple[str, str] = ("#e2e8f0", "#243242")
    NEUTRAL_BTN_HOVER: tuple[str, str] = ("#cbd5e1", "#334155")
    NEUTRAL_TEXT: tuple[str, str] = ("#334155", "#e2e8f0")

    # Semantic Status - Success (Muted Emerald)
    SUCCESS: tuple[str, str] = ("#15803d", "#10b981")
    SUCCESS_HOVER: tuple[str, str] = ("#166534", "#059669")
    SUCCESS_BG: tuple[str, str] = ("#ecfdf5", "#064e3b")
    SUCCESS_TEXT: tuple[str, str] = ("#15803d", "#34d399")

    # Semantic Status - Danger (Muted Crimson / Brick)
    DANGER: tuple[str, str] = ("#b91c1c", "#dc2626")
    DANGER_HOVER: tuple[str, str] = ("#991b1b", "#b91c1c")
    DANGER_BG: tuple[str, str] = ("#fef2f2", "#450a0a")
    DANGER_TEXT: tuple[str, str] = ("#dc2626", "#f87171")

    # Semantic Status - Warning (Warm Amber)
    WARNING: tuple[str, str] = ("#b45309", "#f59e0b")
    WARNING_BG: tuple[str, str] = ("#fffbeb", "#451a03")
    WARNING_TEXT: tuple[str, str] = ("#b45309", "#fbbf24")

    # Semantic Status - Info / Active State
    INFO: tuple[str, str] = ("#0284c7", "#38bdf8")
    INFO_TEXT: tuple[str, str] = ("#0369a1", "#60a5fa")

    # Form Controls
    ENTRY_BG: tuple[str, str] = ("#ffffff", "#0f172a")
    ENTRY_BORDER: tuple[str, str] = ("#cbd5e1", "#243242")
    SLIDER_PROGRESS: tuple[str, str] = ("#2563eb", "#3b82f6")
    SLIDER_BUTTON: tuple[str, str] = ("#1d4ed8", "#60a5fa")
    SLIDER_BUTTON_HOVER: tuple[str, str] = ("#1e40af", "#93c5fd")

    # Service Indicators
    STATUS_ACTIVE: tuple[str, str] = ("#16a34a", "#22c55e")
    STATUS_INACTIVE: tuple[str, str] = ("#94a3b8", "#64748b")


@dataclass(frozen=True)
class RadiusTokens:
    """Corner Radius Metrics."""

    CARD: int = 12
    HEADER: int = 12
    BUTTON: int = 8
    INPUT: int = 8
    BADGE: int = 14
    PILL: int = 20
    SLIDER: int = 6


@dataclass(frozen=True)
class SpacingTokens:
    """Padding and Margin Spacing Scale."""

    XS: int = 4
    SM: int = 8
    MD: int = 16
    LG: int = 24
    XL: int = 32


# Global token instances
Colors: Final[ColorTokens] = ColorTokens()
Radius: Final[RadiusTokens] = RadiusTokens()
Spacing: Final[SpacingTokens] = SpacingTokens()


class TypographyManager:
    """Initializes and manages Montserrat typography across UI components."""

    _initialized: bool = False
    _resolved_family: str = FONT_FAMILY_PRIMARY

    @classmethod
    def initialize_fonts(cls) -> None:
        """Loads bundled Montserrat fonts into the CustomTkinter font registry."""
        if cls._initialized:
            return

        # Attempt to load bundled TTF fonts from local assets
        bundled_dir = Path(__file__).resolve().parent / "assets" / "fonts"
        if bundled_dir.is_dir():
            for ttf_file in bundled_dir.glob("*.ttf"):
                try:
                    ctk.FontManager.load_font(str(ttf_file.resolve()))
                except Exception as exc:
                    logger.debug("Failed loading bundled font %s: %s", ttf_file.name, exc)

        cls._initialized = True

    @classmethod
    def get_font(
        cls,
        size: int = 12,
        weight: Literal["normal", "bold"] = "normal",
        slant: Literal["roman", "italic"] = "roman",
    ) -> ctk.CTkFont:
        """Returns a CTkFont instance guaranteed to follow design system typography."""
        cls.initialize_fonts()
        return ctk.CTkFont(
            family=cls._resolved_family,
            size=size,
            weight=weight,
            slant=slant,
        )

    # Standard Typography Scale
    @classmethod
    def display(cls) -> ctk.CTkFont:
        """Display font for main application title."""
        return cls.get_font(size=22, weight="bold")

    @classmethod
    def title(cls) -> ctk.CTkFont:
        """Section header font."""
        return cls.get_font(size=15, weight="bold")

    @classmethod
    def card_title(cls) -> ctk.CTkFont:
        """Card group heading font."""
        return cls.get_font(size=13, weight="bold")

    @classmethod
    def subheading(cls) -> ctk.CTkFont:
        """Field label or subtitle font."""
        return cls.get_font(size=12, weight="bold")

    @classmethod
    def body(cls) -> ctk.CTkFont:
        """Regular body text font."""
        return cls.get_font(size=12, weight="normal")

    @classmethod
    def body_bold(cls) -> ctk.CTkFont:
        """Bold body text font."""
        return cls.get_font(size=12, weight="bold")

    @classmethod
    def caption(cls) -> ctk.CTkFont:
        """Microcopy / descriptions font."""
        return cls.get_font(size=11, weight="normal")

    @classmethod
    def caption_bold(cls) -> ctk.CTkFont:
        """Bold caption font."""
        return cls.get_font(size=11, weight="bold")

    @classmethod
    def badge(cls) -> ctk.CTkFont:
        """Small badge or pill indicator font."""
        return cls.get_font(size=10, weight="bold")

    @classmethod
    def mono(cls) -> ctk.CTkFont:
        """Monospace font for identifiers, paths, or code."""
        return ctk.CTkFont(
            family="Consolas",
            size=11,
            weight="normal",
        )


def create_card(
    parent: ctk.CTkBaseClass,
    *,
    corner_radius: int = Radius.CARD,
    border_width: int = 1,
    border_color: tuple[str, str] = Colors.BORDER_DEFAULT,
    fg_color: tuple[str, str] = Colors.BG_CARD,
    **kwargs: object,
) -> ctk.CTkFrame:
    """Helper factory that creates a consistently styled card container."""
    return ctk.CTkFrame(
        parent,
        corner_radius=corner_radius,
        border_width=border_width,
        border_color=border_color,
        fg_color=fg_color,
        **kwargs,
    )
