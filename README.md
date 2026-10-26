# freeQRent

Styled and animated SVG QR codes from a zero-dependency, `no_std` Rust core. MIT licensed.

**Status:** early development. The encoder (M0) and static styles (M1) are done; logos, round frames and the Liquid shape (M2) come next, then animation. See [`SPEC.md`](SPEC.md) for the design, [`docs/waves/LOG.md`](docs/waves/LOG.md) for progress, and [`ROADMAP.md`](ROADMAP.md) for ideas.

```rust
use freeqrent::{Ecc, ModuleShape, Palette, QrCode, Style};

let qr = QrCode::encode("https://example.com", Ecc::M)?;
let svg = Style::new()
    .modules(ModuleShape::Dots { size: 0.85 })
    .palette(Palette::from_tokens([("fg", "oklch(32% .08 265)")])?)
    .label("https://example.com")
    .render(&qr)?;
```

From the command line, `freeqrent "https://example.com" --shape dots:0.8 --finder squircle/circle --verify -o code.svg` renders, decodes the result to prove it scans, and writes it. `--gallery page.html` previews every style family for a payload.
