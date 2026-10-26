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
    assert_eq!(html.matches("<figure>").count(), 15);
    // Each family gets a code that can afford it, so all of them decode.
    assert_eq!(html.matches("class=\"verdict\">decodes").count(), 15);
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
