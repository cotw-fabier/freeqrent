# freeqrent-cli

The `freeqrent` command: styled and animated SVG QR codes from the terminal, built on [freeQRent](https://crates.io/crates/freeqrent).

```sh
cargo install freeqrent-cli
freeqrent "https://example.com" --preset ink --verify -o code.svg
freeqrent "https://example.com" --shape dots:0.8 --finder squircle/circle --logo logo.svg --intro ripple -o code.svg
freeqrent "https://example.com" --gallery gallery.html
```

The flags cover the whole style surface: presets, module shapes, finders, colors and palettes taken from images, style sheets or pages, logos with frames and motion, pictures and art mode, intros and idle loops. `--check` prints the scan report, and `--verify` decodes the render (the default `verify` feature). `--gallery` writes a page of every style family, and `--scan-sheet` writes the device test sheet. `freeqrent --help` lists everything.

MIT licensed.
