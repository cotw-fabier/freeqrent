//! Snapshots of whole documents (SPEC §13). Output is deterministic, so any
//! change here is a change in what users get: review it with
//! `cargo insta review` and say why in the wave log.

use freeqrent::{
    AlignmentShape, AlignmentStyle, Axis, Corner, Ecc, Emphasis, EyeShape, FinderStyle,
    ModuleShape, Palette, QrCode, QrMatrix, RingShape, Style, TimingStyle, Version,
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
        .alignment(AlignmentStyle {
            shape: AlignmentShape::Own {
                outer: RingShape::Square,
                eye: EyeShape::Square,
            },
            emphasis: Emphasis::Outline,
        })
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
