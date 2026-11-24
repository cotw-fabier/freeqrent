# Changelog

All four Phase 1 crates (`freeqrent`, `freeqrent-image`, `freeqrent-verify`, `freeqrent-cli`) share one version. From 1.0 on, the core follows semver: enums and reports that may grow are `#[non_exhaustive]`, so new shapes, presets, tokens or warnings arrive in minor releases. Output changes (the SVG for a given input) are called out here, even when the API doesn't change.

## 1.3.0 (unreleased)

### Migrating from 1.2: lenient by default

**Styles that 1.2 refused now render, with warnings.** Every check error is now either a *readability* error (a measured risk, such as low contrast, a narrow quiet zone, a logo past its budget, or glass under its limits) or a *hard* one (the code can't be drawn as asked, or certainly won't decode). `Style::checks(Checks)` picks what is refused:

- `Checks::Lenient` (**the new default**): readability errors render as warnings of the same name (`Warning::Unchecked(error)`; in documents the error's own code under `warnings`), hard errors are refused.
- `Checks::Strict`: refuses both, **1.2's behaviour**. To keep it, pass `.checks(Checks::Strict)`, `"checks": "strict"` or `--checks strict`.
- `Checks::Off`: refuses nothing; out-of-range options are clamped and conflicting ones left out.

Whenever an error renders anyway, `ScanReport.scannable` is false. `Style::allow_unscannable` is deprecated (`true` is `Lenient`, `false` `Strict`). Defaults and presets still check clean and scan at the full bar, and `freeqrent_verify::verify_style` always checks as `Strict` (SPEC §9).

### Core: glass

- **Any shape:** baked glass is cut from the code's own geometry (an in-core path rasterizer and an exact distance transform), so module shapes, finder and alignment shapes, dashed timing and data colors work on glass. Vector glass draws the same shapes. No longer `GlassUnsupported`.
- **Your colours:** `Glass::colors(GlassColors::Palette)` tints each lens with its module's token (lightness stays at the targets); `GlassColors::Photo` (default) and `Neutral`.
- **Texture, refraction, opacity:** `Glass::texture` (photo detail on the plate and finder glass), `Glass::bevel`, `Glass::ior`, `Glass::dome`, `Glass::opacity`.
- **Readability estimate:** `RenderError::GlassLowReadability { share, margin, finder }`, measured: opacity under about 0.95 warns. `GLASS_READABILITY`, `GLASS_DOTS_MIN`.
- Output: every 1.2 glass style renders byte for byte as before (snapshots and conformance hashes unchanged).

### Boundary, CLI and bindings

- Documents: `"checks"`; glass `"colors"`, `"texture"`, `"bevel"`, `"ior"`, `"dome"`, `"opacity"`; error code `glass_low_readability`. Schema and conformance regenerated (120 cases; no existing hash changed).
- CLI and npm bin: `--checks strict|lenient|off`; `--ink glass:colors=…,texture=…,bevel=…,ior=…,dome=…,opacity=…`.
- Python's `Report` and Dart's `ScanReport` gain `scannable`.

## 1.2.0 (unreleased)

### Core

- **Custom shapes** (SPEC §7.1, §7.2): `Glyph::svg` (sanitized like a logo) or `Glyph::raster` (PNG, GIF or WebP with transparency) as the data modules (`Style::module_glyph`), every finder's ring or eye (`Style::finder_glyphs`), or one corner's (`Style::corner_glyphs`). The picture is fitted into each module, 7×7 ring or 3×3 eye, painted as a silhouette in the layer's token, or in its own colors with `Glyph::keep_colors`. New warnings `CustomModules`, `CustomFinder`, `GlyphColors`; new error `GlyphOnPicture`. Plans carry `PlanLayer::glyph`.
- Additive: existing styles render byte for byte as before (snapshots and conformance hashes unchanged).

### Boundary, CLI and bindings

- Style documents take `{"kind":"glyph","svg"|"raster","keep_colors"}` for `modules`, `finders.ring`/`eye` and each corner's; error code `invalid_glyph`. Schema and conformance suite regenerated (88 cases).
- CLI and npm bin: `--shape-file`, `--ring-file`, `--eye-file`, `--corner-ring-file`, `--corner-eye-file`, `--keep-glyph-colors`.
- Typed styles in every binding gain the glyph variants. `FreeQrentAnimated` draws glyph layers: raster glyphs from the style, SVG glyphs through `glyphImage`.
- `freeqrent_verify::Bar::for_report` holds custom finders to the rounded finder's bar.

## 1.1.0 (2026-10-27)

The first GitHub release: the core with M7.5's design styles, and the language bindings (M7, M8) with prebuilt libraries. Nothing is on crates.io, npm, PyPI or pub.dev yet (M9).

### Core (M7.5)

New options, all additive; 1.0 styles render byte for byte as before (snapshots and conformance hashes unchanged):

- **Frames:** `Style::border(Border::Solid | Double)` and a hard `Style::shadow(offset)`, outside the quiet zone, with a `shadow` token.
- **Colors:** data colors `fg.2`–`fg.4`, spread per module or per region (`Style::color_spread`); gradient backgrounds (`bg` takes a gradient, judged at its worst point).
- **Effects:** `Style::effect(Effect::Glow | Emboss | Sketch)`, one SVG filter on the code.
- **Textures:** `Style::texture(Texture::Grain | Scanlines | Ink)`.
- **Captions:** `Style::caption(Caption)`, a line under the code with a `caption` token; the viewBox may now be taller than wide.
- **Fix:** animation plans (`AnimationPlan::to_json`) are valid JSON again; numbers below 1 had lost their leading zero. Plan output changes; SVG output doesn't.
- Every option is in the Style document, the schema, the conformance suite (16 new cases, 74 in all) and the CLI, and plans carry what other players need (`effect`, `textures`, `caption`, layers' `fillOpacity`).

### Bindings (M7, M8)

`freeqrent-doc` and `freeqrent-ffi` stay `publish = false` until the bindings ship to their registries together (M9).

- **`freeqrent-doc`:** Style documents (JSON) in; SVG, scan reports, animation plans and palettes out. Errors are structured as `{code, message, detail}`. This is the boundary every binding shares (SPEC §10.1).
- **Schema and conformance:** `schema/freeqrent.schema.json` is generated from the parser's tables. `conformance/cases.json` has 57 cases every binding has to reproduce byte for byte.
- **`freeqrent-ffi`:** the C ABI and `include/freeqrent.h`.
- **npm:** `bindings/js`, WASM for Node and browsers with generated TypeScript types, and an npm `freeqrent` bin.
- **Go:** `bindings/go`, cgo by default and pure Go with `-tags freeqrent_wasm`.
- **Prebuilt binaries:** the C ABI for every platform (`scripts/build-prebuilt.sh`, `.github/workflows/prebuilt.yml`).
- **Dart and Flutter:** `bindings/dart/freeqrent` (FFI through Native Assets) and `bindings/dart/freeqrent_flutter` with the `FreeQrentAnimated` widget.
- **Python:** `bindings/python`, the `freeqrent` package (PyO3, abi3 wheels for CPython ≥ 3.9) with typed styles generated from the schema.
- **M8 groundwork:** `render_frame` (the CLI's `--frame-at`) at the boundary and the C ABI, `freeqrent_alloc`/`freeqrent_dealloc` for WebAssembly hosts, and `bindings/SURFACE.md`, the CLI's whole surface mapped onto the Style document and checked against the CLI byte for byte.
- **The CLI's palette flags in the document:** `replace_palette` (an extracted palette replaces a preset's, as `--preset` with `--palette-from-*` does) and the palette options' `logo` (an extracted frame keeps apart from an SVG's paints or a raster's pixels, as with `--logo`). The conformance suite has 81 cases.
- **`scripts/bindings.sh`** regenerates everything generated from the library, tests every binding, sets the version and builds the release artifacts.

## 1.0.0 (not released on its own)

The first release. Phase 1 of [`SPEC.md`](SPEC.md), milestones M0–M6:

- **Encoder:** QR versions 1–40 at every ECC level, with numeric, alphanumeric, byte, Kanji (feature `kanji`) and ECI segments and minimal-length segmentation. Masks, version pinning, and ECC raised while the code doesn't grow. Checked against `rqrr`, ISO 18004's worked examples and `qrcodegen`, module for module.
- **Styles:**
  - Square, rounded, dot, bar and liquid modules.
  - Styled finders, with per-corner overrides; alignment and timing patterns.
  - Color tokens and gradients in OKLab-checked palettes; the SVG exposes each token as a CSS custom property.
  - Round frames with filler.
  - Eight presets (`Style::preset`).
- **Logos:** sanitized SVG or raster logos on plates, with frames (ring, beads, stamp, brackets, orbit). Each logo is fitted to its error-correction budget: `encode_*_for` grows the code until the logo fits.
- **Palettes:** from pixels, CSS and HTML (`palette-extract`), with `freeqrent-image` for PNG and JPEG.
- **Pictures:** halftone sub-cells, module steering on the free data bits, clarity maps, and art mode with tracking grids (`halftone`).
- **Animation:** eight intro presets, logo and frame intros, idle loops, palette transitions, reduced-motion fallbacks and the exportable `AnimationPlan` (`animate`). Intros play on load, on hover, when a class is added, or again every few seconds (`Trigger::Every`, CLI `--trigger every:MS`). The static state is always the finished code.
- **Scannability:**
  - `Style::check` refuses what can't scan and warns on soft risks.
  - `freeqrent-verify` decodes every style option with ZXing and rqrr.
  - The device matrix is recorded for iOS Camera and Google Lens.
- **CLI:** `freeqrent`, covering the whole style surface, with `--gallery` and `--scan-sheet`.
