//! `--gallery`: one HTML page showing the payload in every style family,
//! each with the flags that reproduce it and whether it decodes.

use std::fmt::Write;

use freeqrent::QrBuilder;

use crate::StyleArgs;

/// Style families: a name and the flags that make it.
pub(crate) const FAMILIES: &[(&str, &[(&str, &str)])] = &[
    ("Classic", &[]),
    ("Rounded", &[("shape", "rounded:0.4")]),
    ("Gapped squares", &[("shape", "square:0.15")]),
    (
        "Dots",
        &[("shape", "dots:0.8"), ("finder", "rounded:2/circle")],
    ),
    ("Bars", &[("shape", "bars-v:0.5"), ("finder", "rounded:1")]),
    (
        "Liquid",
        &[("shape", "liquid:0.8"), ("finder", "rounded:2/rounded:1")],
    ),
    (
        "Squircle",
        &[
            ("shape", "dots:0.9"),
            ("finder", "squircle"),
            ("timing", "dotted"),
        ],
    ),
    ("Leaf", &[("shape", "rounded:0.5"), ("finder", "leaf")]),
    (
        "Round",
        &[
            ("shape", "liquid:1"),
            ("finder", "rounded:2/rounded:1"),
            ("frame", "circle:0.6"),
            ("frame-ring", "dashed"),
            ("quiet-zone", "2"),
        ],
    ),
    (
        "Ink on paper",
        &[
            ("bg", "oklch(96% .02 85)"),
            ("fg", "oklch(28% .05 265)"),
            ("token", "finder.eye=oklch(45% .15 30)"),
        ],
    ),
    (
        "Gradient",
        &[
            ("shape", "dots:0.85"),
            ("finder", "squircle/circle"),
            (
                "fg",
                "linear-gradient(135deg, oklch(32% .12 265), oklch(30% .14 330))",
            ),
        ],
    ),
    (
        "Accent alignment",
        &[
            ("finder", "rounded:2.5/circle"),
            ("alignment", "rounded:2.5/circle"),
            ("token", "alignment=#8a3b12"),
            ("token", "finder.outer=#8a3b12"),
        ],
    ),
    (
        "Outlined alignment",
        &[("emphasis", "outline"), ("token", "alignment=#1f4e79")],
    ),
    (
        "Logo",
        &[
            ("logo", "sample"),
            ("logo-shape", "rounded:1.5"),
            ("logo-scale", "0.22"),
            ("shape", "rounded:0.5"),
            ("finder", "rounded:2/rounded:1"),
        ],
    ),
    (
        "Settle intro",
        &[
            ("intro", "settle"),
            ("shape", "dots:0.85"),
            ("finder", "rounded:2/circle"),
        ],
    ),
    (
        "Ripple intro",
        &[
            ("intro", "ripple"),
            ("shape", "liquid:0.8"),
            ("finder", "rounded:2/rounded:1"),
        ],
    ),
    (
        "Lock-on intro",
        &[("intro", "lock-on"), ("shape", "rounded:0.4")],
    ),
    (
        "Iris intro",
        &[
            ("intro", "iris"),
            ("shape", "dots:0.85"),
            ("frame", "circle:0.5"),
            ("frame-ring", "solid"),
            ("quiet-zone", "2"),
        ],
    ),
    (
        "Palette shift",
        &[
            ("from-bg", "#ffffff"),
            ("from-fg", "#7a1f10"),
            ("bg", "oklch(96% .02 85)"),
            ("fg", "oklch(30% .12 265)"),
        ],
    ),
    (
        "Finder pulse",
        &[
            ("intro", "lock-on"),
            ("idle", "finder-pulse"),
            ("finder", "rounded:2/circle"),
        ],
    ),
    (
        "Shimmer idle",
        &[
            ("idle", "shimmer"),
            ("shape", "rounded:0.5"),
            ("finder", "rounded:2/rounded:1"),
            ("fg", "oklch(30% .1 250)"),
        ],
    ),
    (
        "Breathe idle",
        &[
            ("idle", "breathe"),
            ("shape", "dots:0.85"),
            ("finder", "squircle/circle"),
            ("bg", "oklch(88% .06 160)"),
            ("fg", "oklch(28% .06 200)"),
        ],
    ),
    (
        "Every idle loop",
        &[
            ("intro", "ripple"),
            ("idle", "finder-pulse"),
            ("idle", "shimmer"),
            ("idle", "breathe"),
            ("shape", "liquid:0.8"),
            ("finder", "rounded:2/circle"),
            ("bg", "oklch(94% .03 85)"),
        ],
    ),
    (
        "Site palette",
        &[
            ("palette-from-css", "sample"),
            ("shape", "rounded:0.4"),
            ("finder", "rounded:2/circle"),
        ],
    ),
    (
        "Halftone",
        &[("halftone", "sample"), ("tracking-grid", "sparse")],
    ),
    (
        "Art mode",
        &[
            ("art", "sample"),
            ("ink", "color:6"),
            ("token", "finder.eye=#c2410c"),
        ],
    ),
    (
        "Develop intro",
        &[
            ("intro", "develop"),
            ("halftone", "sample"),
            ("tracking-grid", "sparse"),
        ],
    ),
    (
        "Mixed corners",
        &[
            ("corner", "top-right=leaf/circle"),
            ("token", "finder.top-left.eye=#b03a2e"),
        ],
    ),
];

/// Logo sizes, frames, and the logo's own intros and idles. Kept apart so
/// the scan sheet adds them after its older cards, whose ids stay put.
pub(crate) const LOGO_FAMILIES: &[(&str, &[(&str, &str)])] = &[
    (
        "Small logo",
        &[
            ("logo", "sample"),
            ("logo-size", "small"),
            ("shape", "dots:0.85"),
            ("finder", "rounded:2/circle"),
        ],
    ),
    (
        "Largest logo",
        &[
            ("logo", "sample"),
            ("logo-size", "largest"),
            ("logo-shape", "rounded:1.5"),
            ("shape", "rounded:0.5"),
            ("finder", "rounded:2/rounded:1"),
        ],
    ),
    (
        "Ring frame",
        &[
            ("logo", "sample"),
            ("logo-shape", "circle"),
            ("logo-frame", "ring"),
            ("token", "logo.frame=#c2410c"),
            ("shape", "dots:0.85"),
            ("finder", "squircle/circle"),
        ],
    ),
    (
        "Beads frame",
        &[
            ("logo", "sample"),
            ("logo-shape", "circle"),
            ("logo-frame", "beads"),
            ("shape", "dots:0.9"),
            ("finder", "rounded:2/circle"),
        ],
    ),
    (
        "Stamp frame",
        &[
            ("logo", "sample"),
            ("logo-shape", "circle"),
            ("logo-frame", "stamp"),
            ("shape", "rounded:0.4"),
            ("bg", "oklch(96% .02 85)"),
            ("fg", "oklch(28% .05 265)"),
            ("token", "logo.frame=#8a3b12"),
        ],
    ),
    (
        "Brackets frame",
        &[
            ("logo", "sample"),
            ("logo-frame", "brackets"),
            ("logo-shape", "rounded:1"),
        ],
    ),
    (
        "Orbit frame",
        &[
            ("logo", "sample"),
            ("logo-shape", "circle"),
            ("logo-frame", "orbit"),
            ("shape", "liquid:0.8"),
            ("finder", "rounded:2/rounded:1"),
        ],
    ),
    (
        "Stamp logo intro",
        &[
            ("intro", "settle"),
            ("logo", "sample"),
            ("logo-shape", "circle"),
            ("logo-frame", "stamp"),
            ("logo-intro", "stamp"),
            ("shape", "dots:0.85"),
        ],
    ),
    (
        "Draw logo intro",
        &[
            ("intro", "ripple"),
            ("logo", "sample"),
            ("logo-shape", "circle"),
            ("logo-frame", "ring"),
            ("logo-intro", "draw"),
            ("shape", "liquid:0.8"),
            ("finder", "rounded:2/rounded:1"),
        ],
    ),
    (
        "Lock logo intro",
        &[
            ("intro", "lock-on"),
            ("logo", "sample"),
            ("logo-frame", "brackets"),
            ("logo-intro", "lock"),
            ("shape", "rounded:0.4"),
        ],
    ),
    (
        "Ink logo intro",
        &[
            ("intro", "settle"),
            ("logo", "sample"),
            ("logo-shape", "rounded:1.5"),
            ("logo-frame", "ring"),
            ("logo-intro", "ink"),
            ("shape", "dots:0.85"),
            ("finder", "rounded:2/circle"),
        ],
    ),
    (
        "Logo first",
        &[
            ("intro", "assemble"),
            ("logo", "sample"),
            ("logo-shape", "circle"),
            ("logo-frame", "orbit"),
            ("logo-intro", "stamp"),
            ("logo-timing", "before"),
            ("shape", "rounded:0.5"),
        ],
    ),
    (
        "Logo glint",
        &[
            ("intro", "settle"),
            ("idle", "logo-glint"),
            ("logo", "sample"),
            ("logo-shape", "rounded:1.5"),
            ("logo-frame", "ring"),
            ("shape", "rounded:0.5"),
            ("finder", "rounded:2/rounded:1"),
        ],
    ),
    (
        "Logo turn",
        &[
            ("intro", "iris"),
            ("idle", "logo-turn"),
            ("logo", "sample"),
            ("logo-shape", "circle"),
            ("logo-frame", "orbit"),
            ("shape", "dots:0.85"),
            ("finder", "squircle/circle"),
        ],
    ),
    (
        "Thick off-color ring",
        &[
            ("logo", "sample"),
            ("logo-shape", "circle"),
            ("logo-frame", "ring:1.5"),
            ("logo-frame-color", "oklch(52% .09 190)"),
            ("shape", "dots:0.85"),
            ("finder", "rounded:2/circle"),
        ],
    ),
    (
        "Palette frame",
        &[
            ("palette-from-css", "sample"),
            ("logo", "sample"),
            ("logo-shape", "circle"),
            ("logo-frame", "stamp"),
            ("shape", "rounded:0.4"),
        ],
    ),
    (
        "Shrink logo intro",
        &[
            ("intro", "settle"),
            ("logo", "sample"),
            ("logo-shape", "rounded:1.5"),
            ("logo-frame", "ring"),
            ("logo-intro", "shrink"),
            ("shape", "rounded:0.5"),
            ("finder", "rounded:2/rounded:1"),
        ],
    ),
    (
        "Logo spin",
        &[
            ("intro", "settle"),
            ("idle", "logo-spin"),
            ("logo", "sample"),
            ("logo-shape", "circle"),
            ("logo-frame", "brackets"),
            ("shape", "dots:0.85"),
        ],
    ),
    (
        "Logo flip",
        &[
            ("intro", "ripple"),
            ("idle", "logo-flip"),
            ("logo", "sample"),
            ("logo-shape", "rounded:1.5"),
            ("logo-frame", "ring"),
            ("shape", "liquid:0.8"),
            ("finder", "rounded:2/rounded:1"),
        ],
    ),
];

/// Pictures with everything else: art mode, halftones, palettes and
/// gradients taken from a picture, logos and motion. `{a}` and `{b}` stand
/// for the pictures given with `--gallery-picture` (`sample` without).
/// Pictures carry clarity 0.5 and, unless a card shows a grid, the sparse
/// grid: measured (W20261026115722) under blur and noise at fixed image
/// sizes, art codes at clarity 0 on the standard grid decoded only at the
/// largest size tried.
pub(crate) const PICTURE_FAMILIES: &[(&str, &[(&str, &str)])] = &[
    (
        "Art, mono ink",
        &[
            ("art", "{a}"),
            ("clarity", "0.5"),
            ("tracking-grid", "sparse"),
            ("ink", "mono"),
        ],
    ),
    (
        "Art, color ink",
        &[
            ("art", "{a}"),
            ("clarity", "0.5"),
            ("tracking-grid", "sparse"),
            ("ink", "color:6"),
        ],
    ),
    (
        "Art, picture palette",
        &[
            ("art", "{b}"),
            ("clarity", "0.5"),
            ("tracking-grid", "sparse"),
            ("ink", "color:6"),
            ("palette-from-image", "{b}"),
        ],
    ),
    (
        "Art, dense grid",
        &[
            ("art", "{b}"),
            ("clarity", "0.75"),
            ("ink", "mono"),
            ("palette-from-image", "{b}"),
            ("tracking-grid", "dense"),
        ],
    ),
    (
        "Halftone dots",
        &[
            ("halftone", "{a}"),
            ("clarity", "0.5"),
            ("tracking-grid", "sparse"),
            ("ink", "color:6"),
            ("shape", "dots:0.85"),
            ("finder", "rounded:2/circle"),
        ],
    ),
    (
        "Halftone, clearer",
        &[
            ("halftone", "{b}"),
            ("tracking-grid", "sparse"),
            ("ink", "color:6"),
            ("clarity", "0.6"),
            ("shape", "rounded:0.4"),
        ],
    ),
    (
        "Picture palette",
        &[
            ("intro", "ripple"),
            ("palette-from-image", "{b}"),
            ("shape", "liquid:0.8"),
            ("finder", "rounded:2/rounded:1"),
        ],
    ),
    (
        "Picture palette, gradient ink",
        &[
            ("intro", "iris"),
            ("palette-from-image", "{a}"),
            (
                "fg",
                "linear-gradient(135deg, oklch(30% .06 265), oklch(26% .09 305))",
            ),
            ("shape", "dots:0.85"),
            ("finder", "squircle/circle"),
        ],
    ),
    (
        "Radial gradient, breathing",
        &[
            ("idle", "breathe"),
            ("palette-from-image", "{b}"),
            (
                "fg",
                "radial-gradient(oklch(34% .1 230), oklch(24% .05 266))",
            ),
            ("shape", "rounded:0.5"),
            ("finder", "rounded:2/circle"),
        ],
    ),
    (
        "Gradient eyes, logo glint",
        &[
            ("intro", "lock-on"),
            ("idle", "logo-glint"),
            ("palette-from-image", "{a}"),
            (
                "token",
                "finder.eye=linear-gradient(135deg, oklch(42% .14 60), oklch(34% .12 30))",
            ),
            ("logo", "sample"),
            ("logo-shape", "circle"),
            ("logo-frame", "beads"),
            ("shape", "dots:0.85"),
        ],
    ),
    (
        "Art with a framed logo",
        &[
            ("art", "{a}"),
            ("clarity", "0.5"),
            ("tracking-grid", "sparse"),
            ("ink", "color:6"),
            ("logo", "sample"),
            ("logo-scale", "0.12"),
            ("logo-shape", "circle"),
            ("logo-frame", "ring"),
        ],
    ),
    (
        "Art, orbit turning",
        &[
            ("intro", "develop"),
            ("idle", "logo-turn"),
            ("art", "{b}"),
            ("clarity", "0.5"),
            ("tracking-grid", "sparse"),
            ("ink", "color:6"),
            ("palette-from-image", "{b}"),
            ("logo", "sample"),
            ("logo-scale", "0.12"),
            ("logo-shape", "circle"),
            ("logo-frame", "orbit"),
        ],
    ),
    (
        "Develop, logo shrinks",
        &[
            ("intro", "develop"),
            ("art", "{a}"),
            ("clarity", "0.5"),
            ("tracking-grid", "sparse"),
            ("ink", "mono"),
            ("logo", "sample"),
            ("logo-scale", "0.12"),
            ("logo-shape", "rounded:1.5"),
            ("logo-frame", "ring"),
            ("logo-intro", "shrink"),
        ],
    ),
    (
        "Halftone, logo spin",
        &[
            ("intro", "settle"),
            ("idle", "logo-spin"),
            ("halftone", "{b}"),
            ("clarity", "0.5"),
            ("tracking-grid", "sparse"),
            ("ink", "color:6"),
            ("palette-from-image", "{b}"),
            ("logo", "sample"),
            ("logo-scale", "0.12"),
            ("logo-shape", "circle"),
            ("logo-frame", "brackets"),
        ],
    ),
    (
        "Halftone, flip and shimmer",
        &[
            ("intro", "sweep"),
            ("idle", "logo-flip"),
            ("idle", "shimmer"),
            ("halftone", "{a}"),
            ("clarity", "0.5"),
            ("tracking-grid", "sparse"),
            ("ink", "mono"),
            ("logo", "sample"),
            ("logo-scale", "0.12"),
            ("logo-shape", "rounded:1.5"),
            ("logo-frame", "ring"),
        ],
    ),
    (
        "Picture palette, ink logo",
        &[
            ("intro", "settle"),
            ("palette-from-image", "{b}"),
            ("logo", "sample"),
            ("logo-shape", "circle"),
            ("logo-frame", "stamp"),
            ("logo-intro", "ink"),
            ("shape", "dots:0.85"),
            ("finder", "rounded:2/circle"),
        ],
    ),
    (
        "Picture palette, thick ring draws",
        &[
            ("intro", "ripple"),
            ("palette-from-image", "{b}"),
            ("logo", "sample"),
            ("logo-shape", "circle"),
            ("logo-frame", "ring:1.4"),
            ("logo-intro", "draw"),
            ("shape", "liquid:0.8"),
            ("finder", "rounded:2/rounded:1"),
        ],
    ),
    (
        "Art, every logo loop",
        &[
            ("intro", "iris"),
            ("idle", "logo-turn"),
            ("idle", "logo-glint"),
            ("idle", "logo-spin"),
            ("art", "{b}"),
            ("clarity", "0.5"),
            ("tracking-grid", "sparse"),
            ("ink", "color:6"),
            ("logo", "sample"),
            ("logo-scale", "0.12"),
            ("logo-shape", "circle"),
            ("logo-frame", "orbit"),
        ],
    ),
];

/// A family's flag value with `{a}` and `{b}` replaced by the pictures:
/// `{b}` is the second, or the first again, and both are `sample` without.
fn picture_value(value: &str, pictures: &[String]) -> String {
    let a = pictures.first().map_or("sample", String::as_str);
    let b = pictures.get(1).map_or(a, String::as_str);
    value.replace("{a}", a).replace("{b}", b)
}

pub(crate) fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Quotes a flag value for a shell when it needs it.
pub(crate) fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "-_.:/=#".contains(c))
    {
        value.to_string()
    } else {
        format!("'{value}'")
    }
}

pub fn page(
    text: &str,
    builder: &QrBuilder,
    pinned: bool,
    base: &StyleArgs,
    pictures: &[String],
) -> String {
    let mut cards = String::new();
    let families = FAMILIES.iter().chain(LOGO_FAMILIES).chain(PICTURE_FAMILIES);
    for (name, flags) in families {
        let mut args = base.clone();
        let mut command = format!(" {}", shell_quote(text));
        let mut problem = None;
        for (flag, value) in *flags {
            let value = &picture_value(value, pictures);
            if let Err(e) = args.set(flag, value) {
                problem = Some(e);
            }
            let _ = write!(command, " --{flag} {}", shell_quote(value));
        }
        let rendered = problem.map_or_else(
            || {
                let style = args.build(text)?;
                // Each family gets the smallest code its style can afford.
                let qr = args
                    .encoder(*builder, pinned)
                    .encode_text_for(text, &style)
                    .map_err(|e| e.to_string())?;
                let qr = &qr;
                let svg = style.render(qr).map_err(|e| e.to_string())?;
                let size = format!("version {}, ECC {:?}", qr.version().number(), qr.ecc());
                let verdict = match crate::verify(&style, qr, text) {
                    Ok(v) => v,
                    Err(e) if cfg!(feature = "verify") => format!("does not decode: {e}"),
                    Err(_) => "not verified".to_string(),
                };
                let warnings: Vec<String> = style
                    .check(qr)
                    .warnings
                    .iter()
                    .map(|w| w.to_string())
                    .collect();
                Ok((svg, format!("{verdict} · {size}"), warnings))
            },
            Err,
        );
        let _ = write!(cards, "<figure><div class=\"code\">");
        match &rendered {
            Ok((svg, _, _)) => cards.push_str(svg),
            Err(e) => {
                let _ = write!(cards, "<p class=\"refused\">Refused: {}</p>", escape(e));
            }
        }
        let _ = write!(
            cards,
            "</div><figcaption><h2>{}</h2><code>freeqrent{}</code>",
            escape(name),
            escape(&command)
        );
        if let Ok((_, verdict, warnings)) = &rendered {
            let _ = write!(cards, "<p class=\"verdict\">{}</p>", escape(verdict));
            for w in warnings {
                let _ = write!(cards, "<p class=\"warning\">{}</p>", escape(w));
            }
        }
        cards.push_str("</figcaption></figure>\n");
    }

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>freeQRent styles</title>
<style>
:root {{
  --page: oklch(97.5% .006 85);
  --ink: oklch(24% .01 265);
  --muted: oklch(48% .01 265);
  --rule: oklch(88% .008 85);
  --warn: oklch(48% .12 60);
  --bad: oklch(48% .16 28);
}}
@media (prefers-color-scheme: dark) {{
  :root {{
    --page: oklch(20% .008 265);
    --ink: oklch(93% .006 85);
    --muted: oklch(70% .01 265);
    --rule: oklch(32% .01 265);
    --warn: oklch(78% .12 70);
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
header {{ max-width: 70ch; margin-bottom: 40px; }}
h1 {{ font-size: 1.9rem; line-height: 1.2; margin: 0 0 8px; font-weight: 650; }}
header p {{ margin: 0; color: var(--muted); overflow-wrap: anywhere; }}
main {{
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(min(100%, 240px), 1fr));
  gap: 40px 28px;
}}
figure {{ margin: 0; }}
.code {{ position: relative; }}
.code svg {{ display: block; width: 100%; height: auto; border-radius: 6px; }}
.refused {{ aspect-ratio: 1; margin: 0; display: grid; place-items: center; text-align: center;
  padding: 16px; border: 1px dashed var(--rule); color: var(--bad); border-radius: 6px; }}
figcaption {{ padding-top: 12px; border-top: 1px solid var(--rule); margin-top: 12px; }}
h2 {{ font-size: 1rem; margin: 0 0 4px; font-weight: 600;
  display: flex; align-items: center; justify-content: space-between; gap: 8px; }}
code {{ display: block; font: 12px/1.45 ui-monospace, SFMono-Regular, Menlo, monospace;
  color: var(--muted); overflow-wrap: anywhere; }}
.code.waiting svg * {{ animation-play-state: paused !important; }}
.replay {{ flex: none; width: 28px; height: 28px; padding: 0;
  display: grid; place-items: center; border: 1px solid var(--rule); border-radius: 50%;
  background: var(--page); color: var(--ink); cursor: pointer; opacity: .75; }}
.replay:hover, .replay:focus-visible {{ opacity: 1; }}
.replay svg {{ width: 14px; height: 14px; }}
@media (prefers-reduced-motion: reduce) {{ .replay {{ display: none; }} }}
.verdict, .warning {{ margin: 6px 0 0; font-size: 13px; }}
.warning {{ color: var(--warn); }}
</style>
</head>
<body>
<header>
<h1>Style families</h1>
<p>{payload}</p>
</header>
<main>
{cards}</main>
<script>
// Animated codes hold their first frame until they scroll into view, and
// a button beside the title replays them (off the code, so it never covers
// modules a scanner needs). A fresh copy of the SVG restarts its
// animations.
const icon = '<svg viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" ' +
  'stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">' +
  '<path d="M13.5 8a5.5 5.5 0 1 1-1.6-3.9"/><path d="M12.5 1.5v3h-3"/></svg>';
const replay = (box) => {{
  const svg = box.querySelector("svg.qr");
  svg.replaceWith(svg.cloneNode(true));
  box.classList.remove("waiting");
}};
const seen = new IntersectionObserver((entries) => {{
  for (const entry of entries) {{
    if (!entry.isIntersecting) continue;
    seen.unobserve(entry.target);
    replay(entry.target);
  }}
}}, {{ threshold: 0.5 }});
for (const box of document.querySelectorAll(".code")) {{
  if (!box.querySelector("svg.qr style")?.textContent.includes("@keyframes")) continue;
  box.classList.add("waiting");
  seen.observe(box);
  const button = document.createElement("button");
  button.type = "button";
  button.className = "replay";
  button.title = button.ariaLabel = "Replay animation";
  button.innerHTML = icon;
  button.addEventListener("click", () => replay(box));
  box.closest("figure").querySelector("h2").append(button);
}}
</script>
</body>
</html>
"#,
        payload = escape(text),
    )
}
