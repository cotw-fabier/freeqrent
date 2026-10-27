# Changelog

All four Phase 1 crates (`freeqrent`, `freeqrent-image`, `freeqrent-verify`, `freeqrent-cli`) share one version. From 1.0 on, the core follows semver: enums and reports that may grow are `#[non_exhaustive]`, so new shapes, presets, tokens or warnings arrive in minor releases. Output changes (the SVG for a given input) are called out here, even when the API doesn't change.

## Unreleased (core 1.1, M7.5)

New options, all additive; 1.0 styles render byte for byte as before (snapshots and conformance hashes unchanged):

- **Frames:** `Style::border(Border::Solid | Double)` and a hard `Style::shadow(offset)`, outside the quiet zone, with a `shadow` token.
- **Colors:** data colors `fg.2`–`fg.4`, spread per module or per region (`Style::color_spread`); gradient backgrounds (`bg` takes a gradient, judged at its worst point).
- **Effects:** `Style::effect(Effect::Glow | Emboss | Sketch)`, one SVG filter on the code.
- **Textures:** `Style::texture(Texture::Grain | Scanlines | Ink)`.
- **Captions:** `Style::caption(Caption)`, a line under the code with a `caption` token; the viewBox may now be taller than wide.
- **Fix:** animation plans (`AnimationPlan::to_json`) are valid JSON again; numbers below 1 had lost their leading zero. Plan output changes; SVG output doesn't.
- Every option is in the Style document, the schema, the conformance suite (16 new cases, 74 in all) and the CLI, and plans carry what other players need (`effect`, `textures`, `caption`, layers' `fillOpacity`).

## Unreleased (Phase 2, M7 and M8)

Not part of the 1.0 core release. These crates are `publish = false` until the bindings ship together (M9).

- **`freeqrent-doc`:** Style documents (JSON) in; SVG, scan reports, animation plans and palettes out. Errors are structured as `{code, message, detail}`. This is the boundary every binding shares (SPEC §10.1).
- **Schema and conformance:** `schema/freeqrent.schema.json` is generated from the parser's tables. `conformance/cases.json` has 57 cases every binding has to reproduce byte for byte.
- **`freeqrent-ffi`:** the C ABI and `include/freeqrent.h`.
- **npm:** `bindings/js`, WASM for Node and browsers with generated TypeScript types, and an npm `freeqrent` bin.
- **Go:** `bindings/go`, cgo by default and pure Go with `-tags freeqrent_wasm`.
- **Prebuilt binaries:** the C ABI for every platform (`scripts/build-prebuilt.sh`, `.github/workflows/prebuilt.yml`).
- **Dart and Flutter:** `bindings/dart/freeqrent` (FFI through Native Assets) and `bindings/dart/freeqrent_flutter` with the `FreeQrentAnimated` widget.
- **Python:** `bindings/python`, the `freeqrent` package (PyO3, abi3 wheels for CPython ≥ 3.9) with typed styles generated from the schema.
- **M8 groundwork:** `render_frame` (the CLI's `--frame-at`) at the boundary and the C ABI, `freeqrent_alloc`/`freeqrent_dealloc` for WebAssembly hosts, 77 conformance cases, and `bindings/SURFACE.md`, the CLI's whole surface mapped onto the Style document and checked against the CLI byte for byte.

## 1.0.0 (unreleased)

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
