# freeqrent for Python

Styled and animated SVG QR codes that scan, from freeQRent's Rust core. One
abi3 wheel per platform covers CPython 3.9 and later; there are no Python
dependencies. The output is deterministic: the same payload and style give
the same bytes here as in Rust, JavaScript, Dart and Go.

## Install

```sh
pip install freeqrent
```

Wheels: manylinux and musllinux (x86_64, aarch64), macOS universal2, Windows
x64. Elsewhere pip builds from source, which needs a Rust toolchain.

## Quick start

```python
import freeqrent as fq

svg = fq.render(
    "https://example.com",
    fq.Style(
        preset="ink",
        modules=fq.Dots(size=0.85),
        finders=fq.Finders(ring="squircle", eye=fq.CircleEye()),
        palette={"fg": "#1d3b72", "finder.eye": "#c2410c"},
        intro="ripple",
    ),
)
open("code.svg", "w").write(svg)
```

A style can also be a plain dict or JSON text in the shape of the Style
document (SPEC §10.1), and a dict may hold typed parts:

```python
fq.render("https://example.com", {"preset": "ink", "modules": {"kind": "dots", "size": 0.85}})
fq.render("https://example.com", '{"preset":"candy","intro":"ripple"}')
fq.render(b"\x00\x01raw bytes", {"modules": fq.Liquid()})   # bytes go in byte mode
```

## API

| Function | Returns |
|---|---|
| `render(payload, style=None)` | the SVG (`str`) |
| `render_frame(payload, style, t_ms)` | the intro stopped at `t_ms`, as a still SVG |
| `check(payload, style=None)` | a `Report`: `ok`, `errors`, `warnings` (each an `Issue` with `code`, `message`, `detail`), `min_contrast`, `ecc_budget_used`, `logo_scale`, `steering`, `code` (version, ecc, size, mask) |
| `plan(payload, style=None)` | the animation plan (SPEC §8.5) as a dict |
| `Palette.from_image(rgba, width, height, **opts)` or `Palette.from_image(image, **opts)` | a `PaletteResult` |
| `Palette.from_css(css, **opts)`, `Palette.from_html(html, **opts)` | a `PaletteResult` |
| `presets()` | the preset names |
| `version()`, `__version__` | `"1.0.0"` |

`payload` is `str` (segmented into the shortest modes) or `bytes`,
`bytearray` or `memoryview` (byte mode). `style` is a `Style`, a dict, JSON
text or `None`.

**Palettes.** The options are `clusters`, `seed`, `min_contrast`,
`allow_inverted` and `logo_colors`, all keyword-only and optional. The
result's `.palette` drops straight into a style; `.swatches` and `.choices`
say where each color came from:

```python
result = fq.Palette.from_html(page_html, logo_colors=["#a3302a"])
svg = fq.render("https://example.com", fq.Style(preset="ink", palette=result.palette))
```

`Palette.from_image` takes raw 8-bit RGBA bytes with their size, or anything
shaped like a Pillow image (`.size`, `.mode`, `.convert("RGBA")`,
`.tobytes()`); Pillow itself isn't a dependency. `fq.Image.from_image(img)`
does the same for a halftone's picture:
`fq.Style(halftone=fq.Halftone(image=fq.Image.from_image(img)))`.

**Errors.** Everything the library refuses raises `FreeQrentError`, with
`.code` (stable across languages: `invalid_style`, `data_too_long`,
`contrast_too_low`, ...), `.message` (English) and `.detail` (a dict of the
error's fields):

```python
try:
    fq.render("https://example.com", {"palette": {"fg": "#ccc"}})
except fq.FreeQrentError as e:
    print(e.code, e.detail)   # contrast_too_low {'token': 'fg', 'delta_l': 0.146...}
```

`check` returns a report (with `ok` false) for a style `render` would refuse;
only a bad document or a payload that doesn't fit raise there. A wrong
argument type is a `TypeError`.

**Threads.** Renders release the GIL, so a thread pool renders in parallel
(about 3.8x on four threads).

## Typed styles

`freeqrent/_styles.py` is generated from `schema/freeqrent.schema.json`:

* Each object is a dataclass. Fields left `None` stay out of the document, so
  the core's defaults apply: `fq.Dots().to_dict() == {"kind": "dots"}`.
* Each union tagged by `kind` is one class per variant, named after the kind:
  `Dots`, `Liquid`, `Bars`, `Glow`, `Emboss`, `Grain`, `Every`, `Custom`,
  `Pixels`. A kind two unions share takes its union's name too
  (`SquareModules`, `RoundedRing`, `CircleEye`, `RoundedFrame`,
  `RoundedLogoShape`, `RingLogoFrame`), as do kinds that aren't usable names
  (`NoneFrame`, `ClassTrigger`). Each variant has its tag as `.kind`.
* The unions are type aliases that also take the bare names
  (`ModuleShape = Union[ModuleShapeName, SquareModules, ...]`), so
  `modules="liquid"` type-checks.
* Names are `Literal` types: `Preset`, `Token`, `Idle`, `IntroName`, `Ecc`, ...
* An intro is a preset name, an `Intro(preset=..., duration_ms=..., ...)` or a
  `PaletteShift(palette_from=...)`; a caption is text or a `Caption(...)`.
* Images and raster logos take `bytes` (base64 text works too).

The package ships `py.typed`, and `mypy --strict` checks it.

## Build from source

You need Rust (1.92 or later) and Python 3.9 or later.

```sh
cd bindings/python
python3 -m venv .venv
.venv/bin/pip install maturin pytest mypy
.venv/bin/maturin develop --release      # build into the venv
.venv/bin/python -m pytest               # the tests
.venv/bin/maturin build --release        # a wheel in target/wheels
```

The tests run the 77-case conformance suite (`conformance/cases.json`),
rebuild every conformance style as typed objects, check every error code
they can reach, render from threads, type-check the package, and install a
release wheel into a fresh virtual environment.

## Regenerate the typed styles

After the schema changes:

```sh
python tools/generate_styles.py           # rewrites python/freeqrent/_styles.py
python tools/generate_styles.py --check   # what the tests run: fails if stale
```
