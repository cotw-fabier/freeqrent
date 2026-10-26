# freeqrent-verify

Proves that a [freeQRent](https://crates.io/crates/freeqrent) code scans. It rasterizes the SVG with resvg at several sizes, plus one blurred and noisy render, and decodes each render with ZXing (`rxing`) and rqrr. Each style is held to the bar its own `ScanReport` implies: for example, a code with rounded finders is judged only by ZXing.

```rust,ignore
let qr = freeqrent::QrCode::encode("https://example.com", freeqrent::Ecc::M)?;
let style = freeqrent::Style::preset("ink").unwrap();
freeqrent_verify::verify_style(&style, &qr, "https://example.com")?;
```

This crate is meant for tests and tooling. The core never decodes.

MIT licensed.
