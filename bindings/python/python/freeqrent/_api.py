"""The boundary functions (SPEC §10.1), in Python."""

from __future__ import annotations

import json
from typing import Any, List, Mapping, Optional, Sequence, Union

from . import _native
from ._base import Doc, to_json_value
from ._pixels import rgba_of
from ._results import PaletteResult, Plan, Report
from ._styles import PaletteOpts

#: What a code carries: text (segmented into the shortest modes) or bytes
#: (byte mode).
Payload = Union[str, bytes, bytearray, memoryview]

#: A style: a typed ``Style`` (or any typed part), a plain document as a
#: dict, JSON text, or ``None`` for the default style.
StyleLike = Union[Doc, Mapping[str, Any], str, None]


def style_json(style: StyleLike) -> str:
    """The Style document for ``style`` as JSON text."""
    if style is None:
        return "{}"
    if isinstance(style, str):
        return style
    if isinstance(style, (Doc, Mapping)):
        return json.dumps(to_json_value(style), ensure_ascii=False)
    raise TypeError(
        f"style must be a Style, a dict, JSON text or None, not {type(style).__name__}"
    )


def _payload(payload: Payload) -> Union[str, bytes, bytearray]:
    if isinstance(payload, memoryview):
        return payload.tobytes()
    if isinstance(payload, (str, bytes, bytearray)):
        return payload
    raise TypeError(
        f"the payload must be str (text) or bytes (byte mode), not {type(payload).__name__}"
    )


def render(payload: Payload, style: StyleLike = None) -> str:
    """The SVG for ``payload`` in ``style``.

    Raises ``FreeQrentError`` for a bad document, a payload that doesn't
    fit, or a style that can't be drawn (the first of ``check``'s errors).
    The render runs without the GIL, so threads render in parallel.
    """
    return _native.render(_payload(payload), style_json(style))


def render_frame(payload: Payload, style: StyleLike, t_ms: int) -> str:
    """The intro stopped at ``t_ms`` milliseconds, as a still SVG with no
    animation. A style without an intro gives its finished code."""
    return _native.render_frame(_payload(payload), style_json(style), t_ms)


def check(payload: Payload, style: StyleLike = None) -> Report:
    """The scan report. A style ``render`` would refuse still has one, with
    ``ok`` false; only a bad document or a payload that doesn't fit raise."""
    return Report.from_json(_native.check(_payload(payload), style_json(style)))


def plan(payload: Payload, style: StyleLike = None) -> Plan:
    """The animation plan (SPEC §8.5) as parsed JSON, for players outside
    CSS."""
    result: Plan = json.loads(_native.plan(_payload(payload), style_json(style)))
    return result


def presets() -> List[str]:
    """The preset names, for ``Style(preset=...)``."""
    names: List[str] = json.loads(_native.presets())
    return names


def version() -> str:
    """The library version, the same in every freeQRent package."""
    return _native.version()


def _opts(
    clusters: Optional[int],
    seed: Optional[int],
    min_contrast: Optional[float],
    allow_inverted: Optional[bool],
    logo_colors: Optional[Sequence[str]],
) -> str:
    return PaletteOpts(
        clusters=clusters,
        seed=seed,
        min_contrast=min_contrast,
        allow_inverted=allow_inverted,
        logo_colors=None if logo_colors is None else list(logo_colors),
    ).to_json()


class Palette:
    """Palettes taken from a picture, a style sheet or a web page. Each
    returns a ``PaletteResult`` whose ``.palette`` drops into a style.

    The options, all optional: ``clusters`` (colors to look for in a
    picture), ``seed``, ``min_contrast`` (the lightness gap to reach),
    ``allow_inverted`` (keep a dark source's polarity), ``logo_colors``
    (colors to keep the frame apart from).
    """

    @staticmethod
    def from_image(
        image: Any,
        width: Optional[int] = None,
        height: Optional[int] = None,
        *,
        clusters: Optional[int] = None,
        seed: Optional[int] = None,
        min_contrast: Optional[float] = None,
        allow_inverted: Optional[bool] = None,
        logo_colors: Optional[Sequence[str]] = None,
    ) -> PaletteResult:
        """A palette from a picture: ``(rgba, width, height)`` with 8-bit
        RGBA bytes row by row, or a Pillow-like image (anything with
        ``.size``, ``.mode``, ``.convert("RGBA")`` and ``.tobytes()``)."""
        if isinstance(image, (bytes, bytearray, memoryview)):
            if width is None or height is None:
                raise TypeError("from_image(rgba, width, height) needs the width and height")
            rgba = bytes(image)
        else:
            if width is not None or height is not None:
                raise TypeError("an image object carries its own size")
            rgba, width, height = rgba_of(image)
        opts = _opts(clusters, seed, min_contrast, allow_inverted, logo_colors)
        return PaletteResult.from_json(_native.palette_from_image(rgba, width, height, opts))

    @staticmethod
    def from_css(
        css: str,
        *,
        clusters: Optional[int] = None,
        seed: Optional[int] = None,
        min_contrast: Optional[float] = None,
        allow_inverted: Optional[bool] = None,
        logo_colors: Optional[Sequence[str]] = None,
    ) -> PaletteResult:
        """A palette from a style sheet's colors."""
        opts = _opts(clusters, seed, min_contrast, allow_inverted, logo_colors)
        return PaletteResult.from_json(_native.palette_from_css(css, opts))

    @staticmethod
    def from_html(
        html: str,
        *,
        clusters: Optional[int] = None,
        seed: Optional[int] = None,
        min_contrast: Optional[float] = None,
        allow_inverted: Optional[bool] = None,
        logo_colors: Optional[Sequence[str]] = None,
    ) -> PaletteResult:
        """A palette from a page's colors: its style sheets and inline
        styles. Nothing is fetched; pass the page's text."""
        opts = _opts(clusters, seed, min_contrast, allow_inverted, logo_colors)
        return PaletteResult.from_json(_native.palette_from_html(html, opts))


__all__: List[str] = [
    "Palette",
    "Payload",
    "StyleLike",
    "check",
    "plan",
    "presets",
    "render",
    "render_frame",
    "style_json",
    "version",
]
