# The surface every binding exposes

Every option the `freeqrent` CLI takes, and what it is in the Style document (SPEC §10.1) and the boundary functions. A binding that covers this page covers the CLI. Each language's typed surface is generated from `schema/freeqrent.schema.json`, so its names match the document's.

The CLI's tests keep this page current: every flag in `freeqrent --help` has to appear here, and every line of the equivalence blocks is run twice, through the CLI binary and through `freeqrent-doc`, and the two outputs must be the same bytes (`crates/freeqrent-cli/tests/surface.rs`).

## Boundary functions

| Function | CLI | Returns |
|---|---|---|
| `render(payload, style)` | `freeqrent TEXT [flags]` | the SVG |
| `render_frame(payload, style, t_ms)` | `--frame-at MS` | the intro stopped at `t_ms`, as a still SVG |
| `check(payload, style)` | `--check` (as text; the binding's report is JSON) | the scan report |
| `plan(payload, style)` | `--plan` | the animation plan (SPEC §8.5) |
| `palette_from_image(rgba, width, height, opts)` | `--palette-from-image FILE` | a palette result |
| `palette_from_css(text, opts)` | `--palette-from-css FILE` | a palette result |
| `palette_from_html(text, opts)` | `--palette-from-html FILE` | a palette result |
| `presets()` | the `--preset` names | the preset names |
| `version()` | | the library version |

A payload is text (UTF-8, segmented like the CLI's `TEXT`) or bytes. Every function takes the document as JSON text; a language's typed style objects serialize to it.

## Encoding

| Flag | Style document |
|---|---|
| `--ecc L\|M\|Q\|H` | `encoding.ecc`; raised while it fits unless `encoding.boost_ecc` is false |
| `--version N` | `encoding.version` |
| `--tracking-grid GRID` | `encoding.tracking_grid` |
| | `encoding.min_version`, `encoding.max_version`, `encoding.mask`, `encoding.eci`, `encoding.boost_ecc`: document only |

```equivalences
--ecc Q                     => {"encoding":{"ecc":"Q"}}
--version 7                 => {"encoding":{"version":7}}
--ecc H --version 12        => {"encoding":{"ecc":"H","version":12}}
--tracking-grid sparse      => {"encoding":{"tracking_grid":"sparse"}}
```

## Shapes and scan boxes

| Flag | Style document |
|---|---|
| `--preset NAME` | `preset` |
| `--shape SHAPE` | `modules`: `{"kind": "square", "gap"}`, `rounded` (`radius`), `dots` (`size`), `liquid` (`smoothness`), `bars` (`axis`, `radius`) |
| `--finder RING[/EYE]` | `finders.ring`, `finders.eye` |
| `--corner CORNER=RING[/EYE]` | `finders.corners.{top-left,top-right,bottom-left}` |
| `--alignment match\|RING[/EYE]` | `alignment.shape`: `"match"` or `{"kind": "own", "ring", "eye"}` |
| `--emphasis none\|accent\|outline` | `alignment.emphasis` |
| `--timing plain\|dotted\|dashed` | `timing` |
| `--quiet-zone N` | `quiet_zone` |
| `--seed N` | `seed` |

```equivalences
--preset candy                       => {"preset":"candy"}
--preset terminal --shape dots:0.9   => {"preset":"terminal","modules":{"kind":"dots","size":0.9}}
--shape square:0.1                   => {"modules":{"kind":"square","gap":0.1}}
--shape rounded:0.4                  => {"modules":{"kind":"rounded","radius":0.4}}
--shape liquid:0.6                   => {"modules":{"kind":"liquid","smoothness":0.6}}
--shape bars-h:0.3                   => {"modules":{"kind":"bars","axis":"horizontal","radius":0.3}}
--shape bars-v                       => {"modules":"bars"}
--finder squircle/circle             => {"finders":{"ring":"squircle","eye":"circle"}}
--finder rounded:1                   => {"finders":{"ring":{"kind":"rounded","radius":1}}}
--corner top-right=leaf              => {"finders":{"corners":{"top-right":{"ring":"leaf"}}}}
--finder squircle --corner bottom-left=square/circle => {"finders":{"ring":"squircle","corners":{"bottom-left":{"ring":"square","eye":"circle"}}}}
--version 7 --alignment rounded/circle  => {"alignment":{"shape":{"kind":"own","ring":"rounded","eye":"circle"}},"encoding":{"version":7}}
--version 7 --alignment match --emphasis accent => {"alignment":{"shape":"match","emphasis":"accent"},"encoding":{"version":7}}
--version 20 --emphasis outline => {"alignment":{"emphasis":"outline"},"encoding":{"version":20}}
--timing dotted                      => {"timing":"dotted"}
--quiet-zone 2                       => {"quiet_zone":2}
--seed 7 --shape dots                => {"seed":7,"modules":"dots"}
```

## Colors

| Flag | Style document |
|---|---|
| `--fg PAINT`, `--bg PAINT`, `--token NAME=PAINT` | `palette.{token}`; tokens merge onto the base's palette |
| `--logo-frame-color PAINT` | `palette."logo.frame"` |
| `--spread modules\|regions` | `color_spread` (the data colors are the tokens `fg.2`–`fg.4`) |
| `--allow-inverted` | `allow_inverted` |
| `--dark` | `dark` |
| `--palette-from-image FILE`, `--palette-from-css FILE`, `--palette-from-html FILE` | two calls: `palette_from_*` returns a result whose `palette` goes into the document's `palette` |
| `--palette-clusters K`, `--palette-contrast D` | palette options `clusters`, `min_contrast` |
| `--show-palette` | the palette result's `swatches` and `choices` |
| | palette options `seed`, `logo_colors` (the CLI fills `logo_colors` from `--logo`), `allow_inverted` (the CLI passes `--allow-inverted`) |

Two gaps remain here (found by the npm bin, W20261026235449), both waiting on a decision:

- **With `--preset`, the CLI replaces the preset's palette** with the extracted one, while the document merges the extracted tokens onto the preset's. So a preset token the extraction doesn't set (`filler`, say) survives through the document. Without `--preset` the two agree, which is what `palette_flags_are_two_calls` checks; the npm bin's tests pin the difference so they fail when it's closed.
- **`--logo FILE.svg` with `--palette-from-*`:** the CLI passes the SVG's paints as `logo_colors`, and the boundary has no way to read a logo's colors. A binding can pass `logo_colors` itself; a PNG logo's colors come from `palette_from_image`.

```equivalences
--fg '#1d3b72' --bg '#f8f5ee'                   => {"palette":{"fg":"#1d3b72","bg":"#f8f5ee"}}
--token 'finder.eye=oklch(55% .2 30)'           => {"palette":{"finder.eye":"oklch(55% .2 30)"}}
--fg 'linear-gradient(135deg, #123, #456)'      => {"palette":{"fg":"linear-gradient(135deg, #123, #456)"}}
--bg 'radial-gradient(#ffffff, #e8eef8)'        => {"palette":{"bg":"radial-gradient(#ffffff, #e8eef8)"}}
--spread regions --shape liquid --token 'fg.2=#7a1f5c' --token 'fg.3=#1f4f7a' => {"color_spread":"regions","modules":"liquid","palette":{"fg.2":"#7a1f5c","fg.3":"#1f4f7a"}}
--allow-inverted --fg '#eeeeee' --bg '#111111'  => {"allow_inverted":true,"palette":{"fg":"#eeeeee","bg":"#111111"}}
--preset ink --dark                             => {"preset":"ink","dark":true}
```

## Frames, borders, effects, textures and captions

| Flag | Style document |
|---|---|
| `--frame FRAME`, `--frame-ring RING` | `frame`: `square`, `none`, `{"kind": "rounded", "radius"}`, `{"kind": "circle", "filler_density", "ring"}` |
| `--border BORDER` | `border`: `{"kind": "solid"\|"double", "width"}` |
| `--shadow OFFSET` | `shadow` |
| `--effect EFFECT` | `effect`: `glow` (`radius`, `strength`), `emboss` (`depth`), `sketch` (`roughness`) |
| `--texture TEXTURE` (repeatable) | `textures`: a list of `grain` (`amount`), `scanlines` (`opacity`), `ink` (`fade`) |
| `--caption TEXT` | `caption` (the text alone, or an object) |
| `--caption-size N`, `--caption-font FONT`, `--caption-tracking N`, `--caption-regular` | `caption.size`, `caption.font`, `caption.tracking`, `caption.bold` false |

```equivalences
--frame rounded:1.5                     => {"frame":{"kind":"rounded","radius":1.5}}
--frame circle:0.4 --frame-ring dashed  => {"frame":{"kind":"circle","filler_density":0.4,"ring":"dashed"}}
--frame none                            => {"frame":"none"}
--border double:2 --shadow 1.5          => {"border":{"kind":"double","width":2},"shadow":1.5}
--border solid                          => {"border":"solid"}
--effect glow:0.3:0.2                   => {"effect":{"kind":"glow","radius":0.3,"strength":0.2}}
--effect emboss:0.1                     => {"effect":{"kind":"emboss","depth":0.1}}
--effect sketch:0.1                     => {"effect":{"kind":"sketch","roughness":0.1}}
--texture grain:0.05 --texture scanlines:0.1 => {"textures":[{"kind":"grain","amount":0.05},{"kind":"scanlines","opacity":0.1}]}
--texture ink:0.2                       => {"textures":[{"kind":"ink","fade":0.2}]}
--caption 'SCAN ME'                     => {"caption":"SCAN ME"}
--caption 'menu' --caption-size 3 --caption-font mono --caption-tracking 10 --caption-regular => {"caption":{"text":"menu","size":3,"font":"mono","tracking":10,"bold":false}}
```

## Logos

| Flag | Style document |
|---|---|
| `--logo FILE` | `logo.svg` (the SVG's text) or `logo.raster` (PNG, JPEG, GIF or WebP, base64) |
| `--logo-scale F`, `--logo-size SIZE` | `logo.scale`, `logo.size` |
| `--logo-shape SHAPE`, `--logo-padding N` | `logo.shape`, `logo.padding` |
| `--no-logo-backdrop`, `--keep-modules` | `logo.backdrop` false, `logo.clear_modules` false |
| `--logo-frame FRAME[:W]`, `--logo-frame-width W`, `--logo-moon D` | `logo.frame`: a name, or `{"kind", "width", "moon"}` |
| `--logo-intro`, `--logo-timing`, `--logo-frame-intro`, `--logo-frame-timing` | `intro.logo`, `intro.logo_timing`, `intro.frame`, `intro.frame_timing` |

`$SVG` and `$PNG` stand for a logo file: its path on the command line, its text or base64 in the document.

```equivalences
--logo $SVG --ecc Q                                => {"logo":{"svg":$SVG},"encoding":{"ecc":"Q"}}
--logo $SVG --logo-scale 0.18 --logo-shape circle --logo-padding 1 --logo-frame ring:0.8 => {"logo":{"svg":$SVG,"scale":0.18,"shape":"circle","padding":1,"frame":{"kind":"ring","width":0.8}}}
--logo $SVG --logo-shape rounded:1 --logo-size large --no-logo-backdrop --keep-modules => {"logo":{"svg":$SVG,"shape":{"kind":"rounded","radius":1},"size":"large","backdrop":false,"clear_modules":false}}
--logo $SVG --logo-frame orbit --logo-frame-width 0.3 --logo-moon 2 --logo-frame-color '#c2410c' => {"logo":{"svg":$SVG,"frame":{"kind":"orbit","width":0.3,"moon":2}},"palette":{"logo.frame":"#c2410c"}}
--logo $SVG --logo-frame ring --intro ripple --logo-intro stamp --logo-timing with --logo-frame-intro draw --logo-frame-timing after => {"logo":{"svg":$SVG,"frame":"ring"},"intro":{"preset":"ripple","logo":"stamp","logo_timing":"with","frame":"draw","frame_timing":"after"}}
--logo $PNG --ecc H                                => {"logo":{"raster":$PNG},"encoding":{"ecc":"H"}}
```

## Pictures

| Flag | Style document |
|---|---|
| `--halftone FILE` | `halftone.image` (`{"width", "height", "rgba"}`; the binding decodes the picture, or takes raw pixels) |
| `--art FILE` | `art` (the same object; can't go with `preset` or `halftone`) |
| `--ink mono\|color[:N]` | `halftone.ink`: `mono` or `{"kind": "color", "colors"}` |
| `--bands D,L`, `--clarity C`, `--clarity-map FILE` | `halftone.bands`, `halftone.clarity` (a number or an image) |
| `--fit cover\|contain`, `--focal X,Y` | `halftone.fit`, `halftone.focal` |
| `--priority FILE`, `--no-steer` | `halftone.priority`, `halftone.steer` false |

`$IMAGE` stands for a picture: its path on the command line, its pixels in the document.

```equivalences
--halftone $IMAGE                                   => {"halftone":{"image":$IMAGE}}
--halftone $IMAGE --ink color:4 --bands 0.3,0.9 --clarity 0.4 --fit contain --focal 0.4,0.6 --no-steer => {"halftone":{"image":$IMAGE,"ink":{"kind":"color","colors":4},"bands":[0.3,0.9],"clarity":0.4,"fit":"contain","focal":[0.4,0.6],"steer":false}}
--halftone $IMAGE --clarity-map $IMAGE --priority $IMAGE => {"halftone":{"image":$IMAGE,"clarity":$IMAGE,"priority":$IMAGE}}
--art $IMAGE --ink mono                             => {"art":{"image":$IMAGE,"ink":"mono"}}
--art $IMAGE --tracking-grid sparse                 => {"art":{"image":$IMAGE},"encoding":{"tracking_grid":"sparse"}}
```

## Motion

| Flag | Style document |
|---|---|
| `--intro PRESET`, `--intro-ms N` | `intro` (a name), or `intro.preset`, `intro.duration_ms` |
| `--stagger ORDER`, `--ease EASE`, `--groups N`, `--trigger TRIGGER` | `intro.stagger`, `intro.ease`, `intro.groups`, `intro.trigger` |
| `--from-token NAME=PAINT`, `--from-fg C`, `--from-bg C` | `intro.palette_from` (a palette shift instead of an intro) |
| `--idle LOOP` (repeatable) | `idle`: a list of names |
| `--theme-transition MS` | `theme_transition_ms` |
| | `intro.delay_ms`: document only |

```equivalences
--intro ripple                                  => {"intro":"ripple"}
--intro ripple --intro-ms 900 --stagger random:3 --ease 0.2,0,0,1 --groups 8 --trigger hover => {"intro":{"preset":"ripple","duration_ms":900,"stagger":{"kind":"random","seed":3},"ease":{"kind":"custom","x1":0.2,"y1":0,"x2":0,"y2":1},"groups":8,"trigger":"hover"}}
--intro settle --trigger every:5000             => {"intro":{"preset":"settle","trigger":{"kind":"every","ms":5000}}}
--intro sweep --ease expo --stagger polar --trigger class:go => {"intro":{"preset":"sweep","ease":"expo","stagger":"polar","trigger":{"kind":"class","name":"go"}}}
--intro-ms 700                                  => {"intro":{"duration_ms":700}}
--from-fg '#c2410c' --intro-ms 600              => {"intro":{"palette_from":{"fg":"#c2410c"},"duration_ms":600}}
--from-token 'finder.eye=#c2410c' --from-bg '#fff7ed' => {"intro":{"palette_from":{"finder.eye":"#c2410c","bg":"#fff7ed"}}}
--idle finder-pulse --idle shimmer              => {"idle":["finder-pulse","shimmer"]}
--frame circle --idle logo-turn --logo $SVG --logo-frame orbit => {"frame":"circle","idle":["logo-turn"],"logo":{"svg":$SVG,"frame":"orbit"}}
--theme-transition 300                          => {"theme_transition_ms":300}
--intro ripple --frame-at 400                   => frame 400 {"intro":"ripple"}
--preset candy --intro develop --frame-at 2000  => frame 2000 {"preset":"candy","intro":"develop"}
--intro sweep --plan                            => plan {"intro":"sweep"}
--logo $SVG --intro iris --logo-intro spin --plan => plan {"logo":{"svg":$SVG},"intro":{"preset":"iris","logo":"spin"}}
```

## Output and sizing

| Flag | Style document |
|---|---|
| `--pretty` | `minify` false |
| `--title TEXT`, `--label TEXT` | `title`, `label` (the CLI labels a URL payload with itself; the document doesn't) |
| `--target-px N`, `--target-mm N` | `target_size`: `{"kind": "pixels", "px"}`, `{"kind": "millimeters", "mm"}` |
| | `description`, `id_prefix`: document only |

```equivalences
--pretty                          => {"minify":false}
--title Menu --label 'Our menu'   => {"title":"Menu","label":"Our menu"}
--target-px 300                   => {"target_size":{"kind":"pixels","px":300}}
--target-mm 25                    => {"target_size":{"kind":"millimeters","mm":25}}
```

## The CLI's own tools

These stay in the CLI; no binding exposes them.

| Flag | Why |
|---|---|
| `-o`, `--output FILE`, `-h`, `--help` | the command line's own plumbing |
| `--verify` | decoding needs a rasterizer and two decoders (`freeqrent-verify`: resvg, ZXing, rqrr), far past a binding's size. Bindings return `check`'s report, and the conformance suite is held to the verified Rust output |
| `--gallery FILE`, `--gallery-picture FILE` | the preview page of style families, built on `--verify` |
| `--scan-sheet FILE` | the real-device scan matrix (SPEC §9) |
