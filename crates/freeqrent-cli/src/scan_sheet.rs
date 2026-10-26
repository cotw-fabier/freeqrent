//! `--scan-sheet`: the real-device scan matrix (SPEC §9) as one HTML page.
//! Every code carries its own id in the payload, so a phone's result says
//! which card it read; each card records a verdict per device, and the
//! page exports the results as a Markdown table for the wave log.

use std::fmt::Write;

use freeqrent::{Ecc, QrBuilder, QrCode, Version};

use crate::StyleArgs;
use crate::gallery::{DARK_FAMILIES, FAMILIES, LOGO_FAMILIES, escape, shell_quote};

/// The devices the matrix asks about.
const DEVICES: [&str; 4] = ["iOS Camera", "Google Lens", "WeChat", "ZXing app"];

/// One code on the sheet.
struct Entry {
    group: &'static str,
    name: String,
    flags: Vec<(&'static str, String)>,
    ecc: Ecc,
    version: Option<u8>,
}

impl Entry {
    fn new(group: &'static str, name: impl Into<String>, flags: &[(&'static str, &str)]) -> Entry {
        Entry {
            group,
            name: name.into(),
            flags: flags.iter().map(|&(f, v)| (f, v.to_string())).collect(),
            ecc: Ecc::M,
            version: None,
        }
    }
}

/// The matrix: every gallery family; art mode per grid and ink, and at
/// each ECC level; pictures with a logo and without steering; and v1 codes
/// near the contrast warning on off-white backgrounds (ROADMAP,
/// "lightness-aware contrast warning"); then the logo sizes, frames and
/// logo motion, and dark palettes, last so the earlier cards keep their
/// ids.
fn entries() -> Vec<Entry> {
    let mut out: Vec<Entry> = FAMILIES
        .iter()
        .map(|(name, flags)| Entry::new("Style families", *name, flags))
        .collect();
    for grid in ["sparse", "standard", "dense"] {
        for ink in ["mono", "color:6"] {
            out.push(Entry::new(
                "Art mode",
                format!("{grid} grid, {ink}"),
                &[("art", "sample"), ("tracking-grid", grid), ("ink", ink)],
            ));
        }
    }
    for ecc in [Ecc::L, Ecc::Q] {
        let mut e = Entry::new(
            "Art mode",
            format!("standard grid, ECC {ecc:?}"),
            &[("art", "sample")],
        );
        e.ecc = ecc;
        out.push(e);
    }
    out.push(Entry::new(
        "Pictures",
        "halftone with a logo",
        &[
            ("halftone", "sample"),
            // A halftone's logo stays off the alignment patterns, which
            // the sparse grid has in the middle (W20261026151955).
            ("tracking-grid", "standard"),
            ("logo", "sample"),
            ("logo-scale", "0.15"),
        ],
    ));
    out.push(Entry::new(
        "Pictures",
        "picture, steering off",
        &[
            ("halftone", "sample"),
            ("tracking-grid", "sparse"),
            ("no-steer", ""),
        ],
    ));
    for bg in [85, 95] {
        for delta in [60, 62, 64, 66] {
            let fg = format!("oklch({}% 0 0)", bg - delta);
            let mut e = Entry::new(
                "Contrast near the warning",
                format!("v1, bg L .{bg}, ΔL .{delta}"),
                &[],
            );
            e.flags = vec![("bg", format!("oklch({bg}% 0 0)")), ("fg", fg)];
            e.version = Some(1);
            out.push(e);
        }
    }
    out.extend(
        LOGO_FAMILIES
            .iter()
            .map(|(name, flags)| Entry::new("Logos", *name, flags)),
    );
    out.extend(
        DARK_FAMILIES
            .iter()
            .map(|(name, flags)| Entry::new("Dark palettes", *name, flags)),
    );
    // Inverted v1 codes at the extraction floor, the weakest case measured
    // (W20261026141235).
    for bg in [4, 16] {
        let fg = format!("oklch({}% 0 0)", bg + 70);
        let mut e = Entry::new("Dark palettes", format!("v1, bg L .{bg:02}, ΔL .70"), &[]);
        e.flags = vec![
            ("bg", format!("oklch({bg}% 0 0)")),
            ("fg", fg),
            ("allow-inverted", String::new()),
        ];
        e.version = Some(1);
        out.push(e);
    }
    out
}

pub fn page(base: &StyleArgs) -> String {
    let mut cards = String::new();
    let mut group = "";
    for (i, entry) in entries().into_iter().enumerate() {
        let id = format!("S{:02}", i + 1);
        // v1 holds only short text; the rest open a URL that names the card.
        let text = match entry.version {
            Some(_) => format!("FQ-{id}"),
            None => format!("https://example.com/scan/{id}"),
        };
        if entry.group != group {
            if !group.is_empty() {
                cards.push_str("</section>\n");
            }
            group = entry.group;
            let _ = writeln!(cards, "<section><h2>{}</h2>", escape(group));
        }
        let mut args = base.clone();
        let mut command = format!(" {}", shell_quote(&text));
        let mut problem = None;
        if entry.ecc != Ecc::M {
            let _ = write!(command, " --ecc {:?}", entry.ecc);
        }
        if let Some(v) = entry.version {
            let _ = write!(command, " --version {v}");
        }
        for (flag, value) in &entry.flags {
            if value.is_empty() && args.set_switch(flag) {
                args.set_switch(flag);
                let _ = write!(command, " --{flag}");
                continue;
            }
            if let Err(e) = args.set(flag, value) {
                problem = Some(e);
            }
            let _ = write!(command, " --{flag} {}", shell_quote(value));
        }
        let mut builder: QrBuilder = QrCode::builder().ecc(entry.ecc);
        if let Some(v) = entry.version.and_then(Version::new) {
            builder = builder.version(v);
        }
        let rendered = problem.map_or_else(
            || -> Result<(String, String), String> {
                let style = args.build(&text)?;
                let qr = args
                    .encoder(builder, entry.version.is_some())
                    .encode_text_for(&text, &style)
                    .map_err(|e| e.to_string())?;
                let svg = style.render(&qr).map_err(|e| e.to_string())?;
                let bench = match crate::verify(&style, &qr, &text) {
                    Ok(v) => v,
                    Err(e) if cfg!(feature = "verify") => format!("does not decode: {e}"),
                    Err(_) => "not verified".to_string(),
                };
                let size = format!("version {}, ECC {:?}", qr.version().number(), qr.ecc());
                Ok((svg, format!("bench: {bench} · {size}")))
            },
            Err,
        );
        let _ = write!(
            cards,
            "<figure data-id=\"{id}\" data-name=\"{}\"><button type=\"button\" class=\"code\" title=\"Show larger\">",
            escape(&entry.name)
        );
        match &rendered {
            Ok((svg, _)) => cards.push_str(svg),
            Err(e) => {
                let _ = write!(
                    cards,
                    "<span class=\"refused\">Refused: {}</span>",
                    escape(e)
                );
            }
        }
        let _ = write!(
            cards,
            "</button><figcaption><h3><span class=\"id\">{id}</span> {}</h3><p class=\"payload\">{}</p><code>freeqrent{}</code>",
            escape(&entry.name),
            escape(&text),
            escape(&command)
        );
        if let Ok((_, bench)) = &rendered {
            let _ = write!(cards, "<p class=\"bench\">{}</p>", escape(bench));
        }
        cards.push_str("<div class=\"devices\">");
        for (d, device) in DEVICES.iter().enumerate() {
            let _ = write!(
                cards,
                "<label>{device}<select data-device=\"{d}\"><option value=\"\">not tried</option><option value=\"ok\">scans</option><option value=\"slow\">scans, slowly</option><option value=\"fail\">fails</option></select></label>"
            );
        }
        cards.push_str("</div></figcaption></figure>\n");
    }
    cards.push_str("</section>\n");

    let devices = DEVICES
        .iter()
        .map(|d| format!("\"{d}\""))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>freeQRent scan matrix</title>
<style>
:root {{
  --page: oklch(97.5% .006 85);
  --ink: oklch(24% .01 265);
  --muted: oklch(48% .01 265);
  --rule: oklch(88% .008 85);
  --ok: oklch(48% .12 150);
  --slow: oklch(52% .12 70);
  --bad: oklch(50% .16 28);
}}
@media (prefers-color-scheme: dark) {{
  :root {{
    --page: oklch(20% .008 265);
    --ink: oklch(93% .006 85);
    --muted: oklch(70% .01 265);
    --rule: oklch(32% .01 265);
    --ok: oklch(76% .12 150);
    --slow: oklch(80% .12 80);
    --bad: oklch(72% .14 28);
  }}
}}
* {{ box-sizing: border-box; }}
body {{
  margin: 0;
  padding: clamp(16px, 4vw, 48px);
  background: var(--page);
  color: var(--ink);
  font: 15px/1.5 ui-sans-serif, system-ui, sans-serif;
}}
header {{ max-width: 70ch; margin-bottom: 32px; }}
h1 {{ font-size: 1.9rem; line-height: 1.2; margin: 0 0 8px; font-weight: 650; }}
header p {{ margin: 0 0 8px; color: var(--muted); }}
.tally {{ color: var(--ink); font-variant-numeric: tabular-nums; }}
section {{ margin-bottom: 48px; }}
h2 {{ font-size: 1.2rem; margin: 0 0 20px; font-weight: 620; }}
section > figure {{ margin: 0; }}
section {{
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(min(100%, 260px), 1fr));
  gap: 36px 28px;
}}
section > h2 {{ grid-column: 1 / -1; margin-bottom: 0; }}
figure.large {{ grid-column: 1 / -1; }}
figure.large .code {{ max-width: min(100%, 78vh); }}
.code {{ display: block; width: 100%; padding: 0; border: 0; background: none; cursor: zoom-in; }}
figure.large .code {{ cursor: zoom-out; }}
.code svg {{ display: block; width: 100%; height: auto; border-radius: 6px; }}
.refused {{ aspect-ratio: 1; display: grid; place-items: center; padding: 16px;
  border: 1px dashed var(--rule); color: var(--bad); border-radius: 6px; }}
figcaption {{ padding-top: 12px; border-top: 1px solid var(--rule); margin-top: 12px; }}
h3 {{ font-size: 1rem; margin: 0 0 2px; font-weight: 600; }}
.id {{ font: 600 13px/1 ui-monospace, SFMono-Regular, Menlo, monospace; color: var(--muted); }}
.payload {{ margin: 0 0 6px; font-size: 13px; color: var(--muted); overflow-wrap: anywhere; }}
code {{ display: block; font: 12px/1.45 ui-monospace, SFMono-Regular, Menlo, monospace;
  color: var(--muted); overflow-wrap: anywhere; }}
.bench {{ margin: 6px 0 0; font-size: 13px; }}
.devices {{ display: grid; grid-template-columns: 1fr 1fr; gap: 6px 12px; margin-top: 10px; }}
.devices label {{ display: grid; gap: 2px; font-size: 12px; color: var(--muted); }}
select {{ font: inherit; font-size: 13px; color: var(--ink); background: var(--page);
  border: 1px solid var(--rule); border-radius: 4px; padding: 3px 4px; }}
select[data-value="ok"] {{ border-color: var(--ok); color: var(--ok); }}
select[data-value="slow"] {{ border-color: var(--slow); color: var(--slow); }}
select[data-value="fail"] {{ border-color: var(--bad); color: var(--bad); }}
.export {{ max-width: 70ch; }}
.export button {{ font: inherit; padding: 6px 12px; border: 1px solid var(--rule); border-radius: 6px;
  background: var(--page); color: var(--ink); cursor: pointer; }}
textarea {{ width: 100%; min-height: 12em; margin-top: 12px; font: 12px/1.45 ui-monospace, monospace;
  background: var(--page); color: var(--ink); border: 1px solid var(--rule); border-radius: 6px; padding: 8px; }}
@media print {{
  .devices, .export, .bench, code {{ display: none; }}
  figure {{ break-inside: avoid; }}
}}
</style>
</head>
<body>
<header>
<h1>Scan matrix</h1>
<p>Scan each code from this screen (or a print) with every device, and pick what happened. Each payload names its card, so check that the id the phone shows matches. Click a code to show it larger. Answers stay in this browser.</p>
<p class="tally"></p>
</header>
<main>
{cards}</main>
<div class="export">
<button type="button" id="export">Write the results as Markdown</button>
<textarea id="results" readonly hidden aria-label="Results as Markdown"></textarea>
</div>
<script>
const devices = [{devices}];
const key = "freeqrent-scan-matrix";
let saved = {{}};
try {{ saved = JSON.parse(localStorage.getItem(key) || "{{}}"); }} catch {{}}
const store = () => {{ try {{ localStorage.setItem(key, JSON.stringify(saved)); }} catch {{}} }};
const figures = [...document.querySelectorAll("figure[data-id]")];
const tally = () => {{
  const all = document.querySelectorAll("select[data-device]");
  const done = [...all].filter((s) => s.value).length;
  document.querySelector(".tally").textContent = `${{done}} of ${{all.length}} answers recorded`;
}};
for (const fig of figures) {{
  for (const select of fig.querySelectorAll("select")) {{
    const k = `${{fig.dataset.id}}/${{select.dataset.device}}`;
    select.value = saved[k] || "";
    select.dataset.value = select.value;
    select.addEventListener("change", () => {{
      saved[k] = select.value;
      select.dataset.value = select.value;
      store();
      tally();
    }});
  }}
  fig.querySelector(".code").addEventListener("click", () => {{
    fig.classList.toggle("large");
    fig.scrollIntoView({{ block: "nearest" }});
  }});
}}
tally();
const words = {{ "": "–", ok: "scans", slow: "slow", fail: "**fails**" }};
document.getElementById("export").addEventListener("click", () => {{
  const rows = ["| Card | Style | " + devices.join(" | ") + " |", "|---|---|" + devices.map(() => "---|").join("")];
  for (const fig of figures) {{
    const cells = devices.map((_, d) => words[saved[`${{fig.dataset.id}}/${{d}}`] || ""]);
    rows.push(`| ${{fig.dataset.id}} | ${{fig.dataset.name}} | ${{cells.join(" | ")}} |`);
  }}
  const out = document.getElementById("results");
  out.hidden = false;
  out.value = rows.join("\n");
  out.select();
}});
</script>
</body>
</html>
"#
    )
}
