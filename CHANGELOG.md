# Changelog

All four Phase 1 crates (`freeqrent`, `freeqrent-image`, `freeqrent-verify`, `freeqrent-cli`) share one version. From 1.0 on, the core follows semver: enums and reports that may grow are `#[non_exhaustive]`, so new shapes, presets, tokens or warnings arrive in minor releases. Output changes (the SVG for a given input) are called out here, even when the API doesn't change.

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
- **Animation:** eight intro presets, logo and frame intros, idle loops, palette transitions, reduced-motion fallbacks and the exportable `AnimationPlan` (`animate`). The static state is always the finished code.
- **Scannability:**
  - `Style::check` refuses what can't scan and warns on soft risks.
  - `freeqrent-verify` decodes every style option with ZXing and rqrr.
  - The device matrix is recorded for iOS Camera and Google Lens.
- **CLI:** `freeqrent`, covering the whole style surface, with `--gallery` and `--scan-sheet`.
