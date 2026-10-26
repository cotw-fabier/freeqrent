# freeQRent: Styled & Animated SVG QR Codes (Rust)

> Status: **v1.0** (Phase 1 core; release candidate until published) · 2026-10-26
> Name: **freeQRent** (package id `freeqrent`; free on crates.io, npm, PyPI, pub.dev, and as a GitHub org as of 2026-10-25)
> License: **MIT**
> Targets: **Rust, Dart/Flutter, TypeScript/JavaScript (Node + browser), Python, Go**
>
> **Phasing:** **Phase 1 builds the Rust core only** (§2, §12). The language bindings are specified here in §10 so the core's design accommodates them, but they are built in **Phase 2**, after the core is stable.

---

## 1. Goals

1. Generate **beautiful, scannable** QR codes as **SVG**, static or animated.
2. **Maximum embeddability:** zero runtime dependencies in the core, `no_std` + `alloc`, compiles to WASM, exposes a C ABI, and its output is deterministic. First-class bindings for **Dart, TS/JS, Python, and Go** follow in Phase 2 (§10). Phase 1 core decisions must not block them: every public option is plain data (serializable), there are no callbacks on the hot path, and the animation plan can be exported (§8.5).
3. **Scannability is a hard constraint, not a suggestion.** Every style option is validated against an error-correction and contrast budget, and the library refuses or warns when a combination is likely to fail.
4. Animation is **optional and additive.** An animated SVG must still render as a complete, scannable static code in renderers that ignore animation (resvg, Inkscape, email clients, print).

### Non-goals (v1)

- Raster output (PNG/JPEG/GIF/video). Output is SVG and animated SVG only. Developers can rasterize with resvg or similar.
- Other animation formats (Lottie, Android VectorDrawable). `AnimationPlan` (§8.5) keeps these possible later; the Flutter widget (§10.4) is the only non-CSS player planned.
- QR **decoding** in the core. Decoding is used only in tests and an optional `verify` crate.
- Network access. The library never fetches URLs. "Site colors" means the caller passes in HTML/CSS text.
- JavaScript in output SVGs. All motion is CSS, with no `<script>`.

---

## 2. Crate Layout

Cargo workspace:

| Crate | Phase | Purpose | Deps |
|---|---|---|---|
| `freeqrent` (core) | 1 | Encoder, matrix, styling, SVG writer, animation, palettes | **none** (`no_std` + `alloc`) |
| `freeqrent-image` | 1 | Decodes PNG/JPEG into the core's `Rgba8Image` buffer type: `decode(&[u8])`, sniffed by signature, sRGB assumed (gamma and ICC ignored), 16-bit reduced to 8, first frame of an APNG, `Limits` checked after the headers and before any pixels are allocated (default 16,384 a side, 40 MP) *(rev. W20261026075708)* | `zune-png`, `zune-jpeg` (small, pure Rust), behind features `png`, `jpeg` (both default) |
| `freeqrent-verify` | 1 | Rasterizes the output and decodes it to prove scannability | `resvg`, `rqrr`, `rxing` (ZXing port) |
| `freeqrent-cli` | 1 | CLI front end (also the main dev/preview tool during Phase 1) | `freeqrent`, `freeqrent-image`, arg parsing (`lexopt`, tiny) |
| `freeqrent-ffi` | 2 | Stable C ABI (`cbindgen` header). This is the base for Dart, Go, and any other FFI host (Swift, Kotlin, C#) | none |
| `freeqrent-wasm` | 2 | `wasm-bindgen` build, published to npm as `freeqrent` for **Node and browsers** | `wasm-bindgen` |
| `freeqrent-py` | 2 | **Python** extension module via PyO3, built with `maturin` and published to PyPI | `pyo3` |
| `bindings/go` | 2 | **Go** module over `freeqrent-ffi` via **cgo, linking prebuilt static libraries** (the default). An optional `freeqrent_wasm` build tag uses an embedded WASM build with `wazero` for pure-Go / `CGO_ENABLED=0` users. | cgo; `wazero` (tag only) |
| `bindings/dart` | 2 | **Dart/Flutter** package (pub.dev) over `freeqrent-ffi` using Native Assets (`native_toolchain_rs`) and `ffigen`-generated bindings, plus a native Flutter animation widget (§10.4). Dart web uses the WASM build. | `freeqrent-ffi` |

Binding crates sit outside the core. Their dependencies (`pyo3`, `wasm-bindgen`) never reach Rust users of `freeqrent`.

Core feature flags (all additive, all dependency-free):

| Feature | Default | Enables |
|---|---|---|
| `std` | yes | `std::io::Write` sink, `std::error::Error` impls |
| `animate` | yes | Intro animations & palette transitions |
| `palette-extract` | yes | Auto color tokens from pixel buffers / CSS text |
| `halftone` | yes | Image-embedded QR (§7.7) |
| `kanji` | no | Kanji mode: a JIS X 0208 table, 6,879 entries (adds ~27 KB) |

**Size budget** *(rev. W20261026180849)*: core WASM build ≤ 200 KB gzipped with default features, checked in CI by `scripts/wasm-size.sh`. It builds a cdylib that reaches every default feature (presets, intros and plans, logos, CSS/HTML/image palettes, art mode) for `wasm32-unknown-unknown`, at `opt-level = "s"` with LTO, one codegen unit, `panic = "abort"` and stripping. Measured: 165 KB gzipped (391 KB raw), and 150 KB at `opt-level = "z"`. The first budget, 120 KB, was set before logos, pictures, extraction and motion existed. Even one preset render links most of the crate, since `render` reaches every feature's code for whatever the style carries (144 KB at `s`). Sharing one sort across the crate took 186 KB down to 165; nothing else found cuts more without cutting features. `wasm-opt` (M8) should take more off.

**MSRV:** latest stable minus 4 releases. `#![forbid(unsafe_code)]` in core.

---

## 3. QR Encoder (in-house, zero-dep)

The encoder is written in-house rather than taken from the `qrcode` crate. That keeps the core dependency-free, lets us expose internals the styling layer needs (function-pattern map, data/ECC module classification, per-module "importance"). It also enables the art-mode features (§7.7, §7.8), which need direct control over padding bits.

### 3.1 Scope

- **Versions** 1–40, **ECC** L / M / Q / H.
- **Modes:** Numeric, Alphanumeric, Byte, Kanji (feature), **ECI**. Byte segments hold UTF-8. With `kanji`, characters that have a JIS X 0208 form (kanji, kana, full-width forms; table generated by `scripts/gen_kanji_table.py`) can go in Kanji mode at 13 bits each. Characters without one, such as U+FF5E, stay in bytes. `EciPolicy::Auto` (default) writes ECI 26 (UTF-8) only when a byte segment carries non-ASCII bytes, so all-Kanji text needs no header; `Never` and `Always` override it. *(rev. W20261025204222, W20261025210004)*
- **Automatic segmentation:** minimal-length mixed-mode encoding (`Segment::optimal(text, version)`). It is a shortest-path DP over characters whose state is the mode plus the position inside its packing group (3 digits, 2 alphanumerics), so every cost is an exact bit count and the result is the true minimum. `encode_text` plans once per count-width band (v1–9, 10–26, 27–40). *(rev. W20261025205148)*
- **Mask selection:** the 4-rule ISO penalty, with an option to force a mask. Rules 1 and 3 are scored from run lengths, with each line framed by light runs standing in for the quiet zone. Rule 3 counts 1:1:3:1:1 runs at any width n that have ≥4n light on one side and ≥n on the other. This is the same formulation as Nayuki's `qrcodegen`, so automatic mask choice is cross-checked against it. *(rev. W20261025210241)*
- **Free-bit solver (QArt technique):** after the payload and terminator, the remaining data bits in a symbol are ignored by decoders. Reed–Solomon is linear over GF(2), so every module in the data/ECC region is an XOR of data bits. The encoder solves this map per Reed–Solomon block: each free bit gets an effect vector (its own module plus the ECC of a lone bit there, from scalar multiples of RS remainders), and the wanted modules are taken in priority order, each claiming a free vector that reaches it and being eliminated from the rest (Cox's column pivoting). The pivots don't depend on the mask, so all 8 masks are scored (priority-weighted matches) from one elimination. Free bits start as the standard pad bytes, so modules nobody asks for keep standard padding. This is the engine behind §7.7 and §7.8; it is crate-internal, reached through `encode_text_for` and its bytes and segments forms (§3.3) *(rev. W20261026173421)*. Measured: 0.7–46 ms per code from v2 to v27 (release). *(rev. W20261026081347)*
- **Version pinning:** choose a version explicitly (or by "alignment grid size", §7.8) even when the payload would fit in a smaller one.
- **Structured Append:** deferred (poor phone support). The matrix model shouldn't preclude it.
- **Micro QR:** stretch goal, v1.x.
- Inputs: `&str` (UTF-8), `&[u8]` (raw bytes), or explicit segments.

### 3.2 Matrix model

```rust
pub struct QrMatrix {          // fields private; read through accessors
    version: Version,
    ecc: Ecc,
    mask: Mask,
    size: u16,                 // modules per side
    modules: Vec<bool>,        // dark/light, row-major, (x, y) = (column, row)
    roles: Vec<ModuleRole>,    // per-module classification
}

pub enum ModuleRole {
    Finder { corner: Corner, ring: FinderRing }, // Outer, Gap, Eye
    Separator,
    Timing,
    Alignment { center: (u16, u16) },
    Format,
    Version,
    DarkModule,                // the single fixed dark module
    Data,
    Ecc,
    Remainder,                 // 0–7 leftover bits after the last codeword
}
```

Accessors: `version()`, `ecc()`, `mask()`, `size()`, `is_dark(x, y)`, `role(x, y)`, `modules()` (iterator of `(x, y, dark, role)`). `ModuleRole::is_function()` is false only for `Data`, `Ecc` and `Remainder`. *(rev. W20261025204222)*

The styling layer uses `roles` to render finders, alignment, and timing patterns separately from data modules. It also uses them to know which modules can be sacrificed for logos and imagery. Function patterns are never sacrificed.

### 3.3 Helpers

- `QrCode::encode(text, Ecc) -> Result<QrMatrix>`
- `QrCode::builder()` with `.min_version()`, `.max_version()`, `.version()` (pin), `.tracking_grid(TrackingGrid)` (an art-mode version range, §7.8), `.ecc()` (minimum, default M), `.boost_ecc(bool)` (raise ECC as long as the version doesn't grow; default `true`), `.mask()`, `.eci()`. It finishes with `.encode_text(&str)`, `.encode_bytes(&[u8])` or `.encode_segments(&[Segment])`, or with `.encode_text_for(&str, &Style)`, `.encode_bytes_for(&[u8], &Style)` and `.encode_segments_for(&[Segment], &Style)` (§7.3; *W20261026160240*), which also steer the code when the style draws a picture (§7.7 A; `encode_text_steered` was folded into them, W20261026173421). A steered code keeps the ECC level asked for: boosting would trade steerable bits for error correction the picture doesn't spend. *(rev. W20261025204222, W20261026081347, W20261026093447)*
- `matrix.capacity_report()` returns data bits used and available, data/ECC/total codewords, block count, correctable codewords (net of the small-version misdecode reserve), and `damage_tolerance_permille()`. The logo and halftone budgets use this.

---

## 4. Rendering Pipeline

```
payload ─▶ encode ─▶ QrMatrix ─▶ Layout ─▶ Geometry ─▶ Paint ─▶ (Animate) ─▶ SVG writer
                                  │          │          │
                          frame/round   shapes→paths  tokens→CSS vars
                          logo cutout   finders       gradients
                          multi-code    merging
```

1. **Layout:** places one or more matrices in the canvas, applies the frame (square/circle), quiet zone, and logo cutout, and generates decorative filler modules for round codes.
2. **Geometry:** turns modules into SVG path data per shape style. Modules are **merged into as few `<path>` elements as possible**, which keeps file size small and makes animation cheap (§8.4).
3. **Paint:** resolves color tokens into CSS custom properties and gradient defs.
4. **Animate:** adds `<style>` keyframes and assigns elements to stagger groups.
5. **SVG writer:** a streaming writer into `core::fmt::Write`. There is no DOM or tree allocation.

### 4.1 Output guarantees

- **Deterministic:** the same inputs and the same library version produce byte-identical output. Any randomness comes from a seeded internal PRNG (SplitMix64). `Style::seed` sets the seed; by default it's an FNV-1a hash of the modules, which stands in for the payload since `QrMatrix` doesn't keep it. *(rev. W20261025222326)*
- Coordinates are in **module units** (`viewBox="0 0 N N"`), numbers are written with ≤ 3 decimals, and there are no redundant attributes.
- Output is valid SVG 1.1 with CSS. It contains no `<script>`, no `<foreignObject>`, and no external references (logos/images are inlined as `data:` URIs or nested SVG).
- Optional `<title>`/`<desc>` and `role="img"` + `aria-label`. `QrMatrix` doesn't keep the payload, so the label defaults to "QR code" and `Style::label` sets it; the CLI passes the URL for link codes. *(rev. W20261025212852)*
- Option: `minify` (default on) or pretty-printed.

---

## 5. Public API Sketch

```rust
use freeqrent::{QrCode, Ecc, Style, ModuleShape, FinderStyle, RingShape, EyeShape, Frame, FrameRing, Palette, Logo, Intro, Preset};

let style = Style::new()
    .modules(ModuleShape::Liquid { smoothness: 0.8 })
    .finders(FinderStyle::new(RingShape::Squircle, EyeShape::Circle))
    .frame(Frame::Circle { filler_density: 0.5, ring: FrameRing::None })
    .palette(Palette::from_tokens([
        ("bg", "#f7f3ec"),
        ("fg", "oklch(32% 0.08 265)"),
        ("finder.eye", "oklch(62% 0.19 35)"),
    ])?)
    .logo(Logo::svg(include_str!("logo.svg"))?.scale(0.22).backdrop(true))
    .intro(Intro::new(Preset::Settle).duration_ms(900));

// The smallest code that can afford the logo (ECC raised, then version):
let qr = QrCode::builder().ecc(Ecc::Q).encode_text_for("https://example.com", &style)?;
let svg: String = style.render(&qr)?;   // -> Result<String, RenderError>

// Streaming, no intermediate String:
style.render_to(&qr, &mut writer)?;

// Validation without rendering:
let report = style.check(&qr);   // ScanReport { errors, warnings, min_contrast, … }; report.ok()
```

- `RenderError` is returned for **hard failures**, such as a logo too large for the ECC budget (`LogoTooLarge { max_scale }`), contrast below the floor, or no quiet zone.
- `ScanReport.warnings` covers **soft risks**, such as inverted colors, low-contrast gradients, rounded finder rings, or very small module size at the target print size.
- **Growth stays additive** *(W20261026173421)*: enums that may gain variants (shapes, finder parts, frames, intros, idles, tokens, paints, errors, warnings) and report structs that may gain fields (`ScanReport`, `CapacityReport`, `PaletteReport`, the plan's parts) are `#[non_exhaustive]`; option structs with public fields have constructors (`FinderStyle::new`, `AlignmentStyle::new`). Fixed sets from ISO 18004 (`Ecc`, `Corner`, `ModuleRole`, `FinderRing`) and `Axis` stay exhaustive. The parts of an `AnimationPlan` live in `freeqrent::plan`.
- `Style` implements `Clone`. Storing styles as JSON comes with the Phase 2 bindings, outside the core, which stays free of serde *(rev. W20261025233019)*.
- **Presets** *(W20261026174357)*: `Style::preset(name) -> Option<Style>` returns one of eight named styles, and `Style::presets()` lists them in gallery order: `classic` (the default style), `ink` (dark blue dots, round eyes, warm paper), `candy` (liquid berry, squircle finders), `terminal` (phosphor green on black, inverted), `sunset` (rounded modules in a red-to-violet gradient), `bubble` (a round code of dots with a ring), `ribbon` (horizontal teal bars) and `leaf` (forest green, leaf finders). A preset is an ordinary `Style`: builder methods change any part, and a logo, picture or intro goes on top. Names are lowercase and exact. Presets are static; motion is the caller's choice. `terminal` raises `Warning::Inverted`, and `candy`, `bubble` and `leaf` raise `Warning::RoundedFinder`; the others check clean. Measured: every preset scans at L, M, Q and H on v1, v5, v10 and v25, and with each of five module shapes in its place and each of four frames (`freeqrent-verify/tests/presets.rs`). `terminal`'s green is #7dffa8, not a more saturated #3cff7a: that one has OKLab ΔL 0.72 from the background, but Rec. 601 luma (what scanners see) puts it nearer, and ZXing lost v1 codes on the degraded render.

---

## 6. Color Tokens (feature 5)

### 6.1 Token set

| Token | Applies to | Default |
|---|---|---|
| `bg` | Canvas background (can be `transparent`; solid colors only) | `#fefcf8` (warm white) |
| `fg` | Data modules | `#17161c` (near-black) |
| `finder.outer` | Finder outer ring | `fg` |
| `finder.eye` | Finder 3×3 center | `fg` |
| `alignment` | Alignment patterns | `finder.outer` |
| `timing` | Timing patterns | `fg` |
| `filler` | Decorative modules (round frame) | `fg` at 0.35 opacity |
| `logo.backdrop` | Plate behind logo | `bg` |
| `logo.frame` | The frame on the logo's plate (§7.3) | `fg`, or an accent found by §6.4 *(rev. W20261026102223, W20261026105648)* |
| `frame` | Frame stroke/ring | `fg` |
| `finder.top-left.outer`, `finder.top-right.eye`, … | One finder's ring or eye (per-corner override, §7.2) | `finder.outer` / `finder.eye` |

Values can be any CSS color: hex (`#rgb[a]`, `#rrggbb[aa]`), `rgb()`/`rgba()`, `hsl()`/`hsla()` (legacy comma and modern space/slash syntax), **`oklch()`**, `oklab()`, the CSS named colors, or `transparent`. The library parses all of these internally, so it can do contrast math. `fg`, `finder.*` and `logo.frame` also accept **gradients**: `linear-gradient(<angle> | to <side>, stops…)` and `radial-gradient(stops…)`, where each stop is a color with an optional percentage (CSS fix-up rules fill in missing positions). Gradients span the whole code, not each module. *(rev. W20261025211731)*

Parsed values are **re-serialized**, never copied: each color keeps the space it was written in (hex, `oklch()`, `oklab()`) at fixed precision and is written back in a canonical short form (`oklch(32% .08 265)`). Nothing from the input string reaches the SVG verbatim. OKLCH/OKLab colors outside sRGB are gamut-mapped by reducing chroma at constant lightness and hue, for the literal fallback and the contrast math. The core implements its own `pow`/`cbrt`/`sin`/`cos` on `f64` (stable `core` has none), so color math is identical on every platform. *(rev. W20261025211731)*

### 6.2 Output form

Paint is written at two levels, because renderers disagree about CSS:

```svg
<svg … class="qr"><style>@supports (color:var(--a)){.qr1a2b3c4d-fg{fill:var(--qr-fg,oklch(32% .08 265))}…}</style>
  <path class="qr1a2b3c4d-fg" fill="#2b2f55" d="…"/>
```

- **Attributes** carry plain sRGB hex (gamut-mapped from OKLCH/OKLab), plus `fill-opacity` for translucent colors. Every renderer understands these.
- **A `<style>` block inside `@supports (color:var(--a))`** gives each painted token a rule that reads its custom property, falling back through the token chain to the canonical value: `var(--qr-finder-outer,var(--qr-fg,#17161c))`. When a value is OKLCH/OKLab the condition adds `and (color:oklch(0% 0 0))`, so a browser without OKLCH keeps the hex. Renderers that don't understand `@supports` skip the block. This matters: resvg paints **black** when it meets `var()` or `oklch()` in an inline `style` or a plain rule, so the §6.2 draft (variables declared on the root, `fill="var(--qr-fg)"`) would have broken every non-browser renderer. `freeqrent-verify` pins this behavior with a test.
- Custom properties are **not declared** on the root, so a host page can set `--qr-fg` on the `<svg>` or any ancestor to re-theme an inline code. Alpha stays in `fill-opacity`, so variable values are opaque.
- Class names and gradient IDs carry a **prefix**, by default `qr` plus an FNV-1a hash of the modules, quiet zone and palette: a `<style>` in inline SVG applies to the whole page, so two different codes must never share a class. `Style::id_prefix` overrides it.
- Gradients become `<linearGradient>`/`<radialGradient>` in user space over the code area (CSS angle convention; radial reaches the corners). Wide-gamut stops get a class rule in the same `@supports` block. Gradient stops aren't re-themable through variables yet.

*(rev. W20261025212852)*

### 6.3 Scannability rules for color

- **Contrast floor:** the luminance difference between every painted token and `bg` (computed in OKLab L) must be ≥ 0.40 to render: at that level codes decode from clean renders. Values below **0.60** produce a warning, because `freeqrent-verify` measured that ZXing starts missing blurred, noisy captures below about 0.6, and finder eyes go first (a grey eye at ΔL 0.51 failed 5 of 6 soft renders; 0.54 and up passed). *(rev. W20261025212852)* Translucent paints are composited over `bg` first (a translucent `bg` is judged over white). For gradients, the check uses the **worst point**: renderers interpolate in premultiplied sRGB, which can drift from the stops, so each span is sampled at 9 points. A gradient that crosses the background's lightness has zero contrast. *(rev. W20261025211731)*
- **Polarity:** dark-on-light is the default. Inverted codes (light modules on dark) are allowed with `.allow_inverted(true)` and a warning, because some older scanners fail on them. `freeqrent-verify` reads them inverted, as scanners that support them do (§9); plain ZXing finds no code in an inverted render. *(rev. W20261026141235)*
- **Dark twins:** `Palette::dark_twin()` flips a palette's polarity while keeping what scanners see. Every set color (gradient stops too) keeps its OKLCH chroma, hue and alpha, and its luma is mirrored to `k − Y′`, where `Y′` is Rec. 601 luma of the gamma-encoded sRGB color, the signal camera binarizers and `freeqrent-verify` threshold. `k` puts `bg` at the luma of OKLab L 0.16, or lower when the darkest color would otherwise pass white. The twin of a gray palette is therefore its photographic negative, and it scans like the original, read inverted. A palette whose `fg` is already lighter than `bg` is its own twin. *(W20261026143324)* Measured: mirroring OKLab L instead kept the library's contrast numbers but not the scanner's. A red eye at ΔL 0.47 that decodes on the light code moved near the binarizer's threshold, and ZXing lost the clean render. Under mirrored luma, OKLab contrast never drops and grows for mid-tones (that eye: 0.44 → 0.59). Hand palettes and the eight extracted corpus palettes scan at the same bar as twins, and none warns about low contrast where its palette doesn't.
- Finder colors may vary in hue but must meet the same contrast floor.
- **The logo's frame** (`logo.frame`, §7.3) sits on the plate, among modules the budget already counts as lost, so it has a floor of its own, `FRAME_FLOOR` = **0.25**, with no warning band, and isn't part of `ScanReport::min_contrast`. *(W20261026160831; until then the check didn't judge the frame at all.)* Measured on the largest logo each code affords (every frame on square and circle plates, v4–v17 × L/M/Q/H, square and dot modules, light and dark twin): frames from 0.25 to 0.66 scan exactly alike, gray, orange or blue; frames at 0.13 and 0.05 missed the degraded render more often on dark twins. The only misses from 0.25 up were the largest logos at ECC H on v16–17, which miss with a 0.66 frame too (ROADMAP).

### 6.4 Auto tokens (feature 6) — `palette-extract`

**From an image:** `Palette::from_image(&Rgba8Image, PaletteOpts) -> Result<(Palette, PaletteReport), ExtractError>` *(rev. W20261026073538)*
- The core takes a raw RGBA buffer (`Rgba8Image`: owned, sRGB, not premultiplied; always available, since halftone reads it too). Decoding PNG/JPEG lives in `freeqrent-image`, so the core stays dependency-free.
- Algorithm: box-average down to ≤ 64×64 in linear light, weighted by alpha (cells under 5% coverage are dropped, so a logo's transparent surroundings don't become a color), then cluster in **OKLab** with weighted k-means++ (k = 6 by default, 2–12; seeded, default 0; at most 24 Lloyd rounds). Every step is f64 in a fixed order, so the result is deterministic. Then assign roles:
  - `bg` = the lightest cluster with ≥ 10% share, L ≥ 0.85 and chroma ≤ 0.12 (pastels yes, vivid colors no). Otherwise a tinted near-white, `oklch(97% c h)`, with the dominant hue at a fifth of its chroma (at most 0.015), or neutral.
  - `fg` = the darkest cluster with ≥ 2% share. If it doesn't reach the minimum contrast, the dominant hue (the most common cluster with chroma ≥ 0.04) is darkened in OKLCH at constant chroma and hue until it does.
  - `finder.eye` = the cluster with chroma ≥ 0.06 and the highest share × chroma, other than `fg`'s, darkened if needed. It's left unset (following `fg`) when there is none or when it lands within ΔE 0.06 of `fg` (a brand blue or teal darkened to pass sits about 0.09 from a near-black ink and still reads as its hue). *(rev. W20261026074436)*
  - `logo.frame` = an off color for the logo's frame (§7.3): the next most prominent hue that lands at least ΔE 0.06 from `fg`, `finder.eye` and the logo's colors, else `finder.eye`'s swatch when it's that far from the logo. It's left unset (following `fg`) when `finder.eye` is, or when nothing is apart from the logo. The frame isn't read (§6.3), so it is only darkened (or lightened) to ΔL 0.30 and keeps a bright accent's lightness: a single accent frames the logo in its own color while the eyes take it darkened. The logo's colors come from `PaletteOpts::logo_colors(&[Color])` (the first four chromatic ones count), filled from `Logo::colors()`, a vector logo's solid `fill`, `stroke` and `stop-color` paints after sanitizing; the CLI passes them for `--logo` (a PNG or JPEG logo gives its swatches). *(rev. W20261026105648, W20261026160831: the frame was darkened to the read tokens' 0.66, so a sunset orange landed at L 0.31, and the sample site's accent framed the sample logo in its own orange)*
  - The minimum contrast is `PaletteOpts::min_contrast`, default **0.66** (`EXTRACT_CONTRAST`), range 0.60–0.90. Measured: v1 codes on backgrounds between L 0.80 and 0.95 fail ZXing on the blurred, noisy render up to ΔL 0.64 and pass from 0.66, while near-white backgrounds pass at 0.60. So extracted palettes check clean and pass the full verify bar. Phones are more forgiving: a Pixel 10 Pro XL and an iPad mini read v1 codes at ΔL 0.60–0.66 on L 0.85 and 0.95 (W20261026164853). The floor stays at 0.66 anyway, because dropping it would leave extracted palettes that check clean but miss the bar *(W20261026170720)*.
  - Colors come out as `oklch()`.
- **Dark sources** keep their polarity with `PaletteOpts::allow_inverted(true)` (off by default; render the result with `Style::allow_inverted`). A source is dark when its most strongly marked low-chroma background (CSS) is below L 0.5, or, when it marks none, when its swatches average below L 0.5 by share. Then the rules above run mirrored: `bg` is the darkest low-chroma swatch with L ≤ 0.25 (the marked one first), else a tinted near-black `oklch(16% c h)`; `fg` is the lightest ink; colors short of the contrast are lightened at constant chroma and hue (gamut-mapped, so chroma drops near white). The minimum contrast is at least **0.70** (`INVERTED_CONTRAST`). Measured (W20261026141235): v1 codes (eight payloads × L/M/Q), light on backgrounds from L 0.04 to 0.28, read inverted, fail ZXing on the blurred, noisy render at ΔL 0.66 on some backgrounds and pass on all of them from 0.68; light backgrounds at L 0.85 behaved the same on that hue. A light source is unchanged by the option. *(W20261026141235)*
- The `PaletteReport` lists the clusters as `Swatch { color, share }`, most common first, and one `Choice { token, color, origin }` per token set, where `origin` is `Swatch(i)`, `Tinted(Option<i>)`, `Darkened { swatch, delta_l }` (the contrast before darkening) or, on a dark `bg`, `Lightened { swatch, delta_l }`. `ExtractError::NoColors` when every pixel is transparent.

**From a site:** `Palette::from_css(&str)` / `Palette::from_html(&str)`, with the same return type, and `from_css_with` / `from_html_with`, which take `PaletteOpts` (its minimum contrast and `allow_inverted`; clusters and seed are for images) *(rev. W20261026074436, W20261026141235)*
- `from_html` reads `<meta name="theme-color">` (weight 6: an accent, or a background when it is a light neutral), `<style>` elements and `style` attributes. It skips comments, `<script>`, `<template>`, `<textarea>` and `<title>` contents, and theme colors and sheets with a dark `media`. `from_css` reads a style sheet.
- The CSS scanner is tolerant: it never fails on malformed input, descends into `@media`, `@supports`, `@layer`, `@container` and `@scope`, skips `@media (prefers-color-scheme: dark)` and every other at-rule block, and drops nested rules. It reads colors only from color-carrying properties (`background*`, `color`, `border*`, `outline*`, `*-color`, `*-shadow`, `fill`, `stroke`) and custom properties: hex, `rgb()`/`hsl()`/`oklch()`/`oklab()`, named colors, colors inside gradients, and `var()` chains (resolved against every custom property in the input, up to 8 deep, with fallbacks; `hsl(var(--primary))` with bare components works). Colors under 50% alpha are ignored; the rest count as opaque. Equal colors merge (`#fff`, `white` and `rgb(255 255 255)` are one swatch).
- **Ranking:** each occurrence adds weight. The selector weight is the largest in the list: `:root`/`html` 4, `body` 3, links and buttons 2 (`a`, `button`, `[type=submit]`, classes with `btn`, `button`, `cta`, `primary`, `accent`, `brand` or `link`), else 1. Borders, shadows and the like count half. Each occurrence also marks a role: `background` → bg, `color` → text, anything on a link or button → accent. Custom property definitions count when the name says what the color is for (`--primary`, `--accent`, `--brand`, `--link` → accent; `--bg`, `--background`, `--surface`, `--paper` → bg; `--text`, `--fg`, `--foreground`, `--ink` → text). A scale like `--color-blue-500` counts only where it's used.
- **Roles** use the image rules, except that the most strongly marked eligible swatch wins each role: the heaviest light, low-chroma bg-marked swatch is `bg`, the heaviest text-marked swatch that passes is `fg`, and the heaviest accent-marked chromatic swatch is `finder.eye`. Colors keep the form the author wrote; darkened and tinted ones are `oklch()`. Dark-first sites still get a light `bg` (their light text color, or a tinted near-white), unless inverted palettes are allowed: then they keep their own dark background. A site's `prefers-color-scheme: dark` rules are still skipped either way.
- **Limits:** 16 levels of block nesting, 256 value and `var()` visits per declaration (so chains that fan out stay linear), 512 bytes per substituted color function, 1,024 distinct colors (the 32 heaviest reach the assigner). The `palette_parse` fuzz target checks that nothing panics and every palette it returns keeps `fg` and `finder.eye` at 0.66 contrast. Fuzzing found and fixed an out-of-bounds slice on a stray `=` before a multibyte character, and an exponential `var()` fan-out.
- No network access. Callers fetch the page; the CLI reads a file or standard input (`curl -s https://example.com | freeqrent … --palette-from-html -`).

### 6.5 Animated palette change

*(rev. W20261025235601)*

- `Intro::palette_from(old: &Palette)` cross-fades every solid token from its value in `old` to the style's. The default is 600 ms, ease-out quart. Tokens that use a gradient, or a different alpha, in either palette switch at once. Each fade is eight keyframes sampled in OKLab at evenly spaced times through the curve, played linearly. Browsers interpolate hex between neighboring keyframes in sRGB, so the path stays close to OKLab. Under reduced motion the swap is instant.
- **Runtime re-theme** is opt-in: `Style::theme_transition(ms)` gives every solid token rule `transition: fill {ms}ms` (ease-out quart). A host page that changes `--qr-fg` on an inline code then gets a smooth change, and reduced motion turns it off. It is opt-in so static output stays the same for everyone else. `palette.to_css_vars()` gives the set tokens as `--qr-bg:#…;--qr-fg:…` for that one-liner in JS (opaque values; the document keeps alpha in `fill-opacity`).
- **Contrast during the fade.** All tokens share one curve and move linearly in OKLab, so each token's lightness difference from `bg` is a linear blend of its two endpoint values. Two palettes that pass therefore pass at every moment. Measured: four payloads (v1–v25) decoded at every 25 ms frame of a same-polarity shift. `Style::check` samples the path the way browsers play it and raises `Warning::MotionContrast` if a token ever dips below the floor. That always happens between palettes of opposite polarity, which no path can cross while keeping contrast. The draft's "route through a darker midpoint" was dropped: with linear OKLab lightness it is never needed for same-polarity palettes, and it can't help opposite ones.

---

## 7. Visual Features

### 7.1 Module shapes (feature 2)

| Shape | Description | Geometry strategy |
|---|---|---|
| `Square` ("box") | Classic squares, optional `gap` (0–0.25; at 0.3 rqrr lost codes at 6 px/module) | Merge horizontal runs into rects, then one path |
| `Rounded { radius }` | Squares with corners rounded only where they don't touch a neighbor | Per-corner neighbor test (4-neighborhood) |
| `Dots { size }` | Circles, size 0.6–1.0 | One arc per module (below); `size < 0.75` raises a warning |
| `Liquid { smoothness }` | Neighboring modules flow into one blob, with concave inner corners filled ("gooey"). Smoothness 0–1 | Contour tracing on the module lattice: boundary edges keep dark on the right (outer contours clockwise, holes counter-clockwise, nonzero fill), and where two modules touch only diagonally the walk turns right so they stay apart. Every corner is cut with one quadratic Bézier whose control point is the vertex, reaching `smoothness / 2` along each edge: convex corners round off, concave ones get the fillet. One subpath per contour. *(rev. W20261025221811)* |
| `Bars { axis, radius }` | Vertical or horizontal pills that merge runs along an axis | Run-length per row/column → rounded rects |
| `Diamond`, `Star`, `Plus`, … | Fun extras (v1.x) | Per-module glyph |

Shape logic is local: a module's geometry depends only on its neighbors in the same layer (`Rounded` looks at the 4-neighborhood). Geometry is integer, in thousandths of a module, and path commands are relative. A circle (dots, filler, beads, round eyes, dotted timing) is one arc from its leftmost point nearly all the way round to a thousandth of a module below it, closed across that chord, rather than two half-circle arcs: 27 bytes a dot instead of 49, and the chord is far below a pixel (circles under a tenth of a module across keep the two arcs). Measured (W20261026163931): v10/v20/v40 Dots fall from 69/215/717 KB to 40/124/413 KB, with a round frame from 116/352/1157 KB to 68/203/667 KB; renders of the old and new v40 differ by at most 15 of 255 grey levels, on edge pixels only. Alignment patterns are never drawn in the module shape, even before they are styled: dots or gaps would break the 1:1:1:1:1 profile scanners look for (`Dots` at 0.6 lost rqrr from v5 up until alignment was drawn solid). *(rev. W20261025212852)*

### 7.2 Scan boxes: finders, alignment & timing patterns (feature 4)

A QR code has two kinds of "scan box":

- **Finders:** the three large 7×7 corner boxes that locate the code.
- **Alignment patterns:** smaller 5×5 boxes that scanners use to correct perspective and curvature. Version 1 has none. From version 2 they form a grid that grows with the version: 1 box (v2–6), 6 (v7–13), 13 (v14–20), 22 (v21–27), up to 46 (v35–40).

**Timing patterns** are the alternating dotted lines that run between the finders.

All three are rendered as separate layers from the data, so they can be emphasized and styled on their own. That is how the reference design gets its grid of crisp boxes over a busy image (§7.8).

Finder styles (`FinderStyle { outer, eye }`, independent ring and eye):
- Ring (`RingShape`): `Square`, `Rounded(r)` (r ≤ 2.5 modules; rounded rings stay concentric, inner radius r − 1), `Squircle` (4-cubic superellipse, n ≈ 4), `Leaf` (three corners rounded, the one pointing at the code's center sharp).
- Eye (`EyeShape`): `Square`, `Rounded(r)` (r ≤ 1.5), `Circle`, `Squircle`, `Leaf`.
- Per-corner overrides of shape (`FinderStyle::corner`) and color (`finder.top-left.outer`, …).
- **Measured, not shipped** (ROADMAP): a `Circle` ring, and `Rounded` above 2.5, made ZXing miss about 1 in 200 clean renders from v15 up (2.75: 1/360; 3.0: 2/360; 2.5: 0/720). A `Diamond` eye failed ZXing on clean 10 px/module renders at every version: ZXing also cross-checks finders along the diagonal. A `Dashed` ring failed clean v40 renders.
- Any non-square ring raises `Warning::RoundedFinder`: ZXing-family scanners read it, but decoders that fit the ring's square corners to find perspective (quirc, rqrr) lose it. Even a 0.25-module radius made rqrr miss renders.

Alignment styles (`AlignmentStyle::new(shape, emphasis)`):
- Shape: `Match` (the top-left finder's shapes, radii scaled × 5/7 for the ring and × 1/3 for the eye), or `Own { outer, eye }` (ring radius ≤ 2.5, which is a circle; eye ≤ 0.5). Alignment patterns never use the data module shape.
- `emphasis`: `None` (painted `fg`, blending with data), `Accent` (the `alignment` token; the default, which falls back to `finder.outer`), or `Outline` (also on a clean `bg` plate one module wider: the data/ECC/remainder modules in that ring are left out). Outline spends the error-correction budget (§7.3 rule): a codeword is damaged when one of its cleared modules was dark, counted per Reed–Solomon block, and each block may lose at most `DAMAGE_BUDGET` = 60% of its correctable codewords (`(ecc − p) / 2`). `ScanReport.ecc_budget_used` reports the worst block. Measured: Outline fits v2–v5 at M and above and v20+ at Q/H, but from v7 to v14 the rings' codewords pile into a few blocks and it's refused. With a steered picture (§7.7 A) the cleared modules are wanted light before anything else, so most of them already are: measured on v7–v14 at M, the worst block's use fell from 0.55–1.44 of the budget to 0–0.44, so Outline fits there too. *(rev. W20261026081347)*

Timing styles (`TimingStyle`): `Plain` (data module shape), `Dotted` (0.8 circles), `Dashed` (dashes along the line, 0.6 thick). A `Hidden-on-image` style was planned for halftone; picture codes keep the timing patterns whole (§7.7 rule 2), and hiding them is in the ROADMAP. *(rev. W20261026092854)*

*(rev. W20261025214510)*

**Constraints:**
- Finders keep the 7×7 footprint and the 1:1:3:1:1 ratio along both axes through the center: every shape crosses each axis at the module boundaries. Alignment patterns keep the 5×5 footprint and the 1:1:1:1:1 ratio.
- Only the real alignment positions for the chosen version are drawn. The library never adds decorative fake boxes, because scanners would try to use them.

### 7.3 Logo in the middle (feature 1)

- `Logo::svg(&str)` or `Logo::raster(bytes, RasterFormat)`, both `Result<Logo, LogoError>`: content is validated when the logo is built, so `Style` stays infallible plain data.
  - **SVG** is sanitized by an allowlist re-serializer, never by filtering the input text: the document is tokenized, only allowlisted elements (shapes, `g`, `defs`, `symbol`, `use`, gradients, `stop`, `clipPath`, `mask`, `text`/`tspan`, `image` with a base64 raster data URI) and attributes are kept as parsed values, entities are decoded, and everything is re-escaped on output. Dropped: scripts, event handlers, `foreignObject`, animation, links, `pattern`, DOCTYPEs and custom entities, `class`, namespaced attributes, characters XML can't carry. `url()` survives only as `url(#id)` and `href` only as `#id`. Simple `.class { … }` rules from `<style>` and the `style` attribute are folded into presentation attributes (attribute < class rule < `style`); everything else in CSS is dropped. A `use` may only point at an element without a `use` in it, at most 256 of them, so references can't expand exponentially. IDs and references get the code's prefix (`{prefix}-logo-…`) so an inline logo can't collide with the page. The root's viewBox (or width and height) sizes the content. `Logo::sanitized_svg` returns the clean document; sanitizing it again changes nothing (fuzzed: `logo_svg`).
  - **Raster** bytes must start with the format's signature (PNG, JPEG, GIF, WebP); they are inlined as a base64 `data:` URI (in-house encoder) without being decoded.
- Options: `scale` (the logo's width as a fraction of the code's, default 0.2, at most 0.5), `size` (named scales, below), `shape` (`Square` / `Rounded(r)` / `Circle`, the outline of the plate and the excavation), `backdrop` (a plate in `logo.backdrop`, default on), `padding` (modules between logo and code, default 0.5), `clear_modules` (leave out the modules under the plate, default on), `frame`, `frame_width` and `moon_size` (below).
- **Sizes:** `LogoSize::Small` / `Medium` / `Large` set the scale to 0.14 / 0.2 / 0.26. `Largest` grows the logo to the largest hundredth up to 0.5 that the code's budget affords, and at least 0.26 (or a `scale` set after it). Fitting asks for that minimum; the logo then grows into what the chosen code leaves. `ScanReport::logo_scale` says what it grew to. *(rev. W20261026102223)*
- **Frames:** `LogoFrame::Ring` (a solid band, default 0.8 modules wide), `Beads` (a ring of beads in the code's own module shape: dots, squares or rounded squares, 0.9), `Stamp` (a band with a scalloped outer edge, 0.9), `Brackets` (four corner brackets, or four 45° arcs on a circle, 0.8) and `Orbit` (a circle inscribed in the plate, with a gap and a moon in it; the width is the circle's line, 0.4, and `moon_size` the moon's diameter, default 2.4 (`DEFAULT_MOON`), 0.4–4), painted in `logo.frame`. `frame_width` is 0.1–2.5 modules. Unless set, widths and the moon grow with the logo: the defaults hold up to a logo 8 modules wide (`FRAME_REFERENCE`) and scale in proportion past it, up to 2.5 modules (or the default, if larger). The band an Orbit takes is its line or its moon, whichever is wider. *(rev. W20261026105648: the first defaults, Ring 0.45 to Orbit's 0.8 moon, looked thin next to a logo; the moon is three times its first size)* A frame sits inside the plate, a quarter module in from its edge so a light gap always parts it from the modules, and the plate grows by the frame's width plus that inset, so the budget counts it like the rest of the plate. With a frame, the logo is clipped to the window inside the frame rather than to the plate. Ring and Brackets are strokes along the band's middle line (Brackets as a dash pattern, one dash per corner), so Ring can draw on (§8.3); the others are filled. Measured: with the worst-case dark logo grown to the largest affordable size, every frame on every plate shape, at L/M/Q/H × v4/v8/v15 wherever the code affords the framed plate, with and without a plate, on square and dot modules, scans at the full bar (rqrr and the degraded render). A square ring around a dark logo is the shape most like a fourth finder; none of these codes confused a decoder. *(rev. W20261026102223)*
- **Placement:** the plate is the shape, `scale · N + 2 · padding` wide (plus twice the frame's width and inset), centered on the code. The content is fitted (`meet`) into the inner `scale · N` box and, for round shapes, clipped to the plate, so nothing paints outside the modules the budget counts.
- **Covered modules** are the modules whose cell overlaps the plate; excavation is always whole modules. Covering a finder, separator, timing, format or version module, the dark module, or the bottom-right alignment pattern (the one scanners use) is not allowed. Other alignment patterns under the plate are left out whole. On a picture (§7.7), the plate may cover none unless the alignment patterns are outlined and the timing dotted, as art mode draws them (`RenderError::LogoOverAlignment`; fitting steps past such versions). rqrr fits its grid to the bottom-right pattern and refines it by scoring every pattern, covered ones too, and on a halftone the fit drifted across the lower half of the code. Measured on the gallery's halftone cards (a logo at 0.12, two wallpapers and the sample sunset, light and dark, every size from 4 to 14 px/module): with the plate over a pattern, 13 of 18 missed rqrr at 6, 7 or 12 px, outside the bar's 4, 6 and 10 by luck in most; with Outline alone, the 9 dark ones still missed at 7; with Outline and dotted timing, and in art mode, none did, and neither did any code whose plate cleared the patterns. Frames made no difference. Every sparse-grid version (v7–13) has a pattern in the middle, so a halftone's logo needs the standard grid or a larger version. *(W20261026151955)*
- **ECC budget:** every codeword with a covered module counts as damaged, whatever the module's value, because a logo pixel may read either way (Outline only counts dark modules it clears, since those read light). Damage is counted per Reed–Solomon block against `DAMAGE_BUDGET` = 60% of each block's correctable codewords (`DAMAGE_BUDGET_H` = 45% at ECC H, below), together with any Outline clearing. A logo over budget, or one that reaches a forbidden module, fails with `RenderError::LogoTooLarge { max_scale }`: the largest scale, in hundredths, that fits this matrix. Measured: the largest affordable logo scans at the full bar (rqrr and the degraded render) for every ECC level at v2–v40, as a dark logo with no plate too; affordable scales are about 0.1–0.25 of the width at L/M and 0.16–0.37 at Q/H. Version 1 holds almost nothing: its format information sits in row and column 8 and 13. **At ECC H the budget is 45%** (*W20261026171519*): H's error correction lets a logo cover far more of the code, and with a larger probe (a striped dark mark on square and circle plates, framed and not, square and dot modules, light and dark twin) the largest logos at 60% went unfound by ZXing on the degraded render in 10 of 80 codes from v16 to v20, at 0.22–0.35 of the width. At 45%, none of 608 from v2 to v40 missed, and the largest logo at H is 0.12–0.30 of the width. The same probe found scattered misses at M and Q that don't follow the budget (2 of 16 at M v24 at every budget from 45% to 60%, and 3 of 176 at Q); those are in the ROADMAP.
- **Fitting:** `QrBuilder::encode_text_for(text, &style)` walks versions up from the smallest that holds the text, each at the highest ECC level it fits (with `boost_ecc`, the default), and returns the first code whose budget affords the style: "boost ECC, then bump the version". When nothing in range fits, it returns the plain code, and rendering reports `LogoTooLarge`. `encode_bytes_for` and `encode_segments_for` fit raw bytes and explicit segments the same way, steered for a picture as text is. *(W20261026160240)*

*(rev. W20261025223145)*

### 7.4 Round QR codes (feature 3)

- `Frame::Circle { filler_density, ring }`: the quiet-zone square (code plus `quiet_zone` on each side) is inscribed in a circle, and the cells between the square and the circle are filled with **decorative filler modules** in the same shape style, in the `filler` token. A cell is a candidate when it lies wholly inside the circle and outside the quiet-zone square; each candidate is kept with probability `filler_density` (0–1), drawn from the seeded generator (§4.1) in row-major order.
- The quiet zone is the gap between code and filler, so §7.6 holds unchanged: filler never enters it. Filler is also kept out of a **2-module halo** beyond each finder's quiet zone. Measured: with that halo, filler as dark as the data at density 1 and a 2-module quiet zone passed the full bar (rqrr and the degraded render) at every tested version, so filler is decoration and isn't contrast-checked; nor is the ring.
- Optional `ring` (`FrameRing::None | Solid | Dashed`): a half-module ring one module outside the filler circle, in the `frame` token, drawn as filled geometry (an annulus, or annular-sector dashes about 1.2 modules long) so it takes the same paint form as everything else. The background is a disc one module beyond the circle (or the ring), and the canvas is the smallest square around it whose side has the code's parity, so the code stays on whole modules.
- Other frames: `Square` (default), `RoundedSquare(r)` (corner radius 0 up to the quiet-zone width), `None` (no background element; contrast is still judged against `bg`).
- A non-square frame moves the code on the canvas, so it feeds the class/ID prefix hash (§6.2); `Square` doesn't, which keeps existing prefixes stable.

*(rev. W20261025222326)*

### 7.5 Gradients & texture

- Linear/radial gradients on `fg` and finders (§6.1).
- Optional per-module **opacity jitter** or **hue drift** (seeded, small amplitude, contrast-checked) for an organic, hand-printed look.

### 7.6 Quiet zone

- Default is 4 modules (spec minimum). This can be reduced to 2 with a warning, since many modern scanners cope with that. Frames and filler never intrude on it; in a round frame it is the gap between code and filler (§7.4).

### 7.7 Image-embedded QR / halftone (feature 6b) — `halftone`

The code is built *from* the image: the picture is visible, and the code scans.

Two techniques combine:

**A. Module steering (QArt, Cox 2012).** The free-bit solver (§3.1) sets the free data bits so that as many modules as possible already match the image's light/dark at that spot. All 8 masks are tried, and the one that matches the image best wins. The shorter the payload and the bigger the version, the more modules can be steered. *(rev. W20261026081347)*

- `Halftone::new(Rgba8Image)` (feature `halftone`) is plain data: `.fit(Fit::Cover | Contain)`, `.focal(x, y)` (fractions, default the center), `.priority(Rgba8Image)` (light = steer first, black = don't care), `.steer(bool)`. `Style::halftone(h)` attaches it.
- The picture covers the code's modules (not the quiet zone). Each module is a box average in linear light (bilinear when a module is smaller than a pixel), composited over `bg`, in OKLab L, then stretched so the 2nd–98th percentiles span 0–1 (only when they're ≥ 0.05 apart).
- **Targets are squeezed.** Lightness is mapped into [0.2, 0.8] before Floyd–Steinberg dithering at module resolution, so every neighborhood keeps modules of both colors. Measured: unsqueezed, steered codes with large flat areas failed the blurred, noisy ZXing render in 29% of 132 cases (v5–v27 × L/M/Q/H); the module errors sat inside the flat areas, where ZXing's local threshold (an average over 5 × 5 blocks) lands at the area's own level and noise flips half its pixels. From a span of 0.15 up, every case passed. Light pinholes every 4 modules were tried first and weren't enough.
- **Priority:** 0.45 × decisiveness (|L − ½|·2) + 0.35 × edge strength (Sobel) + 0.2 × closeness to the center, times the priority map. Modules a style clears (Outline rings) come first, wanted light.
- **Measured:** steered codes match 67–90% of the targets at L and M (v5–v27), against about 50% unsteered, and pass the full bar (ZXing on every render, rqrr on the clean ones) across five synthetic pictures, v2–v27 × L/M/Q/H.
- `ScanReport.steering` (`Steering { steerable_modules, code_modules, matched_modules }`) reports the free bits, the data and ECC modules, and how many match.

**B. Halftone sub-cells (Chu et al., 2013).** Each data, ECC and remainder module is split into a 3×3 sub-grid. The **center sub-cell carries the true module value**, since that is where scanners sample. The 8 surrounding sub-cells follow the image's dithered lightness (Floyd–Steinberg, serpentine, in OKLab L, error weighted by sub-cell area, centers included so the picture's tone survives them). *(rev. W20261026092854)*

- **The grid isn't even.** Cell edges sit at 0, 0.2, 0.8 and 1 of a module: the center is 0.6 wide, the edges 0.2. Measured on clean renders: an even grid (a third-wide center) failed ZXing on 118 of 150 codes at 4 px/module, where the center covers 1.3 px. A half-module center fixed that, but rqrr, which samples one pixel through a perspective fitted to one alignment pattern, still missed 2 of 216 renders from 4 to 12 px/module (18 of 216 with a logo). At 0.6 none missed.
- **Sub-cells are squeezed** like module targets: lightness goes into [0.2, 0.8] before dithering, so flat areas of the picture keep both colors. Measured: unsqueezed, every code failed the blurred, noisy render; 0.15 still lost the color disc at ECC L (the blur swallows the thin light edges); 0.2 lost nothing.
- **Measured:** with clarity 0, five synthetic pictures × v5–v27 × L/M/Q × mono and color pass the full bar (ZXing on every render, rqrr on the clean ones); so do steering off, Outline with dotted timing, round frames, clarity maps, `Fit::Contain`, gradient `fg`, two-color ink, every intro preset, and logos at every size from 4 to 12 px/module. At the module centers, sampled ideally, no module reads wrong at 4, 6 or 10 px/module.

A makes the code's own pattern resemble the image. B fills in detail at 3× resolution. Together they let the whole picture read as the code.

Further rules:
1. ECC level is configurable. **M is the default in art mode.** Lower ECC frees more bits for steering. Halftone barely spends the error budget, because the sampled centers are correct, but decoders that sample off-center do lose a few codewords: a picture code's logo and Outline get `HALFTONE_DAMAGE_BUDGET` = 40% of each block's correctable codewords instead of 60%. Measured: with a logo at 50%, rqrr missed 3 of 216 clean renders from 4 to 12 px/module; at 40%, none.
2. Function patterns (finders, separators, timing, alignment, format/version, the dark module) are drawn as whole modules at full contrast, though they may be tinted. Modules a logo or Outline clears get no sub-cells.
3. **Ink** (`Halftone::ink`): `Ink::Mono` (default) paints dark sub-cells in `fg` over `bg`. `Ink::Color { colors }` gives every sub-cell the picture's color with its lightness clamped into the dark band (≤ 0.28) or the light band (≥ 0.94) (`Halftone::bands`, default `DEFAULT_BANDS`), each band reduced to `colors` (2–16) by k-means in OKLab, started from lightness quantiles so it's deterministic. The band gap is checked like a token's contrast (reported as `fg`): an error below 0.40, a warning below 0.60.
4. **Light ink:** when `fg` is lighter than `bg` (an inverted palette, such as a dark twin, §6.3), ink is light, so the picture's lightness is turned over (`1 − L`, after the level stretch) for steering targets and sub-cells: ink goes where the picture is light, and the picture shows the right way round on the dark background. In color, ink sub-cells take the light band and the rest the dark one, and colors still come from the picture. Measured (W20261026145847): art mode on the default palette's dark twin, every grid × L/M/Q × the disc and photo pictures × mono and color ink × two payloads, scans at the full bar read inverted and steers as well as light (over 60% of modules match). Halftones on that twin, five pictures × both inks × clarity 0 and 0.5 × v5/v14/v27 × L/M/Q, with a logo at v14, do the same. A trial that mirrored the color bands about the dark `bg` made no difference and was dropped. Every dark picture code tried misses rqrr at 3 px per module, below the bar's 4. *(W20261026145847)*
4. A **clarity map** (`Halftone::clarity`: `Clarity::Uniform(c)` or `Clarity::Map(Rgba8Image)`, light = clear) pulls the surrounding sub-cells toward the module's value by `c`, giving scanners a cleaner area where it's set. The default is 0 everywhere: the squeeze above did what an automatic map was meant to do, and with it every measured code passed at 0. That bar is clean renders at 4–10 px/module. A harsher, camera-like one (*W20261026115722*: the whole code 160–400 px wide, one or two 3×3 blurs, ±24 noise, ZXing) tells settings apart, and phones agreed with it: two photographic art codes the bench passed were hard to capture. Each line is the share of ten renders that decoded, for two wallpapers in color and mono ink:

| Art mode | Decoded |
|---|---|
| clarity 0, standard grid (the defaults) | 1/10 |
| clarity 0.5, standard | 3/10 |
| clarity 0.75, standard | 5/10 |
| clarity 0, sparse grid | 3/10 |
| clarity 0.5, sparse | 6/10 |
| clarity 0.6, sparse | 7/10 |
| clarity 0.75, sparse | 8–9/10 |

Bands (0.20/0.97, 0.12/0.99), ECC H and ink made no difference. Clarity is the lever, and a smaller grid helps because the same print gets bigger modules; at 0.75 the picture is mostly gone, at 0.5–0.6 it still reads. The defaults are unchanged: they favor the picture, and §7.8 says how to set art mode up for fast scans *(W20261026170720)*. The gallery's picture cards use 0.5 and the sparse grid.
5. **Output:** per paint, one path for the thin edge rows and one for the center rows: horizontal runs as `m dx dy h w` lines along each row's middle, stroked at the row's height with butt caps, which halves the bytes of outlined rectangles. Paths are in fifths of a module inside a nested `<svg viewBox>` over the code, so integer coordinates scale exactly. Mono strokes carry a `{prefix}-fg-s` class whose rule sets `stroke` from `--qr-fg`; a gradient `fg` is restated as a `-q` gradient with a transform into that space. **Measured:** v20 in mono is 65–115 KB across the five pictures (≤ 150 KB, tested); in color with 6 colors per band, 140–270 KB. A v20 renders in about 35 ms (release; 1600 px picture: 48 ms).
6. In an `AnimationPlan` (§8.5) each paint is a `LayerRole::Halftone` layer with `color` (in color ink), `cells` (the code's corner; the path is in fifths of a module from there) and `stroke` (1 or 3). Group intros cut the picture into tiles of 3 × 3 modules, cut on module edges so strokes meet exactly, and stagger them like modules: each (paint, height, group) is a layer, and live, a group's strokes share one animated `<g>`, so browsers animate one element per group however many paints there are. Measured: scannable from 750–850 ms of 1200 (Settle, Ripple, Assemble, LockOn; mono and four-color ink, v10), and the static picture is pixel-identical for every preset *(rev. W20261026121853: the picture used to move as one piece)*. Develop still fades the picture's layers whole; reveal presets mask it with everything else; a palette shift leaves its strokes on the new palette from the start.

Input is `Rgba8Image` (same as palette extraction). The image is cropped/fit with `Fit::Cover | Contain` and optional focal point. The sub-cells sample it at five per module and gather it into the uneven cells.

**Compatibility note:** steered padding bits are non-standard (the spec suggests alternating `0xEC 0x11` pad bytes), but decoders stop reading at the terminator. ZXing, iOS Camera, and Google Lens all accept them, and the verify suite (§9) pins this per release. `Halftone::steer(false)` (the `strict_padding` of earlier drafts) turns steering off for conservative environments. rqrr and rxing accept steered padding in the verify suite; the device check is M5's scan matrix.

### 7.8 Art mode: large image codes with a tracking grid (feature 7)

The goal: a **short payload** (such as a website URL) rendered as a **large, image-filled code**, studded with a grid of crisp tracking boxes, so the whole picture looks like the QR code and still scans reliably.

**How it works.** This is one real QR symbol at a deliberately oversized version, not several codes stitched together:

1. **Pin a large version.** The user picks a tracking grid density instead of a version:

   | `TrackingGrid` | Alignment boxes | Versions | Modules/side |
   |---|---|---|---|
   | `Sparse` (3×3 grid) | 6 | 7–13 | 45–69 |
   | `Standard` (4×4 grid) | 13 | 14–20 | 73–97 |
   | `Dense` (5×5 grid) | 22 | 21–27 | 101–125 |

   The reference design is `Standard`. `QrBuilder::tracking_grid(grid)` sets the version range, and `encode_text_for` picks the smallest version in it that holds the text and affords the style (Outline on a picture code uses `HALFTONE_DAMAGE_BUDGET`, so a longer URL can land a few versions up). `TrackingGrid::versions()` gives the range. *(rev. W20261026093447)*
2. **Steer the spare capacity.** A ~30-byte URL in a v17-M code uses under 10% of the data capacity. The free-bit solver (§3.1) turns the rest into image-matching modules (§7.7 A), and halftone sub-cells add detail (§7.7 B).
3. **Make the tracking boxes stand out.** Alignment patterns are drawn with `emphasis: Outline` on clean plates, finders are drawn in an accent color, and timing lines are dotted (§7.2). `Style::art(halftone)` sets the picture, Outline and dotted timing; the accent comes from the palette (`finder.outer`, `finder.eye`), and every option can still be changed after. The boxes are real: they are what lets a scanner lock onto a large, busy code and correct perspective across it. That is where the "better scan result" comes from.
4. **Optional clarity map** (§7.7 rule 4) keeps a calmer zone for scanners while the rest of the picture carries the visual.

**Measured** (W20261026093447): art mode passes the full bar for every grid × L/M/Q × mono and color × two pictures × a 19- and a 57-character URL. Between 31% (Sparse, Q, long URL) and 79% (Dense, L, short URL) of the data and ECC modules are steerable, and 64–87% follow the picture; Outline uses at most 40% of the budget, after any growth within the grid.

**On phones** (scan sheet, W20261026164853): art mode on the sparse grid (v8–9) scans fast on a Pixel 10 Pro XL and an iPad mini. On the standard and dense grids (v14–21, any ECC or ink) it scans slowly on both, and one standard-grid code in six-color ink fails on iOS, although the bench passes them all. A v14 code without a picture scans fast, so it's the picture at that size, not the size alone. The defaults stay on the standard grid at clarity 0, because they draw the most picture; the API docs and CLI help say so. **For fast scans:** use the sparse grid when the payload fits v13, and raise clarity to 0.5–0.6 on larger grids (§7.7: six to seven times as many decodes under camera-like blur and noise; not yet checked on phones). Print or display large (below). *(W20261026170720)*

**Feasibility:** proven. This is the QArt technique plus halftone QR, both demonstrated in the literature, and the reference image matches a v14–20 code with emphasized alignment patterns. The trade-offs are:
- **Minimum size goes up.** A v17 code is 85 modules plus quiet zone. The code must be displayed at ≥ 3 device pixels per module (about 280 px on screen) or printed at ≥ 0.4 mm per module (about 4 cm; 5 cm or more is recommended). `Style::target_size(TargetSize::Pixels(n) | TargetSize::Millimeters(x))` (the whole symbol, quiet zone included) makes `Style::check` warn below these thresholds; it's a style field rather than a `check` argument so styles stay plain data. *(rev. W20261025215711)*
- **Scanning is slower** on the standard and dense grids at the default clarity (above).
- **Payload must stay small.** As the payload grows, fewer bits can be steered and the image fidelity drops. `ScanReport::steering` reports `steerable_modules` against `code_modules` so callers can see the effect. *(rev. W20261026081347)*
- **Decoding is still global.** A scanner must read the whole symbol; there is no "scan only the clean zone." The clarity map improves the odds, but it is not a separate code.

**Deferred:** layouts with multiple independent codes (several redundant mini codes in a row, Structured Append, rMQR strips). These may return later as `MultiCode` layouts.

---

## 8. Animation (`animate` feature)

### 8.1 Motion principles

Based on the impeccable `animate` guidelines:

- **One signature entrance.** A code gets one well-orchestrated intro, not a pile of effects.
- **Easing:** exponential ease-outs only. No bounce, no elastic.
  ```css
  --qr-ease-quart: cubic-bezier(0.25, 1, 0.5, 1);   /* default, refined */
  --qr-ease-quint: cubic-bezier(0.22, 1, 0.36, 1);  /* snappier */
  --qr-ease-expo:  cubic-bezier(0.16, 1, 0.3, 1);   /* confident */
  ```
- **Durations:** each element animates in 500–800 ms. **Total intro ≤ 1200 ms** by default (the stagger is compressed to fit). The code must be scannable soon after it appears. Configurable, but anything over 2000 ms raises a warning.
- **Stagger:** 8–16 groups with 40–100 ms offsets (derived from total duration).
- **Properties:** `transform` and `opacity` first. `clip-path`/`mask` for wipes, `stroke-dashoffset` for draw-ons, `fill` for color. Layout-ish attributes (`x`, `width`, `r`) are never animated.
- **Reduced motion is mandatory.** Every animated SVG includes:
  ```css
  @media (prefers-reduced-motion: reduce) {
    .qr * { animation: none !important; transition: none !important; }
  }
  ```
  Palette transitions degrade to an instant swap.
- **No looping by default.** Idle loops (§8.3) are opt-in, subtle, and also disabled under reduced motion.

### 8.2 The static-fallback rule (critical)

The **un-animated document state is the finished, scannable code.** Keyframes animate *from* an intro state *to* the natural state using `animation-fill-mode: backwards`. This means:

- Renderers that ignore CSS animation (resvg, print, many email clients, thumbnailers) show the complete code.
- No SMIL `<animate>` elements that set initial hidden states.
- Tests (`freeqrent-verify/tests/intros.rs`) assert that the animated document, as resvg draws it with no CSS animation, is pixel-identical to the same style without an intro and to the plan's last frame, and that it scans. *(rev. W20261025233019)*

### 8.3 Preset intros (brainstorm)

**Core set (v1):** *(rev. W20261025234738)*

| Preset | Motion | Default ease / stagger | Best with |
|---|---|---|---|
| `Settle` | Groups fade in from `opacity 0, scale .92` toward their own centers | quart / random | Everything (default) |
| `Ripple` | Groups fade in from `scale .8` about the code's center, center outward, so the code spreads like a ripple | quint / radial | Dots, Liquid, Round |
| `LockOn` | The finder rings close in from `scale 1.35` (corners 60 ms apart), the eyes pop in from 0 at 150 ms, then the rest settles from 60% of the finders' time | expo / random | Branded/"scan me" moments |
| `Sweep` | A soft-edged mask wipe (feather ¼ of the side) from top-left to bottom-right | quart | Square, Bars |
| `Raster` | A crisp mask wipe (half-module edge) from top to bottom, or left to right for vertical Bars, like a printer head | quart | Bars, Square |
| `Iris` | A soft-edged circular mask opens from the center | expo | Round frame |
| `Assemble` | Groups converge from a seeded offset of 1.5–3 modules | expo / random | Dots, Square |
| `Develop` | Like a photo developing: the picture's layers fade in to 35% opacity over the first 45% of the intro, then resolve to full contrast while everything else (finders, boxes, timing) fades in. Opacity only, on whole layers, so it reaches the picture's strokes. On a code without a picture the data modules play the picture | quart | Pictures (§7.7, §7.8) *(rev. W20261026095000, W20261026102223)* |

`LockOn` scales the rings instead of drawing them on with `stroke-dashoffset`. Our rings are filled shapes, and a stroke or mask that draws them on would have to cover every ring shape exactly to keep the static state intact. A closing reticle reads as "locking on" using `transform` alone. The reveal presets mask everything but the background, and the logo too unless it has its own reveal. The stylesheet applies the mask (`mask:url(#…)` in the `@supports` block), so renderers that skip the block draw the code unmasked, and the mask's own natural state covers the canvas anyway. Measured time to scannable (ZXing, 6 px/module, v1–v25, 1200 ms): Settle 600–800 ms, Ripple 600–900, LockOn 700–850, Sweep 300, Raster 450–700, Iris 200–350, Assemble 650–750, Develop 650 (plain v5–v25, halftone mono and color, art mode on the standard and dense grids).

**Logo intros** *(rev. W20261026102223)*: `Intro::logo(LogoIntro::…)` moves only the logo's layers (the plate with the logo, its frame, and Ink's silhouette), on top of whichever preset the code uses, so the two stack. `Intro::logo_reveal(bool)` is a shorthand for `Reveal` or `None`.

| Logo intro | Motion | Length |
|---|---|---|
| `None` | The logo appears with the code | |
| `Reveal` (default) | Logo and frame scale 0.94 → 1 with a fade | 400 ms, at most 40% of the intro |
| `Stamp` | Logo and frame land from 112%, opaque a third of the way in | 450 ms |
| `Draw` | A Ring frame draws itself on from the top (`stroke-dashoffset` on a dash as long as the ring), then the logo fades in over the last 60%. Other frames fade in instead | 700 ms |
| `Lock` | The frame closes in from 130% like a viewfinder, then the logo fades in over the last 55% | 550 ms |
| `Ink` | A silhouette of the logo in `fg` (the logo's own alpha as an `alpha` mask) fades in, then out over the colored logo, which appears under it at 25% | 800 ms |
| `Shrink` | Logo, plate and frame fade in as wide as the code (by 8%), hold there until 15%, then shrink to their place (the intro's ease-out) while the code's own intro plays beneath them. It always plays with the code: the timing doesn't apply *(rev. W20261026105648)* | the whole intro |
| `Spin` | Logo and frame turn in from a quarter turn back (−90° → 0), growing from 90%, opaque by 40%. With a frame, the frame turns and the logo fades in (0.94 → 1) over the last 65% *(W20261026130519)* | 600 ms |

Logo intros other than Reveal and Shrink are capped at 60% of the intro. `Intro::logo_timing(LogoTiming::…)` places the logo's intro against the code's: `After` (default: last, the code finishes halfway through it), `Before` (first, the code starts halfway through it) or `With` (centered in the code's intro). Either way the whole intro keeps its length; the code's own stagger is compressed into what is left. Track ends are rounded rather than lengths, so nothing ends past the intro. With the default Reveal and After, documents are byte-for-byte what they were before logo intros existed. Measured time to scannable (ZXing, 6 px/module, Settle, circle plate with a ring, v5 and v10 at Q, 1200 ms): After 500–650 ms, With 700–750, Before 750–900, and 700–750 with no logo intro; Shrink 700–750 *(W20261026105648)*.

**Cut-loose frames** *(W20261026130519)*: by default the frame plays the logo's intro with it. `Intro::frame(LogoIntro::…)` cuts it loose: the frame plays that intro on its own, as a frame plays it (Draw draws a ring, Lock closes in, Spin turns), at `Intro::frame_timing(LogoTiming::…)` (default: the logo's timing; setting the timing alone cuts the frame loose with the logo's intro), while the plate and logo play theirs as if they had no frame, so they no longer wait for it. So a ring can draw itself in as a Shrink lands (Shrink with a Draw frame `After`), or brackets turn in with the code while an Ink logo blooms after it. `LogoIntro::None` holds the frame still while the logo moves. The code's stagger makes room for both: it starts halfway into whichever intro plays `Before` and ends halfway through whichever plays `After`. Under a reveal preset a logo whose frame has its own intro leaves the mask, so a logo with `None` then shows from the start. No effect without a frame. Measured time to scannable (as above): 500–750 ms of 1200 over Shrink with Draw or Spin, Ink with Spin, Stamp with Lock before, Spin with Shrink, and Reveal with a still frame.

**Extended (v1.x):**

| Preset | Motion |
|---|---|
| `Bloom` | Liquid blobs grow from region centroids (scale per connected region) |
| `Rain` | Columns drop in from above with a staggered offset (no overshoot) |
| `Spiral` | Stagger ordered by polar angle from the center |
| `Ink` | Radial mask with a noisy edge, like ink soaking into paper |
| `Glitch` | 2–3 quick horizontal offset slices, then lock (very short, opt-in) |
| `Typeset` | For multi-code strips: codes reveal left to right one after another |

**Idle loops (opt-in, off by default):** `Style::idle(Idle::…)`. These loops are endless, start when the intro ends (or at load), use `animation-fill-mode: none`, and stop under reduced motion. They are symmetric rather than entrances, so they ease in and out with a sine curve (`cubic-bezier(.37,0,.63,1)`) instead of the ease-out rule. *(rev. W20261025235601)*

- `FinderPulse`: each finder eye scales 1 → 1.06 → 1 in the first 600 ms of every 4 s.
- `Shimmer`: a white band (18% at its peak) crosses the canvas in the first 1.5 s of every 6 s, then returns invisibly. At rest it is parked outside the viewBox. `check` composites it over every read token and warns with `MotionContrast` if a token would dip below the floor.
- `Breathe`: `bg` lightness drifts 2% in OKLab *away from the ink* and back every 8 s, so contrast only ever grows. The draft's "±2%" could have lowered it.
- `LogoGlint`: a slanted white band (45% at its peak) crosses the logo's plate in the first 1.4 s of every 7 s, clipped to the plate, so it never reaches a module. *(rev. W20261026102223)*
- `LogoTurn`: the logo's frame turns clockwise at a constant speed, once every 24 s, or every 12 s for an Orbit, whose moon then orbits. Only a round frame stays inside the plate while it turns, so it needs an Orbit frame or a frame on a `Circle` plate; anything else fails with `RenderError::TurnNeedsRoundFrame`. *(rev. W20261026102223)*
- `LogoSpin`: every 8 s the logo turns once in its plane over 1.2 s, then rests. `LogoFlip`: every 8 s it flips once about its horizontal axis like a coin over 1 s (its height follows the cosine of the turn, so it shows mirrored halfway), then rests. Both start and land softly (ease-in-out quart, `cubic-bezier(.76,0,.24,1)`, sampled into 13 keyframes on a linear track), and together they take turns half a loop apart in one track. Only the logo's content moves: it is drawn as a `LogoArt` layer inside the plate's group, clipped to the window, so the plate, the frame and every module stay where they are, and a square logo's corners turn inside its window. Each keyframe states every transform function the track uses (`rotate(..) scale(x,y)`), so browsers interpolate angles, not matrices. *(rev. W20261026105648)*

Measured: every 100 ms through 8 s of each loop after a Settle intro, three payloads (v5–v25) decoded on every frame (the logo loops on a code with an Orbit-framed logo; spin and flip with a square logo and no frame). The logo loops with the largest affordable dark logo, all four at once, also decode at every eighth of a turn, at eight points of the glint's pass, and every 100 ms through the spin and the flip, with rqrr too. *(rev. W20261026105648)*

**Transitions:** `Intro::palette_from` (§6.5) and `Morph` (v2, cross-fade between two shape styles of the same code).

All intros take `.duration_ms()` (the whole intro), `.delay_ms()`, `.ease(Ease::Quart|Quint|Expo|Custom(..))`, `.stagger(StaggerOrder::Radial|Linear|Diagonal|Random(seed)|Polar)`, `.groups(2..=16)`, `.logo(LogoIntro)` (or `.logo_reveal(bool)`), `.logo_timing(LogoTiming)`, `.frame(LogoIntro)` and `.frame_timing(LogoTiming)` for a cut-loose frame, and `.trigger(Trigger::Load|Hover|Class("is-visible"))`. Without `ease` or `stagger`, each preset uses its own. The `Class` trigger lets host pages start the intro on scroll-into-view without the SVG containing JS.

### 8.4 Implementation strategy

- **Group, don't atomize.** A v40 code has ~31,000 modules, and one animated element per module would bloat file size and stall browsers. Each layer is cut into **pieces** that are never split: 4-connected regions for shapes whose modules touch (Square with no gap, Rounded, Liquid, Bars), single modules for Dots and gapped squares, one piece per finder, alignment pattern and frame ring, and a picture's 3 × 3-module tiles *(W20261026121853)*. Pieces are binned into *K* stagger groups (default 12, range 2–16) by the stagger key of their center. Each (layer, group) is **one `<path>`**, and finders are one element per corner. Every shape's geometry depends only on 4-neighbors in its own layer, so a group has exactly the outline its pieces have in the static drawing. As a result, the static state of an animated code is **pixel-identical** to the same style without an intro (verified for 61 shape × frame × order × version combinations), and no two touching edges sit in different elements, so there are no anti-aliasing seams. The largest region measured was 1,366 modules at v40 and about 15% of a v10 code. It appears at its center's group time, which is invisible next to 600 ms fades staggered about 50 ms apart. *(rev. W20261025233019: Liquid regions are no longer split along group boundaries.)*
- Element budget: more than 64 animated elements raises `Warning::ManyAnimatedElements`.
- Keyframes are written once and shared. Elements whose tracks differ only in delay share one `animation` rule, and each element's own rule sets only `animation-delay` and `transform-origin`. All of it is inside the stylesheet's `@supports` block, never in inline styles, so renderers that skip the block show the finished code.
- Keyframe lists never contain their end (`100%`/`to`), so every animation ends on the element's own computed value, which keeps re-theming through custom properties working. Intro tracks use `animation-fill-mode: backwards`. Idle loops use none.
- Transform origins are explicit points in viewBox units (`transform-box: view-box; transform-origin: Xpx Ypx`), so CSS and other players agree.
- When a style animates, the root `<svg>` gets the id prefix as a second class. Reduced motion is scoped to it: `@media (prefers-reduced-motion:reduce){.{prefix} *{animation:none!important;transition:none!important}}`. The intro is part of the prefix hash, so two animations of one code can share a page.
- **Triggers.** `Load` writes the animation rules directly. `Hover` scopes them under `.{prefix}:hover`. `Class(name)` starts them paused and runs them under `.{name} .{prefix}-eN`. The name is written as an escaped CSS identifier. A page that never adds the class leaves the code in its intro state; reduced motion still shows it.
- **The animation is data first, CSS second.** The animate stage produces an `AnimationPlan` (§8.5), and the SVG writer serializes that plan to CSS. `Style::render_frame(&qr, t_ms)` evaluates the same plan at time *t* into a still SVG with no animation CSS (CSS semantics: easing between neighboring keyframes, backwards fill, later tracks win). `freeqrent_verify::scannable_after` uses these frames to measure when a code becomes readable for good.
- **CSS keyframes over SMIL.** They work for inline SVG and in `<img src="x.svg">` in all evergreen browsers, respect `prefers-reduced-motion`, and allow class triggers. A `smil` fallback option may be added if a target environment needs it.

### 8.5 `AnimationPlan` (exportable)

`Style::plan` returns it; its parts (`PlanLayer`, `Track`, `Keyframe`, `Property`, `Value`, `Target`, `Reveal`, `Fallback`, …) are in `freeqrent::plan` *(W20261026173421)*.

```rust
pub struct AnimationPlan {
    pub view_box: (f32, f32),
    pub palette: Palette,            // what the layers' tokens resolve through
    pub layers: Vec<PlanLayer>,      // paint order: background, split layers, logo
    pub tracks: Vec<Track>,
    pub reveal: Option<Reveal>,
    pub total_ms: u32,               // end of the last finite track
    pub trigger: Trigger,
    pub reduced_motion: Fallback,    // always FinalState
}

pub struct PlanLayer {
    pub role: LayerRole,             // Background, Ring, Filler, Data, Timing, Alignment, FinderOuter, FinderEye, Logo, LogoArt, LogoInk, LogoFrame, LogoGlint, Shimmer, Halftone
    pub token: Token,
    pub color: Option<Color>,        // a picture's fixed color (§7.7 rule 6)
    pub path: String,                // SVG path data in modules; the logo's plate for Logo, its window for LogoArt (drawn inside Logo's layer)
    pub cells: Option<(f32, f32)>,   // a picture's path is in fifths of a module from here
    pub stroke: Option<f32>,         // …and stroked this wide instead of filled
    pub outline: Option<PlanOutline>, // a logo frame stroked along its path: width, length, dashes, dash_offset
    pub opacity: f32,                // at rest: 1, or 0 for Ink's silhouette
    pub evenodd: bool,
    pub group: Option<u8>,
    pub corner: Option<Corner>,
    pub origin: (f32, f32),          // scale origin
}

pub enum Target { Layer(usize), Reveal }

pub struct Reveal {                  // Sweep, Raster, Iris
    pub shape: RevealShape,          // Wipe { from, to } or Iris { center, radius }, in viewBox units
    pub feather: f32,
    pub layers: (usize, usize),      // first and last layer masked
}

pub struct Track {
    pub target: Target,
    pub delay_ms: u32,
    pub duration_ms: u32,
    pub ease: CubicBezier,           // applied between neighboring keyframes, as in CSS
    pub iterations: Option<u32>,     // None = endless (idle loops)
    pub keys: Vec<Keyframe>,         // offset 0..1, property, value; offsets 0 and 1 left out take the natural value
}

pub enum Property { Opacity, Scale, TranslateX, TranslateY, Fill, Reveal, Rotate, Draw, ScaleY }
// Reveal: mask progress 0–1. Rotate: degrees clockwise about the origin. Draw: share of an outline drawn from its start, 0–1.
// ScaleY: vertical scale on top of Scale, negative mirrors (LogoFlip).
```

`style.plan(&qr) -> Result<AnimationPlan, RenderError>` is public in Phase 1. A style without an intro gives its static layers and no tracks. `plan.to_json()` serializes it deterministically: hand-written, with no serde, so the core stays dependency-free *(rev. W20261025233019, by decision of 2026-10-25)*. It is the contract the Flutter widget (and any future native player, such as Lottie export) is built on. Only properties every player can reproduce are allowed in a plan.

---

## 9. Scannability Assurance

| Layer | What it checks | Where |
|---|---|---|
| Static rules | Contrast floor, quiet zone, logo ECC budget, finder ratio, min dot size, filler gap | core `Style::check` (always runs before render) |
| Per-feature budgets | Halftone sub-cell center preserved; palette transition midpoints | core |
| Round-trip verify | Rasterize with resvg at 4, 6 and 10 px/module plus a 3×3-blurred, ±24-noise render at 8 px/module (and at intro midpoints for animations); decode with ZXing (`rxing`) on every render and rqrr on the clean ones. rqrr loses large plain codes after even a 3×3 blur, so it isn't a fair judge of soft renders. Each style is held to the bar its own `ScanReport` implies (`freeqrent_verify::Bar::for_report`): ZXing must decode the clean renders, always; without warnings, also the degraded render, and rqrr the clean ones. A low-contrast warning drops the degraded render; a rounded-finder warning drops rqrr and the degraded render; an inverted warning reads every render inverted (`Bar::inverted`), and so does `scannable_after`. Perspective is still to come. *(rev. W20261025212852, W20261025214510, W20261026141235)* | `freeqrent-verify` crate and CI |
| Real-device matrix | iOS Camera, Android Google Lens, WeChat, ZXing | manual, per release: `freeqrent --scan-sheet FILE` writes the matrix as a page (every gallery family, art mode per grid, ink and ECC level, pictures with a logo and without steering, v1 codes at ΔL 0.60–0.66 on backgrounds of L 0.85 and 0.95, then the logo sizes, frames and logo motion as S47–S60, and the thick off-color ring, palette frame, Shrink, spin and flip as S61–S65 *(W20261026105648)*, then the dark site palette and inverted v1 codes at ΔL 0.70 on L 0.04 and 0.16 as S67–S69 *(W20261026141235)*, then the eight presets as S70–S77 *(W20261026174357)*, last so earlier card ids keep their meaning). Each payload names its card; the page records a verdict per device and exports a Markdown table. First results (W20261026100651): a Pixel 10 XL's built-in camera read all eight v1 contrast cards, six of which fail the bench's degraded render, so that render is the stricter judge near the contrast warning. S01–S69 on a Pixel 10 Pro XL and an iPad mini (W20261026164853): every card reads on both except S32 (art, standard grid, `color:6`, v14), which iOS misses; picture codes at v14 and up are slow on both, while the sparse grid (v8–9) and every family without a picture read fast. iOS also reads all eight v1 contrast cards, and both read every logo, frame, logo-motion and dark-palette card (S47–S69) fast, inverted v1 codes included. For pictures, then, the bench is the more lenient judge. At the 1.0 release candidate (W20261026190646), the regenerated sheet read on both devices, including the logo cards that changed in W20261026171519 (S14, S47–S66) and the presets (S70–S77). WeChat and a ZXing app are pending (W20261026171229). *(rev. W20261026095000, W20261026102223, W20261026164853)* |

CI runs the round-trip suite over the full preset × shape × frame matrix (`freeqrent-verify/tests/presets.rs`; an inverted preset skips `Frame::None`, whose host page would have to supply the dark background while the bench rasterizes over white). A preset that fails is a release blocker. *(rev. W20261026174357)*

---

## 10. Language Bindings & Interfaces (Phase 2)

Not built in Phase 1. Specified here so the core is designed for it. Dart, TS/JS, Python, and Go ship together at the end of Phase 2. To keep them in sync, the **binding boundary is narrow and data-driven**: each binding exposes a few functions that take a **Style document** (JSON, schema generated from the Rust types) plus typed wrappers in each language. New styles and presets reach every language without binding changes.

### 10.1 Shared boundary

```
render(payload: string | bytes, style: Style) -> string        // SVG
check(payload, style) -> ScanReport
palette_from_image(rgba: bytes, width, height, opts) -> Palette
palette_from_css(text) / palette_from_html(text) -> Palette
presets() -> list of preset names
```

Errors cross the boundary as a structured `{ code, message, detail }` value and become an idiomatic exception or `Result` in each language.

### 10.2 Per language

| Language | Mechanism | Package | Idiomatic surface |
|---|---|---|---|
| **Rust** | Native crate | crates.io `freeqrent` | Builder API (§5) |
| **TypeScript / JS** | WASM (`wasm-bindgen`), one artifact for **Node ≥ 18** and browsers (ESM + CJS entry points) | npm `freeqrent` | `render(text, style): string`, full `.d.ts` types generated from the style schema, discriminated unions for shapes/intros. Optional native `napi-rs` build later if WASM speed isn't enough. |
| **Python** | PyO3 + maturin, **abi3** wheels (CPython ≥ 3.9) for manylinux/musllinux x86_64 + aarch64, macOS universal2, Windows x64 | PyPI `freeqrent` | `freeqrent.render(text, style=Style(...))`, dataclass-like style objects with type stubs (`.pyi`), `Palette.from_image()` also accepts a Pillow `Image` duck-typed via `tobytes()` (no Pillow dependency) |
| **Dart / Flutter** | `dart:ffi` over `freeqrent-ffi`, compiled through **Native Assets** (`native_toolchain_rs`) for iOS, Android, macOS, Linux, Windows; Dart web via the WASM build | pub.dev `freeqrent` | `FreeQrent.render(text, style: QrStyle(...))` with sealed classes for shapes/intros. The SVG string works with `flutter_svg` for static codes. Animated codes use the native `FreeQrentAnimated` widget (§10.4). |
| **Go** | Default: **cgo** over `freeqrent-ffi`, linking prebuilt static libraries that ship in the module for linux/darwin/windows × amd64/arm64 (consumers need a C toolchain but no Rust). Opt-in: `-tags freeqrent_wasm` for pure Go via embedded WASM + `wazero` (`CGO_ENABLED=0`, roughly 2–5× slower). | Go module `github.com/freeqrent/freeqrent-go` | `freeqrent.Render(text, freeqrent.Style{...}) (string, error)`, safe for concurrent use. Functional options for styles. |
| **C / others** | `freeqrent-ffi`: `freeqrent_render(const char* text, const char* style_json, char** out_svg, QrsvgError* err) -> int`, `freeqrent_free(char*)` | GitHub release artifacts + header | For Swift, Kotlin, Go, and C# later |

### 10.3 Consistency rules

- One **conformance suite** (a JSON list of payload + style → expected SVG hash) runs in all five languages in CI. Because output is deterministic, every binding must produce byte-identical SVG.
- Versions are released in lockstep: `freeqrent` 1.2.0 means the same core in every package.
- Prebuilt binaries are produced for every platform, so users never need a Rust toolchain. Dart Native Assets can download prebuilt libraries and falls back to a local build.

### 10.4 Flutter native animation

`flutter_svg` draws static SVG only, so the Dart package includes a `FreeQrentAnimated` widget that replays intro presets with Flutter's own animation system.

- The core exports an **`AnimationPlan`** (§8.5): the static geometry as path data, plus per-group keyframes (property, from/to values, delay, duration, easing as cubic-bezier control points).
- The widget parses the path data into `ui.Path` once and drives it with a `CustomPainter` and an `AnimationController`. It respects `MediaQuery.disableAnimations` (Flutter's reduced-motion setting).
- Intro triggers: on build, on visible (via a visibility callback), or manually through a controller.
- Palette transitions are exposed as an implicit animation: change `palette` and the widget tweens in OKLab.
- Conformance: golden-image tests compare the widget's final frame against the static SVG rendered by resvg.

### 10.5 CLI

`freeqrent "https://x.y" --shape dots:0.8 --finder squircle/circle --fg "oklch(32% .08 265)" --verify -o code.svg`

Built in M1 (crate `freeqrent-cli`, binary `freeqrent`, arguments via `lexopt`). Style flags mirror `Style`: `--preset NAME` (the starting style; every other flag changes its part, and `--art` can't be combined with it; W20261026174357), `--shape`, `--finder RING[/EYE]`, `--corner CORNER=RING[/EYE]`, `--alignment`, `--emphasis`, `--timing`, `--token NAME=PAINT` (plus `--fg`/`--bg`), `--quiet-zone`, `--allow-inverted`, `--target-px`/`--target-mm`; encoding with `--ecc` and `--version`; output with `-o`, `--pretty`, `--title`, `--label` (defaults to the payload when it's an http(s) URL). `--check` prints the `ScanReport`; `--verify` decodes the render against the style's own bar (feature `verify`, on by default, uses `freeqrent-verify`); `--gallery FILE` writes an HTML page of style families for the payload (fifteen as of M2), each with the command that reproduces it and its verdict (twenty-five as of M5, including the three idle loops). Animated cards hold their first frame until they scroll into view and carry a replay button, beside the card's title so it never covers the code *(rev. W20261026115722: it sat on the code's top-right corner and stopped phones reading the gallery)*. Exit codes: 0 ok, 1 refused or failed verification, 2 bad arguments. M2 added `--shape liquid[:s]`; `--frame square|rounded[:r]|circle[:density]|none`, `--frame-ring none|solid|dashed`, `--seed N`; `--logo FILE|sample` (SVG by extension or content, rasters by signature) with `--logo-scale`, `--logo-shape square|rounded[:r]|circle`, `--logo-padding`, `--no-logo-backdrop` and `--keep-modules`. The CLI always encodes with `encode_text_for`, so a logo or Outline grows the code instead of being refused, and each gallery family gets its own fitted code. M3 added `--intro` and the animation flags. M4 added `--palette-from-image FILE` (PNG/JPEG through `freeqrent-image`), `--palette-from-css FILE|-|sample` and `--palette-from-html FILE|-` (`-` reads stdin, so `curl -s URL | freeqrent … --palette-from-html -` takes a live site's colors without the CLI fetching anything), `--palette-clusters K` and `--palette-contrast D` for images, and `--show-palette`, which prints the swatches, each token's origin, and the `--bg`/`--fg`/`--token` flags that reproduce the palette. `--fg`, `--bg` and `--token` override extracted tokens. A source with no colors or an unreadable image exits 2. M5 added `--halftone FILE|sample` (the picture, PNG/JPEG; `sample` is a procedural sunset) and `--art FILE|sample` (the same plus `Style::art` and the `Standard` grid unless `--version` pins one), `--ink mono|color[:N]`, `--bands D,L`, `--clarity C`, `--clarity-map FILE`, `--fit cover|contain`, `--focal X,Y`, `--priority FILE`, `--no-steer`, and `--tracking-grid sparse|standard|dense`; picture options without a picture exit 2, and `--check` prints the steering numbers. The gallery gained Halftone, Art mode and Develop intro cards (twenty-eight families). `--intro develop`; `--scan-sheet FILE` writes the real-device scan matrix (§9) and needs no TEXT. Logo frames and motion added `--logo-size small|medium|large|largest`, `--logo-frame ring|beads|stamp|brackets|orbit[:WIDTH]`, `--logo-intro none|reveal|stamp|draw|lock|ink`, `--logo-timing after|before|with` (any of these sets up a Settle intro if none was given) and the `logo-glint` and `logo-turn` idles; logo options without `--logo` exit 2. The gallery gained fourteen logo cards (two sizes, the five frames, five logo intros including "logo first", and the two logo idles: forty-two families). Then `--logo-frame-width W`, `--logo-frame-color PAINT` (a shortcut for `--token logo.frame=PAINT`), `--logo-moon D` (width and moon without `--logo-frame` exit 2), `--logo-intro shrink` and the `logo-spin` and `logo-flip` idles, with five more cards (forty-seven families). Then eighteen picture cards (sixty-five families): art mode in mono and color ink and on the dense grid, halftones, palettes and gradients taken from a picture, and pictures with logos, logo intros and idles. They use the pictures given with `--gallery-picture FILE` (repeatable; the first two, the second falling back to the first) or `sample`, so a gallery can show someone's own images without the repository holding them; the scan sheet doesn't include them. `--palette-from-image` takes `sample` too. Logos on pictures are at scale 0.12, since a picture's budget is 0.40 (§7.7) and a framed plate at 0.2 doesn't fit it. *(W20261026114047)* Two more art cards show the tiled intros, Assemble and Ripple (sixty-seven families). *(W20261026121853)* `--logo-intro spin`, and `--logo-frame-intro INTRO` and `--logo-frame-timing WHEN` for a cut-loose frame (without `--logo-frame` they exit 2). The Shrink card's ring now draws in after the Shrink, the "Picture palette, ink logo" card's stamp frame spins in with the code, and a Spin card comes last among the logo cards, so the scan sheet's earlier ids keep their places (sixty-eight families). *(W20261026130519)* `--allow-inverted` also lets `--palette-from-*` keep a dark source's polarity, `--palette-from-css sample-dark` is a dark-first sample site, `--show-palette` reports lightened colors and adds `--allow-inverted` to the reproducing flags for a dark palette, and `--verify` says "read inverted" for inverted codes. The gallery gained a "Dark site palette" card after the logo cards and a "Dark picture palette" card after the picture cards (seventy families). *(W20261026141235)* `--dark` renders the palette's dark twin (§6.3) and implies `--allow-inverted`. The gallery renders every card twice, as is and with `--dark` (each with its own command and verdict), and a switch floating in the page's corner (Auto, Light, Dark, remembered per browser; *rev. W20261026150437*, it sat in the header); both versions share the card's space, so flipping it doesn't move the page *(W20261026150957)* picks the page theme and which codes show. Auto follows the system. Inverted warnings show as one line per card. *(W20261026143324)* Picture codes take `--dark` too, with light ink (§7.7 rule 4), so picture cards have dark twins as well. Cards may set `--ecc`. *(W20261026145847)* The halftone cards with a logo ("Halftone, logo spin", "Halftone, flip and shimmer") use the standard grid, since a halftone's logo stays off the alignment patterns (§7.3); "Halftone, logo spin" is back at ECC M, from the Q it took for its dark twin. *(W20261026151955)* With `--logo`, `--palette-from-*` keeps the extracted frame apart from the logo's colors (§6.4). *(W20261026160831)* *(rev. W20261025215711, W20261025222326, W20261025223145, W20261026075708, W20261026093447, W20261026102223, W20261026105648)*

Phase 1: `cargo install` and GitHub releases. Phase 2: also an npm `bin` (WASM, so it works anywhere Node runs).

---

## 11. Decisions

No open questions remain. Settled so far:

- **Name:** freeQRent, package id `freeqrent` everywhere.
- **License:** MIT.
- **Scan boxes (feature 4):** styled finders + emphasized alignment/timing patterns (§7.2).
- **Tracking boxes (feature 7):** art mode, one oversized code with an alignment grid and steered modules (§7.8). Multi-code layouts are deferred.
- **Image input:** the core takes a raw RGBA buffer; PNG/JPEG decoding lives in the optional `freeqrent-image` crate. No SVG image input.
- **Output formats:** SVG and animated SVG only. No PNG export, Lottie, or VectorDrawable for now.
- **Art-mode defaults:** ECC M and the `Standard` (4×4) tracking grid. They pass the synthetic bar on every grid and ECC level (§7.8); the real-device scan matrix (§9) is where they're confirmed.
- **Flutter:** native `FreeQrentAnimated` widget driven by `AnimationPlan` (§10.4).
- **Go:** cgo by default; pure-Go WASM via the `freeqrent_wasm` tag (§10.2).
- **Phasing:** Rust core first (Phase 1), language bindings second (Phase 2).

---

## 12. Milestones

Each milestone is an **M wave**, built from small timestamp-named **W waves**. See `docs/waves/README.md` for the protocol and `docs/waves/LOG.md` for progress.

### Phase 1: Rust core

| Milestone | Scope |
|---|---|
| **M0: Encoder** | Versions 1–40, all ECC, auto segmentation, masks, `QrMatrix` + roles, version pinning. Verified against `rqrr`, the ISO 18004 worked examples, and module-for-module against `qrcodegen`. Fuzzing (`cargo fuzz`) on inputs. **Done 2026-10-25.** |
| **M1: Static styles** | SVG writer, Square/Rounded/Dots/Bars, finder/alignment/timing styles, tokens + contrast checks, quiet zone, `Style::check`, CLI for previewing. Verified with resvg + ZXing (`rxing`) + rqrr. **Done 2026-10-25.** |
| **M2: Liquid + logo + round** | Contour-traced Liquid, logo excavation with ECC budget, circle frame with filler. **Done 2026-10-25.** |
| **M3: Animation** | `AnimationPlan`, grouping engine, core intro presets, reduced motion, static-fallback tests, palette transitions. **Done 2026-10-26** (`Develop` moves to M5 with halftone). |
| **M4: Palettes** | OKLab k-means extraction, CSS/HTML parsing, `freeqrent-image`. **Done 2026-10-26.** |
| **M5: Halftone + art mode** | Free-bit solver, module steering, 3×3 halftone, clarity map, tracking grids, `Develop` intro, real-device scan matrix. **Built 2026-10-26**; the scan matrix is recorded for iOS Camera and Google Lens (W20261026164853). WeChat and a ZXing app are pending: no device with them yet (W20261026171229). |
| **M6: Core 1.0** | API review, docs, preset gallery, `freeqrent` 1.0 on crates.io. The done-when list is below *(rev. W20261026173249)*. **Done 2026-10-26**, apart from the publish itself, which waits for the owner's go. |

**M6 is done when** *(W20261026173249)*:

1. **The API is reviewed.** Every public item in `freeqrent` has been read once for naming, argument order, plain-data options and structured errors, and §5 matches the real API. Enums that will grow in 1.x (module shapes, finder parts, frames, logo frames, intros, idles, warnings, errors) are `#[non_exhaustive]`, so the ROADMAP's additions (a data form of custom modules, extra glyphs, more finder shapes) stay additive. Breaking changes land before 1.0; after it, semver.
2. **Style presets ship.** `Style::preset(name)` returns one of a curated, named set, and `Style::presets()` lists the names (the list §10.1's bindings expose). Every preset is held to its bar across module shapes and frames (§9's preset × shape × frame matrix), has a gallery section, and the CLI takes `--preset`.
3. **Docs are complete.** `missing_docs` is denied in the core; the crate docs tour each feature with doctests that run; `RUSTDOCFLAGS="-D warnings" cargo doc` is clean with all features and with none. The README describes 1.0: a quick start, the feature flags, the MSRV and the scannability guarantee.
4. **§2 and §13's numbers are measured.** Render times for a v10 styled static code (target < 1 ms) and Liquid with animation (< 5 ms) are benchmarked without a new dependency and recorded; the core's default-feature WASM build is measured against the gzipped budget (200 KB, rev. W20261026180849), and CI checks it.
5. **The scan matrix holds at the release.** The scan sheet regenerated from the release candidate reads on iOS Camera and Google Lens, including the cards that changed since the first results (S14, S47–S66; W20261026171519).
6. **The release is prepared.** The workspace is at 1.0.0; the core, `freeqrent-image`, `freeqrent-verify` and `freeqrent-cli` have complete crate metadata and publish together; a CHANGELOG exists; `cargo publish --dry-run` is clean for each, in dependency order; this spec's status reads 1.0. Publishing, tagging and pushing wait for the owner's go.

### Phase 2: Language bindings

| Milestone | Scope |
|---|---|
| **M7: Boundary** | `freeqrent-ffi` C ABI, Style JSON schema, cross-language conformance suite. |
| **M8: Bindings** | npm (WASM), Python (PyO3), Dart (FFI + Native Assets + `FreeQrentAnimated`), Go (cgo + `wazero` tag), prebuilt binaries for all platforms. |
| **M9: Release** | Lockstep 1.x release in all registries, docs site with live WASM playground. |

---

## 13. Testing & Quality

- **Docs** *(W20261026175640)*: `missing_docs` is denied in the core; the crate docs tour each feature with doctests, and the README's examples run as doctests too. `RUSTDOCFLAGS="-D warnings" cargo doc` is clean with all features and with none: links to feature-gated items are allowed to dangle when those features are off, and docs.rs builds with all of them.
- Unit tests per stage. **Snapshot tests** (`insta`, dev-dep; `crates/freeqrent/tests/snapshots.rs`) on whole pretty-printed documents; determinism makes these reliable. Review changes with `cargo insta review`. The CLI has end-to-end tests that run the binary. *(rev. W20261025215711)*
- Round-trip scan tests (§9) in CI.
- `cargo fuzz` targets: encoder (`encode_text`, `encode_segments`), CSS color/gradient parser (`color_parse`: no panics, canonical output re-parses to the same value), logo SVG sanitizer (`logo_svg`: no panics, the clean document sanitizes to itself, rendered codes carry no dropped markup; seeds in `fuzz/seeds/logo_svg`), CSS/HTML palette parser (`palette_parse`: no panics, returned palettes keep the extraction contrast; seeds in `fuzz/seeds/palette_parse`). *(rev. W20261026074436)*
- Benchmarks: target < 1 ms to render a v10 styled static code, and < 5 ms with Liquid + animation, on a modern laptop. `cargo run --release -p freeqrent --example bench` prints medians. Measured on an Apple-silicon laptop (W20261026180849): v10 rounded with squircle finders and a gradient takes 0.26 ms, Liquid with a Ripple intro 0.45 ms, and every preset 0.08–0.38 ms; at v40, 2.2 and 4.2 ms; encoding 0.03 ms.
- WASM size check in CI (§2).
- `cargo deny` for licenses, advisories and sources; `scripts/check-core-deps.sh` keeps the core's dependency tree empty.
- Golden fingerprints (`tests/golden.rs`) on a fixed corpus catch unintended output changes.
