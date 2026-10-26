# freeQRent

Styled and animated SVG QR codes from a zero-dependency Rust core. MIT licensed.

- **Styles:** square, rounded, dot, bar and liquid modules; styled finders, alignment and timing patterns; color tokens and gradients; round frames; logos with frames; pictures drawn into the code; art mode.
- **Motion:** intros that play *into* the finished code as CSS keyframes. Renderers that ignore animation, and viewers who prefer reduced motion, see the static code.
- **Scannability first:** every style is checked against an error-correction and contrast budget, and the crate refuses what it can't verify. The limits come from measurements with ZXing and rqrr, and from scans on iOS Camera and Google Lens.
- **Embeddable:** `no_std` + `alloc`, no dependencies, no `unsafe`, deterministic output.

**Status:** the Rust core is being prepared for 1.0 (milestone M6 in [`SPEC.md`](SPEC.md) §12). Bindings for TypeScript, Python, Dart/Flutter and Go come after it. See [`docs/waves/LOG.md`](docs/waves/LOG.md) for progress and [`ROADMAP.md`](ROADMAP.md) for ideas.

## Quick start

```rust
use freeqrent::{Ecc, ModuleShape, Palette, QrCode, Style};

let qr = QrCode::encode("https://example.com", Ecc::M)?;
let svg = Style::new()
    .modules(ModuleShape::Dots { size: 0.85 })
    .palette(Palette::from_tokens([("fg", "oklch(32% .08 265)")])?)
    .label("https://example.com")
    .render(&qr)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

Or start from a preset, an ordinary style to change like any other: `classic`, `ink`, `candy`, `terminal`, `sunset`, `bubble`, `ribbon` or `leaf` (`Style::presets()` lists them).

```rust
use freeqrent::{Ecc, ModuleShape, QrCode, Style};

let qr = QrCode::encode("https://example.com", Ecc::M)?;
let svg = Style::preset("candy").unwrap()
    .modules(ModuleShape::Dots { size: 0.9 })
    .render(&qr)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Logos

Encode *for* the style, and the code grows until the logo fits its error-correction budget. Logos are SVG (sanitized) or PNG, JPEG, GIF or WebP.

```rust
use freeqrent::{Ecc, Frame, FrameRing, Logo, LogoFrame, LogoShape, ModuleShape, QrCode, Style};

let mark = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
    <circle cx="5" cy="5" r="4" fill="#a3302a"/></svg>"##;
let style = Style::new()
    .modules(ModuleShape::Liquid { smoothness: 0.8 })
    .frame(Frame::Circle { filler_density: 0.5, ring: FrameRing::Solid })
    .logo(Logo::svg(mark)?.scale(0.22).shape(LogoShape::Circle).frame(LogoFrame::Ring));
let qr = QrCode::builder().ecc(Ecc::Q).encode_text_for("https://example.com", &style)?;
let svg = style.render(&qr)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Palettes from a brand

The core derives a palette that passes the contrast rules from raw RGBA pixels, a style sheet or a page. It never fetches anything. The `freeqrent-image` crate decodes PNG and JPEG into the pixel buffer.

```rust
use freeqrent::{Palette, PaletteOpts, Rgba8Image, Style};

let (palette, report) = Palette::from_css(":root { --brand: #1d3b72; --paper: #f8f5ee; }")?;
// or: Palette::from_html(html)?, or from pixels:
// let image = freeqrent_image::decode(&std::fs::read("brand.png")?)?;
let image = Rgba8Image::filled(8, 8, [29, 59, 114, 255])?;
let (from_pixels, _) = Palette::from_image(&image, PaletteOpts::new())?;
let style = Style::new().palette(palette); // report.choices says where each token came from
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Pictures and motion

`Halftone::new(image)` draws a picture into any code's modules, choosing the code's spare data bits so its modules follow the picture. `Style::art(halftone)` is the large-image setup, with crisp tracking boxes. `Intro::new(Preset::Settle)` and seven other presets play the code in. `Style::plan` exports the same motion as data, for players outside CSS.

```rust
use freeqrent::{Ecc, Intro, Preset, QrCode, Style};

let qr = QrCode::encode("https://example.com", Ecc::M)?;
let svg = Style::preset("ink").unwrap()
    .intro(Intro::new(Preset::Ripple).duration_ms(900))
    .render(&qr)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Checking and verifying

`style.check(&qr)` returns a `ScanReport`. Its errors (contrast below the floor, a logo past the budget, no quiet zone) make `render` refuse, and its warnings name soft risks such as inverted colors or rounded finder rings. The `freeqrent-verify` crate rasterizes a render with resvg and decodes it with ZXing and rqrr, held to the bar the report implies. That is how every style option in this repository is tested.

## Command line

```sh
cargo install freeqrent-cli
freeqrent "https://example.com" --preset ink --verify -o code.svg
freeqrent "https://example.com" --shape dots:0.8 --finder squircle/circle --logo logo.svg --intro ripple -o code.svg
freeqrent "https://example.com" --gallery gallery.html   # every style family, with the flags for each
```

The CLI covers the whole style surface. `freeqrent --help` lists every flag. It covers palettes from images, style sheets and pages (`curl -s URL | freeqrent … --palette-from-html -`), logos and their frames and motion, pictures and art mode, intros and idle loops, `--check` and `--verify`. It also writes the device scan sheet (`--scan-sheet sheet.html`).

## Crates

| Crate | What it is |
|---|---|
| `freeqrent` | The core: encoder, styles, SVG writer, animation, palettes. No dependencies. |
| `freeqrent-image` | PNG and JPEG decoding into the core's `Rgba8Image`. |
| `freeqrent-verify` | Rasterizes a render and decodes it to prove it scans. |
| `freeqrent-cli` | The `freeqrent` command-line tool. |

## Features

| Feature | Default | Enables |
|---|---|---|
| `std` | yes | `std::error::Error` impls, `Style::render_io` |
| `animate` | yes | Intros, idle loops, palette transitions, `AnimationPlan` |
| `palette-extract` | yes | Palettes from pixels and CSS/HTML text |
| `halftone` | yes | Pictures in codes and art mode |
| `kanji` | no | Kanji mode (a ~27 KB JIS X 0208 table) |

`default-features = false` gives a `no_std` build. The minimum supported Rust version is 1.92.

## License

MIT. See [`LICENSE`](LICENSE).
