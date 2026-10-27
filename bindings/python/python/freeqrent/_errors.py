"""The one exception the library raises for its own errors (SPEC §10.1)."""

from __future__ import annotations

import json
from typing import Any, Dict, Optional


class FreeQrentError(Exception):
    """A freeQRent error: a refused document, a payload that doesn't fit, a
    style that can't be drawn, a source with no colors.

    Attributes:
        code: A stable snake_case name (``invalid_style``, ``data_too_long``,
            ``contrast_too_low``, ...), the same in every language.
        message: What went wrong, in English.
        detail: The error's fields, e.g. ``{"path": "intro.trigger.ms",
            "reason": "..."}`` for ``invalid_style`` or ``{"token": "fg",
            "delta_l": 0.2}`` for ``contrast_too_low``.
    """

    code: str
    message: str
    detail: Dict[str, Any]

    def __init__(self, code: str, message: str, detail: Optional[Dict[str, Any]] = None) -> None:
        super().__init__(code, message, detail or {})
        self.code = code
        self.message = message
        self.detail = detail if detail is not None else {}

    @classmethod
    def from_json(cls, text: str) -> "FreeQrentError":
        """The error for the boundary's error JSON ``{code, message, detail}``."""
        data = json.loads(text)
        return cls(str(data["code"]), str(data["message"]), dict(data.get("detail") or {}))

    def __str__(self) -> str:
        return f"{self.code}: {self.message}"

    def __repr__(self) -> str:
        return f"FreeQrentError(code={self.code!r}, message={self.message!r}, detail={self.detail!r})"
