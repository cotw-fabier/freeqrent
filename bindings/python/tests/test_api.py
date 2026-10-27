"""The rest of the surface: payload kinds, reports, plans, palettes from
every source (a duck-typed image too), presets and the version."""

import base64
import hashlib
import json
from typing import Any, List, Tuple

import pytest

import freeqrent as fq

from conftest import load_cases

URL = "https://example.com/conformance"


def test_bytes_payloads_are_byte_mode() -> None:
    data = bytes(range(256))
    svg = fq.render(data, None)
    assert svg.startswith("<svg")
    assert fq.render(bytearray(data)) == svg
    assert fq.render(memoryview(data)) == svg
    # Not UTF-8, so only byte mode can carry it.
    assert fq.check(data).ok
    with pytest.raises(UnicodeDecodeError):
        data.decode("utf-8")


def test_bytes_match_the_conformance_byte_case() -> None:
    cases = [c for c in load_cases() if "bytes" in c.get("payload", {})]
    assert cases
    for case in cases:
        data = base64.b64decode(case["payload"]["bytes"])
        svg = fq.render(data, case["style"])
        assert hashlib.sha256(svg.encode()).hexdigest() == case["expect"]["sha256"]


def test_style_forms_agree() -> None:
    typed = fq.Style(preset="ink", modules=fq.Dots(size=0.9))
    as_dict = {"preset": "ink", "modules": {"kind": "dots", "size": 0.9}}
    as_text = json.dumps(as_dict)
    assert fq.render(URL, typed) == fq.render(URL, as_dict) == fq.render(URL, as_text)
    assert fq.render(URL) == fq.render(URL, {}) == fq.render(URL, "{}")


def test_render_frame_stops_the_intro() -> None:
    style = fq.Style(preset="candy", intro="ripple")
    mid = fq.render_frame(URL, style, 400)
    assert "@keyframes" not in mid
    assert mid != fq.render_frame(URL, style, 0)
    # Long after the intro: the finished code, as for a style without one.
    assert fq.render_frame(URL, fq.Style(preset="ink"), 0) == fq.render(URL, fq.Style(preset="ink"))


def test_check_returns_a_parsed_report() -> None:
    report = fq.check(URL, fq.Style(preset="terminal", modules=fq.Dots(size=0.7)))
    assert isinstance(report, fq.Report)
    assert report.ok
    assert [w.code for w in report.warnings]
    assert all(isinstance(w, fq.Issue) and isinstance(w.detail, dict) for w in report.warnings)
    assert report.code.ecc in ("L", "M", "Q", "H")
    assert report.code.size == 17 + 4 * report.code.version
    assert json.loads(report.json)["ok"] is True
    refused = fq.check(URL, {"palette": {"fg": "#bbb"}})
    assert not refused.ok and refused.errors[0].code == "contrast_too_low"


def test_check_reports_steering_for_pictures() -> None:
    rgba = bytes([30, 40, 90, 255]) * 256
    report = fq.check(URL, fq.Style(halftone=fq.Halftone(image=fq.Image(16, 16, rgba))))
    assert report.steering is not None
    assert report.steering.code_modules > 0


def test_plans_parse() -> None:
    plan = fq.plan(URL, fq.Style(intro="ripple"))
    assert plan["tracks"]


def test_palettes_drop_into_a_style() -> None:
    result = fq.Palette.from_css(":root { --brand: #1d3b72; --accent: #c2410c; --paper: #f8f5ee; }")
    assert isinstance(result, fq.PaletteResult)
    assert "fg" in result.palette and "bg" in result.palette
    assert result.swatches and all(0 <= s.share <= 1 for s in result.swatches)
    assert {c.token for c in result.choices} <= set(result.palette)
    assert all(c.origin.kind in ("swatch", "tinted", "darkened", "lightened") for c in result.choices)
    typed = fq.render(URL, fq.Style(palette=result.palette))
    assert typed == fq.render(URL, {"palette": json.loads(result.json)["palette"]})
    html = fq.Palette.from_html("<style>body{background:#fffaf0;color:#2b2118}</style>", logo_colors=["#a3302a"])
    assert fq.render(URL, fq.Style(preset="ink", palette=html.palette)).startswith("<svg")


class FakeImage:
    """Just enough of Pillow's Image: size, mode, convert, tobytes."""

    def __init__(self, mode: str, size: Tuple[int, int], data: bytes) -> None:
        self.mode = mode
        self.size = size
        self._data = data
        self.converted: List[str] = []

    def convert(self, mode: str) -> "FakeImage":
        assert self.mode == "RGB" and mode == "RGBA"
        self.converted.append(mode)
        px = [self._data[i : i + 3] + b"\xff" for i in range(0, len(self._data), 3)]
        return FakeImage("RGBA", self.size, b"".join(px))

    def tobytes(self) -> bytes:
        return self._data


def picture() -> Tuple[bytes, bytes]:
    """An 8x8 picture: navy on the left, cream on the right, as RGB and RGBA."""
    rgb = b""
    for _y in range(8):
        for x in range(8):
            rgb += bytes([20, 30, 80]) if x < 4 else bytes([245, 240, 230])
    rgba = b"".join(rgb[i : i + 3] + b"\xff" for i in range(0, len(rgb), 3))
    return rgb, rgba


def test_palette_from_a_duck_typed_image() -> None:
    rgb, rgba = picture()
    from_bytes = fq.Palette.from_image(rgba, 8, 8, clusters=2, seed=1)
    as_rgba = FakeImage("RGBA", (8, 8), rgba)
    assert fq.Palette.from_image(as_rgba, clusters=2, seed=1) == from_bytes
    as_rgb = FakeImage("RGB", (8, 8), rgb)
    assert fq.Palette.from_image(as_rgb, clusters=2, seed=1) == from_bytes
    assert as_rgb.converted == ["RGBA"]
    assert fq.render(URL, fq.Style(palette=from_bytes.palette)).startswith("<svg")


def test_a_duck_typed_image_makes_a_halftone() -> None:
    rgb, rgba = picture()
    image = fq.Image.from_image(FakeImage("RGB", (8, 8), rgb))
    assert image == fq.Image(8, 8, rgba)
    assert fq.render(URL, fq.Style(halftone=fq.Halftone(image=image))) == fq.render(
        URL, {"halftone": {"image": {"width": 8, "height": 8, "rgba": base64.b64encode(rgba).decode()}}}
    )


def test_presets_and_version() -> None:
    names = fq.presets()
    assert names[:2] == ["classic", "ink"] and len(names) == 8
    for name in names:
        assert fq.render(URL, fq.Style(preset=name)).startswith("<svg")  # type: ignore[arg-type]
    assert fq.version() == fq.__version__ and fq.version().count(".") == 2
    from importlib.metadata import version

    assert version("freeqrent") == fq.version()


def test_the_package_docstring_runs() -> None:
    import doctest

    result = doctest.testmod(fq)
    assert result.attempted > 0 and result.failed == 0


def test_render_is_deterministic() -> None:
    style: Any = fq.Style(preset="sunset", intro=fq.Intro(preset="assemble", stagger=fq.Random(seed=3)))
    assert len({fq.render(URL, style) for _ in range(5)}) == 1
