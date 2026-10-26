//! Round trips for frames (SPEC §7.4): round codes with filler and rings,
//! rounded squares, and no background at all.

mod common;

use common::assert_scans;
use freeqrent::{Frame, FrameRing, ModuleShape, Palette, Style};

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
