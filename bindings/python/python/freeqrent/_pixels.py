"""Pixels from a Pillow-like image, without depending on Pillow."""

from __future__ import annotations

from typing import Any, Tuple


def rgba_of(image: Any) -> Tuple[bytes, int, int]:
    """8-bit RGBA bytes, row by row, and the size of ``image``.

    ``image`` is duck-typed like a Pillow ``Image``: it has ``.size`` (width,
    height), ``.mode``, ``.convert("RGBA")`` and ``.tobytes()``.
    """
    try:
        width, height = image.size
        mode = image.mode
    except (AttributeError, TypeError, ValueError):
        raise TypeError(
            "expected a Pillow-like image (with .size, .mode, .convert() and .tobytes())"
        ) from None
    if mode != "RGBA":
        image = image.convert("RGBA")
    data = image.tobytes()
    return bytes(data), int(width), int(height)
