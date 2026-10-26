# freeQRent

Styled and animated SVG QR codes from a zero-dependency, `no_std` Rust core. MIT licensed.

**Status:** early development. The encoder (M0), static styles (M1) logos, round frames and the Liquid shape (M2), animation (M3), and palettes from images and sites (M4) are done; halftone and art mode (M5) are in progress: steering, halftone sub-cells and art mode have landed; so has the `Develop` intro. The device matrix (`freeqrent --scan-sheet sheet.html`) is scanned with iOS Camera and Google Lens; WeChat and a ZXing app are pending. See [`SPEC.md`](SPEC.md) for the design, [`docs/waves/LOG.md`](docs/waves/LOG.md) for progress, and [`ROADMAP.md`](ROADMAP.md) for ideas.

```rust
use freeqrent::{Ecc, ModuleShape, Palette, QrCode, Style};

let qr = QrCode::encode("https://example.com", Ecc::M)?;
let svg = Style::new()
    .modules(ModuleShape::Dots { size: 0.85 })
    .palette(Palette::from_tokens([("fg", "oklch(32% .08 265)")])?)
    .label("https://example.com")
    .render(&qr)?;
```

With a logo, encode for the style so the code grows until the logo fits its error-correction budget:

```rust
use freeqrent::{Ecc, Frame, FrameRing, Logo, LogoShape, ModuleShape, QrCode, Style};

let style = Style::new()
    .modules(ModuleShape::Liquid { smoothness: 0.8 })
    .frame(Frame::Circle { filler_density: 0.5, ring: FrameRing::Solid })
    .logo(Logo::svg(include_str!("logo.svg"))?.scale(0.22).shape(LogoShape::Circle));
let qr = QrCode::builder().ecc(Ecc::Q).encode_text_for("https://example.com", &style)?;
let svg = style.render(&qr)?;
```

Palettes can come from a brand's own colors. The core reads raw RGBA pixels or CSS/HTML text and never fetches anything; `freeqrent-image` decodes PNG and JPEG:

```rust
use freeqrent::{Palette, PaletteOpts, Style};

let image = freeqrent_image::decode(&std::fs::read("brand.png")?)?;
let (palette, report) = Palette::from_image(&image, PaletteOpts::new())?;
// or: Palette::from_css(css)?, Palette::from_html(html)?
let style = Style::new().palette(palette); // report.choices says where each token came from
```

Presets are a quick start: `Style::preset("ink")` (or `candy`, `terminal`, `sunset`, `bubble`, `ribbon`, `leaf`, `classic`; `Style::presets()` lists them) is an ordinary style to build on, and `--preset ink` does the same from the command line.

From the command line, `freeqrent "https://example.com" --shape dots:0.8 --finder squircle/circle --verify -o code.svg` renders, decodes the result to prove it scans, and writes it. Add `--logo logo.svg` (or `--logo sample`), `--shape liquid` or `--frame circle --frame-ring dashed` for the M2 styles. `--intro ripple` (or settle, lock-on, sweep, raster, iris, assemble) adds an entrance animation: CSS keyframes that end on the finished code and stop under reduced motion. `--from-fg '#7a1f10'` cross-fades from old colors instead, and `--idle finder-pulse` adds a subtle loop. `--frame-at 400` writes a still of the animation, and `--plan` writes its plan as JSON. `--palette-from-image brand.png`, `--palette-from-css site.css` or `curl -s https://example.com | freeqrent … --palette-from-html -` take the colors from a brand, and `--show-palette` prints what was found as reusable flags. `--art photo.jpg` (or `--art sample`) builds a large code out of a picture, with crisp tracking boxes (the defaults favor the picture and scan slowly on phones: `--tracking-grid sparse` scans fast, and `--clarity 0.5` helps); `--halftone photo.jpg --ink color` draws a picture into any code. Logos take a size (`--logo-size largest` grows as far as error correction allows), a frame (`--logo-frame ring`, `beads`, `stamp`, `brackets` or `orbit`, with `--logo-frame-width`, `--logo-frame-color` and an orbit's `--logo-moon`), their own intro on top of the code's (`--logo-intro stamp`, `draw`, `lock`, `ink`, `shrink` or `spin`, played `--logo-timing after`, `before` or `with` it; `--logo-frame-intro draw` cuts the frame loose to play its own, at `--logo-frame-timing`) and idle loops (`--idle logo-glint`, `logo-turn`, `logo-spin`, `logo-flip`). `--gallery page.html` previews every style family for a payload; add `--gallery-picture photo.png` (once or twice) to see art mode, halftones and picture palettes with your own images.
