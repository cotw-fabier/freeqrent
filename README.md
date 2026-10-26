# freeQRent

Styled and animated SVG QR codes from a zero-dependency, `no_std` Rust core. MIT licensed.

**Status:** early development. The encoder (M0) is landing now. Styling, SVG output, and animation come next. See [`SPEC.md`](SPEC.md) for the design, [`docs/waves/LOG.md`](docs/waves/LOG.md) for progress, and [`ROADMAP.md`](ROADMAP.md) for ideas.

```rust
use freeqrent::{Ecc, QrCode};

let qr = QrCode::encode("https://example.com", Ecc::M)?;
for (x, y, dark, role) in qr.modules() {
    // role says whether a module is a finder, timing, data, ECC, …
}
# Ok::<(), freeqrent::EncodeError>(())
```
