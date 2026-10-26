//! Snapshots of whole documents (SPEC §13). Output is deterministic, so any
//! change here is a change in what users get: review it with
//! `cargo insta review` and say why in the wave log.

use freeqrent::{
    AlignmentShape, AlignmentStyle, Axis, Corner, Ecc, Emphasis, EyeShape, FinderStyle, Frame,
    FrameRing, Intro, Logo, LogoShape, ModuleShape, Palette, Preset, QrCode, QrMatrix, RingShape,
    Style, TimingStyle, Version,
};

fn code(version: u8) -> QrMatrix {
    QrCode::builder()
        .ecc(Ecc::M)
        .version(Version::new(version).unwrap())
        .encode_text("HELLO")
        .unwrap()
}

fn render(style: Style, version: u8) -> String {
    style.minify(false).render(&code(version)).unwrap()
}

#[test]
fn default_style() {
    insta::assert_snapshot!(render(Style::new(), 1));
}

#[test]
fn dots_and_squircles() {
    let style = Style::new()
        .modules(ModuleShape::Dots { size: 0.8 })
        .finders(FinderStyle::new(RingShape::Squircle, EyeShape::Circle))
        .timing(TimingStyle::Dotted);
    insta::assert_snapshot!(render(style, 2));
}

#[test]
fn rounded_with_leaf_corners_and_tokens() {
    let style = Style::new()
        .modules(ModuleShape::Rounded { radius: 0.4 })
        .finders(
            FinderStyle::new(RingShape::Rounded(2.0), EyeShape::Rounded(1.0)).corner(
                Corner::TopRight,
                RingShape::Leaf,
                EyeShape::Leaf,
            ),
        )
        .palette(
            Palette::from_tokens([
                ("bg", "oklch(97% .015 85)"),
                ("fg", "#1d2433"),
                ("finder.top-right.eye", "oklch(45% .15 30)"),
            ])
            .unwrap(),
        )
        .title("Menu")
        .label("https://example.com/menu");
    insta::assert_snapshot!(render(style, 2));
}

#[test]
fn bars_gradient_and_outlined_alignment() {
    let style = Style::new()
        .modules(ModuleShape::Bars {
            axis: Axis::Vertical,
            radius: 0.5,
        })
        .alignment(AlignmentStyle::new(
            AlignmentShape::Own {
                outer: RingShape::Square,
                eye: EyeShape::Square,
            },
            Emphasis::Outline,
        ))
        .timing(TimingStyle::Dashed)
        .palette(
            Palette::from_tokens([
                ("fg", "linear-gradient(135deg, #1d2b64, oklch(30% .14 330))"),
                ("alignment", "#8a3b12"),
            ])
            .unwrap(),
        );
    insta::assert_snapshot!(render(style, 3));
}

#[test]
fn gapped_squares_translucent_on_transparent() {
    let style = Style::new()
        .modules(ModuleShape::Square { gap: 0.2 })
        .palette(Palette::from_tokens([("bg", "transparent"), ("fg", "rgb(0 0 0 / .9)")]).unwrap())
        .quiet_zone(2);
    insta::assert_snapshot!(render(style, 1));
}

#[test]
fn liquid_with_rounded_finders() {
    let style = Style::new()
        .modules(ModuleShape::Liquid { smoothness: 0.8 })
        .finders(FinderStyle::new(RingShape::Rounded(2.0), EyeShape::Circle));
    insta::assert_snapshot!(render(style, 1));
}

#[test]
fn round_frame_with_dashed_ring() {
    let style = Style::new()
        .modules(ModuleShape::Dots { size: 0.85 })
        .quiet_zone(2)
        .frame(Frame::Circle {
            filler_density: 0.5,
            ring: FrameRing::Dashed,
        });
    insta::assert_snapshot!(render(style, 1));
}

#[test]
fn circle_logo_on_liquid() {
    let logo = Logo::svg(
        r##"<svg viewBox="0 0 10 10"><defs><linearGradient id="g"><stop offset="0" stop-color="#c2410c"/><stop offset="1" stop-color="#7c2d12"/></linearGradient></defs><circle cx="5" cy="5" r="5" fill="url(#g)"/><script>alert(1)</script></svg>"##,
    )
    .unwrap()
    // At most 0.21 here since ECC H got its own budget (W20261026171519).
    .scale(0.2)
    .shape(LogoShape::Circle);
    let style = Style::new()
        .modules(ModuleShape::Liquid { smoothness: 1.0 })
        .logo(logo);
    let qr = QrCode::builder()
        .ecc(Ecc::H)
        .version(Version::new(5).unwrap())
        .encode_text("HELLO")
        .unwrap();
    insta::assert_snapshot!(style.minify(false).render(&qr).unwrap());
}

#[test]
fn settle_intro_on_dots() {
    let style = Style::new()
        .modules(ModuleShape::Dots { size: 0.85 })
        .intro(Intro::new(Preset::Settle).groups(4));
    insta::assert_snapshot!(render(style, 1));
}

#[test]
fn sweep_reveal_with_logo() {
    let logo =
        Logo::svg(r##"<svg viewBox="0 0 4 4"><circle cx="2" cy="2" r="2" fill="#c33"/></svg>"##)
            .unwrap()
            .scale(0.2);
    let style = Style::new().logo(logo).intro(Preset::Sweep);
    let qr = QrCode::builder()
        .ecc(Ecc::H)
        .version(Version::new(5).unwrap())
        .encode_text("HELLO")
        .unwrap();
    insta::assert_snapshot!(style.minify(false).render(&qr).unwrap());
}

#[cfg(feature = "halftone")]
#[test]
fn halftone_in_mono_and_color() {
    use freeqrent::{Halftone, Ink, Rgba8Image};
    // A dark disc on a light page, small enough to read the paths.
    let mut picture = Rgba8Image::filled(16, 16, [245, 240, 230, 255]).unwrap();
    for y in 0..16i32 {
        for x in 0..16i32 {
            if (x - 8).pow(2) + (y - 8).pow(2) < 30 {
                picture.set_pixel(x as u32, y as u32, [30, 40, 90, 255]);
            }
        }
    }
    for ink in [Ink::Mono, Ink::Color { colors: 2 }] {
        let style = Style::new().halftone(Halftone::new(picture.clone()).ink(ink));
        let qr = QrCode::builder()
            .ecc(Ecc::M)
            .version(Version::new(1).unwrap())
            .encode_text_for("HELLO", &style)
            .unwrap();
        insta::assert_snapshot!(style.minify(false).render(&qr).unwrap());
    }
}

#[test]
fn presets() {
    for name in Style::presets() {
        let style = Style::preset(name).unwrap();
        insta::assert_snapshot!(format!("preset_{name}"), render(style, 1));
    }
}
