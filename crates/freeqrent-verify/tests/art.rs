//! Art mode (SPEC §7.8): one oversized code at a tracking-grid version,
//! picture-steered, with outlined alignment boxes and dotted timing. It
//! must scan at the full bar for every grid, ECC level and ink.

mod common;

use common::picture;
use freeqrent::{Ecc, Halftone, Ink, QrCode, Style, TrackingGrid};
use freeqrent_verify::verify_style;

#[test]
fn art_mode_scans_on_every_grid() {
    for text in [
        "https://example.com",
        "https://example.com/menu/order?table=12&ref=freeqrent-art",
    ] {
        for grid in [
            TrackingGrid::Sparse,
            TrackingGrid::Standard,
            TrackingGrid::Dense,
        ] {
            let (min, max) = grid.versions();
            for ecc in [Ecc::L, Ecc::M, Ecc::Q] {
                for kind in ["disc", "photo"] {
                    for ink in [Ink::Mono, Ink::Color { colors: 6 }] {
                        let style = Style::art(Halftone::new(picture(kind, 200)).ink(ink));
                        let qr = QrCode::builder()
                            .ecc(ecc)
                            .tracking_grid(grid)
                            .encode_text_for(text, &style)
                            .unwrap();
                        let label = format!("{} {grid:?} {ecc:?} {kind} {ink:?}", text.len());
                        assert!((min..=max).contains(&qr.version()), "{label}");
                        assert_eq!(qr.ecc(), ecc, "{label}");
                        let report = style.check(&qr);
                        assert!(report.ok(), "{label}: {report:?}");
                        assert!(report.warnings.is_empty(), "{label}: {report:?}");
                        // Measured: 31–79% of modules steerable, 64–87% match.
                        let s = report.steering.unwrap();
                        assert!(s.steerable_modules * 4 > s.code_modules, "{label}");
                        assert!(s.matched_modules * 5 > s.code_modules * 3, "{label}");
                        if let Err(e) = verify_style(&style, &qr, text) {
                            panic!("{label} v{}: {e}", qr.version().number());
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn grids_pick_the_smallest_version_that_affords_the_style() {
    let style = Style::art(Halftone::new(picture("photo", 120)));
    let qr = QrCode::builder()
        .tracking_grid(TrackingGrid::Standard)
        .encode_text_for("https://example.com", &style)
        .unwrap();
    assert_eq!(qr.version().number(), 14);
    // Without the outlines, every version affords the picture: the
    // smallest in range is used.
    let plain = Style::new().halftone(Halftone::new(picture("photo", 120)));
    for grid in [
        TrackingGrid::Sparse,
        TrackingGrid::Standard,
        TrackingGrid::Dense,
    ] {
        let qr = QrCode::builder()
            .tracking_grid(grid)
            .encode_text_for("https://example.com", &plain)
            .unwrap();
        assert_eq!(qr.version(), grid.versions().0, "{grid:?}");
    }
}
