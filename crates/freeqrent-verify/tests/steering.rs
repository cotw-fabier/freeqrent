//! Steered codes (SPEC §7.7 A): the free bits after the payload follow a
//! picture, and the code still decodes to the same payload.

mod common;

use common::picture;
use freeqrent::{
    AlignmentShape, AlignmentStyle, Ecc, Emphasis, Fit, HALFTONE_DAMAGE_BUDGET, Halftone, QrCode,
    QrMatrix, Style, Version,
};
use freeqrent_verify::{Bar, verify_style, verify_with};

const PICTURES: [&str; 5] = ["disc", "gradient", "rings", "photo", "logo"];

fn pinned(version: u8, ecc: Ecc) -> freeqrent::QrBuilder {
    QrCode::builder()
        .ecc(ecc)
        .version(Version::new(version).unwrap())
}

fn share(qr: &QrMatrix, style: &Style) -> f64 {
    let s = style.check(qr).steering.unwrap();
    s.matched_modules as f64 / s.code_modules as f64
}

#[test]
fn steered_codes_scan_and_follow_the_picture() {
    let text = "https://example.com";
    for kind in PICTURES {
        let picture = picture(kind, 160);
        for version in [2u8, 5, 10, 14, 20, 27] {
            for ecc in [Ecc::L, Ecc::M, Ecc::Q, Ecc::H] {
                let style = Style::new().halftone(Halftone::new(picture.clone()));
                let Ok(qr) = pinned(version, ecc).encode_text_for(text, &style) else {
                    continue; // v2-H can't hold the URL
                };
                assert_eq!(qr.ecc(), ecc, "steering never boosts the ECC level");
                let report = style.check(&qr);
                assert!(report.ok() && report.warnings.is_empty(), "{report:?}");
                if let Err(e) = verify_style(&style, &qr, text) {
                    panic!("{kind} v{version} {ecc:?}: {e}");
                }
                // An unsteered code matches about half; steering adds what
                // its free bits allow.
                let plain = Style::new().halftone(Halftone::new(picture.clone()).steer(false));
                let qr0 = pinned(version, ecc).encode_text_for(text, &plain).unwrap();
                let (steered, unsteered) = (share(&qr, &style), share(&qr0, &plain));
                let steerable = report.steering.unwrap().steerable_modules as f64
                    / report.steering.unwrap().code_modules as f64;
                // Below a tenth, chance and the mask decide.
                assert!(
                    steerable < 0.1 || steered >= unsteered + steerable * 0.25,
                    "{kind} v{version} {ecc:?}: {steered:.3} vs {unsteered:.3} ({steerable:.3} steerable)"
                );
            }
        }
    }
}

#[test]
fn steering_off_is_the_standard_encoding() {
    let style = Style::new().halftone(Halftone::new(picture("disc", 64)).steer(false));
    let b = pinned(10, Ecc::M);
    let qr = b.encode_text_for("HELLO", &style).unwrap();
    assert_eq!(qr, b.boost_ecc(false).encode_text("HELLO").unwrap());
    assert_eq!(style.check(&qr).steering.unwrap().steerable_modules, 0);
}

#[test]
fn steered_outlines_fit() {
    // Unsteered, outlined alignment patterns are mostly refused from v7 to
    // v14 (SPEC §7.2); steered, the ring modules come out light and use at
    // most half as much of the budget. A picture code's budget is the
    // smaller `HALFTONE_DAMAGE_BUDGET`; where it's still exceeded, the
    // fitted code grows instead.
    let text = "https://example.com";
    let outline = AlignmentStyle::new(AlignmentShape::Match, Emphasis::Outline);
    let mut fit = 0;
    for version in 7..=14u8 {
        let qr = pinned(version, Ecc::M)
            .boost_ecc(false)
            .encode_text(text)
            .unwrap();
        let unsteered = Style::new().alignment(outline).check(&qr).ecc_budget_used;
        let style = Style::new()
            .alignment(outline)
            .halftone(Halftone::new(picture("photo", 120)));
        let qr = pinned(version, Ecc::M)
            .encode_text_for(text, &style)
            .unwrap();
        // Measured: 0.55–1.44 unsteered, 0–0.44 steered.
        let used = style.check(&qr).ecc_budget_used;
        assert!(
            used <= unsteered / 2.0 && used < 0.5,
            "v{version}: {used} vs {unsteered}"
        );
        if used <= HALFTONE_DAMAGE_BUDGET {
            fit += 1;
            if let Err(e) = verify_style(&style, &qr, text) {
                panic!("v{version}: {e}");
            }
        }
        let grown = QrCode::builder()
            .ecc(Ecc::M)
            .min_version(Version::new(version).unwrap())
            .encode_text_for(text, &style)
            .unwrap();
        if let Err(e) = verify_style(&style, &grown, text) {
            panic!("v{version} grown to v{}: {e}", grown.version().number());
        }
    }
    assert!(fit >= 7, "{fit} of 8 versions fit");
}

#[test]
fn priority_maps_and_fits_still_scan() {
    let text = "https://example.com";
    let mut map = freeqrent::Rgba8Image::filled(40, 40, [0, 0, 0, 255]).unwrap();
    for y in 0..40 {
        for x in 0..20 {
            map.set_pixel(x, y, [255, 255, 255, 255]);
        }
    }
    let wide = freeqrent::Rgba8Image::filled(300, 100, [30, 30, 30, 255]).unwrap();
    for halftone in [
        Halftone::new(picture("rings", 120)).priority(map),
        Halftone::new(wide.clone())
            .fit(Fit::Contain)
            .focal(0.0, 1.0),
        Halftone::new(wide).fit(Fit::Cover).focal(1.0, 0.0),
    ] {
        let style = Style::new().halftone(halftone);
        let qr = pinned(14, Ecc::M).encode_text_for(text, &style).unwrap();
        verify_with(&style.render(&qr).unwrap(), text, Bar::FULL).unwrap();
    }
}
