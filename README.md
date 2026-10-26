# freeQRent

Styled and animated SVG QR codes from a zero-dependency, `no_std` Rust core. MIT licensed.

**Status:** early development. The encoder (M0), static styles (M1) and logos, round frames and the Liquid shape (M2) are done; animation (M3) comes next. See [`SPEC.md`](SPEC.md) for the design, [`docs/waves/LOG.md`](docs/waves/LOG.md) for progress, and [`ROADMAP.md`](ROADMAP.md) for ideas.

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

From the command line, `freeqrent "https://example.com" --shape dots:0.8 --finder squircle/circle --verify -o code.svg` renders, decodes the result to prove it scans, and writes it. Add `--logo logo.svg` (or `--logo sample`), `--shape liquid` or `--frame circle --frame-ring dashed` for the M2 styles. `--gallery page.html` previews every style family for a payload.
