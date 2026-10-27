//! Round trips for frames (SPEC §7.4): round codes with filler and rings,
//! rounded squares, no background at all, and borders and shadows.

mod common;

use common::assert_scans;
use freeqrent::{Border, Frame, FrameRing, ModuleShape, Palette, Style};

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
