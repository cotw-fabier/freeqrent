# Wave log

One short entry per W wave, newest first. The protocol is in `README.md`.

## 20261026121853 · M5 · Pictures break up in intros

- Landed: group intros (Settle, Ripple, LockOn, Assemble) cut a picture into 3 × 3-module tiles staggered like modules, instead of moving it as one piece. Each group's strokes are plan layers of their own; in the SVG they share one animated group (`Element::joins`, `Motion::joined`), so a six-color art code animates 12 groups, not 281 strokes. Two art gallery cards (Assemble, Ripple; 67 families, all decode).
- Measured: scannable from 750–850 ms of 1200 on a v10 picture in mono and four-color ink; static picture and last frame pixel-identical for every preset (existing test); no animated-element warning.
- Decisions: tiles on module edges (no seams, and clarity still pulls toward each module); Develop keeps whole layers, since it is about the picture developing as one.
- Checks: a verify test that the picture lands in all 12 groups with 12 animated wrappers and scans before the end.

## 20261026115722 · M5 · Gallery replay button off the code; art mode capture

- Landed: the gallery's replay button sits beside each card's title instead of on the code's corner, where it covered modules. The gallery's picture cards use clarity 0.5 and the sparse grid (the dense-grid card 0.75).
- Measured: a harsher bar, close to a phone camera (whole code 160–400 px, one or two blurs, noise, ZXing), separates settings the bench passed alike. With two wallpapers the art defaults decoded 1 time in 10, clarity 0.5 on the sparse grid 6, 0.75 on sparse 8–9; bands, ECC H and ink made no difference. The picture cards went from 0–1 in 10 to 4–7 (the dense grid stays at 2; it needs a bigger print), and the plain picture-palette cards decode 9–10.
- Decisions: library defaults unchanged; the clarity default is a choice for after phone results (ROADMAP).
- Checks: the gallery test follows the cards' new flags.

## 20261026114047 · M5 · Picture gallery cards

- Landed: eighteen picture cards in the gallery (65 families, all decode): art mode (mono, color, picture palette, dense grid), halftones, palettes and linear and radial gradients from a picture, and pictures with framed logos, Shrink, Draw, Ink and Develop intros and the turn, glint, spin, flip, shimmer and breathe idles. `--gallery-picture FILE` (repeatable) supplies the pictures, `sample` otherwise; `--palette-from-image sample`.
- Measured with two of the user's wallpapers (a wolf under a moon for art mode, a city of clouds for color; not in the repository): all 65 cards decode. The wolf and a birds-on-a-branch picture are muted, so extraction gives only `bg` and `fg`, even at 12 clusters; the city gives a blue `finder.eye` and a separate red `logo.frame`, both darkened to L 0.31. A framed logo at the default 0.2 doesn't fit a picture's 0.40 budget (refused, as it should be), so picture logos use 0.12; halftones need a tracking grid to get a version big enough to show the picture.
- Decisions: pictures come in by flag rather than being committed, so the cards work with anyone's images and the repository holds none; the scan sheet's ids stay as they are.
- Checks: the gallery test covers the default (sample) and a given picture.

## 20261026105648 · M5 · Thicker frames, frame colors, Shrink, spin and flip

- Landed: frame defaults Ring 0.8, Beads 0.9, Stamp 0.9, Brackets 0.8 (were 0.45–0.7), growing in proportion for logos wider than 8 modules (`FRAME_REFERENCE`, up to 2.5); `frame_width` up to 2.5; `Logo::moon_size` for the Orbit (default 2.4, `DEFAULT_MOON`, three times the old moon, 0.4–4; the frame width is now the Orbit's line, default 0.4). Palette extraction sets `logo.frame`: a second accent apart from `fg` and the eyes, else the eyes' accent. `LogoIntro::Shrink` (from as wide as the code, hold to 15%, then down over the whole intro, with the code). `Idle::LogoSpin` and `Idle::LogoFlip` (every 8 s, 1.2 s and 1 s, ease-in-out quart sampled into keyframes; together half a loop apart), on a new `LayerRole::LogoArt` drawn inside the plate's group and clipped to the window; `Property::ScaleY`. CLI: `--logo-frame-width`, `--logo-frame-color`, `--logo-moon`, `--logo-intro shrink`, `--idle logo-spin|logo-flip`; five gallery cards (47 families, all decode), scan sheet S61–S65.
- Measured: every frame at width 0.1 and 2.5 with moons 0.4 and 4 scans on the largest logo; the largest dark logo with turn, glint, spin and flip together decodes every 100 ms through the spin and flip (rqrr too); Shrink is scannable from 700–750 ms of 1200, like no logo intro.
- Decisions: spin and flip move only the content, so they can't touch a module and need no round frame; the art nests in the plate's group so a Shrink scales it with its window. Keyframe lists that turn or flip state every transform function in every rule, so browsers interpolate angles rather than matrices. Shrink ignores `LogoTiming`. Following impeccable's animate guidance: no bounce, ease-out for the entrance, in-out for the loops, reduced motion stops them all. Fixed in passing: a layer with both an intro and an idle track had its CSS selector and delays written twice. Snapshots unchanged (none has a frame).
- Checks: core tests for frame growth, set widths and moons, Shrink's start scale, spin and flip tracks and frames, and the second-accent palette; verify tests for width and moon limits, spin and flip scanning; a CLI flag test. The intro tests pin small codes, so their Orbit gets a 0.8 moon.

## 20261026102223 · M5 · Logo sizes, frames, logo intros and idles

- Landed: `LogoSize` (Small/Medium/Large; `Largest` grows to the budget's limit, at least 0.26) and `ScanReport::logo_scale`; `LogoFrame::{Ring, Beads, Stamp, Brackets, Orbit}` with `frame_width` and a `logo.frame` token, inside a plate grown to hold them, with the logo clipped to the frame's window; `LogoIntro::{None, Reveal, Stamp, Draw, Lock, Ink}` and `LogoTiming::{After, Before, With}` on `Intro`, stacking with any preset; `Idle::LogoGlint` and `Idle::LogoTurn`; `Property::Rotate` and `Property::Draw`, `PlanLayer::outline` and `PlanLayer::opacity`; `RenderError::TurnNeedsRoundFrame`. CLI: `--logo-size`, `--logo-frame`, `--logo-intro`, `--logo-timing`; fourteen logo gallery cards (42 families, all decode), appended to the scan sheet as S47–S60.
- Measured: the largest affordable dark logo with each frame, on each plate shape, L–H × v4/v8/v15, scans at the full bar; logo idles decode at every eighth of a turn and through the glint. Time to scannable with a logo intro: After 500–650 ms, With 700–750, Before 750–900 (of 1200).
- Decisions: Turn only on round frames (a turning square leaves the plate); the Orbit idle from the options list became Turn on an Orbit frame; Reveal + After keeps documents byte-identical (a hand-written `Debug` for `Intro` keeps old ID prefixes), and every snapshot is unchanged. Track ends are now rounded instead of lengths, so `Before` can't run 1 ms past the intro.
- Checks: 7 new verify tests (`logo_frames.rs`), 5 core unit tests, a CLI flag test; the intro tests give the logo idles an Orbit-framed logo.

## 20261026100651 · M5 · First device results

- A Pixel 10 XL (built-in camera, which reads codes through Google Lens) read all eight v1 contrast cards of the scan sheet (ΔL 0.60–0.66 on bg L 0.85 and 0.95); the bench's blurred, noisy render had failed six. Decision: no lightness-aware warning in `check`; `EXTRACT_CONTRAST` stays 0.66 until an iPhone result is in (ROADMAP).
- Still to record: the other 38 cards, and iPhone Camera, WeChat and a ZXing app.

## 20261026095000 · M5 · Develop intro and the scan sheet

- Landed: `Preset::Develop` (opacity only, whole layers: the picture to 35% by 45% of the intro, then everything resolves; data modules play the picture on plain codes). `--intro develop`, a Develop gallery card (28 families, all decode). `--scan-sheet FILE`: 46 cards (gallery families, art mode per grid, ink and ECC, pictures with a logo and without steering, v1 contrast probes), each payload naming its card, per-device verdicts saved in the browser and exported as Markdown.
- Measured: Develop is scannable from 650 ms of 1200 everywhere tested (plain v5–v25, halftone, art on standard and dense grids). Bench verdicts on the sheet's v1 contrast cards: on L 0.85 none passes the degraded render up to ΔL 0.66; on L 0.95 from 0.64.
- Not done: the device scans themselves. They need a person with phones; until they're recorded M5 isn't closed.
- Checks: Develop joins every preset loop (static state, last frame, scannable before the end, halftone intros); a CLI test for the sheet.

## 20261026093447 · M5 · Art mode, tracking grids, CLI

- Landed: `TrackingGrid::{Sparse, Standard, Dense}` and `QrBuilder::tracking_grid` (a version range; `encode_text_for` picks the smallest that affords the style); `Style::art(halftone)` (picture, Outline, dotted timing). CLI: `--halftone`, `--art`, `--ink`, `--bands`, `--clarity`, `--clarity-map`, `--fit`, `--focal`, `--priority`, `--no-steer`, `--tracking-grid`; `--check` prints steering; a procedural `sample` picture; Halftone and Art mode gallery cards (27 families, all decode).
- Measured: art mode passes the full bar on every grid × L/M/Q × mono/color × two pictures × short and long URLs; 31–79% of modules steerable, 64–87% follow the picture, Outline ≤ 40% of the budget. A long URL on `Standard` at M lands on v17, not v14.
- Decision: `--art` implies the standard grid unless `--version` pins one; picture options without a picture are a usage error.
- Checks: 2 verify tests (72 art codes; smallest-version picking), a CLI test for the picture flags; the gallery test counts 27.

## 20261026092854 · M5 · Halftone sub-cells

- Landed: `Style::halftone` now draws the picture: 3 × 3 sub-cells per data, ECC and remainder module, the center carrying the module, the rest Floyd–Steinberg-dithered from the picture (area-weighted, centers included). `Ink::Mono` (`fg` over `bg`) and `Ink::Color { colors }` (the picture's colors clamped into dark and light bands, k-means per band); `Halftone::bands`; `Clarity::Uniform | Map`; `HALFTONE_DAMAGE_BUDGET` (0.40) for logos and Outline on picture codes. Output is stroked rows in fifths of a module inside a nested viewBox; plans carry `color`, `cells` and `stroke`; group intros move the picture as one piece.
- Decisions (measured): a 0.6-wide center (even 3 × 3: 118/150 failed at 4 px/module; half-module: rqrr missed 2–18 of 216 off-center); sub-cells squeezed to [0.2, 0.8] (unsqueezed every code failed the degraded render); clarity 0 by default (it all passed); 40% budget (50% left 3/216 rqrr misses with a logo); color bands 0.28/0.94.
- Decision: strokes instead of rectangles halved the paths (v20 mono 65–115 KB, under the 150 KB target; color 140–270 KB). Linear-light OKLab conversion took a v20 render from 114 to 35 ms.
- Checks: 7 core unit tests (centers carry modules, strokes cover exactly their cells, bands hold, plan JSON), a halftone snapshot, 6 verify tests (5 pictures × v5–v27 × L/M/Q × mono/color at the full bar; logos at every size 4–12 px with rqrr; 8 style combinations; size; every preset's static and last frame equal the plain code).
- Follow-up: palette fades on strokes, timing hidden on the picture, smaller color output (ROADMAP).

## 20261026081347 · M5 · Free-bit solver and module steering

- Landed: feature `halftone` (default on, no deps) with `Halftone` (picture, `Fit::Cover|Contain`, focal point, priority map, `steer`) and `Style::halftone`. The free-bit solver: per Reed–Solomon block, effect vectors for every bit after the terminator, elimination in priority order, all 8 masks scored from one elimination. `QrBuilder::encode_text_steered`; `encode_text_for` steers when the style has a picture. `ScanReport::steering` (steerable, code and matched modules).
- Decision (measured): targets are the picture's lightness squeezed into [0.2, 0.8] before dithering. Unsqueezed, flat areas came out solid and 29% of 132 steered codes failed the blurred, noisy ZXing render (hundreds of module errors inside the flat areas: ZXing's 5 × 5-block threshold sits at a flat area's own level); from 0.15 up none failed. Light pinholes every 4 modules weren't enough.
- Decision: steering never boosts the ECC level; Outline rings are wanted light first, which took v7–v14 Outline from 0.55–1.44 of the damage budget to 0–0.44.
- Checks: solver unit tests (payload bits untouched, every block still valid RS, sparse wants always met, forced masks, determinism); 4 verify tests, including five synthetic pictures × v2–v27 × L/M/Q/H at the full bar with matches of 67–90% at L/M (about 50% unsteered). 0.7–46 ms per steered code in release.
- Next: the halftone renderer (§7.7 B).

## M4 complete · 2026-10-26

Palettes: OKLab k-means extraction from pixels, CSS/HTML parsing with role hints, `freeqrent-image`, CLI flags. Waves 20261026073538, 20261026074436, 20261026075708.

## 20261026075708 · M4 · freeqrent-image and CLI palettes

- Landed: the `freeqrent-image` crate (`decode`/`decode_with`, PNG through zune-png and JPEG through zune-jpeg behind default features, every PNG color type, 16-bit stripped to 8, `Limits` checked after headers and before allocation). CLI `--palette-from-image`, `--palette-from-css` (`sample`, `-`), `--palette-from-html` (`-`), `--palette-clusters`, `--palette-contrast`, `--show-palette` (swatches, origins, and reproducing flags); explicit tokens override extracted ones. A "Site palette" gallery card (22 families).
- Decision: the decoders' own size caps are lifted so every oversized image is reported as `TooLarge` by our check, not as a decoder message.
- Checks: round trips through the `png` and `jpeg-encoder` crates; CLI end to end (the printed flags reproduce the same SVG byte for byte); contact sheet of three extracted site palettes rendered with resvg and reviewed. The gallery's 22 families all decode.

## 20261026074436 · M4 · Site palettes from CSS and HTML

- Landed: `Palette::from_css` and `Palette::from_html` (theme-color, `<style>`, `style` attributes), a tolerant scanner with selector and property weights, role hints from properties and custom-property names, `var()` resolution (including `hsl(var(--x))`), dark-scheme rules skipped. The role assigner is now shared: role hints win where present, the pixel rules apply otherwise. `palette_parse` fuzz target with seeds, in CI.
- Decision: the accent may sit ΔE 0.06 from `fg` (was 0.10). Brand blues and teals darkened to pass landed at 0.093 and were dropped.
- Decision: no `--palette-from-url`. The CLI won't fetch (an HTTP client would pull TLS into the CLI's tree); it reads a file or stdin instead. See ROADMAP.
- Fuzzing found two bugs, both fixed and kept as regression tests: an out-of-bounds slice on a stray `=` before a multibyte character, and out-of-memory from `var()` chains that fan out (now capped at 256 visits per declaration). Then 3M runs were clean.
- Checks: five hand-written site styles (classic, design tokens with a dark scheme, dark-first, brand HTML page, colorless) check clean and scan with Rounded modules on v1–v10.

## 20261026073538 · M4 · Image palettes

- Landed: `palette-extract` feature (default, no deps) with `Palette::from_image(&Rgba8Image, PaletteOpts)`: alpha-weighted box downsample to ≤ 64×64, seeded weighted k-means++ in OKLab, role assignment for `bg`/`fg`/`finder.eye` with darkening at constant hue, and a `PaletteReport` (swatches, choices, origins). `Rgba8Image` in the core, always available.
- Decision: extraction's minimum contrast is 0.66, not 0.60. Measured: v1 codes on backgrounds from L 0.80 to 0.95 failed ZXing on the blurred, noisy render up to ΔL 0.64 (every v1 payload tried, none from v2 up); near-white backgrounds pass at 0.60. Vivid colors (chroma > 0.12) never become `bg`; a saturated green background picked from a hue wheel was what exposed this.
- Finding: `Style::check`'s 0.60 warning line has the same v1 gap for hand-set palettes on darker backgrounds. Not changed here; see ROADMAP.
- Checks: eight synthetic sources × three option sets check clean and scan (Square, Dots; v1–v10); extraction and render are deterministic.

## M3 complete · 2026-10-26

Animation done: intros as data (`AnimationPlan`, `to_json`, `render_frame`), CSS keyframes in the `@supports` block that end on the finished code, a grouping engine of whole pieces that keeps the static state pixel-identical, seven presets plus the logo reveal, triggers, reduced motion, palette shifts and theme transitions, and three idle loops. `Develop` moves to M5 with halftone. Waves 20261025233019, 20261025234738, 20261025235601.

## 20261025235601 · M3 · Palette transitions and idle loops

- Landed: `Intro::palette_from(&old)` (an 8-stop OKLab fade per changed solid token, linear between the stops, 600 ms quart), `Style::theme_transition(ms)` (opt-in `transition: fill`, off under reduced motion), `Palette::to_css_vars()`, `Style::idle` with `Idle::{FinderPulse, Shimmer, Breathe}` (endless, fill-mode none, start at the intro's end, sine ease), `LayerRole::Shimmer`, `Warning::MotionContrast`, and fill evaluation in frames (sRGB between the stops, as browsers do). CLI `--from-token`/`--from-fg`/`--from-bg`, `--idle`, `--theme-transition`. Gallery: 21 families, all decode.
- Decisions: linear OKLab lightness keeps contrast between the endpoints' values, so the draft's darker-midpoint routing was dropped. Opposite-polarity shifts warn instead, since nothing can cross them while keeping contrast. Breathe drifts only away from the ink. The theme transition is opt-in, so static output is unchanged. Tokens with a gradient or a different alpha swap at once (§6.5, §8.3 revised).
- Checks: 5 new core unit tests (fade CSS and frames, theme transition, Breathe direction, idle timing and band rest position, CSS vars); `tests/intros.rs`: static state pixel-identical for a shift, a theme transition and each loop with and without Ripple; a same-polarity shift scans at every 25 ms frame (v1–v25) with no warnings; an opposite-polarity shift warns; 720 frames through each loop decode. The two intro snapshots changed in their prefix only (the hash now covers idle loops and transitions).

## 20261025234738 · M3 · The core intro presets

- Landed: `Ripple` (center outward, from 80% about the code's center), `LockOn` (rings close in from 135%, eyes pop at 150 ms, then the rest settles), `Assemble` (seeded 1.5–3 module offsets), and the mask reveals `Sweep` (soft diagonal), `Raster` (crisp top-down, left-right for vertical bars) and `Iris` (soft circle). Plan gains `Target::{Layer, Reveal}`, `Reveal { shape, feather, layers }` and `Property::Reveal`. Each preset has its own default ease and stagger. CLI `--intro` takes every preset. Gallery: 19 families, all decode.
- Decisions: the stylesheet applies the reveal mask (an attribute mask shifted resvg pixels by up to 2 levels even fully open), so the static state stays exact. LockOn scales its rings instead of the spec's dashoffset draw-on, which can't keep every ring shape exact (§8.3 revised). `Develop` waits for halftone (M5).
- Checks: 3 new unit tests (reveals masked only through CSS, LockOn timeline, Assemble offsets); `tests/intros.rs`: 7 presets × 3 shapes × 2 frames × 2 versions plus logos, pixel-identical to the plain style and to the last frame; 7 presets × 5 shapes scan; time to scannable at 1200 ms, v1–v25: Settle 600–800, Ripple 600–900, LockOn 700–850, Sweep 300, Raster 450–700, Iris 200–350, Assemble 650–750 ms. New snapshot `sweep_reveal_with_logo`. Settle's snapshot changed in its prefix only (the intro's hash input gained the optional ease).

## 20261025233019 · M3 · AnimationPlan, grouping, Settle

- Landed: `animate` feature (default on): `Intro` (preset, whole-intro `duration_ms`, `delay_ms`, `Ease` Quart/Quint/Expo/Custom, `StaggerOrder` Radial/Linear/Diagonal/Random/Polar, 2–16 `groups`, `Trigger` Load/Hover/Class, `logo_reveal`), preset `Settle`, `Style::plan` → `AnimationPlan` with a hand-written deterministic `to_json()`, and `Style::render_frame(&qr, t)`, which evaluates the plan to a still. The writer now goes through one element list, so static output is byte-identical (all snapshots unchanged). `freeqrent_verify::scannable_after`. CLI: `--intro`, `--intro-ms`, `--stagger`, `--ease`, `--groups`, `--trigger`, `--frame-at`, `--plan`. `--verify` reports when an intro becomes scannable. Gallery family "Settle intro" (16, all decode), with click-to-replay.
- Decisions: groups are made of whole pieces (4-connected regions, single dots, finders per corner) instead of splitting Liquid regions, so the static state is pixel-identical to the un-animated style and seam-free. The largest region was measured at 1,366 modules (v40). Keyframes leave out their end, and all animation CSS sits in the `@supports` block. Shared rules, with per-element delay and origin. Reduced motion is scoped to a root prefix class. No serde: the user chose `to_json()` (spec §5/§8.5 revised).
- Checks: 8 animate unit tests (easing, partition, regions, timing, last frame, CSS placement, escaped class trigger, JSON); `tests/intros.rs`: 61 shape × frame × order × version cases, plus a logo, pixel-identical to the plain render and to the last frame, all five shapes scan; Settle readable from 600–800 ms of 1200 (v1–v25). Release render v10 Liquid + Settle 0.48 ms (target < 5 ms), v40 3.95 ms. New snapshot `settle_intro_on_dots`. The browser extension wasn't connected, so the motion was reviewed from `render_frame` contact sheets.

## 20261025223145 · M2 · Logos with an error-correction budget

- Landed: `Logo::svg` (allowlist re-serializing sanitizer: tokenize, keep allowlisted elements/attributes as parsed values, fold `.class` rules and `style` into attributes, `url(#id)`/`#id` references only, prefixed IDs, bounded `use`) and `Logo::raster` (signature-checked, base64 `data:` URI). Options `scale`, `shape` (Square/Rounded/Circle), `backdrop`, `padding`, `clear_modules`. Whole-module excavation; finders, timing, format/version and the bottom-right alignment pattern are off limits, other alignment patterns under the plate are dropped whole. `RenderError::LogoTooLarge { max_scale }`; `QrBuilder::encode_text_for` grows ECC then version until the style fits. Fuzz target `logo_svg` with committed seeds. CLI `--logo FILE|sample` and five logo options; the CLI and each gallery family encode for their style.
- Decisions: a logo-covered codeword counts as damaged whatever its value (a logo can read either way), sharing the M1 per-block 60% budget with Outline; validation when a `Logo` is built, so `Style` stays infallible. Measured: the largest affordable logo, dark with no plate included, scanned at the full bar for L/M/Q/H × v2–v40 × three shapes.
- Checks: 9 sanitizer + 5 logo + 1 writer unit tests; `tests/logos.rs` (4 ECC × 4 versions × 3 shapes × plate on/off at `max_scale`, fitting, PNG logo, logo with Liquid/dots/round frame/kept modules); new snapshot `circle_logo_on_liquid`; 2.07M fuzz runs clean after fixing one idempotence bug it found (text of XML-forbidden characters became an empty node). Gallery: 15 families, all decode (Outline now fits at v3/Q, the logo at v5/H).

## M2 complete · 2026-10-25

Liquid, round frames and logos done: contour-traced Liquid with filled inner corners, `Frame` (square, rounded, circle with seeded filler and ring, none) on a shared canvas layout, logos (sanitized SVG or raster) excavated within the per-block ECC budget, and `encode_text_for` to grow a code until its style fits. Waves 20261025221811, 20261025222326, 20261025223145.

## 20261025222326 · M2 · Frames and round codes

- Landed: `Frame::{Square, RoundedSquare(r), Circle { filler_density, ring }, None}`, `FrameRing::{None, Solid, Dashed}`, `Style::seed` with SplitMix64 (default seed: FNV-1a of the modules). A canvas layout every stage reads from. Round codes: the quiet-zone square inscribed in a circle, seeded filler in the cells between (whole cells only, 2-module halo beyond the finders' quiet zones), a filled annulus or dashed ring, a background disc. CLI `--frame`, `--frame-ring`, `--seed`; gallery "Round".
- Decisions (measured): the quiet zone is the code–filler gap, so §7.6 holds; filler as dark as data at density 1 with qz 2 passed the full bar on 60 codes (v1–40), so filler and ring aren't contrast-checked. Square frames leave the prefix hash alone (no output change for M1 styles).
- Checks: 4 scene + 1 writer + 1 rng (reference sequence) unit tests; `tests/frames.rs` (3 densities × 3 rings × 3 shapes × qz 2/4, dark filler, rounded square, none) on the payload set; new snapshot `round_frame_with_dashed_ring`, existing snapshots unchanged.

## 20261025221811 · M2 · Liquid modules

- Landed: `ModuleShape::Liquid { smoothness }` (0–1): contour tracing on the module lattice (dark on the right, so holes wind backwards and nonzero fill suffices; diagonal touches turn right and stay separate blobs), each corner cut by a quadratic with the vertex as control, which rounds convex corners and fills concave ones. `Path::quad_to`. CLI `--shape liquid[:s]` (default 0.8) and a gallery family.
- Decision: one corner rule for convex and concave corners instead of separate fillet geometry; reach is `smoothness / 2`, so a unit edge's two corners never overlap.
- Checks: 4 geometry unit tests (outline, fillet, diagonal split, hole winding); Liquid 0/0.5/1 in the shape and palette round trips; a probe of 168 codes (v1–40 step 3, ECC L and H, smoothness 0.5 and 1) passed the full bar, rqrr and the degraded render included. New snapshot `liquid_with_rounded_finders`; gallery now 13 families, 12 decoding. Release render v10 0.29 ms, v40 2.1 ms (352 KB).

## 20261025214510 · M1 · Scan boxes: finder, alignment and timing styles

- Landed: `FinderStyle` (rings Square/Rounded/Squircle/Leaf, eyes Square/Rounded/Circle/Squircle/Leaf, per-corner overrides), `AlignmentStyle` (Match/Own × None/Accent/Outline), `TimingStyle` (Plain/Dotted/Dashed). `QrMatrix::codeword_map` (verified by rebuilding every RS block from the modules) and a per-block damage budget for cleared modules (`DAMAGE_BUDGET` 60%, `ScanReport.ecc_budget_used`). `freeqrent_verify::{Bar, verify_style}` hold each style to the bar its own report implies.
- Decisions (measured): dropped the `Circle` ring (~1/200 clean ZXing misses from v15), finder `Rounded` capped at 2.5, dropped the `Diamond` eye (ZXing diagonal check) and `Dashed` ring (clean v40 misses); any non-square ring warns `RoundedFinder` (rqrr can't read even r = 0.25). Outline is refused v7–v14 at every ECC; it's mostly for art mode.
- Checks: 72 unit + 13 verify tests (36 ring × eye pairs, 40 alignment combos, timing × shapes, Outline on every budget-fitting version × ECC, a payload sweep for round rings at v15/25/40).

## 20261025212852 · M1 · SVG writer, module shapes, verify crate

- Landed: `Style` (module shape, palette, quiet zone, inverted opt-in, minify, title/desc/label, id prefix) with `render`, `render_to` (any `fmt::Write`), `render_io`, and `check` → `ScanReport { errors, warnings, min_contrast }`. Shapes `Square { gap }`, `Rounded`, `Dots`, `Bars`; integer path geometry with relative commands. Finders and alignment patterns drawn solid on their own layers and tokens. New crate `freeqrent-verify`: resvg rasterization, ZXing (`rxing`) + rqrr decoding, four renders (4/6/10 px/module, and blur + noise at 8).
- Decisions (all measured): resvg paints black for `var()`/`oklch()` outside `@supports`, so attributes carry hex and the token variables live in a prefixed `<style>@supports(…)` block; warning threshold raised to ΔL 0.60 (ZXing misses soft, noisy captures below it, finder eyes first); `Square.gap` capped at 0.25; alignment patterns never use the module shape (dots broke rqrr from v5 up). rqrr judges clean renders only.
- Checks: 62 unit + 6 verify tests (10 shapes, 4 quiet zones, 9 palettes, each at v1/v5/v10/v25 under every render); release render v10 0.08–0.29 ms.
- Follow-up: `Custom` shapes and smaller dot output moved to ROADMAP.

## 20261025211731 · M1 · Colors, tokens, contrast

- Landed: `Color` (hex, `rgb()`, `hsl()`, `oklch()`, `oklab()`, named, `transparent`) with OKLab conversion and chroma-reducing gamut mapping; `Paint` (solid, linear and radial gradients with CSS stop fix-up); `Palette` with the §6.1 token set, per-corner finder tokens and the fallback chain; `Palette::contrast` (OKLab ΔL, compositing, worst point along gradients, polarity). Fuzz target `color_parse`.
- Decisions: color strings are parsed and re-serialized in canonical form, never copied into output; own `f64` `pow`/`cbrt`/`sin`/`cos` because stable `core` lacks them (also keeps math identical across platforms).
- Checks: 49 unit tests (OKLab against Ottosson's reference values, sRGB → OKLab → sRGB over a 18³ grid); bare-metal build; 27.7M fuzz runs clean after fixing one panic it found (byte slicing across a multi-byte char in the gradient prefix check).

## M1 complete · 2026-10-25

Static styles done: CSS colors and OKLab contrast, token palette with gradients, streaming SVG writer (hex attributes plus a prefixed `@supports` token layer), Square/Rounded/Dots/Bars, finder/alignment/timing styles with per-corner overrides, quiet zone, `Style::check` with measured limits and a damage budget, `freeqrent-verify`, the `freeqrent` CLI. Waves 20261025211731, 20261025212852, 20261025214510, 20261025215711.

## 20261025215711 · M1 · Check completion, preview CLI, snapshots

- Landed: `TargetSize` (pixels or millimeters, whole symbol) with `SmallOnScreen`/`SmallInPrint` warnings; `freeqrent-cli` (binary `freeqrent`): style flags mirroring `Style`, `--check`, `--verify`, `--gallery` (twelve style families with the reproducing command and verdict), exit codes 0/1/2; `insta` snapshots of five whole documents; pretty mode nests `<defs>` children.
- Decision: target size is a `Style` field, not a `check` argument, so styles stay plain data. The CLI's `verify` feature (default on) keeps resvg/rxing optional.
- Checks: 73 core unit + 5 snapshot + 9 CLI tests (6 end-to-end on the binary); CLI builds without `verify`; gallery for `https://freeqrent.dev/menu` decodes 11 of 12 families (Outline is refused at v2 over the damage budget, and the page says so).

## M0 complete · 2026-10-25

Encoder done: v1–40, L/M/Q/H, numeric/alphanumeric/byte/Kanji/ECI, exact minimal segmentation, roles, pinning, capacity report. Waves 20261025204222, 20261025205148, 20261025210004, 20261025210241.

## 20261025210241 · M0 · Verification: reference encoder, fuzzing, deny, CI

- Landed: `tests/reference.rs` (module-identical to `qrcodegen` for every version × ECC × mask, same automatic mask on 640 cases, mixed segments + ECI, ISO Annex I); `tests/golden.rs` fingerprints; `fuzz/` with `encode_text` and `encode_segments` targets (decode-back oracle); `deny.toml`; `scripts/check-core-deps.sh`; GitHub Actions CI (checks, bare-metal no_std build, MSRV 1.92, deny, 60 s fuzz smoke).
- Decision: mask penalty rules 1 and 3 rewritten as run-length scoring (qrcodegen's formulation), so auto mask selection has a known answer. Encoding got faster too (v10 ≈0.19 ms).
- Checks: 49 tests with all features; about 388k fuzz runs with no crashes; `cargo deny` clean; bare-metal `thumbv7em-none-eabihf` and Rust 1.92 builds pass.

## 20261025210004 · M0 · Kanji mode

- Landed: `kanji` feature with `Segment::kanji`, a Kanji state in the optimal segmentation DP, and a 6,879-entry JIS X 0208 table generated by `scripts/gen_kanji_table.py` (Python `shift_jis` codec). `Mode::Kanji` always exists so the enum doesn't change shape with features.
- Decision: `EciPolicy::Auto` now looks at the planned segments, not the text: ECI 26 only when a byte segment carries non-ASCII.
- Checks: ISO 7.4.6 example bits; `rqrr` returns Shift-JIS bytes matching Python's encoder for Kanji-only and mixed text, and Kanji text at full capacity in every count band. no_std builds with and without `kanji`.

## 20261025205148 · M0 · Minimal mixed-mode segmentation

- Landed: `Segment::optimal`, an exact shortest-path segmentation over numeric/alphanumeric/byte. `encode_text` uses it, planning once per count-width band. The single-mode `Segment::from_text` is gone; `EciPolicy::applies_to` replaces its ECI logic.
- Decision: track the position inside numeric triples and alphanumeric pairs as DP state instead of the usual fractional per-character costs, so the result is exactly minimal rather than approximately.
- Checks: DP equals an exhaustive memoized search on 8 handwritten and 300 random strings × 3 bands; mixed-mode round trips through `rqrr`, including a code that needs v10+.

## 20261025204222 · M0 · Workspace and encoder core

- Landed: Cargo workspace; `freeqrent` core (`no_std` + `alloc`, zero deps). Encoder for v1–40 × L/M/Q/H with numeric/alphanumeric/byte/ECI segments, RS + interleaving, function patterns, zigzag placement, 8 masks with ISO penalty scoring, `QrMatrix` with per-module roles, builder (version range/pin, ECC boost, forced mask, ECI policy), `capacity_report()`.
- Decisions: `ModuleRole` gains `DarkModule` and `Remainder`; ECI 26 only for non-ASCII text by default; `boost_ecc` on by default; feature flags beyond `std` get declared only when they do something.
- Checks: 24 unit + 7 round-trip + 2 doc tests. `rqrr` decodes every version × ECC at full capacity, every forced mask, mixed segments, UTF-8 with and without ECI, and pixel renders v1–40. Release: v10 encode ≈0.24 ms, v40 ≈2.5 ms.
- Next (M0): minimal mixed-mode segmentation, Kanji feature, known-answer vectors, `cargo fuzz`, `cargo deny`.

## 20261025203745 · pre-M0 · Spec and project setup

- Landed: `SPEC.md` draft v0.3 (core features, animation presets, art mode, bindings design), MIT `LICENSE`, wave protocol, `ROADMAP.md`, `AGENTS.md`.
- Decisions: name freeQRent (`freeqrent` everywhere); Rust core first (Phase 1), bindings second (Phase 2); SVG and animated SVG output only; features 4 and 7 interpreted as styled finder/alignment/timing patterns and art mode (§7.2, §7.8).
- Next: M0, the in-house QR encoder.
