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
- **Extra module glyphs:** `Diamond`, `Star`, `Plus`, … (SPEC §7.1). The design-system cards wanted diamonds (art deco), triangles (Bauhaus, playful geometric), hexagons (web3) and plus signs (retro pixels) (W20261026204201).
- **Design-system follow-ups** (W20261026224505): M7.5 built the borders, shadows, data colors, gradient backgrounds, glow, emboss, sketch, textures and captions the design cards asked for. Still open: extra glyphs (above), a soft blurred card shadow (clay, glass), mesh gradients with several centers (aurora), captions in a font the SVG carries (it names generic families only), a left-aligned or top caption, and scan-sheet cards for the M7.5 options, which have bench measurements only.
- **More finder shapes, once they verify:** a `Circle` ring (ZXing missed ~1/200 clean renders from v15 up), a `Diamond` eye (fails ZXing's diagonal cross-check), a `Dashed` ring (failed clean v40 renders). Measured 2026-10-25 (W20261025214510). A design that keeps the diagonal profile might rescue the diamond.
- **`ModuleShape::Custom`:** user-drawn modules. A per-module path callback was in the first spec (§7.1) and is out of 1.0: callbacks aren't plain data and can't cross the binding boundary (deferred 2026-10-25, dropped from the spec W20261026171519). A data form, such as a path template per neighborhood class, would be additive in 1.x.
- **Even smaller dots:** a dot is now one arc, 27 bytes (W20261026163931), and v40 Dots is still ~410 KB. A zero-length stroke with round caps would take ~6 bytes, but palette shifts animate fills, and `AnimationPlan` players fill the geometry; both would need a stroke property first (as halftone strokes do, above). Compact arc flags (`01` with no separator) would save two more bytes, but some path parsers misread them.
- **Re-themable gradient stops** through custom properties (SPEC §6.2).
- **Richer logo CSS:** the sanitizer folds only `.class` rules into attributes; element and descendant selectors, and `pattern` fills, are dropped (W20261025223145).
- **Steering logo-covered modules:** like art mode's free bits (SPEC §7.8), choose data so modules under the plate are light, which would let a backdrop-plated logo count only dark-covered codewords and grow larger (W20261025223145).
- **`Morph` transition:** cross-fade between two shape styles of the same code (SPEC §8.3).
- **Draw-on finder rings for `LockOn`:** a stroke-dashoffset draw-on needs stroke or mask geometry that covers each ring shape exactly; LockOn scales its rings instead (W20261025234738).
- **Intro plus palette shift together:** today `Intro::palette_from` is its own intro with no entrance (W20261025235601).
- **Shimmer clipped to the frame:** the band crosses the whole square canvas, including the corners outside a round frame (W20261025235601).
- **Palette shifts on picture codes:** a picture's sub-cells are strokes, which fill tracks don't reach, so they take the new palette at once (W20261026092854). A stroke property in plans would let them fade.
- **Timing hidden on the picture:** the planned `Hidden-on-image` timing style. Picture codes keep timing whole (SPEC §7.7 rule 2); drawing it into sub-cells needs its own measurement (W20261026092854).
- **Smaller color halftones:** color ink at v20 is 140–270 KB with 6 colors per band, over the mono 150 KB target. Fewer colors, or merging runs across colors with similar lightness, would cut it (W20261026092854).
- **More logo frames:** a double hairline and a dashed "ticket" ring were in the options list but not built (W20261026102223). Both are strokes, like Ring.
- **More logo motion:** a `Spin` intro (the frame turns 90° → 0 as it lands); `Heartbeat` (two quick shrink-pulses every 6 s) and `Sync` (the logo pulses with `FinderPulse`) idles; a hover lift on `Trigger::Hover`; Draw for filled frames (beads appearing in order round the ring) (W20261026102223).
- **True 3D logo turns:** `LogoFlip` fakes an X-axis turn with a vertical scale; a `rotateX` with perspective (a Y-axis turn too) would need CSS 3D transforms in SVG, which renderers handle unevenly, and a plan property every player can reproduce (W20261026105648).
- **Spin and flip intervals:** both run every 8 s; an option for the interval (and for flipping to a back face, such as a second logo) (W20261026105648).
- **Turning square frames:** `LogoTurn` needs a round frame because a turning square leaves the plate. Quarter turns that rest between steps, or a frame inset far enough to clear the plate's corners as it turns, would allow it (W20261026102223).
- **Animated gradient tokens:** palette shifts swap gradients at once; re-themable stops (above) would let them fade (W20261025235601).

## Palettes

- **`--palette-from-url`:** the CLI fetching a page itself, behind an opt-in feature with a small HTTP client. Dropped from M4 to keep TLS out of the CLI's tree; `curl -s URL | freeqrent … --palette-from-html -` does the same (W20261026074436).
- **Linked style sheets:** `from_html` reads inline `<style>` only; a caller could pass `<link rel=stylesheet>` targets back in (W20261026074436).
- **Dark-scheme palettes:** `from_css`/`from_html` skip `@media (prefers-color-scheme: dark)` rules even with `allow_inverted`; reading a light site's dark scheme would give it a dark palette of its own (W20261026141235).
- **Accents from small areas:** a wallpaper's yellow eyes or a single bright sun are too small a share to become an accent, so muted pictures give no `finder.eye` or `logo.frame`; a saliency or chroma-weighted pick could find them (W20261026114047).

## Size

- **A smaller WASM build:** 165 KB gzipped against a 200 KB budget (W20261026180849). Still to try: hash the logo's numbers for the ID prefix by their bits, not their `Debug` text, which links float formatting (about 5 KB gzipped; it changes the prefixes of logo codes); `wasm-opt -Oz` in the M8 build; and `opt-level = "z"` (150 KB).

## Scannability

- **Pending: WeChat and ZXing app scans.** The scan sheet (S01–S69) is recorded for iOS Camera and Google Lens (W20261026164853); its WeChat and ZXing app columns are empty until someone has those apps (W20261026171229). Regenerate the sheet at the release being checked, scan, and add the results to the log and SPEC §9.
- **Scattered logo misses at M and Q:** with the largest logos ECC H now has its own budget (45%, W20261026171519), but the same probe found ZXing missing the degraded render on 2 of 16 codes at M v24 at every budget from 45% to 60% (logos 0.21–0.26 wide), and 3 of 176 at Q v14–24 with no pattern in the budget. Every miss was "no code found", not a failed decode, on the bar's one noisy render, so it may be that render's luck: a second noise seed, or a softer degraded render, would tell.
- **More sizes in the bar:** halftone logos over an alignment pattern passed the bar's 4, 6 and 10 px and missed rqrr at 7 or 12 (W20261026151955). The rule that fixed them came from a 4–14 px sweep, which the picture tests now run; an odd size such as 7 px in `RENDERS` would catch the next gap of this kind everywhere, for about a third more verify time.
- **Contrast in luma, too:** the contrast floor and warning are OKLab lightness differences, but scanners threshold luma. Saturated light colors sit lower in luma than in OKLab L: `terminal`'s first green, #3cff7a on #07100a (ΔL 0.72, no warning), lost v1 codes on the degraded render, while #7dffa8 (ΔL 0.70) passed (W20261026174357). A luma check beside the OKLab one, or a warning when the two disagree, would catch the next case. A second case (W20261026204201): an inverted vermillion finder eye, #ff8a5c on #0a0a0a (ΔL 0.61, luma difference 0.62, no warning), missed ZXing on the degraded render for every payload tried, as eye or as a whole code; #ff9a70 (ΔL 0.64, luma 0.66) passed, and the same color as the ring passed too. The eye is the weak spot, so a stricter floor for `finder.eye` alone is another option.
- **Frameless codes over their declared background:** with `Frame::None` the host page shows through, and contrast is judged against `bg`, but the verify bench rasterizes over white, so an inverted frameless code can't be verified (the preset matrix skips that case, W20261026174357). Rasterizing over `bg` would cover it.
- **Art mode tuned for capture:** the defaults (standard grid, clarity 0) favor the picture and scan slowly on phones; the docs say how to set art mode up for fast scans (SPEC §7.8, W20261026170720). Still open, without changing the defaults: a `Clarity::Auto` that picks the least clarity passing a harsh bar, or the harsh bar as a stricter `verify` level, and a phone check of clarity 0.5–0.6 on the standard grid (scan sheet cards).

## Bindings

- **Native Node build via `napi-rs`**, if WASM performance isn't enough (SPEC §10.2).
- **Swift, Kotlin, and C# packages** over the C ABI.
