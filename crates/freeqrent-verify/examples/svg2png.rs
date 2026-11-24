//! Renders an SVG to a color PNG with resvg, to look at styles by eye:
//! `cargo run -p freeqrent-verify --example svg2png -- IN.svg OUT.png [WIDTH]`.

use resvg::{tiny_skia, usvg};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (input, output) = (&args[1], &args[2]);
    let width: f32 = args.get(3).map_or(720.0, |w| w.parse().expect("a width"));
    let svg = std::fs::read_to_string(input).expect("the SVG");
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).expect("a valid SVG");
    let size = tree.size();
    let scale = width / size.width();
    let mut pixmap = tiny_skia::Pixmap::new(
        (size.width() * scale).ceil() as u32,
        (size.height() * scale).ceil() as u32,
    )
    .expect("a size");
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap.save_png(output).expect("writing the PNG");
}
