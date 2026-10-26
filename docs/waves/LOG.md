# Wave log

One short entry per W wave, newest first. The protocol is in `README.md`.

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
