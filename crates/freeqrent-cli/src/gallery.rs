//! `--gallery`: one HTML page showing the payload in every style family,
//! each with the flags that reproduce it and whether it decodes.

use std::fmt::Write;

use freeqrent::{QrBuilder, Warning};

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
        "Logo shrinks, then its ring draws",
        &[
            ("intro", "settle"),
            ("logo", "sample"),
            ("logo-shape", "rounded:1.5"),
            ("logo-frame", "ring"),
            ("logo-intro", "shrink"),
            ("logo-frame-intro", "draw"),
            ("logo-frame-timing", "after"),
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
    (
        "Spin logo intro",
        &[
            ("intro", "ripple"),
            ("logo", "sample"),
            ("logo-shape", "rounded:1.5"),
            ("logo-frame", "brackets"),
            ("logo-size", "large"),
            ("logo-intro", "spin"),
            ("shape", "rounded:0.4"),
        ],
    ),
];

/// Dark palettes (SPEC §6.4): a dark-first site keeping its polarity, read
/// inverted. After the logos, so the scan sheet's earlier ids keep their
/// cards.
pub(crate) const DARK_FAMILIES: &[(&str, &[(&str, &str)])] = &[(
    "Dark site palette",
    &[
        ("palette-from-css", "sample-dark"),
        ("allow-inverted", ""),
        ("shape", "rounded:0.4"),
        ("finder", "rounded:2/circle"),
    ],
)];

/// One card per design system in the owner's design library, built from
/// each one's palette and signature shapes. Dark designs keep their
/// polarity, read inverted. An accent too close to `bg` in lightness for
/// modules (Bauhaus yellow, Swiss red, electric blue) keeps its hue and
/// chroma at a lightness that clears the 0.60 warning band, or goes where
/// contrast isn't checked: a border, a shadow, a caption, a round frame's
/// filler and ring (W20261026224505).
pub(crate) const DESIGN_FAMILIES: &[(&str, &[(&str, &str)])] = &[
    (
        "Academia",
        &[
            ("allow-inverted", ""),
            ("bg", "#1c1714"),
            ("fg", "#e8dfd4"),
            ("token", "finder.eye=#d9bf7f"),
            ("finder", "square/rounded:0.5"),
            ("shape", "square:0.08"),
            ("frame", "rounded:1"),
            ("border", "double:1"),
            ("token", "frame=#c9a962"),
            ("texture", "grain:0.06"),
        ],
    ),
    (
        "American agriculture",
        &[
            ("bg", "#fbf6ea"),
            ("fg", "oklch(36% .08 135)"),
            ("token", "finder.eye=oklch(36% .07 65)"),
            ("shape", "rounded:0.3"),
            ("finder", "rounded:1.5/rounded:0.5"),
            ("frame", "rounded:2"),
            ("border", "solid:0.5"),
            ("token", "frame=#e6b85f"),
            ("caption", "Fresh from the farm"),
            ("caption-size", "1.5"),
            ("token", "caption=#79562a"),
        ],
    ),
    (
        "Art deco",
        &[
            ("allow-inverted", ""),
            ("bg", "#0a0a0a"),
            (
                "fg",
                "linear-gradient(135deg, #e6c65c, #d4af37 50%, #e6c65c)",
            ),
            ("token", "finder.eye=#f2f0e4"),
            ("token", "frame=#d4af37"),
            ("token", "filler=#1e3d59"),
            ("shape", "square:0.1"),
            ("frame", "circle:0.5"),
            ("frame-ring", "dashed"),
            ("border", "double:1"),
            ("effect", "glow:0.3:0.2"),
        ],
    ),
    (
        "Aurora mesh",
        &[
            ("allow-inverted", ""),
            ("bg", "radial-gradient(#1a0f33, #09090b)"),
            (
                "fg",
                "linear-gradient(135deg, #c4b5fd, #f0abfc 50%, #67e8f9)",
            ),
            ("shape", "liquid:0.9"),
            ("finder", "squircle/circle"),
            ("effect", "glow:0.4"),
            ("idle", "shimmer"),
            ("frame", "rounded:3"),
        ],
    ),
    (
        "Bauhaus",
        &[
            ("bg", "#f0f0f0"),
            ("fg", "#121212"),
            ("token", "fg.2=oklch(35% .16 27)"),
            ("token", "fg.3=oklch(35% .18 265)"),
            ("finder", "square/circle"),
            ("corner", "top-right=squircle/square"),
            ("token", "finder.top-left.eye=oklch(35% .16 27)"),
            ("token", "finder.top-right.eye=oklch(35% .18 265)"),
            ("shape", "dots:0.95"),
            ("border", "solid:1"),
            ("token", "frame=#121212"),
            ("shadow", "1.5"),
            ("token", "shadow=#f0c020"),
        ],
    ),
    (
        "Bold typography",
        &[
            ("allow-inverted", ""),
            ("bg", "#0a0a0a"),
            ("fg", "#fafafa"),
            ("token", "finder.eye=#ff9a70"),
            ("shape", "bars-h:0"),
            ("caption", "SCAN"),
            ("caption-size", "3"),
            ("caption-tracking", "20"),
            ("token", "caption=#ff3d00"),
        ],
    ),
    (
        "Botanical",
        &[
            ("bg", "#f9f8f4"),
            ("fg", "#2d3a31"),
            ("token", "finder.eye=oklch(36% .09 38)"),
            ("shape", "liquid:0.7"),
            ("finder", "leaf/leaf"),
            ("frame", "rounded:4"),
            ("texture", "grain:0.05"),
            ("shadow", "1"),
            ("token", "shadow=#dccfc2"),
        ],
    ),
    (
        "Cinematic editorial",
        &[
            ("bg", "#f2f2f2"),
            ("fg", "#333333"),
            ("token", "finder.eye=oklch(34% .14 29)"),
            ("shape", "square"),
            ("border", "solid:0.5"),
            ("token", "frame=#d10000"),
            ("caption", "NOW SHOWING"),
            ("caption-size", "1.5"),
            ("caption-tracking", "25"),
            ("token", "caption=#d10000"),
        ],
    ),
    (
        "Claymorphism",
        &[
            ("bg", "#f4f1fa"),
            (
                "fg",
                "linear-gradient(135deg, oklch(34% .17 293), oklch(34% .14 355))",
            ),
            ("shape", "liquid:1"),
            ("finder", "rounded:2.5/circle"),
            ("frame", "rounded:4"),
            ("effect", "emboss:0.12"),
            ("shadow", "1"),
            ("token", "shadow=#c4b5fd"),
        ],
    ),
    (
        "Cosmic observatory",
        &[
            ("allow-inverted", ""),
            ("bg", "radial-gradient(#251728, #0d0710)"),
            ("fg", "#f5ede6"),
            ("token", "finder.eye=#f7b98a"),
            ("token", "frame=#e8734a"),
            ("token", "filler=#9b4b6c"),
            ("shape", "dots:0.85"),
            ("finder", "rounded:2/circle"),
            ("frame", "circle:0.35"),
            ("frame-ring", "solid"),
            ("effect", "glow:0.3:0.2"),
            ("intro", "iris"),
        ],
    ),
    (
        "Cyberpunk",
        &[
            ("allow-inverted", ""),
            ("bg", "#0a0a0f"),
            ("fg", "#7dffb0"),
            ("token", "finder.eye=#ff66ff"),
            ("token", "finder.outer=#66e5ff"),
            ("shape", "square:0.12"),
            ("timing", "dashed"),
            ("effect", "glow:0.25"),
            ("texture", "scanlines:0.15"),
            ("intro", "lock-on"),
        ],
    ),
    (
        "Enterprise",
        &[
            ("bg", "linear-gradient(160deg, #f8fafc, #eef2ff)"),
            (
                "fg",
                "linear-gradient(135deg, oklch(35% .15 277), oklch(35% .17 293))",
            ),
            ("shape", "rounded:0.35"),
            ("finder", "rounded:2/rounded:1"),
            ("frame", "rounded:2"),
            ("border", "solid:0.5"),
            ("token", "frame=#c7d2fe"),
            ("shadow", "1"),
            ("token", "shadow=#c7d2fe"),
        ],
    ),
    (
        "Flat design",
        &[
            ("bg", "#ffffff"),
            ("fg", "#111827"),
            ("token", "fg.2=oklch(37% .14 262)"),
            ("token", "fg.3=oklch(37% .08 165)"),
            ("spread", "regions"),
            ("token", "finder.top-left.outer=oklch(37% .14 262)"),
            ("token", "finder.top-right.outer=oklch(37% .08 165)"),
            ("token", "finder.bottom-left.outer=oklch(37% .1 55)"),
            ("shape", "square"),
        ],
    ),
    (
        "Glassmorphism",
        &[
            ("allow-inverted", ""),
            ("bg", "linear-gradient(135deg, #172554, #3b0764)"),
            ("fg", "#f5f5f7"),
            ("shape", "dots:0.9"),
            ("finder", "rounded:2.5/circle"),
            ("frame", "rounded:4"),
            ("border", "solid:0.25"),
            ("token", "frame=#a5b4fc"),
            ("idle", "breathe"),
        ],
    ),
    (
        "Industrial",
        &[
            ("bg", "#e0e5ec"),
            ("fg", "#1f2426"),
            ("shape", "bars-v:0.5"),
            ("finder", "rounded:1/circle"),
            ("effect", "emboss:0.1"),
            ("logo", "sample"),
            ("logo-shape", "circle"),
            ("logo-frame", "ring"),
            ("logo-frame-color", "#ff4757"),
            ("frame", "rounded:1.5"),
        ],
    ),
    (
        "Kinetic",
        &[
            ("allow-inverted", ""),
            ("bg", "#09090b"),
            ("fg", "#fafafa"),
            ("token", "finder.eye=#dfe104"),
            ("shape", "bars-h:0"),
            ("caption", "SCAN NOW"),
            ("caption-size", "2.5"),
            ("token", "caption=#dfe104"),
            ("intro", "sweep"),
            ("stagger", "linear"),
            ("ease", "expo"),
        ],
    ),
    (
        "Luxury",
        &[
            ("bg", "#f9f8f6"),
            ("fg", "#1a1a1a"),
            ("shape", "square:0.2"),
            ("border", "double:1"),
            ("token", "frame=#d4af37"),
            ("caption", "THE MENU"),
            ("caption-font", "serif"),
            ("caption-regular", ""),
            ("caption-tracking", "30"),
            ("caption-size", "1.5"),
            ("token", "caption=#8c6d1f"),
            ("intro", "settle"),
            ("intro-ms", "2000"),
        ],
    ),
    (
        "Material",
        &[
            ("bg", "#f3edf7"),
            ("fg", "#381e72"),
            ("token", "finder.eye=#31111d"),
            ("shape", "bars-v:0.5"),
            ("finder", "rounded:2.5/rounded:1"),
            ("frame", "rounded:4"),
            ("caption", "Scan to open"),
            ("caption-size", "1.5"),
            ("token", "caption=#6750a4"),
        ],
    ),
    (
        "Maximalism",
        &[
            ("allow-inverted", ""),
            ("bg", "#0d0d1a"),
            ("fg", "#00f5d4"),
            ("token", "fg.2=#ffe600"),
            ("token", "fg.3=#ff77ff"),
            ("spread", "regions"),
            ("token", "finder.eye=#ff77ff"),
            ("token", "filler=#ff3af2"),
            ("token", "frame=#ff6b35"),
            ("shape", "liquid:1"),
            ("finder", "leaf/circle"),
            ("frame", "circle:0.7"),
            ("frame-ring", "dashed"),
            ("effect", "glow:0.3:0.2"),
            ("intro", "ripple"),
            ("stagger", "random"),
        ],
    ),
    (
        "Minimal dark",
        &[
            ("allow-inverted", ""),
            ("bg", "#0a0a0f"),
            ("fg", "#fafafa"),
            ("token", "finder.eye=#f59e0b"),
            ("shape", "rounded:0.4"),
            ("finder", "rounded:2/circle"),
            ("frame", "rounded:3"),
            ("effect", "glow:0.4:0.15"),
            ("idle", "breathe"),
        ],
    ),
    (
        "Modern dark",
        &[
            ("allow-inverted", ""),
            ("bg", "radial-gradient(#0a0b17, #050506)"),
            ("fg", "linear-gradient(160deg, #ededef, #a5acf0)"),
            ("token", "finder.eye=#a5acf0"),
            ("shape", "dots:0.85"),
            ("finder", "squircle/squircle"),
            ("effect", "glow:0.3:0.2"),
            ("idle", "shimmer"),
        ],
    ),
    (
        "Monochrome",
        &[
            ("bg", "#ffffff"),
            ("fg", "#000000"),
            ("shape", "dots:1"),
            ("finder", "square/square"),
            ("caption", "SCAN TO READ"),
            ("caption-font", "serif"),
            ("caption-tracking", "30"),
            ("caption-size", "1.5"),
        ],
    ),
    (
        "Neo-brutalism",
        &[
            ("bg", "#ffd93d"),
            ("fg", "#000000"),
            ("shape", "square"),
            ("timing", "dashed"),
            ("border", "solid:1"),
            ("shadow", "1.5"),
            ("caption", "SCAN ME"),
            ("caption-tracking", "10"),
        ],
    ),
    (
        "Neumorphism",
        &[
            ("bg", "#e0e5ec"),
            ("fg", "#2b333b"),
            ("shape", "dots:0.9"),
            ("finder", "rounded:2.5/circle"),
            ("frame", "rounded:4"),
            ("effect", "emboss:0.12"),
        ],
    ),
    (
        "Newsprint",
        &[
            ("bg", "#f9f9f7"),
            ("fg", "#111111"),
            ("token", "finder.eye=#7a0000"),
            ("shape", "dots:0.8"),
            ("timing", "dotted"),
            ("texture", "ink:0.2"),
            ("texture", "grain:0.06"),
            ("border", "double:0.6"),
            ("caption", "EXTRA! EXTRA!"),
            ("caption-font", "serif"),
            ("caption-size", "1.5"),
        ],
    ),
    (
        "Organic",
        &[
            ("bg", "#fdfcf8"),
            ("fg", "#2c2c24"),
            ("token", "finder.eye=#34402d"),
            ("token", "filler=#c18c5d"),
            ("token", "frame=#5d7052"),
            ("shape", "liquid:1"),
            ("finder", "leaf/leaf"),
            ("frame", "circle:0.3"),
            ("effect", "sketch:0.06"),
            ("texture", "grain:0.05"),
        ],
    ),
    (
        "Playful geometric",
        &[
            ("bg", "#fffdf5"),
            ("fg", "#1e293b"),
            ("token", "fg.2=oklch(36% .17 293)"),
            ("token", "fg.3=oklch(36% .13 355)"),
            ("token", "fg.4=oklch(36% .08 165)"),
            ("finder", "squircle/circle"),
            ("corner", "top-right=square/square"),
            ("corner", "bottom-left=leaf/leaf"),
            ("token", "finder.top-left.outer=oklch(36% .17 293)"),
            ("token", "finder.top-right.outer=oklch(36% .13 355)"),
            ("token", "finder.bottom-left.outer=oklch(36% .08 165)"),
            ("shape", "dots:0.9"),
            ("frame", "rounded:2"),
            ("border", "solid:0.5"),
            ("token", "frame=#1e293b"),
            ("shadow", "1"),
            ("token", "shadow=#f472b6"),
        ],
    ),
    (
        "Professional",
        &[
            ("bg", "#fafaf8"),
            ("fg", "#1a1a1a"),
            ("token", "finder.eye=oklch(37% .07 75)"),
            ("shape", "square:0.08"),
            ("frame", "rounded:1"),
            ("border", "double:0.6"),
            ("token", "frame=#b8860b"),
            ("caption", "Read the report"),
            ("caption-font", "serif"),
            ("caption-regular", ""),
            ("caption-size", "1.5"),
        ],
    ),
    (
        "Retro",
        &[
            ("bg", "#ffffcc"),
            ("fg", "#000000"),
            ("token", "fg.2=#000080"),
            ("token", "fg.3=#7a0000"),
            ("token", "finder.eye=#000080"),
            ("shape", "square"),
            ("border", "solid:0.5"),
            ("shadow", "1"),
            ("token", "shadow=#808080"),
            ("intro", "raster"),
        ],
    ),
    (
        "SaaS",
        &[
            ("bg", "linear-gradient(180deg, #fafafa, #eef2ff)"),
            ("fg", "#0f172a"),
            (
                "token",
                "finder.eye=linear-gradient(135deg, oklch(33% .2 264), oklch(35% .19 264))",
            ),
            ("shape", "rounded:0.4"),
            ("finder", "rounded:2/rounded:1"),
            ("frame", "rounded:2"),
            ("caption", "Get started →"),
            ("caption-size", "1.5"),
            ("token", "caption=#0052ff"),
        ],
    ),
    (
        "Sketch",
        &[
            ("bg", "#fdfbf7"),
            ("fg", "#2d2d2d"),
            ("token", "finder.eye=#1f4275"),
            ("shape", "rounded:0.3"),
            ("finder", "leaf/rounded:0.5"),
            ("effect", "sketch:0.12"),
            ("texture", "grain:0.04"),
        ],
    ),
    (
        "Swiss minimalist",
        &[
            ("bg", "#ffffff"),
            ("fg", "#000000"),
            ("token", "finder.top-left.eye=oklch(38% .15 33)"),
            ("shape", "square:0.1"),
            ("border", "solid:0.5"),
            ("token", "frame=#ff3000"),
            ("caption", "SCAN"),
            ("caption-tracking", "10"),
            ("token", "caption=#ff3000"),
        ],
    ),
    (
        "Terminal",
        &[
            ("preset", "terminal"),
            ("token", "finder.eye=#ffb000"),
            ("effect", "glow:0.25"),
            ("texture", "scanlines:0.2"),
            ("caption", "$ open menu_"),
            ("caption-font", "mono"),
            ("caption-size", "1.5"),
            ("token", "caption=#ffb000"),
            ("intro", "raster"),
        ],
    ),
    (
        "Vaporwave",
        &[
            ("allow-inverted", ""),
            ("bg", "linear-gradient(180deg, #120428, #090014)"),
            (
                "fg",
                "linear-gradient(180deg, #ffaa33, #ff77ff 50%, #66ffff)",
            ),
            ("token", "filler=#2d1b4e"),
            ("token", "frame=#ff00ff"),
            ("shape", "square:0.15"),
            ("frame", "circle:0.6"),
            ("frame-ring", "solid"),
            ("effect", "glow:0.4"),
            ("texture", "scanlines:0.15"),
            ("intro", "sweep"),
        ],
    ),
    (
        "Web3",
        &[
            ("allow-inverted", ""),
            ("bg", "#030304"),
            ("fg", "linear-gradient(135deg, #ffa940, #ffd600)"),
            ("token", "finder.outer=#f7931a"),
            ("shape", "square:0.1"),
            ("finder", "rounded:1/square"),
            ("effect", "glow:0.3"),
            ("border", "solid:0.25"),
            ("token", "frame=#f7931a"),
            ("idle", "shimmer"),
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
        "Art, assembling",
        &[
            ("intro", "assemble"),
            ("art", "{a}"),
            ("clarity", "0.5"),
            ("tracking-grid", "sparse"),
            ("ink", "color:6"),
        ],
    ),
    (
        "Art, rippling in",
        &[
            ("intro", "ripple"),
            ("art", "{b}"),
            ("clarity", "0.5"),
            ("tracking-grid", "sparse"),
            ("ink", "color:6"),
            ("palette-from-image", "{b}"),
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
            // A halftone's logo stays off the alignment patterns, and every
            // sparse-grid version has one in the middle (SPEC §7.3).
            ("tracking-grid", "standard"),
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
            // A halftone's logo stays off the alignment patterns, and every
            // sparse-grid version has one in the middle (SPEC §7.3).
            ("tracking-grid", "standard"),
            ("ink", "mono"),
            ("logo", "sample"),
            ("logo-scale", "0.12"),
            ("logo-shape", "rounded:1.5"),
            ("logo-frame", "ring"),
        ],
    ),
    (
        "Picture palette, ink logo, frame spins in",
        &[
            ("intro", "settle"),
            ("palette-from-image", "{b}"),
            ("logo", "sample"),
            ("logo-shape", "circle"),
            ("logo-frame", "stamp"),
            ("logo-intro", "ink"),
            ("logo-frame-intro", "spin"),
            ("logo-frame-timing", "with"),
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
    (
        "Dark picture palette",
        &[
            ("palette-from-image", "{b}"),
            ("allow-inverted", ""),
            ("shape", "dots"),
            ("finder", "rounded:2/circle"),
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

/// One card's code, command and verdict, as is or as its dark twin.
#[allow(clippy::too_many_arguments)]
fn variant(
    cards: &mut String,
    name: &str,
    flags: &[(&str, &str)],
    dark: bool,
    text: &str,
    builder: &QrBuilder,
    pinned: bool,
    base: &StyleArgs,
    pictures: &[String],
) {
    let mut args = base.clone();
    let mut command = format!(" {}", shell_quote(text));
    let mut problem = None;
    let mut builder = *builder;
    let dark_flag = [("dark", "")];
    let extra: &[(&str, &str)] = if dark { &dark_flag } else { &[] };
    for (flag, value) in flags.iter().chain(extra) {
        let value = &picture_value(value, pictures);
        if *flag == "ecc" {
            match crate::options::ecc(value) {
                Ok(ecc) => builder = builder.ecc(ecc),
                Err(e) => problem = Some(e),
            }
            let _ = write!(command, " --ecc {value}");
            continue;
        }
        if value.is_empty() && args.set_switch(flag) {
            let _ = write!(command, " --{flag}");
            continue;
        }
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
                .encoder(builder, pinned)
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
            // One line for inverted colors, not one per token.
            let mut warnings = Vec::new();
            let mut inverted = false;
            for w in &style.check(qr).warnings {
                if matches!(w, Warning::Inverted { .. }) {
                    if !inverted {
                        warnings.push(
                            "light on dark: some scanners can't read inverted codes".to_string(),
                        );
                    }
                    inverted = true;
                } else {
                    warnings.push(w.to_string());
                }
            }
            Ok((svg, format!("{verdict} · {size}"), warnings))
        },
        Err,
    );
    let _ = write!(cards, "<div class=\"code\">");
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
        let failed = if verdict.starts_with("does not decode") {
            " failed"
        } else {
            ""
        };
        let _ = write!(
            cards,
            "<p class=\"verdict{failed}\">{}</p>",
            escape(verdict)
        );
        for w in warnings {
            let _ = write!(cards, "<p class=\"warning\">{}</p>", escape(w));
        }
    }
    cards.push_str("</figcaption>");
}

pub fn page(
    text: &str,
    builder: &QrBuilder,
    pinned: bool,
    base: &StyleArgs,
    pictures: &[String],
) -> String {
    let mut cards = String::new();
    // The presets lead, each as it ships.
    let presets: Vec<(String, [(&str, &str); 1])> = freeqrent::Style::presets()
        .iter()
        .map(|&name| {
            let mut title = name.to_string();
            title[..1].make_ascii_uppercase();
            (format!("{title} preset"), [("preset", name)])
        })
        .collect();
    let presets: Vec<_> = presets
        .iter()
        .map(|(name, flags)| (name.as_str(), flags.as_slice()))
        .collect();
    let families: Vec<_> = FAMILIES
        .iter()
        .chain(LOGO_FAMILIES)
        .chain(DARK_FAMILIES)
        .copied()
        .collect();
    let sections = [
        ("Presets", presets.as_slice()),
        ("Style families", families.as_slice()),
        ("Design systems", DESIGN_FAMILIES),
        ("Pictures", PICTURE_FAMILIES),
    ];
    for (section, family) in sections.iter().flat_map(|(title, families)| {
        families
            .iter()
            .enumerate()
            .map(move |(i, family)| ((i == 0).then_some(*title), family))
    }) {
        if let Some(title) = section {
            let _ = writeln!(cards, "<h2 class=\"section\">{}</h2>", escape(title));
        }
        let (name, flags) = *family;
        // Every card also has its dark twin (`--dark`).
        cards.push_str("<figure><div class=\"variant light\">");
        variant(
            &mut cards, name, flags, false, text, builder, pinned, base, pictures,
        );
        cards.push_str("</div><div class=\"variant dark\">");
        variant(
            &mut cards, name, flags, true, text, builder, pinned, base, pictures,
        );
        cards.push_str("</div></figure>\n");
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
  --page: oklch(97.5% .012 125);
  --ink: oklch(23% .025 140);
  --muted: oklch(45% .025 140);
  --rule: oklch(89% .03 125);
  --fluor: oklch(92% .2 122);
  --on-fluor: oklch(22% .04 135);
  --pink: oklch(63% .24 356);
  --ok: oklch(74% .21 130);
  --warn: oklch(76% .16 75);
  --bad: oklch(55% .2 25);
}}
/* The switch in the header sets data-theme; without it the page and the
   codes follow the system. */
@media (prefers-color-scheme: dark) {{
  :root:not([data-theme="light"]) {{
    --page: oklch(19% .014 140);
    --ink: oklch(94% .012 125);
    --muted: oklch(72% .02 130);
    --rule: oklch(31% .02 140);
    --fluor: oklch(89% .19 122);
    --pink: oklch(70% .2 356);
    --ok: oklch(86% .2 125);
    --bad: oklch(70% .17 25);
  }}
}}
:root[data-theme="dark"] {{
    --page: oklch(19% .014 140);
    --ink: oklch(94% .012 125);
    --muted: oklch(72% .02 130);
    --rule: oklch(31% .02 140);
    --fluor: oklch(89% .19 122);
    --pink: oklch(70% .2 356);
    --ok: oklch(86% .2 125);
    --bad: oklch(70% .17 25);
}}
* {{ box-sizing: border-box; }}
body {{
  margin: 0;
  background: var(--page);
  color: var(--ink);
  font: 15px/1.5 ui-sans-serif, system-ui, sans-serif;
}}
header {{
  background: var(--fluor);
  color: var(--on-fluor);
  padding: clamp(28px, 5vw, 56px) clamp(16px, 4vw, 48px) clamp(24px, 4vw, 40px);
}}
h1 {{ font-size: clamp(2rem, 5vw, 3rem); line-height: 1.05; margin: 0 0 12px;
  font-weight: 750; letter-spacing: -.02em; }}
header p {{ margin: 0; max-width: 70ch; overflow-wrap: anywhere;
  font: 14px/1.45 ui-monospace, SFMono-Regular, Menlo, monospace; }}
main {{
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(min(100%, 240px), 1fr));
  gap: 48px 28px;
  padding: clamp(28px, 4vw, 48px) clamp(16px, 4vw, 48px) 96px;
}}
figure {{ margin: 0; display: grid; }}
h2.section {{ grid-column: 1 / -1; margin: 24px 0 -16px; font-size: 1.5rem; font-weight: 750;
  letter-spacing: -.01em; min-height: 0; padding-bottom: 10px; border-bottom: 2px solid var(--ink); }}
h2.section:first-child {{ margin-top: 0; }}
.code {{ position: relative; }}
.code svg {{ display: block; width: 100%; height: auto; border-radius: 6px; }}
.refused {{ aspect-ratio: 1; margin: 0; display: grid; place-items: center; text-align: center;
  padding: 16px; border: 1px dashed var(--bad); color: var(--bad); border-radius: 6px; }}
figcaption {{ padding-top: 12px; border-top: 1px solid var(--rule); margin-top: 14px; }}
h2 {{ font-size: 1rem; line-height: 1.3; margin: 0 0 6px; font-weight: 650; min-height: 28px;
  display: flex; align-items: center; justify-content: space-between; gap: 8px; }}
code {{ display: block; font: 12px/1.45 ui-monospace, SFMono-Regular, Menlo, monospace;
  color: var(--muted); overflow-wrap: anywhere; }}
.code.waiting svg * {{ animation-play-state: paused !important; }}
.replay {{ flex: none; width: 28px; height: 28px; padding: 0;
  display: grid; place-items: center; border: 1px solid var(--rule); border-radius: 50%;
  background: var(--page); color: var(--ink); cursor: pointer;
  transition: background-color .2s cubic-bezier(.25, 1, .5, 1), border-color .2s cubic-bezier(.25, 1, .5, 1); }}
.replay:hover {{ background: var(--pink); border-color: var(--pink); color: var(--on-fluor); }}
.replay:focus-visible {{ outline: 2px solid var(--pink); outline-offset: 2px; }}
.replay svg {{ width: 14px; height: 14px; }}
@media (prefers-reduced-motion: reduce) {{ .replay {{ display: none; }} }}
/* Each verdict and warning leads with one module: lime when it decodes,
   amber for a warning, red when it doesn't. */
.verdict, .warning {{ margin: 8px 0 0; font-size: 13px; line-height: 1.4;
  display: flex; gap: 8px; align-items: baseline; }}
.verdict::before, .warning::before {{ content: ""; flex: none; width: 8px; height: 8px;
  border-radius: 2px; background: var(--ok); }}
.warning {{ color: var(--muted); }}
.warning::before {{ background: var(--warn); }}
.verdict.failed {{ color: var(--bad); }}
.verdict.failed::before {{ background: var(--bad); }}
/* Each card holds its code and the code's dark twin in one grid cell, and
   only one is visible: the card is as tall as the taller of the two either
   way, so flipping the switch doesn't move the page. */
.variant {{ grid-area: 1 / 1; }}
.variant.dark {{ visibility: hidden; }}
:root[data-theme="dark"] .variant.dark {{ visibility: visible; }}
:root[data-theme="dark"] .variant.light {{ visibility: hidden; }}
@media (prefers-color-scheme: dark) {{
  :root:not([data-theme="light"]) .variant.dark {{ visibility: visible; }}
  :root:not([data-theme="light"]) .variant.light {{ visibility: hidden; }}
}}
/* The switch floats in the corner, so it's in reach anywhere on the page. */
.theme {{ position: fixed; right: 16px; bottom: 16px; z-index: 10;
  display: inline-flex; padding: 3px; gap: 2px; border-radius: 999px;
  background: var(--fluor); border: 1.5px solid var(--on-fluor);
  box-shadow: 0 6px 20px oklch(20% .03 140 / .28); }}
.theme button {{ font: 600 13px/1 ui-sans-serif, system-ui, sans-serif; color: var(--on-fluor);
  background: transparent; border: 0; border-radius: 999px; padding: 8px 14px; cursor: pointer;
  transition: background-color .2s cubic-bezier(.25, 1, .5, 1), color .2s cubic-bezier(.25, 1, .5, 1); }}
.theme button[aria-pressed="true"] {{ background: var(--on-fluor); color: var(--fluor); }}
.theme button:focus-visible {{ outline: 2px solid var(--pink); outline-offset: 2px; }}
</style>
</head>
<body>
<header>
<h1>Style families</h1>
<p>{payload}</p>
</header>
<div class="theme" role="group" aria-label="Codes and page">
<button type="button" data-theme="auto" aria-pressed="true">Auto</button>
<button type="button" data-theme="light" aria-pressed="false">Light</button>
<button type="button" data-theme="dark" aria-pressed="false">Dark</button>
</div>
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
  box.closest(".variant").querySelector("h2").append(button);
}}
// Light, dark or the system's choice, for the page and the codes. Dark
// shows each code's dark twin (`--dark`). Both twins sit in the card all
// along, so a twin's intro plays when the card scrolls into view, whichever
// is showing; the replay button restarts the visible one.
const buttons = document.querySelectorAll(".theme button");
const setTheme = (theme) => {{
  if (theme === "auto") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = theme;
  for (const b of buttons) b.setAttribute("aria-pressed", String(b.dataset.theme === theme));
  try {{ localStorage.setItem("freeqrent-theme", theme); }} catch {{}}
}};
for (const b of buttons) b.addEventListener("click", () => setTheme(b.dataset.theme));
try {{
  const saved = localStorage.getItem("freeqrent-theme");
  if (saved === "light" || saved === "dark") setTheme(saved);
}} catch {{}}
</script>
</body>
</html>
"#,
        payload = escape(text),
    )
}
