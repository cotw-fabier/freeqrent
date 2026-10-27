"""The conformance suite (SPEC §10.3): every case in conformance/cases.json,
with styles and options re-serialized by Python's own json.dumps, must give
the same SHA-256 and length, or the same error code."""

import base64
import hashlib
import json
from typing import Any, Dict

import pytest

import freeqrent
from freeqrent import _native

from conftest import load_cases

CASES = load_cases()


def payload(case: Dict[str, Any]) -> Any:
    p = case["payload"]
    if "text" in p:
        return p["text"]
    return base64.b64decode(p["bytes"])


def run(case: Dict[str, Any]) -> str:
    """The case's result text, through the public API where it returns text
    and through the native module (whose text the API parses) otherwise."""
    fn = case["fn"]
    if fn == "render":
        return freeqrent.render(payload(case), json.dumps(case["style"]))
    if fn == "render_frame":
        return freeqrent.render_frame(payload(case), json.dumps(case["style"]), case["t_ms"])
    if fn == "check":
        text = _native.check(payload(case), json.dumps(case["style"]))
        assert freeqrent.check(payload(case), json.dumps(case["style"])) == freeqrent.Report.from_json(text)
        return text
    if fn == "plan":
        # The parse is test_plan_parses, below.
        return _native.plan(payload(case), json.dumps(case["style"]))
    if fn == "palette_from_css":
        text = _native.palette_from_css(case["text"], json.dumps(case["opts"]))
        assert freeqrent.Palette.from_css(case["text"], **case["opts"]) == freeqrent.PaletteResult.from_json(text)
        return text
    if fn == "palette_from_html":
        text = _native.palette_from_html(case["text"], json.dumps(case["opts"]))
        assert freeqrent.Palette.from_html(case["text"], **case["opts"]) == freeqrent.PaletteResult.from_json(text)
        return text
    if fn == "palette_from_image":
        image = case["image"]
        rgba = base64.b64decode(image["rgba"])
        text = _native.palette_from_image(rgba, image["width"], image["height"], json.dumps(case["opts"]))
        parsed = freeqrent.Palette.from_image(rgba, image["width"], image["height"], **case["opts"])
        assert parsed == freeqrent.PaletteResult.from_json(text)
        return text
    if fn == "presets":
        text = _native.presets()
        assert freeqrent.presets() == json.loads(text)
        return text
    raise AssertionError(f"unknown fn {fn}")


PLANS = [c for c in CASES if c["fn"] == "plan"]


@pytest.mark.parametrize("case", PLANS, ids=[c["name"] for c in PLANS])
def test_plan_parses(case: Dict[str, Any]) -> None:
    style = json.dumps(case["style"])
    parsed = freeqrent.plan(payload(case), style)
    assert parsed == json.loads(_native.plan(payload(case), style))
    assert parsed["layers"] and "tracks" in parsed


def test_the_suite_has_every_case() -> None:
    assert len(CASES) == 77
    assert len({c["name"] for c in CASES}) == len(CASES)


@pytest.mark.parametrize("case", CASES, ids=[c["name"] for c in CASES])
def test_case(case: Dict[str, Any]) -> None:
    expect = case["expect"]
    if "error" in expect:
        with pytest.raises(freeqrent.FreeQrentError) as info:
            run(case)
        assert info.value.code == expect["error"]
        return
    data = run(case).encode("utf-8")
    assert (hashlib.sha256(data).hexdigest(), len(data)) == (expect["sha256"], expect["length"])
