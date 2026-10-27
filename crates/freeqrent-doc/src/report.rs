//! Results as JSON: the scan report and extracted palettes. Same input,
//! same bytes.

use freeqrent::{Origin, Palette, PaletteReport, QrMatrix, ScanReport};

use crate::error::{Detail, DocError, ecc_name, render_error_parts, warning_parts};
use crate::json::{self, ObjectWriter};

fn entry(out: &mut String, code: &str, message: &str, detail: Detail) {
    let mut o = ObjectWriter::new(out);
    o.str("code", code);
    o.str("message", message);
    o.raw("detail", &detail.finish());
    o.end();
}

/// The report for `qr`:
/// `{"ok","errors","warnings","min_contrast","ecc_budget_used","logo_scale","steering","code"}`.
/// Errors and warnings are `{code, message, detail}` entries; `code` says
/// what the payload was encoded as.
pub fn report_json(report: &ScanReport, qr: &QrMatrix) -> String {
    let mut out = String::new();
    let mut o = ObjectWriter::new(&mut out);
    o.bool("ok", report.ok());
    let list = o.key("errors");
    list.push('[');
    for (i, e) in report.errors.iter().enumerate() {
        if i > 0 {
            list.push(',');
        }
        let (code, detail) = render_error_parts(e);
        entry(list, code, &e.to_string(), detail);
    }
    list.push(']');
    let list = o.key("warnings");
    list.push('[');
    for (i, w) in report.warnings.iter().enumerate() {
        if i > 0 {
            list.push(',');
        }
        let (code, detail) = warning_parts(w);
        entry(list, code, &w.to_string(), detail);
    }
    list.push(']');
    o.f32("min_contrast", report.min_contrast);
    o.f32("ecc_budget_used", report.ecc_budget_used);
    match report.logo_scale {
        Some(s) => o.f32("logo_scale", s),
        None => o.raw("logo_scale", "null"),
    }
    match report.steering {
        Some(s) => {
            let mut st = ObjectWriter::new(o.key("steering"));
            st.int("steerable_modules", s.steerable_modules);
            st.int("code_modules", s.code_modules);
            st.int("matched_modules", s.matched_modules);
            st.end();
        }
        None => o.raw("steering", "null"),
    }
    let mut c = ObjectWriter::new(o.key("code"));
    c.int("version", qr.version().number());
    c.str("ecc", ecc_name(qr.ecc()));
    c.int("size", qr.size());
    c.int("mask", qr.mask().pattern());
    c.end();
    o.end();
    out
}

/// `{token: css}`, the form a Style document's `palette` takes.
pub fn palette_json(palette: &Palette) -> String {
    let mut out = String::new();
    let mut o = ObjectWriter::new(&mut out);
    for (token, paint) in palette.tokens() {
        o.str(token.name(), &paint.to_string());
    }
    o.end();
    out
}

/// An extracted palette:
/// `{"palette":{token: css},"swatches":[{color, share}],"choices":[{token, color, origin}]}`.
/// `origin` is `{"kind":"swatch","swatch":i}`, `{"kind":"tinted","swatch":i|null}`,
/// or `{"kind":"darkened"|"lightened","swatch":i,"delta_l":d}`.
pub fn extracted_json(
    found: Result<(Palette, PaletteReport), freeqrent::ExtractError>,
) -> Result<String, DocError> {
    let (palette, report) = found?;
    let mut out = String::new();
    let mut o = ObjectWriter::new(&mut out);
    o.raw("palette", &palette_json(&palette));
    let list = o.key("swatches");
    list.push('[');
    for (i, s) in report.swatches.iter().enumerate() {
        if i > 0 {
            list.push(',');
        }
        let mut w = ObjectWriter::new(list);
        w.str("color", &s.color.to_string());
        w.f32("share", s.share);
        w.end();
    }
    list.push(']');
    let list = o.key("choices");
    list.push('[');
    for (i, c) in report.choices.iter().enumerate() {
        if i > 0 {
            list.push(',');
        }
        let mut w = ObjectWriter::new(list);
        w.str("token", c.token.name());
        w.str("color", &c.color.to_string());
        let mut g = ObjectWriter::new(w.key("origin"));
        match c.origin {
            Origin::Swatch(s) => {
                g.str("kind", "swatch");
                g.int("swatch", s);
            }
            Origin::Tinted(s) => {
                g.str("kind", "tinted");
                match s {
                    Some(s) => g.int("swatch", s),
                    None => g.raw("swatch", "null"),
                }
            }
            Origin::Darkened { swatch, delta_l } => {
                g.str("kind", "darkened");
                g.int("swatch", swatch);
                g.f32("delta_l", delta_l);
            }
            Origin::Lightened { swatch, delta_l } => {
                g.str("kind", "lightened");
                g.int("swatch", swatch);
                g.f32("delta_l", delta_l);
            }
            _ => g.str("kind", "other"),
        }
        g.end();
        w.end();
    }
    list.push(']');
    o.end();
    Ok(out)
}

/// `["a","b"]`.
pub fn string_array(items: &[&str]) -> String {
    let mut out = String::from("[");
    for (i, s) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        json::write_str(&mut out, s);
    }
    out.push(']');
    out
}
