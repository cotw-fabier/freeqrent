"""Typed styles: generated from the schema, never stale, and the same
document (so the same SVG) as the JSON they stand for."""

import base64
import dataclasses
import hashlib
import json
import subprocess
import sys
import typing
from pathlib import Path
from typing import Any, Dict, List, Literal, Union

import pytest

import freeqrent as fq
from freeqrent import _styles

from conftest import load_cases

HERE = Path(__file__).resolve().parent
URL = "https://example.com/conformance"
SVG = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><circle cx="5" cy="5" r="4" fill="#a3302a"/></svg>'


def test_the_generated_module_is_fresh() -> None:
    tool = HERE.parent / "tools" / "generate_styles.py"
    done = subprocess.run([sys.executable, str(tool), "--check"], capture_output=True, text=True)
    assert done.returncode == 0, done.stderr


# --- typed parts from plain documents, by their annotations ---------------

HINTS: Dict[type, Dict[str, Any]] = {}


def hints(cls: type) -> Dict[str, Any]:
    if cls not in HINTS:
        HINTS[cls] = typing.get_type_hints(cls, vars(_styles))
    return HINTS[cls]


def build(tp: Any, value: Any) -> Any:
    """``value`` (plain JSON data) as the typed part ``tp`` describes."""
    origin = typing.get_origin(tp)
    args = typing.get_args(tp)
    if origin is Union:
        members = [a for a in args if a is not type(None)]
        if len(members) == 1:
            return build(members[0], value)
        if isinstance(value, str):
            for m in members:
                if typing.get_origin(m) is Literal and value in typing.get_args(m):
                    return value
            if str in members:
                return value
        if isinstance(value, dict):
            classes = [m for m in members if dataclasses.is_dataclass(m)]
            if "kind" in value:
                for m in classes:
                    if getattr(m, "kind", None) == value["kind"]:
                        return build(m, value)
                raise AssertionError(f"no variant of {tp} for kind {value['kind']!r}")
            if len(classes) == 1 and getattr(classes[0], "kind", None) is None:
                return build(classes[0], value)
            for m in classes:  # an untagged union: the class with these keys
                keys = {m._keys.get(f.name, f.name) for f in dataclasses.fields(m)}
                if set(value) <= keys and getattr(m, "kind", None) is None:
                    return build(m, value)
            raise AssertionError(f"no variant of {tp} for {value}")
        if bytes in members and isinstance(value, str):
            return value
        if isinstance(value, (int, float)) and (float in members or int in members):
            return value
        if isinstance(value, list):
            seqs = [m for m in members if typing.get_origin(m) is not None]
            return build(seqs[0], value)
        raise AssertionError(f"{value!r} isn't a {tp}")
    if origin is Literal:
        assert value in args, (value, args)
        return value
    if dataclasses.is_dataclass(tp):
        assert isinstance(tp, type)
        kw: Dict[str, Any] = {}
        keys = {tp._keys.get(f.name, f.name): f.name for f in dataclasses.fields(tp)}
        for key, v in value.items():
            if key == "kind":
                assert v == tp.kind  # type: ignore[attr-defined]
                continue
            assert key in keys, f"{tp.__name__} has no field for {key!r}"
            name = keys[key]
            if tp is fq.Image and name == "rgba":
                kw[name] = base64.b64decode(v)  # bytes go back to base64
            elif tp is fq.Logo and name == "raster":
                kw[name] = base64.b64decode(v)
            else:
                kw[name] = build(hints(tp)[name], v)
        return tp(**kw)
    if origin in (list, tuple, typing.Sequence, getattr(__import__("collections.abc").abc, "Sequence")):
        item = args[0]
        out = [build(item, v) for v in value]
        return tuple(out) if origin is tuple else out
    if origin in (dict, typing.Mapping, getattr(__import__("collections.abc").abc, "Mapping")):
        return dict(value)
    if tp in (int, float, str, bool):
        return value
    raise AssertionError(f"can't build {tp} from {value!r}")


# Every case with a style, except those whose document the parser refuses
# (an unknown key has no typed form, which is the point of the case).
STYLED = [
    c for c in load_cases() if "style" in c and c["expect"].get("error") != "invalid_style"
]


def test_only_the_refused_documents_are_left_out() -> None:
    left = [c["name"] for c in load_cases() if "style" in c and c not in STYLED]
    assert left == ["error unknown key"]


@pytest.mark.parametrize("case", STYLED, ids=[c["name"] for c in STYLED])
def test_every_conformance_style_has_a_typed_form(case: Dict[str, Any]) -> None:
    style = build(fq.Style, case["style"])
    assert isinstance(style, fq.Style)
    assert style.to_dict() == case["style"]
    expect = case["expect"]
    if case["fn"] not in ("render", "render_frame"):
        return
    p = case["payload"]
    payload = p["text"] if "text" in p else base64.b64decode(p["bytes"])
    if "error" in expect:
        with pytest.raises(fq.FreeQrentError) as info:
            fq.render(payload, style)
        assert info.value.code == expect["error"]
        return
    if case["fn"] == "render":
        svg = fq.render(payload, style)
    else:
        svg = fq.render_frame(payload, style, case["t_ms"])
    data = svg.encode("utf-8")
    assert (hashlib.sha256(data).hexdigest(), len(data)) == (expect["sha256"], expect["length"])


# --- hand-written typed styles and their JSON -------------------------------

PAIRS: List[Any] = [
    (fq.Style(), {}),
    (fq.Style(preset="ink", modules=fq.Dots(size=0.8)), {"preset": "ink", "modules": {"kind": "dots", "size": 0.8}}),
    (fq.Style(modules="liquid"), {"modules": "liquid"}),
    (fq.Style(modules=fq.Bars(axis="vertical")), {"modules": {"kind": "bars", "axis": "vertical"}}),
    (fq.Style(modules=fq.RoundedModules(radius=0.3)), {"modules": {"kind": "rounded", "radius": 0.3}}),
    (
        fq.Style(
            finders=fq.Finders(
                ring=fq.RoundedRing(radius=2),
                eye="circle",
                corners=fq.Corners(top_left=fq.RingAndEye(ring="leaf", eye=fq.LeafEye())),
            )
        ),
        {
            "finders": {
                "ring": {"kind": "rounded", "radius": 2},
                "eye": "circle",
                "corners": {"top-left": {"ring": "leaf", "eye": {"kind": "leaf"}}},
            }
        },
    ),
    (
        fq.Style(alignment=fq.Alignment(shape=fq.Own(ring="squircle", eye="circle"), emphasis="accent")),
        {"alignment": {"shape": {"kind": "own", "ring": "squircle", "eye": "circle"}, "emphasis": "accent"}},
    ),
    (fq.Style(palette={"fg": "#1d3b72", "finder.eye": "#c2410c"}, dark=True), {"palette": {"fg": "#1d3b72", "finder.eye": "#c2410c"}, "dark": True}),
    (fq.Style(frame=fq.CircleFrame(filler_density=0.4, ring="dashed")), {"frame": {"kind": "circle", "filler_density": 0.4, "ring": "dashed"}}),
    (fq.Style(border=fq.Double(width=0.75), shadow=1.0), {"border": {"kind": "double", "width": 0.75}, "shadow": 1.0}),
    (fq.Style(effect=fq.Glow(radius=0.2, strength=0.1)), {"effect": {"kind": "glow", "radius": 0.2, "strength": 0.1}}),
    (fq.Style(effect=fq.Sketch()), {"effect": {"kind": "sketch"}}),
    (fq.Style(textures=[fq.Grain(amount=0.05), "scanlines"]), {"textures": [{"kind": "grain", "amount": 0.05}, "scanlines"]}),
    (fq.Style(caption="scan me"), {"caption": "scan me"}),
    (fq.Style(caption=fq.Caption("$ scan", font="mono", tracking=10)), {"caption": {"text": "$ scan", "font": "mono", "tracking": 10}}),
    (
        fq.Style(logo=fq.Logo(svg=SVG, frame=fq.RingLogoFrame(width=0.4), shape=fq.RoundedLogoShape(radius=0.3)), encoding=fq.Encoding(ecc="H")),
        {"logo": {"svg": SVG, "frame": {"kind": "ring", "width": 0.4}, "shape": {"kind": "rounded", "radius": 0.3}}, "encoding": {"ecc": "H"}},
    ),
    (
        fq.Style(
            intro=fq.Intro(
                preset="ripple",
                duration_ms=900,
                ease=fq.Custom(0.2, 0.0, 0.0, 1.0),
                stagger=fq.Random(seed=7),
                trigger=fq.Every(ms=5000),
            ),
            idle=["shimmer"],
        ),
        {
            "intro": {
                "preset": "ripple",
                "duration_ms": 900,
                "ease": {"kind": "custom", "x1": 0.2, "y1": 0.0, "x2": 0.0, "y2": 1.0},
                "stagger": {"kind": "random", "seed": 7},
                "trigger": {"kind": "every", "ms": 5000},
            },
            "idle": ["shimmer"],
        },
    ),
    (fq.Style(intro=fq.Intro(preset="sweep", trigger=fq.ClassTrigger(name="go"))), {"intro": {"preset": "sweep", "trigger": {"kind": "class", "name": "go"}}}),
    (
        fq.Style(intro=fq.PaletteShift(palette_from={"fg": "#a33"}, duration_ms=800, trigger="hover")),
        {"intro": {"palette_from": {"fg": "#a33"}, "duration_ms": 800, "trigger": "hover"}},
    ),
    (fq.Style(target_size=fq.Millimeters(mm=30)), {"target_size": {"kind": "millimeters", "mm": 30}}),
    (fq.Style(frame=fq.NoneFrame(), quiet_zone=4, seed=2**64 - 1), {"frame": {"kind": "none"}, "quiet_zone": 4, "seed": 2**64 - 1}),
    (
        fq.Style(encoding=fq.Encoding(ecc="Q", min_version=3, max_version=9, mask=5, eci="always")),
        {"encoding": {"ecc": "Q", "min_version": 3, "max_version": 9, "mask": 5, "eci": "always"}},
    ),
    (
        fq.Style(halftone=fq.Halftone(image=fq.Image(2, 1, bytes([0, 0, 0, 255, 255, 255, 255, 255])), ink=fq.Color(colors=3), bands=(0.2, 0.8))),
        {
            "halftone": {
                "image": {"width": 2, "height": 1, "rgba": base64.b64encode(bytes([0, 0, 0, 255, 255, 255, 255, 255])).decode()},
                "ink": {"kind": "color", "colors": 3},
                "bands": [0.2, 0.8],
            }
        },
    ),
]


@pytest.mark.parametrize("typed,doc", PAIRS)
def test_typed_styles_are_their_json(typed: fq.Style, doc: Dict[str, Any]) -> None:
    assert typed.to_dict() == doc
    assert json.loads(typed.to_json()) == doc
    try:
        expected = fq.render(URL, json.dumps(doc))
    except fq.FreeQrentError as e:
        with pytest.raises(fq.FreeQrentError) as info:
            fq.render(URL, typed)
        assert (info.value.code, info.value.detail) == (e.code, e.detail)
        return
    assert fq.render(URL, typed) == expected
    assert fq.render(URL, doc) == expected


def test_unset_fields_are_left_out() -> None:
    assert fq.Dots().to_dict() == {"kind": "dots"}
    assert fq.Style(dark=False).to_dict() == {"dark": False}
    assert fq.Intro().to_dict() == {}


def test_a_dict_may_hold_typed_parts() -> None:
    mixed = {"preset": "candy", "modules": fq.Dots(size=0.9), "textures": (fq.Ink(),)}
    assert fq.render(URL, mixed) == fq.render(URL, '{"preset":"candy","modules":{"kind":"dots","size":0.9},"textures":[{"kind":"ink"}]}')


def test_names_are_literal_types(schema: Dict[str, Any]) -> None:
    assert list(typing.get_args(fq.Preset)) == fq.presets()
    assert list(typing.get_args(fq.Token)) == schema["$defs"]["Palette"]["propertyNames"]["enum"]
    assert list(typing.get_args(fq.IntroName)) == schema["$defs"]["Intro"]["oneOf"][0]["enum"]
    assert fq.SCHEMA_VERSION == fq.version()


def test_every_style_key_is_a_field(schema: Dict[str, Any]) -> None:
    fields = {f.name for f in dataclasses.fields(fq.Style)}
    assert fields == set(schema["$defs"]["Style"]["properties"])
    opts = {f.name for f in dataclasses.fields(fq.PaletteOpts)}
    assert opts == set(schema["$defs"]["PaletteOpts"]["properties"])


def test_variants_carry_their_kind() -> None:
    assert fq.Dots.kind == "dots" and fq.Glow().kind == "glow"
    assert fq.NoneFrame.kind == "none" and fq.ClassTrigger.kind == "class"
    shapes = typing.get_args(fq.ModuleShape)
    assert [getattr(s, "kind", None) for s in shapes[1:]] == ["square", "rounded", "dots", "liquid", "bars"]


def test_a_bad_value_is_refused_by_the_core_with_its_path() -> None:
    with pytest.raises(fq.FreeQrentError) as info:
        fq.render(URL, fq.Style(intro=fq.Intro(trigger=fq.Every(ms=-1))))
    assert info.value.code == "invalid_style"
    assert info.value.detail["path"] == "intro.trigger.ms"


def test_a_value_with_no_place_in_a_document_is_a_type_error() -> None:
    with pytest.raises(TypeError):
        fq.render(URL, {"title": object()})
    with pytest.raises(TypeError):
        fq.render(URL, 42)  # type: ignore[arg-type]
