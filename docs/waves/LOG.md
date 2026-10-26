# Wave log

One short entry per W wave, newest first. The protocol is in `README.md`.

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
