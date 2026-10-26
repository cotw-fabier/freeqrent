//! Runs the `freeqrent` binary end to end.

use std::process::{Command, Output};

fn freeqrent(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_freeqrent"))
        .args(args)
        .output()
        .expect("runs")
}

fn stdout(out: &Output) -> String {
    String::from_utf8(out.stdout.clone()).unwrap()
}

fn stderr(out: &Output) -> String {
    String::from_utf8(out.stderr.clone()).unwrap()
}

#[test]
fn renders_to_stdout_with_the_url_as_label() {
    let out = freeqrent(&["https://example.com", "--shape", "dots:0.8"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let svg = stdout(&out);
    assert!(svg.starts_with("<svg "));
    assert!(svg.contains("aria-label=\"https://example.com\""));
}

#[test]
fn verify_decodes_the_render() {
    let out = freeqrent(&["HELLO", "--finder", "leaf", "--verify"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stderr(&out).contains("verified: decodes with ZXing"));
    assert!(stderr(&out).contains("warning: rounded scan-box rings"));
}

#[test]
fn check_reports_and_sets_the_exit_code() {
    let ok = freeqrent(&["HELLO", "--check", "--target-px", "60"]);
    assert_eq!(ok.status.code(), Some(0));
    assert!(stdout(&ok).starts_with("ok: renders"));
    assert!(stdout(&ok).contains("warning: 2.1 px per module"));
    let refused = freeqrent(&["HELLO", "--check", "--fg", "#aaa"]);
    assert_eq!(refused.status.code(), Some(1));
    assert!(stdout(&refused).contains("error: fg has contrast"));
}

#[test]
fn bad_arguments_exit_2() {
    for args in [
        &["HELLO", "--shape", "hex"][..],
        &["HELLO", "--nope", "x"],
        &["--ecc", "Z", "HELLO"],
        &[],
    ] {
        let out = freeqrent(args);
        assert_eq!(out.status.code(), Some(2), "{args:?}: {}", stderr(&out));
    }
}

#[test]
fn refused_styles_exit_1_and_write_nothing() {
    let out = freeqrent(&["HELLO", "--finder", "rounded:3"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stdout(&out).is_empty());
    assert!(stderr(&out).contains("FinderStyle.outer radius"));
}

#[test]
fn gallery_shows_every_family() {
    let dir = std::env::temp_dir().join(format!("freeqrent-gallery-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("gallery.html");
    let out = freeqrent(&[
        "https://example.com/menu",
        "--gallery",
        path.to_str().unwrap(),
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    let html = std::fs::read_to_string(&path).unwrap();
    assert_eq!(html.matches("<figure>").count(), 28);
    // Each family gets a code that can afford it, so all of them decode.
    assert_eq!(html.matches("class=\"verdict\">decodes").count(), 28);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn logos_grow_the_code_until_they_fit() {
    let out = freeqrent(&[
        "https://example.com",
        "--logo",
        "sample",
        "--logo-scale",
        "0.3",
        "--check",
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
    assert!(stdout(&out).contains("error-correction budget used"));
    let missing = freeqrent(&["HELLO", "--logo", "/nonexistent/logo.svg"]);
    assert_eq!(missing.status.code(), Some(2));
    assert!(stderr(&missing).contains("--logo /nonexistent/logo.svg"));
    // Pinned too small, the logo is refused with the scale that would fit.
    let pinned = freeqrent(&["HELLO", "--logo", "sample", "--version", "2", "--check"]);
    assert_eq!(pinned.status.code(), Some(1));
    assert!(
        stdout(&pinned).contains("at most scale"),
        "{}",
        stdout(&pinned)
    );
}

/// A 40×40 PNG: a warm white page with a navy block and an orange one.
fn sample_png(path: &std::path::Path) {
    let mut data = Vec::new();
    for y in 0..40u32 {
        for x in 0..40u32 {
            data.extend_from_slice(match (x < 16, y < 12) {
                (true, true) => &[230, 90, 20, 255],
                (true, false) => &[20, 30, 80, 255],
                _ => &[250, 248, 244, 255],
            });
        }
    }
    let file = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), 40, 40);
    enc.set_color(png::ColorType::Rgba);
    enc.write_header().unwrap().write_image_data(&data).unwrap();
}

#[test]
fn palettes_come_from_images_and_pages() {
    let dir = std::env::temp_dir().join(format!("freeqrent-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let png = dir.join("brand.png");
    sample_png(&png);
    let png = png.to_str().unwrap();

    let out = freeqrent(&[
        "HELLO",
        "--palette-from-image",
        png,
        "--show-palette",
        "--verify",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    let err = stderr(&out);
    assert!(err.contains("palette: 3 colors found"), "{err}");
    assert!(err.contains("finder.eye"), "{err}");
    assert!(err.contains("darkened"), "{err}");
    assert!(err.contains("as flags: --bg"), "{err}");
    assert!(err.contains("verified: decodes"), "{err}");
    assert!(!err.contains("warning"), "{err}");

    // The printed flags reproduce the same document.
    let flags = err
        .lines()
        .find_map(|l| l.trim().strip_prefix("as flags: "))
        .unwrap();
    let mut args = vec!["HELLO".to_string()];
    for part in flags.split(" --").map(|p| p.trim_start_matches("--")) {
        let (flag, value) = part.split_once(' ').unwrap();
        args.push(format!("--{flag}"));
        args.push(value.trim_matches('\'').to_string());
    }
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let again = freeqrent(&args);
    let first = freeqrent(&["HELLO", "--palette-from-image", png]);
    assert_eq!(stdout(&again), stdout(&first));

    // Explicit tokens win over extracted ones.
    let out = freeqrent(&["HELLO", "--palette-from-css", "sample", "--fg", "#101010"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stdout(&out).contains("fill=\"#101010\""));

    let html = dir.join("page.html");
    std::fs::write(
        &html,
        r##"<meta name="theme-color" content="#0f7b6c"><style>body{background:#fbfbf8;color:#1f2328}</style>"##,
    )
    .unwrap();
    let out = freeqrent(&[
        "HELLO",
        "--palette-from-html",
        html.to_str().unwrap(),
        "--show-palette",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(stderr(&out).contains("#fbfbf8"), "{}", stderr(&out));

    for bad in [
        &["HELLO", "--palette-from-image", "/nonexistent.png"][..],
        &["HELLO", "--palette-from-image", html.to_str().unwrap()],
    ] {
        assert_eq!(freeqrent(bad).status.code(), Some(2), "{bad:?}");
    }
    let plain = dir.join("plain.css");
    std::fs::write(&plain, "p { margin: 0 }").unwrap();
    let empty = freeqrent(&["HELLO", "--palette-from-css", plain.to_str().unwrap()]);
    assert_eq!(empty.status.code(), Some(2), "{}", stderr(&empty));
    assert!(stderr(&empty).contains("no colors"), "{}", stderr(&empty));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn pictures_build_the_code() {
    let out = freeqrent(&["https://example.com", "--art", "sample", "--check"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let report = stdout(&out);
    assert!(report.contains("steering: "), "{report}");
    assert!(report.contains("follow the picture"), "{report}");

    // Art mode uses the standard grid; the flag picks another.
    let svg = |args: &[&str]| {
        let out = freeqrent(args);
        assert!(out.status.success(), "{args:?}: {}", stderr(&out));
        stdout(&out)
    };
    assert!(svg(&["HELLO", "--art", "sample"]).contains("viewBox=\"0 0 81 81\""));
    let dense = svg(&["HELLO", "--art", "sample", "--tracking-grid", "dense"]);
    assert!(dense.contains("viewBox=\"0 0 109 109\""));
    assert!(svg(&["HELLO", "--art", "sample", "--version", "8"]).contains("viewBox=\"0 0 57 57\""));
    let color = svg(&[
        "HELLO",
        "--halftone",
        "sample",
        "--ink",
        "color:3",
        "--tracking-grid",
        "sparse",
        "--verify",
    ]);
    assert!(color.contains("stroke-width=\"3\""));

    for bad in [
        &["HELLO", "--ink", "mono"][..],
        &["HELLO", "--halftone", "/nonexistent.png"],
        &["HELLO", "--halftone", "sample", "--ink", "color:40"],
        &["HELLO", "--halftone", "sample", "--focal", "0.5"],
        &["HELLO", "--tracking-grid", "huge"],
    ] {
        let out = freeqrent(bad);
        assert_eq!(out.status.code(), Some(2), "{bad:?}: {}", stderr(&out));
    }
}

#[test]
fn the_scan_sheet_names_every_card() {
    let dir = std::env::temp_dir().join(format!("freeqrent-sheet-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("sheet.html");
    let out = freeqrent(&["--scan-sheet", path.to_str().unwrap()]);
    assert!(out.status.success(), "{}", stderr(&out));
    let html = std::fs::read_to_string(&path).unwrap();
    let cards = html.matches("<figure data-id=").count();
    assert_eq!(cards, 46);
    // Every code renders, and each payload names its card.
    assert!(!html.contains("Refused"));
    assert!(html.contains("https://example.com/scan/S01"));
    assert!(html.contains("FQ-S46"));
    assert_eq!(html.matches("<select data-device=").count(), cards * 4);
    std::fs::remove_dir_all(&dir).unwrap();
}
