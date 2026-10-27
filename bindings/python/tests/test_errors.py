"""Errors are FreeQrentError with code, message and parsed detail, for the
codes of SPEC §10.1."""

import base64
import pickle
from typing import Any, Callable, Dict, List, Tuple

import pytest

import freeqrent as fq
from freeqrent import _native

URL = "https://example.com"
SVG = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><circle cx="5" cy="5" r="4"/></svg>'
GRAY = base64.b64encode(bytes([90, 90, 90, 255]) * 64).decode()


def render(style: Any, payload: Any = URL) -> Callable[[], Any]:
    return lambda: fq.render(payload, style)


# (code, the call, the detail it must carry: key -> value, or None for "any")
CASES: List[Tuple[str, Callable[[], Any], Dict[str, Any]]] = [
    ("invalid_style", render('{"intro":{"trigger":{"kind":"every","ms":"1s"}}}'), {"path": "intro.trigger.ms", "reason": None}),
    ("invalid_style", render("{"), {"path": "", "reason": None, "offset": 1}),
    ("invalid_style", render({"shape": "dots"}), {"path": "shape", "reason": "unknown key"}),
    ("invalid_logo", render({"logo": {"svg": "<html></html>"}}), {"path": "logo.svg", "reason": "not_svg"}),
    ("invalid_logo", render({"logo": {"svg": '<svg xmlns="http://www.w3.org/2000/svg"/>'}}), {"path": "logo.svg", "reason": "no_size"}),
    ("invalid_image", render({"halftone": {"image": {"width": 2, "height": 2, "rgba": "AAAA"}}}), {"path": "halftone.image", "expected": 16, "actual": 3}),
    ("invalid_image", lambda: fq.Palette.from_image(b"\0" * 7, 2, 1), {"expected": 8, "actual": 7}),
    ("invalid_argument", render({}, "\ud800"), {"reason": None}),
    ("invalid_argument", render('{"title":"\ud800"}'), {"reason": None}),
    ("data_too_long", render({"encoding": {"ecc": "H"}}, "x" * 3000), {"bits_needed": None, "bits_available": None, "max_version": 40, "ecc": "H"}),
    ("invalid_version_range", render({"encoding": {"min_version": 9, "max_version": 3}}), {"min": 9, "max": 3}),
    ("contrast_too_low", render({"palette": {"fg": "#ccc"}}), {"token": "fg", "delta_l": None}),
    ("inverted_colors", render({"palette": {"fg": "#fff", "bg": "#000"}}), {"token": "fg"}),
    ("quiet_zone_too_small", render({"quiet_zone": 0}), {"modules": 0}),
    ("option_out_of_range", render({"modules": {"kind": "dots", "size": 5}}), {"option": "Dots.size", "value": 5, "min": 0.6, "max": 1}),
    ("ecc_budget_exceeded", render({"alignment": {"emphasis": "outline"}, "encoding": {"ecc": "L", "version": 2}}), {"damaged": None, "allowed": None}),
    ("logo_too_large", render({"logo": {"svg": SVG, "scale": 0.45}, "encoding": {"ecc": "L"}}), {"max_scale": None}),
    (
        "logo_over_alignment",
        render({"halftone": {"image": {"width": 8, "height": 8, "rgba": GRAY}}, "logo": {"svg": SVG, "scale": 0.1}, "encoding": {"ecc": "H", "version": 7}}),
        {},
    ),
    ("turn_needs_round_frame", render({"logo": {"svg": SVG}, "idle": ["logo-turn"]}), {}),
    ("replay_too_soon", render({"intro": {"preset": "ripple", "duration_ms": 900, "trigger": {"kind": "every", "ms": 1000}}}), {"every_ms": 1000, "intro_ms": 900}),
    ("shadow_needs_background", render({"frame": "none", "shadow": 1}), {}),
    ("no_colors", lambda: fq.Palette.from_css(""), {}),
    ("no_colors", lambda: fq.Palette.from_html("<p>plain</p>"), {}),
]


@pytest.mark.parametrize("code,call,detail", CASES, ids=[f"{c[0]}-{i}" for i, c in enumerate(CASES)])
def test_errors_carry_their_fields(code: str, call: Callable[[], Any], detail: Dict[str, Any]) -> None:
    with pytest.raises(fq.FreeQrentError) as info:
        call()
    e = info.value
    assert e.code == code, (e.code, e.message, e.detail)
    assert isinstance(e.message, str) and e.message
    assert isinstance(e.detail, dict)
    assert set(detail) <= set(e.detail), (detail, e.detail)
    for key, value in detail.items():
        if value is not None:
            assert e.detail[key] == value, (key, e.detail)
    assert str(e) == f"{code}: {e.message}"


def test_the_native_module_raises_the_same_error() -> None:
    with pytest.raises(fq.FreeQrentError) as info:
        _native.render("hi", '{"palette":{"fg":"#ccc"}}')
    assert info.value.code == "contrast_too_low"


def test_render_refuses_with_the_reports_first_error() -> None:
    style = {"palette": {"fg": "#ccc"}}
    report = fq.check(URL, style)
    assert not report.ok
    with pytest.raises(fq.FreeQrentError) as info:
        fq.render(URL, style)
    first = report.errors[0]
    assert (info.value.code, info.value.message, info.value.detail) == (first.code, first.message, first.detail)


def test_check_still_raises_for_a_bad_document_or_a_payload_that_does_not_fit() -> None:
    with pytest.raises(fq.FreeQrentError) as info:
        fq.check(URL, {"modules": 4})
    assert info.value.code == "invalid_style"
    with pytest.raises(fq.FreeQrentError) as info:
        fq.check("x" * 3000, {"encoding": {"ecc": "H"}})
    assert info.value.code == "data_too_long"


def test_errors_pickle_and_compare_by_fields() -> None:
    with pytest.raises(fq.FreeQrentError) as info:
        fq.render(URL, {"quiet_zone": 0})
    e = info.value
    back = pickle.loads(pickle.dumps(e))
    assert (back.code, back.message, back.detail) == (e.code, e.message, e.detail)
    assert isinstance(e, Exception)


@pytest.mark.parametrize(
    "code",
    # The codes no document reaches: text is always segmented into modes it
    # fits (invalid_character), ECI designators come from the core's own
    # table (invalid_eci), writing SVG to a string can't fail (write), and
    # the core doesn't panic (internal). They still arrive the same way.
    ["invalid_character", "invalid_eci", "write", "internal"],
)
def test_every_code_parses_from_the_error_json(code: str) -> None:
    e = fq.FreeQrentError.from_json(f'{{"code":"{code}","message":"m","detail":{{"index":3}}}}')
    assert (e.code, e.message, e.detail) == (code, "m", {"index": 3})


def test_wrong_argument_types_are_type_errors() -> None:
    with pytest.raises(TypeError):
        fq.render(3)  # type: ignore[arg-type]
    with pytest.raises(TypeError):
        fq.Palette.from_image(b"\0" * 4)
    with pytest.raises(TypeError):
        fq.Palette.from_image(object())
