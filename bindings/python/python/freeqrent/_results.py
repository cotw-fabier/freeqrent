"""What comes back: scan reports and extracted palettes, parsed.

They are frozen dataclasses rather than ``TypedDict``s: attribute access,
a readable ``repr``, equality, and immutability, so a report can be cached
or shared between threads. Each keeps the JSON it was parsed from in
``.json`` for anyone who wants the raw text. The plan stays a plain dict
(``Plan``): it is a large tree of geometry and keyframes that players walk
generically, and it has no schema of its own to type it from.
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from typing import Any, Dict, List, Literal, Optional

from ._styles import Ecc, Token

#: An animation plan (SPEC §8.5) as parsed JSON.
Plan = Dict[str, Any]


@dataclass(frozen=True)
class Issue:
    """One of a report's errors or warnings: ``{code, message, detail}``."""

    code: str
    message: str
    detail: Dict[str, Any]

    @classmethod
    def from_dict(cls, d: Dict[str, Any]) -> "Issue":
        return cls(code=d["code"], message=d["message"], detail=dict(d["detail"]))


@dataclass(frozen=True)
class Steering:
    """How far a picture steered the code's free bits (halftones, art)."""

    steerable_modules: int
    code_modules: int
    matched_modules: int


@dataclass(frozen=True)
class CodeInfo:
    """The code a report is about."""

    version: int
    ecc: Ecc
    size: int
    mask: int


@dataclass(frozen=True)
class Report:
    """A scan report: whether the style can be drawn, and how close to the
    limits it is. ``ok`` is false for a style ``render`` would refuse;
    ``errors`` then says why."""

    ok: bool
    errors: List[Issue]
    warnings: List[Issue]
    min_contrast: Optional[float]
    ecc_budget_used: Optional[float]
    logo_scale: Optional[float]
    steering: Optional[Steering]
    code: CodeInfo
    json: str = field(repr=False, compare=False, default="")

    @classmethod
    def from_json(cls, text: str) -> "Report":
        d = json.loads(text)
        steering = d["steering"]
        return cls(
            ok=d["ok"],
            errors=[Issue.from_dict(e) for e in d["errors"]],
            warnings=[Issue.from_dict(w) for w in d["warnings"]],
            min_contrast=d["min_contrast"],
            ecc_budget_used=d["ecc_budget_used"],
            logo_scale=d["logo_scale"],
            steering=None if steering is None else Steering(**steering),
            code=CodeInfo(**d["code"]),
            json=text,
        )


@dataclass(frozen=True)
class Swatch:
    """A color found in the source, and its share of it (0 to 1)."""

    color: str
    share: float


@dataclass(frozen=True)
class Origin:
    """Where a token's color came from: a swatch as found (``swatch``), a
    tint of one or of nothing (``tinted``), or a swatch darkened or
    lightened by ``delta_l`` to reach contrast."""

    kind: Literal["swatch", "tinted", "darkened", "lightened"]
    swatch: Optional[int] = None
    delta_l: Optional[float] = None


@dataclass(frozen=True)
class Choice:
    """The color a token got, and why."""

    token: Token
    color: str
    origin: Origin


@dataclass(frozen=True)
class PaletteResult:
    """An extracted palette. ``palette`` drops straight into a style:
    ``Style(palette=result.palette)``."""

    palette: Dict[Token, str]
    swatches: List[Swatch]
    choices: List[Choice]
    json: str = field(repr=False, compare=False, default="")

    @classmethod
    def from_json(cls, text: str) -> "PaletteResult":
        d = json.loads(text)
        return cls(
            palette=dict(d["palette"]),
            swatches=[Swatch(**s) for s in d["swatches"]],
            choices=[
                Choice(token=c["token"], color=c["color"], origin=Origin(**c["origin"]))
                for c in d["choices"]
            ],
            json=text,
        )
