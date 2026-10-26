#!/bin/sh
# The core's WASM size against SPEC §2's budget: 200 KB gzipped with default
# features. Builds a small cdylib that calls into every default feature (so
# link-time optimization can't drop them) for wasm32-unknown-unknown, with
# size-tuned release settings, and checks the gzipped module.
set -eu
budget=$((200 * 1024))
root=$(cd "$(dirname "$0")/.." && pwd)
dir="${CARGO_TARGET_DIR:-$root/target}/wasm-size"
mkdir -p "$dir/src"
cat > "$dir/Cargo.toml" <<TOML
[package]
name = "freeqrent-wasm-size"
version = "0.0.0"
edition = "2024"
publish = false

[lib]
crate-type = ["cdylib"]

[dependencies]
freeqrent = { path = "$root/crates/freeqrent" }

[profile.release]
opt-level = "s"
lto = true
codegen-units = 1
panic = "abort"
strip = true

[workspace]
TOML
cat > "$dir/src/lib.rs" <<'RS'
//! Entry points that reach every default feature, so none is stripped.
use freeqrent::*;

const TEXT: &str = "https://example.com/wasm-size";
const LOGO: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><circle cx="5" cy="5" r="4" fill="#a33"/></svg>"##;

fn len(style: &Style) -> u32 {
    QrCode::builder()
        .encode_text_for(TEXT, style)
        .ok()
        .and_then(|qr| style.render(&qr).ok())
        .map_or(0, |svg| svg.len() as u32)
}

#[unsafe(no_mangle)]
pub extern "C" fn preset(n: u32) -> u32 {
    let names = Style::presets();
    let style = Style::preset(names[n as usize % names.len()]).unwrap_or_default();
    len(&style)
}

#[unsafe(no_mangle)]
pub extern "C" fn animated(n: u32) -> u32 {
    let preset = Preset::ALL[n as usize % Preset::ALL.len()];
    let style = Style::new()
        .modules(ModuleShape::Liquid { smoothness: 0.8 })
        .intro(Intro::new(preset))
        .idle(Idle::FinderPulse);
    let plan = QrCode::encode(TEXT, Ecc::M)
        .ok()
        .and_then(|qr| style.plan(&qr).ok())
        .map_or(0, |p| p.to_json().len() as u32);
    len(&style) + plan
}

#[unsafe(no_mangle)]
pub extern "C" fn logo(n: u32) -> u32 {
    match Logo::svg(LOGO) {
        Ok(logo) => len(&Style::new().logo(logo.scale(0.1 + (n % 10) as f32 / 100.0))),
        Err(_) => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn palettes(n: u32) -> u32 {
    let css = if n % 2 == 0 { ":root{--a:#1d3b72;--b:#f8f5ee}" } else { "<style>body{color:#123;background:#fff}</style>" };
    let from_css = if n % 2 == 0 { Palette::from_css(css) } else { Palette::from_html(css) };
    let image = Rgba8Image::filled(16, 16, [n as u8, 60, 120, 255]).ok();
    let from_image = image.and_then(|i| Palette::from_image(&i, PaletteOpts::new()).ok());
    let a = from_css.map_or(0, |(p, _)| len(&Style::new().palette(p)));
    let b = from_image.map_or(0, |(p, _)| len(&Style::new().palette(p)));
    a + b
}

#[unsafe(no_mangle)]
pub extern "C" fn art(n: u32) -> u32 {
    match Rgba8Image::filled(32, 32, [n as u8, 90, 160, 255]) {
        Ok(picture) => len(&Style::art(Halftone::new(picture).ink(Ink::Color { colors: 4 }))),
        Err(_) => 0,
    }
}
RS
cargo build -q --release --target wasm32-unknown-unknown --manifest-path "$dir/Cargo.toml" --target-dir "$dir/target"
wasm="$dir/target/wasm32-unknown-unknown/release/freeqrent_wasm_size.wasm"
raw=$(wc -c < "$wasm" | tr -d ' ')
gz=$(gzip -9 -c "$wasm" | wc -c | tr -d ' ')
echo "freeqrent wasm32 (default features): $raw bytes, $gz gzipped (budget $budget)"
if [ "$gz" -gt "$budget" ]; then
    echo "over SPEC §2's 200 KB gzipped budget" >&2
    exit 1
fi
