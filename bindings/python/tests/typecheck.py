"""Code a type checker must accept (and the lines marked must reject):
``mypy --strict`` runs over it in test_typing.py. Not collected by pytest."""

from typing import List, Optional

import freeqrent as fq

style = fq.Style(
    preset="ink",
    modules=fq.Dots(size=0.8),
    finders=fq.Finders(ring="squircle", eye=fq.CircleEye()),
    palette={"fg": "#1d3b72", "finder.eye": "#c2410c"},
    effect=fq.Glow(radius=0.2),
    textures=[fq.Grain(), "scanlines"],
    intro=fq.Intro(preset="ripple", trigger=fq.Every(ms=4000), ease="expo"),
    idle=["shimmer"],
    encoding=fq.Encoding(ecc="Q"),
)
svg: str = fq.render("https://example.com", style)
frame: str = fq.render_frame(b"\x00\x01", {"preset": "candy"}, 300)
report: fq.Report = fq.check("hi", '{"preset":"ink"}')
ok: bool = report.ok
version: int = report.code.version
plan: fq.Plan = fq.plan("hi", style)
names: List[str] = fq.presets()
result: fq.PaletteResult = fq.Palette.from_css(":root{--a:#123}", min_contrast=0.5)
again = fq.Style(palette=result.palette)

try:
    fq.render("hi", style)
except fq.FreeQrentError as e:
    code: str = e.code
    path = e.detail.get("path")

shape: fq.ModuleShape = fq.Bars(axis="vertical")
if isinstance(shape, fq.Bars):
    axis: Optional[fq.Axis] = shape.axis

bad_preset = fq.Style(preset="nope")  # type: ignore[arg-type]
bad_token = fq.Style(palette={"fgg": "#000"})  # type: ignore[dict-item]
bad_idle = fq.Style(idle=["spin"])  # type: ignore[list-item]
