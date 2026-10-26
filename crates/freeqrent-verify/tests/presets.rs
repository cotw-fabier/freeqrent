//! Style presets across the preset × shape × frame matrix (SPEC §9): each
//! preset as it ships, at every ECC level, then with every module shape
//! and every frame in its place, held to the style's own bar.

mod common;

use common::{PAYLOADS, assert_scans, assert_scans_at};
use freeqrent::{Axis, Ecc, Frame, FrameRing, ModuleShape, Style, Token};

fn shapes() -> [ModuleShape; 5] {
    [
        ModuleShape::SQUARE,
        ModuleShape::Rounded { radius: 0.5 },
        ModuleShape::Dots { size: 0.8 },
        ModuleShape::Liquid { smoothness: 0.8 },
        ModuleShape::Bars {
            axis: Axis::Vertical,
            radius: 0.5,
        },
    ]
}

fn frames() -> [Frame; 4] {
    [
        Frame::Square,
        Frame::RoundedSquare(2.0),
        Frame::Circle {
            filler_density: 0.5,
            ring: FrameRing::Dashed,
        },
        Frame::None,
    ]
}

#[test]
fn presets_scan_at_every_ecc_level() {
    for name in Style::presets() {
        let style = Style::preset(name).unwrap();
        for ecc in [Ecc::L, Ecc::M, Ecc::Q, Ecc::H] {
            assert_scans_at(&style, &format!("{name} {ecc:?}"), &PAYLOADS, ecc);
        }
    }
}

#[test]
fn presets_scan_with_every_shape_and_frame() {
    for name in Style::presets() {
        let preset = Style::preset(name).unwrap();
        // With no background the host page has to be `bg`, and the bench
        // rasterizes over white, so a light-on-dark preset needs a frame.
        let inverted = preset.colors().contrast(Token::Fg).inverted;
        for shape in shapes() {
            for frame in frames() {
                if inverted && frame == Frame::None {
                    continue;
                }
                let style = preset.clone().modules(shape).frame(frame);
                assert_scans(&style, &format!("{name} {shape:?} {frame:?}"));
            }
        }
    }
}
