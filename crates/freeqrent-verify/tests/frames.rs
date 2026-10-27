//! Round trips for frames (SPEC §7.4): round codes with filler and rings,
//! rounded squares, no background at all, borders and shadows, and
//! captions.

mod common;

use common::assert_scans;
use freeqrent::{
    Border, Caption, Ecc, Frame, FrameRing, ModuleShape, Palette, QrCode, Style, Version,
};
use freeqrent_verify::{Bar, verify_with};

#[test]
fn round_frames_scan() {
    let shapes = [
        ModuleShape::SQUARE,
        ModuleShape::Dots { size: 0.85 },
        ModuleShape::Liquid { smoothness: 1.0 },
    ];
    for (density, ring) in [
        (0.3, FrameRing::None),
        (0.6, FrameRing::Solid),
        (1.0, FrameRing::Dashed),
    ] {
        for shape in shapes {
            for qz in [2, 4] {
                let style = Style::new()
                    .modules(shape)
                    .quiet_zone(qz)
                    .frame(Frame::Circle {
                        filler_density: density,
                        ring,
                    });
                assert_scans(
                    &style,
                    &format!("circle {density} {ring:?} {shape:?} qz{qz}"),
                );
            }
        }
    }
}

#[test]
fn filler_as_dark_as_the_data_still_scans() {
    // The quiet zone and the finder halo are what keep scanners on the
    // code; the filler's faintness is only taste.
    let palette = Palette::from_tokens([("filler", "#17161c"), ("frame", "#17161c")]).unwrap();
    let style = Style::new()
        .palette(palette)
        .quiet_zone(2)
        .frame(Frame::Circle {
            filler_density: 1.0,
            ring: FrameRing::Solid,
        });
    assert_scans(&style, "solid filler");
}

#[test]
fn square_frames_scan() {
    for frame in [Frame::RoundedSquare(4.0), Frame::None] {
        assert_scans(&Style::new().frame(frame), &format!("{frame:?}"));
    }
}

#[test]
fn borders_and_shadows_scan() {
    // A dark border sits beyond the quiet zone, so a code with the
    // narrowest quiet zone and the widest border is the hard case; the
    // shadow beyond the card is darker than anything a scanner reads.
    let dark = Palette::from_tokens([("frame", "#000000"), ("shadow", "#000000")]).unwrap();
    let frames = [
        Frame::Square,
        Frame::RoundedSquare(2.0),
        Frame::Circle {
            filler_density: 0.5,
            ring: FrameRing::Solid,
        },
    ];
    let borders = [
        Border::Solid { width: 0.1 },
        Border::Solid { width: 2.0 },
        Border::Double { width: 3.0 },
    ];
    for frame in frames {
        for border in borders {
            for qz in [2, 4] {
                for shape in [ModuleShape::SQUARE, ModuleShape::Dots { size: 0.85 }] {
                    let style = Style::new()
                        .palette(dark.clone())
                        .modules(shape)
                        .quiet_zone(qz)
                        .frame(frame)
                        .border(border)
                        .shadow(3.0);
                    let label = format!("{frame:?} {border:?} qz{qz} {shape:?}");
                    assert_scans(&style, &label);
                    // The dark twin: a light border on a dark card.
                    let twin = style.clone().palette(dark.dark_twin()).allow_inverted(true);
                    assert_scans(&twin, &format!("{label} dark"));
                }
            }
        }
    }
}

#[test]
fn captions_scan_even_as_solid_bars() {
    // The bench renders without fonts, so each caption's line is drawn
    // here as a solid bar in the ink, as wide as it may get and as tall as
    // its letters: a harder case than any text in it.
    let palettes = [
        Palette::default(),
        Palette::from_tokens([("bg", "#0a0a0f"), ("fg", "#7dffb0")]).unwrap(),
    ];
    let frames = [
        (Frame::Square, None),
        (Frame::Square, Some(Border::Solid { width: 1.0 })),
        (
            Frame::Circle {
                filler_density: 0.5,
                ring: FrameRing::Solid,
            },
            None,
        ),
        (Frame::None, None),
    ];
    for palette in palettes {
        let ink = palette.contrast(freeqrent::Token::Fg);
        let fill = if ink.inverted { "#7dffb0" } else { "#17161c" };
        for (frame, border) in frames {
            // The bench rasterizes over white, so a frameless inverted
            // code can't be judged (as in the preset matrix).
            if ink.inverted && frame == Frame::None {
                continue;
            }
            for size in [1.0, 4.0] {
                for qz in [2, 4] {
                    let mut style = Style::new()
                        .palette(palette.clone())
                        .allow_inverted(true)
                        .quiet_zone(qz)
                        .frame(frame)
                        .caption(Caption::new("SCAN ME").size(size));
                    if let Some(b) = border {
                        style = style.border(b);
                    }
                    for &(text, version) in &common::PAYLOADS {
                        let qr = QrCode::builder()
                            .ecc(Ecc::M)
                            .boost_ecc(false)
                            .version(Version::new(version).unwrap())
                            .encode_text(text)
                            .unwrap();
                        let svg = style.render(&qr).unwrap();
                        let svg = solid_caption(&svg, size, fill);
                        let bar = Bar::for_report(&style.check(&qr));
                        if let Err(e) = verify_with(&svg, text, bar) {
                            panic!(
                                "{frame:?} {border:?} size {size} qz{qz} {fill} v{version}: {e}"
                            );
                        }
                    }
                }
            }
        }
    }
}

/// Swaps the caption's `<text>` for a bar over its whole line.
fn solid_caption(svg: &str, size: f32, fill: &str) -> String {
    let start = svg.find("<text").expect("a caption");
    let end = svg[start..].find("</text>").unwrap() + start + "</text>".len();
    let attr = |name: &str| -> f32 {
        let at = svg[start..].find(&format!(" {name}=\"")).unwrap() + start + name.len() + 3;
        svg[at..at + svg[at..].find('"').unwrap()].parse().unwrap()
    };
    let (x, y) = (attr("x"), attr("y"));
    // The line is centered on `x`, half the card's width: one module in
    // from each edge.
    let bar = format!(
        r#"<rect x="1" y="{}" width="{}" height="{size}" fill="{fill}"/>"#,
        y - size / 2.0,
        2.0 * x - 2.0
    );
    format!("{}{bar}{}", &svg[..start], &svg[end..])
}
