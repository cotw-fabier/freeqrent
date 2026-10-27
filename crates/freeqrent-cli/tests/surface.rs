//! `bindings/SURFACE.md` is the CLI's surface as the bindings see it: every
//! flag in `--help` is on it, and every equivalence line renders the same
//! bytes through the CLI binary and through `freeqrent-doc`.

use std::path::{Path, PathBuf};
use std::process::Command;

use freeqrent_doc::Payload;

const TEXT: &str = "HELLO freeQRent";

fn surface() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bindings/SURFACE.md");
    std::fs::read_to_string(path).expect("bindings/SURFACE.md")
}

fn freeqrent(args: &[String]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_freeqrent"))
        .args(args)
        .output()
        .expect("runs")
}

#[test]
fn every_flag_is_on_the_surface() {
    let help = String::from_utf8(freeqrent(&["--help".into()]).stdout).unwrap();
    let surface = surface();
    let mut flags: Vec<&str> = help
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .filter(|w| {
            w.len() > 2
                && w.starts_with("--")
                && !w.ends_with('-')
                && w.as_bytes()[2].is_ascii_lowercase()
        })
        .collect();
    flags.sort_unstable();
    flags.dedup();
    assert!(flags.len() > 80, "{flags:?}");
    let missing: Vec<_> = flags
        .iter()
        .filter(|f| !surface.contains(&format!("`{f}")) && !surface.contains(&format!("{f} ")))
        .collect();
    assert!(
        missing.is_empty(),
        "not in bindings/SURFACE.md: {missing:?}"
    );
}

/// Splits a command line on spaces, keeping single-quoted words whole.
fn words(line: &str) -> Vec<String> {
    let (mut out, mut word, mut quoted, mut any) = (Vec::new(), String::new(), false, false);
    for c in line.chars() {
        match c {
            '\'' => (quoted, any) = (!quoted, true),
            ' ' if !quoted => {
                if any {
                    out.push(std::mem::take(&mut word));
                }
                any = false;
            }
            c => (word, any) = (word + &c.to_string(), true),
        }
    }
    if any {
        out.push(word);
    }
    out
}

fn base64(data: &[u8]) -> String {
    const ABC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            out.push(if i <= chunk.len() {
                ABC[(n >> (18 - 6 * i) & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    out
}

/// A 24×24 picture: a warm white page, a navy block and an orange one.
fn picture_png(path: &Path) {
    let mut data = Vec::new();
    for y in 0..24u32 {
        for x in 0..24u32 {
            data.extend_from_slice(match (x < 10, y < 8) {
                (true, true) => &[230, 90, 20, 255],
                (true, false) => &[20, 30, 80, 255],
                _ => &[250, 248, 244, 255],
            });
        }
    }
    let file = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), 24, 24);
    enc.set_color(png::ColorType::Rgba);
    enc.write_header().unwrap().write_image_data(&data).unwrap();
}

const MARK: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><circle cx="5" cy="5" r="4" fill="#c2410c"/></svg>"##;

struct Files {
    dir: PathBuf,
    svg: String,
    png: Vec<u8>,
}

/// The test's files, in a directory of its own.
fn files(test: &str) -> Files {
    let dir = std::env::temp_dir().join(format!("freeqrent-{test}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("mark.svg"), MARK).unwrap();
    picture_png(&dir.join("picture.png"));
    let png = std::fs::read(dir.join("picture.png")).unwrap();
    Files {
        dir,
        svg: MARK.to_string(),
        png,
    }
}

impl Files {
    fn flag(&self, word: &str) -> String {
        match word {
            "$SVG" => self.dir.join("mark.svg").display().to_string(),
            "$PNG" | "$IMAGE" => self.dir.join("picture.png").display().to_string(),
            w => w.to_string(),
        }
    }

    fn document(&self, json: &str) -> String {
        let mut svg = String::new();
        freeqrent_doc::json::write_str(&mut svg, &self.svg);
        let image = freeqrent_image::decode(&self.png).unwrap();
        let image = format!(
            r#"{{"width":{},"height":{},"rgba":"{}"}}"#,
            image.width(),
            image.height(),
            base64(image.data())
        );
        json.replace("$SVG", &svg)
            .replace("$PNG", &format!("\"{}\"", base64(&self.png)))
            .replace("$IMAGE", &image)
    }
}

/// Every equivalence line: `FLAGS => [plan | frame MS] DOCUMENT`.
fn equivalences() -> Vec<(String, String)> {
    let surface = surface();
    let mut lines = Vec::new();
    let mut inside = false;
    for line in surface.lines() {
        match line.trim() {
            "```equivalences" => inside = true,
            "```" => inside = false,
            l if inside && !l.is_empty() => {
                let (flags, doc) = l.split_once(" => ").expect("FLAGS => DOCUMENT");
                lines.push((flags.trim().to_string(), doc.trim().to_string()));
            }
            _ => {}
        }
    }
    lines
}

#[test]
fn the_cli_and_the_style_document_agree() {
    let files = files("equivalences");
    let lines = equivalences();
    assert!(lines.len() >= 60, "{}", lines.len());
    for (flags, doc) in &lines {
        let mut args = vec![TEXT.to_string()];
        args.extend(words(flags).iter().map(|w| files.flag(w)));
        let out = freeqrent(&args);
        assert!(
            out.status.success(),
            "{flags}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let cli = String::from_utf8(out.stdout).unwrap();
        let payload = Payload::Text(TEXT);
        let ours = match doc.split_once(' ') {
            Some(("plan", d)) => freeqrent_doc::plan(payload, &files.document(d)),
            Some(("frame", rest)) => {
                let (ms, d) = rest.split_once(' ').expect("frame MS DOCUMENT");
                freeqrent_doc::render_frame(payload, &files.document(d), ms.parse().unwrap())
            }
            _ => freeqrent_doc::render(payload, &files.document(doc)),
        }
        .unwrap_or_else(|e| panic!("{doc}: {}", e.to_json()));
        assert!(cli == ours, "{flags} and {doc} differ");
    }
    std::fs::remove_dir_all(&files.dir).unwrap();
}

/// `--palette-from-*` is two calls at the boundary: the palette result's
/// `palette` goes into the document.
#[test]
fn palette_flags_are_two_calls() {
    let files = files("palettes");
    let css = ":root { --brand: #1d3b72; --accent: #c2410c; --paper: #f8f5ee; }";
    let html = r##"<meta name="theme-color" content="#0f3d3e"><body style="background:#fdf6e3;color:#073642">"##;
    std::fs::write(files.dir.join("brand.css"), css).unwrap();
    std::fs::write(files.dir.join("page.html"), html).unwrap();
    let image = files.document("$IMAGE");
    let image = freeqrent_doc::json::parse(&image).unwrap();
    let field = |v: &freeqrent_doc::json::Value, k: &str| match v {
        freeqrent_doc::json::Value::Object(m) => m.iter().find(|(n, _)| n == k).unwrap().1.clone(),
        _ => panic!(),
    };
    let rgba = match field(&image, "rgba") {
        freeqrent_doc::json::Value::String(s) => s,
        _ => panic!(),
    };
    let decoded = freeqrent_image::decode(&files.png).unwrap();
    assert_eq!(base64(decoded.data()), rgba);
    let cases: Vec<(Vec<String>, String)> = vec![
        (
            vec!["--palette-from-css".into(), files.flag_path("brand.css")],
            freeqrent_doc::palette_from_css(css, "").unwrap(),
        ),
        (
            vec!["--palette-from-html".into(), files.flag_path("page.html")],
            freeqrent_doc::palette_from_html(html, "").unwrap(),
        ),
        (
            vec![
                "--palette-from-image".into(),
                files.flag("$IMAGE"),
                "--palette-clusters".into(),
                "4".into(),
                "--palette-contrast".into(),
                "0.7".into(),
            ],
            freeqrent_doc::palette_from_image(
                decoded.data(),
                decoded.width(),
                decoded.height(),
                r#"{"clusters":4,"min_contrast":0.7,"allow_inverted":false}"#,
            )
            .unwrap(),
        ),
    ];
    let mut svg = String::new();
    freeqrent_doc::json::write_str(&mut svg, &files.svg);
    // With a preset the extracted palette replaces the preset's, and with a
    // logo the frame keeps apart from the logo's colors.
    let with_preset_and_logo: Vec<(Vec<String>, String, &str)> = vec![
        (
            vec![
                "--preset".into(),
                "candy".into(),
                "--palette-from-html".into(),
                files.flag_path("page.html"),
            ],
            freeqrent_doc::palette_from_html(html, "").unwrap(),
            r#""preset":"candy","replace_palette":true,"#,
        ),
        (
            vec![
                "--palette-from-css".into(),
                files.flag_path("brand.css"),
                "--logo".into(),
                files.flag("$SVG"),
            ],
            freeqrent_doc::palette_from_css(css, &format!(r#"{{"logo":{{"svg":{svg}}}}}"#))
                .unwrap(),
            "",
        ),
    ];
    let logo_doc = format!(r#","logo":{{"svg":{svg}}}"#);
    let cases = cases
        .into_iter()
        .map(|(f, r)| (f, r, ""))
        .chain(with_preset_and_logo);
    for (flags, result, prefix) in cases {
        let palette = freeqrent_doc::json::parse(&result).unwrap();
        let mut doc = format!(r#"{{{prefix}"palette":"#);
        freeqrent_doc::json::write_compact(&mut doc, &field(&palette, "palette"));
        if flags.iter().any(|f| f == "--logo") {
            doc.push_str(&logo_doc);
        }
        doc.push('}');
        let mut args = vec![TEXT.to_string()];
        args.extend(flags.iter().cloned());
        let out = freeqrent(&args);
        assert!(out.status.success(), "{flags:?}");
        let ours = freeqrent_doc::render(Payload::Text(TEXT), &doc).unwrap();
        assert!(String::from_utf8(out.stdout).unwrap() == ours, "{flags:?}");
    }
    std::fs::remove_dir_all(&files.dir).unwrap();
}

impl Files {
    fn flag_path(&self, name: &str) -> String {
        self.dir.join(name).display().to_string()
    }
}
