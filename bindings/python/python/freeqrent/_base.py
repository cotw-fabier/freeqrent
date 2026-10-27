"""The base of the typed style classes: turning them into a Style document.

Every generated class (``freeqrent._styles``) is a dataclass over this base.
A field left as ``None`` is left out of the document, so the core's default
applies; a variant of a ``kind`` union writes its ``kind`` first.
"""

from __future__ import annotations

import base64
import dataclasses
import json
from typing import Any, ClassVar, Dict, Mapping, Sequence


class Doc:
    """A part of a Style document."""

    #: Python field name to JSON key, where they differ (``top_left``).
    _keys: ClassVar[Mapping[str, str]] = {}

    def to_dict(self) -> Dict[str, Any]:
        """This part as plain JSON data (dicts, lists, str, numbers, bools)."""
        out: Dict[str, Any] = {}
        # A variant of a ``kind`` union has its tag as a class variable.
        kind = getattr(type(self), "kind", None)
        if kind is not None:
            out["kind"] = kind
        for f in dataclasses.fields(self):  # type: ignore[arg-type]
            value = getattr(self, f.name)
            if value is None:
                continue
            out[self._keys.get(f.name, f.name)] = to_json_value(value)
        return out

    def to_json(self) -> str:
        """This part as JSON text."""
        return json.dumps(self.to_dict(), ensure_ascii=False)


def to_json_value(value: Any) -> Any:
    """``value`` as plain JSON data.

    Typed parts become objects, ``bytes`` become base64 text (images,
    rasters), tuples become lists, and mappings and lists are converted all
    the way down, so a plain dict may hold typed parts.
    """
    if isinstance(value, Doc):
        return value.to_dict()
    if isinstance(value, (bytes, bytearray, memoryview)):
        return base64.b64encode(bytes(value)).decode("ascii")
    if value is None or isinstance(value, (str, bool, int, float)):
        return value
    if isinstance(value, Mapping):
        out: Dict[str, Any] = {}
        for k, v in value.items():
            if not isinstance(k, str):
                raise TypeError(f"Style document keys are text, not {type(k).__name__}")
            out[k] = to_json_value(v)
        return out
    if isinstance(value, Sequence):  # str and bytes were handled above
        return [to_json_value(v) for v in value]
    raise TypeError(f"{type(value).__name__} has no place in a Style document")
