"""freeQRent: styled and animated SVG QR codes that scan.

    >>> import freeqrent as fq
    >>> svg = fq.render("https://example.com", fq.Style(preset="ink", modules=fq.Dots(size=0.9)))
    >>> svg.startswith("<svg")
    True

A style is a typed ``Style``, a plain dict in the Style document's shape
(SPEC §10.1), or JSON text. Output is deterministic: the same payload and
style give the same bytes in every language.
"""

from . import _native, _styles
from ._api import (
    Palette,
    Payload,
    StyleLike,
    check,
    plan,
    presets,
    render,
    render_frame,
    style_json,
    version,
)
from ._base import Doc
from ._errors import FreeQrentError
from ._results import (
    Choice,
    CodeInfo,
    Issue,
    Origin,
    PaletteResult,
    Plan,
    Report,
    Steering,
    Swatch,
)
from ._styles import *  # noqa: F403

__version__: str = _native.__version__

__all__ = [
    "Choice",
    "CodeInfo",
    "Doc",
    "FreeQrentError",
    "Issue",
    "Origin",
    "Palette",
    "PaletteResult",
    "Payload",
    "Plan",
    "Report",
    "Steering",
    "StyleLike",
    "Swatch",
    "__version__",
    "check",
    "plan",
    "presets",
    "render",
    "render_frame",
    "style_json",
    "version",
]
__all__ += _styles.__all__
