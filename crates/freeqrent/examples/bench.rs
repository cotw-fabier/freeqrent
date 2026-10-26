//! Render times against SPEC §13's targets: `cargo run --release -p freeqrent
//! --example bench`. No benchmark harness, to keep dev-dependencies as they
//! are; each case prints the median of 31 batches.

use std::hint::black_box;
use std::time::Instant;

use freeqrent::{
    Ecc, EyeShape, FinderStyle, Intro, ModuleShape, Palette, Preset, QrCode, QrMatrix, RingShape,
    Style, Version,
};

fn code(version: u8) -> QrMatrix {
    QrCode::builder()
        .ecc(Ecc::M)
        .version(Version::new(version).unwrap())
        .encode_text("https://example.com/bench")
        .unwrap()
}

/// The median time of one call, in milliseconds.
fn median_ms(mut f: impl FnMut()) -> f64 {
    // Batches long enough for the clock: about 20 ms each.
    let start = Instant::now();
    f();
    let once = start.elapsed().as_secs_f64().max(1e-7);
    let reps = ((0.02 / once) as usize).max(1);
    let mut batches: Vec<f64> = (0..31)
        .map(|_| {
            let start = Instant::now();
            for _ in 0..reps {
                f();
            }
            start.elapsed().as_secs_f64() * 1000.0 / reps as f64
        })
        .collect();
    batches.sort_by(f64::total_cmp);
    batches[batches.len() / 2]
}

fn report(name: &str, target_ms: Option<f64>, ms: f64) {
    match target_ms {
        Some(t) => println!(
            "{name:<40} {ms:>8.3} ms  (target < {t} ms: {})",
            if ms < t { "ok" } else { "MISSED" }
        ),
        None => println!("{name:<40} {ms:>8.3} ms"),
    }
}

fn main() {
    let palette = Palette::from_tokens([
        ("bg", "#f7f3ec"),
        (
            "fg",
            "linear-gradient(135deg, oklch(36% .15 25), oklch(31% .14 305))",
        ),
    ])
    .unwrap();
    let styled = Style::new()
        .modules(ModuleShape::Rounded { radius: 0.4 })
        .finders(FinderStyle::new(RingShape::Squircle, EyeShape::Circle))
        .palette(palette);
    let liquid = Style::new()
        .modules(ModuleShape::Liquid { smoothness: 0.8 })
        .intro(Intro::new(Preset::Ripple));
    let v10 = code(10);
    let v40 = code(40);

    report(
        "encode (smallest version, ECC M)",
        None,
        median_ms(|| {
            black_box(QrCode::encode(black_box("https://example.com/bench"), Ecc::M).unwrap());
        }),
    );
    report(
        "render v10, styled, static",
        Some(1.0),
        median_ms(|| {
            black_box(styled.render(black_box(&v10)).unwrap());
        }),
    );
    report(
        "render v10, Liquid + Ripple intro",
        Some(5.0),
        median_ms(|| {
            black_box(liquid.render(black_box(&v10)).unwrap());
        }),
    );
    report(
        "render v40, styled, static",
        None,
        median_ms(|| {
            black_box(styled.render(black_box(&v40)).unwrap());
        }),
    );
    report(
        "render v40, Liquid + Ripple intro",
        None,
        median_ms(|| {
            black_box(liquid.render(black_box(&v40)).unwrap());
        }),
    );
    for name in Style::presets() {
        let style = Style::preset(name).unwrap();
        report(
            &format!("render v10, preset {name}"),
            Some(1.0),
            median_ms(|| {
                black_box(style.render(black_box(&v10)).unwrap());
            }),
        );
    }
}
