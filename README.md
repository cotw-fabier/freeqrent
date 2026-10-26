# freeQRent

Styled and animated SVG QR codes from a zero-dependency, `no_std` Rust core. MIT licensed.

**Status:** early development. The encoder (M0) is done; static styles (M1) are landing now, and animation comes after. See [`SPEC.md`](SPEC.md) for the design, [`docs/waves/LOG.md`](docs/waves/LOG.md) for progress, and [`ROADMAP.md`](ROADMAP.md) for ideas.

```rust
use freeqrent::{Ecc, ModuleShape, Palette, QrCode, Style};

let qr = QrCode::encode("https://example.com", Ecc::M)?;
let svg = Style::new()
    .modules(ModuleShape::Dots { size: 0.85 })
    .palette(Palette::from_tokens([("fg", "oklch(32% .08 265)")])?)
    .label("https://example.com")
    .render(&qr)?;
```
