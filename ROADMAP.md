# Roadmap

Ideas for future waves that **haven't landed in `SPEC.md` yet**. When an idea is taken up, it moves into the spec (and out of this file) as part of a wave. Milestones that are already planned live in `SPEC.md` §12, and finished work lives in `docs/waves/LOG.md`.

Each item says where it came from, so we remember why it's here.

## Output formats

- **PNG/raster export:** an optional crate on top of resvg. Deferred 2026-10-25 to stay focused on SVG.
- **Lottie export** from `AnimationPlan`. Deferred 2026-10-25.
- **Android VectorDrawable / AnimatedVectorDrawable** export. Deferred 2026-10-25.
- **SMIL fallback** for animation, for environments that ignore CSS animation in SVG (SPEC §8.4).

## Encoding

- **Micro QR:** the small symbol format (M1–M4). Stretch goal from SPEC §3.1.
- **Structured Append:** one payload split across up to 16 symbols. Deferred because phone camera support is poor.
- **rMQR (rectangular Micro QR):** the strip format from the ISO 18004:2024 extension, for long thin placements.

## Layouts

- **Multi-code layouts:** several redundant mini codes in a row or grid for wide or curved surfaces (the original reading of feature 7, deferred in favor of art mode).

## Styles and motion

- **Extended intro presets:** `Bloom`, `Rain`, `Spiral`, `Ink`, `Glitch`, `Typeset` (SPEC §8.3, extended set).
- **Extra module glyphs:** `Diamond`, `Star`, `Plus`, … (SPEC §7.1).
- **More finder shapes, once they verify:** a `Circle` ring (ZXing missed ~1/200 clean renders from v15 up), a `Diamond` eye (fails ZXing's diagonal cross-check), a `Dashed` ring (failed clean v40 renders). Measured 2026-10-25 (W20261025214510). A design that keeps the diagonal profile might rescue the diamond.
- **`ModuleShape::Custom`:** a per-module path callback (SPEC §7.1). Deferred 2026-10-25: callbacks aren't plain data and can't cross the binding boundary; a data form (e.g. a path template per neighborhood class) could replace it.
- **Smaller dot/rounded output:** v40 `Dots` is ~700 KB. Implicit command repetition or a `<use>`-based dot could cut it a lot. Measured 2026-10-25 (W20261025212852).
- **Re-themable gradient stops** through custom properties (SPEC §6.2).
- **Richer logo CSS:** the sanitizer folds only `.class` rules into attributes; element and descendant selectors, and `pattern` fills, are dropped (W20261025223145).
- **`encode_segments_for` / `encode_bytes_for`:** style-fitting for explicit segments and raw bytes, like `encode_text_for` (W20261025223145).
- **Steering logo-covered modules:** like art mode's free bits (SPEC §7.8), choose data so modules under the plate are light, which would let a backdrop-plated logo count only dark-covered codewords and grow larger (W20261025223145).
- **`Morph` transition:** cross-fade between two shape styles of the same code (SPEC §8.3).
- **Draw-on finder rings for `LockOn`:** a stroke-dashoffset draw-on needs stroke or mask geometry that covers each ring shape exactly; LockOn scales its rings instead (W20261025234738).
- **Intro plus palette shift together:** today `Intro::palette_from` is its own intro with no entrance (W20261025235601).
- **Shimmer clipped to the frame:** the band crosses the whole square canvas, including the corners outside a round frame (W20261025235601).
- **Animated gradient tokens:** palette shifts swap gradients at once; re-themable stops (above) would let them fade (W20261025235601).

## Bindings

- **Native Node build via `napi-rs`**, if WASM performance isn't enough (SPEC §10.2).
- **Swift, Kotlin, and C# packages** over the C ABI.
