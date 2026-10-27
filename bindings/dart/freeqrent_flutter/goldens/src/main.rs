//! Writes `test/goldens/<name>.png` for every case in
//! `test/goldens/cases.json`: the style's static SVG (what an intro ends
//! on, SPEC §8.2) from freeqrent-doc, rasterized by resvg at the cases'
//! scale with `freeqrent_verify::rasterize` (over white, Rec. 601 luma),
//! as an 8-bit grayscale PNG. The widget tests draw the same styles'
//! finished frames and compare them with these, module by module.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use freeqrent_doc::json::{self, Value};
use freeqrent_doc::{Payload, render};

fn field<'a>(v: &'a Value, key: &str) -> &'a Value {
    let Value::Object(fields) = v else {
        panic!("expected an object around {key}");
    };
    fields
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v)
        .unwrap_or_else(|| panic!("no {key}"))
}

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../test/goldens");
    let text = std::fs::read_to_string(dir.join("cases.json")).expect("cases.json");
    let suite = json::parse(&text).expect("cases.json is JSON");
    let Value::Number(scale) = field(&suite, "px_per_module") else {
        panic!("px_per_module");
    };
    let scale: f32 = scale.parse().expect("a scale");
    let Value::String(payload) = field(&suite, "payload") else {
        panic!("payload");
    };
    let Value::Array(cases) = field(&suite, "cases") else {
        panic!("cases");
    };
    for case in cases {
        let Value::String(name) = field(case, "name") else {
            panic!("name");
        };
        let mut style = String::new();
        json::write_compact(&mut style, field(case, "style"));
        let svg = render(Payload::Text(payload), &style)
            .unwrap_or_else(|e| panic!("{name}: {}", e.to_json()));
        let gray = freeqrent_verify::rasterize(&svg, scale).expect("rasterizes");
        let path = dir.join(format!("{name}.png"));
        let file = BufWriter::new(File::create(&path).expect("writable"));
        let mut encoder = png::Encoder::new(file, gray.width as u32, gray.height as u32);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("PNG header");
        writer.write_image_data(&gray.pixels).expect("PNG data");
        writer.finish().expect("PNG written");
        println!("{name}: {}x{}", gray.width, gray.height);
    }
}
