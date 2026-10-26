# freeqrent-image

Decodes PNG and JPEG into [freeQRent](https://crates.io/crates/freeqrent)'s `Rgba8Image`, for palettes taken from a brand's images and for pictures drawn into codes.

```rust,ignore
let image = freeqrent_image::decode(&std::fs::read("brand.png")?)?;
let (palette, report) = freeqrent::Palette::from_image(&image, freeqrent::PaletteOpts::new())?;
```

The format is sniffed by signature. Colors are taken as sRGB (gamma and ICC profiles are ignored), 16-bit images are reduced to 8 bits, and an APNG gives its first frame. `Limits` are checked after the headers, before any pixels are allocated: by default, 16,384 pixels a side and 40 megapixels. The features `png` and `jpeg` are both on by default; they use the pure-Rust `zune-png` and `zune-jpeg`.

MIT licensed.
