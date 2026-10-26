//! `freeqrent`: render styled QR codes as SVG from the command line, check
//! and verify them, or write a gallery page of styles for a payload.

mod gallery;
mod options;
mod sample;

use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use freeqrent::{
    AlignmentShape, AlignmentStyle, Clarity, Corner, Ease, Ecc, Emphasis, EyeShape, FinderStyle,
    Fit, FrameRing, Halftone, Idle, Ink, Intro, Logo, LogoShape, ModuleShape, Origin, Paint,
    Palette, PaletteError, PaletteOpts, PaletteReport, Preset, QrBuilder, QrCode, QrMatrix,
    RasterFormat, Rgba8Image, RingShape, ScanReport, StaggerOrder, Style, TargetSize, TimingStyle,
    Token, TrackingGrid, Trigger, Version,
};

const HELP: &str = "\
freeqrent: styled SVG QR codes

Usage: freeqrent [options] <TEXT>

Encoding:
  --ecc L|M|Q|H            minimum error correction (default M; raised while it fits,
                           except for pictures, which keep it)
  --version N              pin the symbol version (1-40)
  --tracking-grid GRID     sparse (v7-13), standard (v14-20), dense (v21-27)

Style:
  --shape SHAPE            square[:gap] rounded[:r] dots[:size] liquid[:smoothness]
                           bars-v[:r] bars-h[:r]
  --finder RING[/EYE]      rings: square rounded[:r] squircle leaf
                           eyes:  square rounded[:r] circle squircle leaf
  --corner CORNER=RING[/EYE]
                           override one finder (top-left, top-right, bottom-left)
  --alignment match|RING[/EYE]
  --emphasis none|accent|outline
  --timing plain|dotted|dashed
  --fg COLOR, --bg COLOR   shortcuts for --token fg=COLOR, --token bg=COLOR
  --token NAME=PAINT       any color token, e.g. finder.eye=oklch(55% .2 30)
                           or fg=\"linear-gradient(135deg, #123, #456)\"
  --palette-from-image FILE
                           take bg, fg and finder.eye from a PNG or JPEG
  --palette-from-css FILE  ... from a style sheet (`-` for stdin, `sample`)
  --palette-from-html FILE ... from a page: theme-color, <style>, style attributes
                           (`-` for stdin, e.g. curl -s URL | freeqrent ...)
  --palette-clusters K     colors to cluster an image into, 2-12 (default 6)
  --palette-contrast D     least contrast for image palettes, 0.6-0.9 (0.66)
                           --fg, --bg and --token override what was found
  --quiet-zone N           modules of margin (default 4)
  --frame FRAME            square, rounded[:r], circle[:filler density], none
  --frame-ring RING        none, solid, dashed (circle frames)
  --seed N                 seed for decorative choices (default: from the code)
  --logo FILE              SVG, PNG, JPEG, GIF or WebP for the middle; `sample`
                           for a built-in mark. The code grows until it fits.
  --logo-scale F           logo width as a fraction of the code's (default 0.2)
  --logo-shape SHAPE       square, rounded[:r], circle
  --logo-padding N         modules between logo and code (default 0.5)
  --no-logo-backdrop       no plate behind the logo
  --keep-modules           draw the modules under the logo
  --intro PRESET           settle, ripple, lock-on, sweep, raster, iris, assemble
  --intro-ms N             whole intro length (default 1200)
  --stagger ORDER          radial, linear, diagonal, random[:seed], polar
  --ease EASE              quart, quint, expo, or x1,y1,x2,y2
  --groups N               stagger groups, 2-16 (default 12)
  --trigger TRIGGER        load, hover, class:NAME
  --from-token NAME=PAINT  cross-fade from these colors instead of an intro
  --from-fg C, --from-bg C shortcuts for --from-token fg=C, bg=C
  --idle LOOP              finder-pulse, shimmer, breathe (repeatable)
  --theme-transition MS    ease re-themes through the --qr-* properties
  --halftone FILE          build the code from a PNG or JPEG picture (`sample` for a
                           built-in one): modules steered to match, 3x3 sub-cells
  --art FILE               art mode: --halftone plus outlined alignment boxes,
                           dotted timing and the standard tracking grid
  --ink mono|color[:N]     sub-cells in fg, or the picture's colors, N per band (6)
  --bands D,L              darkest light and lightest dark sub-cell (0.28,0.94)
  --clarity C              pull sub-cells toward their module, 0-1 (default 0)
  --clarity-map FILE       ... per area: light parts clear, dark ones pictorial
  --fit cover|contain      how the picture fills the code (default cover)
  --focal X,Y              the point to keep in view, 0-1 each (default 0.5,0.5)
  --priority FILE          where steering matters most: light first, black never
  --no-steer               keep standard padding (no module steering)
  --allow-inverted         permit light modules on a dark background
  --target-px N            displayed size in device pixels (warns when too small)
  --target-mm N            printed size in millimeters (warns when too small)

Output:
  -o, --output FILE        write the SVG here (default: stdout)
  --pretty                 one element per line
  --title TEXT             add a <title>
  --label TEXT             aria-label (default: TEXT when it's a URL, else \"QR code\")
  --frame-at MS            write the intro stopped at MS as a still SVG
  --plan                   write the animation plan as JSON instead
  --check                  print the scan report instead of the SVG
  --show-palette           print what --palette-from-* found, as flags
  --verify                 rasterize and decode the result before writing it
  --gallery FILE           write an HTML page of style families for TEXT
  -h, --help               show this help
";

/// Why the program stopped.
enum Failure {
    /// Bad arguments (exit 2).
    Usage(String),
    /// The code was refused, failed verification, or couldn't be written
    /// (exit 1).
    Failed(String),
}

impl From<lexopt::Error> for Failure {
    fn from(e: lexopt::Error) -> Failure {
        Failure::Usage(e.to_string())
    }
}

/// Style flags, kept as given so the gallery can layer its own on top.
#[derive(Clone, Default)]
pub struct StyleArgs {
    shape: Option<ModuleShape>,
    finder: Option<(RingShape, EyeShape)>,
    corners: Vec<(Corner, RingShape, EyeShape)>,
    alignment: Option<AlignmentShape>,
    emphasis: Option<Emphasis>,
    timing: Option<TimingStyle>,
    tokens: Vec<(String, String)>,
    quiet_zone: Option<u8>,
    allow_inverted: bool,
    pretty: bool,
    title: Option<String>,
    label: Option<String>,
    target: Option<TargetSize>,
    frame: Option<String>,
    frame_ring: Option<FrameRing>,
    seed: Option<u64>,
    logo: Option<String>,
    logo_scale: Option<f32>,
    logo_shape: Option<LogoShape>,
    logo_padding: Option<f32>,
    no_logo_backdrop: bool,
    keep_modules: bool,
    intro: Option<Preset>,
    intro_ms: Option<u32>,
    stagger: Option<StaggerOrder>,
    ease: Option<Ease>,
    groups: Option<u8>,
    trigger: Option<Trigger>,
    from_tokens: Vec<(String, String)>,
    idle: Vec<Idle>,
    theme_transition: Option<u32>,
    palette_source: Option<PaletteSource>,
    palette_clusters: Option<u8>,
    palette_contrast: Option<f32>,
    picture: Option<Rgba8Image>,
    art: bool,
    ink: Option<Ink>,
    bands: Option<(f32, f32)>,
    clarity: Option<Clarity>,
    fit: Option<Fit>,
    focal: Option<(f32, f32)>,
    priority: Option<Rgba8Image>,
    no_steer: bool,
    tracking_grid: Option<TrackingGrid>,
}

/// Reads a PNG or JPEG for a picture flag; `sample` is the built-in one.
fn load_picture(flag: &str, source: &str) -> Result<Rgba8Image, String> {
    if source == "sample" {
        return Ok(sample::picture());
    }
    let bytes = fs::read(source).map_err(|e| format!("{source}: {e}"))?;
    freeqrent_image::decode(&bytes).map_err(|e| format!("--{flag} {source}: {e}"))
}

/// Where `--palette-from-*` takes colors from, read once.
#[derive(Clone)]
enum PaletteSource {
    Image(Rgba8Image),
    Css(String),
    Html(String),
}

/// The style sheet `--palette-from-css sample` uses.
const SAMPLE_CSS: &str = ":root { --paper: #f6f1e7; --ink: #23201c; --brand: #c2410c }
body { background: var(--paper); color: var(--ink) }
a, .button { color: var(--brand) }";

/// Reads a text source: a file, or standard input for `-`.
fn read_text(flag: &str, source: &str) -> Result<String, String> {
    if source == "-" {
        let mut text = String::new();
        io::Read::read_to_string(&mut io::stdin(), &mut text)
            .map_err(|e| format!("--{flag} -: {e}"))?;
        return Ok(text);
    }
    fs::read_to_string(source).map_err(|e| format!("--{flag} {source}: {e}"))
}

/// The mark `--logo sample` uses.
const SAMPLE_LOGO: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect width="24" height="24" rx="6" fill="#c2410c"/><path d="M7 6h10v4h-6v2h5v4h-5v3H7z" fill="#fff7ed"/></svg>"##;

/// Reads a logo file: SVG by extension or content, otherwise a raster
/// format recognized by its signature.
fn load_logo(source: &str) -> Result<Logo, String> {
    if source == "sample" {
        return Logo::svg(SAMPLE_LOGO).map_err(|e| e.to_string());
    }
    let bytes = fs::read(source).map_err(|e| format!("--logo {source}: {e}"))?;
    let looks_svg =
        source.to_ascii_lowercase().ends_with(".svg") || bytes.trim_ascii_start().starts_with(b"<");
    let logo = if looks_svg {
        let text = String::from_utf8(bytes).map_err(|_| format!("--logo {source}: not UTF-8"))?;
        Logo::svg(&text)
    } else {
        let format = RasterFormat::sniff(&bytes)
            .ok_or_else(|| format!("--logo {source}: not an SVG, PNG, JPEG, GIF or WebP file"))?;
        Logo::raster(bytes, format)
    };
    logo.map_err(|e| format!("--logo {source}: {e}"))
}

impl StyleArgs {
    /// Applies `--flag value`. Returns `Ok(false)` when `flag` isn't a
    /// style flag.
    pub fn set(&mut self, flag: &str, value: &str) -> Result<bool, String> {
        match flag {
            "shape" => self.shape = Some(options::shape(value)?),
            "finder" => self.finder = Some(options::finder(value)?),
            "corner" => self.corners.push(options::corner(value)?),
            "alignment" => self.alignment = Some(options::alignment(value)?),
            "emphasis" => self.emphasis = Some(options::emphasis(value)?),
            "timing" => self.timing = Some(options::timing(value)?),
            "token" => self.tokens.push(options::token(value)?),
            "fg" | "bg" => self.tokens.push((flag.to_string(), value.to_string())),
            "quiet-zone" => {
                self.quiet_zone = Some(
                    value
                        .parse()
                        .map_err(|_| format!("quiet zone {value:?} isn't a number 0-255"))?,
                )
            }
            "target-px" => {
                self.target = Some(TargetSize::Pixels(
                    value
                        .parse()
                        .map_err(|_| format!("{value:?} isn't a pixel count"))?,
                ))
            }
            "target-mm" => {
                self.target =
                    Some(TargetSize::Millimeters(value.parse().map_err(|_| {
                        format!("{value:?} isn't a size in millimeters")
                    })?))
            }
            "frame" => {
                options::frame(value, FrameRing::None)?;
                self.frame = Some(value.to_string());
            }
            "frame-ring" => self.frame_ring = Some(options::frame_ring(value)?),
            "seed" => {
                self.seed = Some(
                    value
                        .parse()
                        .map_err(|_| format!("seed {value:?} isn't a whole number"))?,
                )
            }
            "logo" => self.logo = Some(value.to_string()),
            "logo-scale" => self.logo_scale = Some(options::number(flag, value)?),
            "logo-shape" => self.logo_shape = Some(options::logo_shape(value)?),
            "logo-padding" => self.logo_padding = Some(options::number(flag, value)?),
            "intro" => self.intro = Some(options::preset(value)?),
            "intro-ms" => {
                self.intro_ms = Some(
                    value
                        .parse()
                        .map_err(|_| format!("{value:?} isn't a whole number of ms"))?,
                )
            }
            "stagger" => self.stagger = Some(options::stagger(value)?),
            "ease" => self.ease = Some(options::ease(value)?),
            "groups" => {
                self.groups = Some(
                    value
                        .parse()
                        .map_err(|_| format!("{value:?} isn't a number 2-16"))?,
                )
            }
            "trigger" => self.trigger = Some(options::trigger(value)?),
            "from-token" => self.from_tokens.push(options::token(value)?),
            "from-fg" | "from-bg" => self
                .from_tokens
                .push((flag["from-".len()..].to_string(), value.to_string())),
            "idle" => self.idle.push(options::idle(value)?),
            "theme-transition" => {
                self.theme_transition = Some(
                    value
                        .parse()
                        .map_err(|_| format!("{value:?} isn't a whole number of ms"))?,
                )
            }
            "palette-from-image" => {
                let bytes = fs::read(value).map_err(|e| format!("{value}: {e}"))?;
                let image = freeqrent_image::decode(&bytes).map_err(|e| format!("{value}: {e}"))?;
                self.palette_source = Some(PaletteSource::Image(image));
            }
            "palette-from-css" => {
                let css = match value {
                    "sample" => SAMPLE_CSS.to_string(),
                    _ => read_text(flag, value)?,
                };
                self.palette_source = Some(PaletteSource::Css(css));
            }
            "palette-from-html" => {
                self.palette_source = Some(PaletteSource::Html(read_text(flag, value)?));
            }
            "palette-clusters" => {
                self.palette_clusters = Some(
                    value
                        .parse()
                        .map_err(|_| format!("{value:?} isn't a number 2-12"))?,
                )
            }
            "palette-contrast" => self.palette_contrast = Some(options::number(flag, value)?),
            "halftone" | "art" => {
                self.picture = Some(load_picture(flag, value)?);
                self.art = flag == "art";
            }
            "ink" => self.ink = Some(options::ink(value)?),
            "bands" => self.bands = Some(options::pair(flag, value)?),
            "clarity" => self.clarity = Some(Clarity::Uniform(options::number(flag, value)?)),
            "clarity-map" => self.clarity = Some(Clarity::Map(load_picture(flag, value)?)),
            "fit" => self.fit = Some(options::fit(value)?),
            "focal" => self.focal = Some(options::pair(flag, value)?),
            "priority" => self.priority = Some(load_picture(flag, value)?),
            "tracking-grid" => self.tracking_grid = Some(options::tracking_grid(value)?),
            "title" => self.title = Some(value.to_string()),
            "label" => self.label = Some(value.to_string()),
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// The palette `--palette-from-*` derives, with its report.
    pub fn extract(&self) -> Result<Option<(Palette, PaletteReport)>, String> {
        let found = match &self.palette_source {
            None => return Ok(None),
            Some(PaletteSource::Image(image)) => {
                let mut opts = PaletteOpts::new();
                if let Some(k) = self.palette_clusters {
                    opts = opts.clusters(k);
                }
                if let Some(d) = self.palette_contrast {
                    opts = opts.min_contrast(d);
                }
                Palette::from_image(image, opts)
            }
            Some(PaletteSource::Css(css)) => Palette::from_css(css),
            Some(PaletteSource::Html(html)) => Palette::from_html(html),
        };
        found
            .map(Some)
            .map_err(|e| format!("--palette-from-*: {e}"))
    }

    /// `builder` with the flags' tracking grid: the one given, or the
    /// standard grid for art mode unless the version is `pinned`.
    pub fn encoder(&self, builder: QrBuilder, pinned: bool) -> QrBuilder {
        match self.tracking_grid {
            Some(grid) => builder.tracking_grid(grid),
            None if self.art && !pinned => builder.tracking_grid(TrackingGrid::Standard),
            None => builder,
        }
    }

    /// The picture flags as a `Halftone`, if a picture was given.
    fn halftone(&self) -> Option<Halftone> {
        let mut h = Halftone::new(self.picture.clone()?).steer(!self.no_steer);
        if let Some(ink) = self.ink {
            h = h.ink(ink);
        }
        if let Some((dark, light)) = self.bands {
            h = h.bands(dark, light);
        }
        if let Some(clarity) = &self.clarity {
            h = h.clarity(clarity.clone());
        }
        if let Some(fit) = self.fit {
            h = h.fit(fit);
        }
        if let Some((x, y)) = self.focal {
            h = h.focal(x, y);
        }
        if let Some(map) = &self.priority {
            h = h.priority(map.clone());
        }
        Some(h)
    }

    /// The style these flags describe, labeled for `text`.
    pub fn build(&self, text: &str) -> Result<Style, String> {
        let mut palette = match self.extract()? {
            Some((palette, _)) => palette,
            None => Palette::new(),
        };
        for (name, value) in &self.tokens {
            let wrap = |error| {
                PaletteError {
                    token: name.clone(),
                    error,
                }
                .to_string()
            };
            let token =
                Token::from_name(name).ok_or_else(|| wrap(freeqrent::ColorError::UnknownToken))?;
            let paint = Paint::parse(value).map_err(wrap)?;
            palette.set(token, paint).map_err(wrap)?;
        }
        let (ring, eye) = self.finder.unwrap_or((RingShape::Square, EyeShape::Square));
        let mut finders = FinderStyle::new(ring, eye);
        for &(corner, ring, eye) in &self.corners {
            finders = finders.corner(corner, ring, eye);
        }
        let halftone = self.halftone();
        if halftone.is_none()
            && (self.ink.is_some()
                || self.bands.is_some()
                || self.clarity.is_some()
                || self.fit.is_some()
                || self.focal.is_some()
                || self.priority.is_some()
                || self.no_steer)
        {
            return Err("picture options need --halftone or --art".to_string());
        }
        // Art mode's own defaults, under whatever was given.
        let base = match &halftone {
            Some(h) if self.art => Style::art(h.clone()),
            _ => Style::new(),
        };
        let (default, timing) = (base.alignment_style(), base.timing_style());
        let mut style = base
            .modules(self.shape.unwrap_or_default())
            .finders(finders)
            .alignment(AlignmentStyle {
                shape: self.alignment.unwrap_or(default.shape),
                emphasis: self.emphasis.unwrap_or(default.emphasis),
            })
            .timing(self.timing.unwrap_or(timing))
            .palette(palette)
            .quiet_zone(self.quiet_zone.unwrap_or(4))
            .allow_inverted(self.allow_inverted)
            .minify(!self.pretty);
        if let Some(frame) = &self.frame {
            let ring = self.frame_ring.unwrap_or_default();
            style = style.frame(options::frame(frame, ring)?);
        }
        if let Some(seed) = self.seed {
            style = style.seed(seed);
        }
        if let Some(h) = halftone {
            style = style.halftone(h);
        }
        if let Some(source) = &self.logo {
            let mut logo = load_logo(source)?
                .backdrop(!self.no_logo_backdrop)
                .clear_modules(!self.keep_modules);
            if let Some(scale) = self.logo_scale {
                logo = logo.scale(scale);
            }
            if let Some(shape) = self.logo_shape {
                logo = logo.shape(shape);
            }
            if let Some(padding) = self.logo_padding {
                logo = logo.padding(padding);
            }
            style = style.logo(logo);
        }
        let animated = self.intro_ms.is_some()
            || self.stagger.is_some()
            || self.ease.is_some()
            || self.groups.is_some()
            || self.trigger.is_some();
        if !self.from_tokens.is_empty() {
            if self.intro.is_some() {
                return Err("--intro and --from-* can't be combined".to_string());
            }
            let old = Palette::from_tokens(
                self.from_tokens
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str())),
            )
            .map_err(|e| format!("--from-token: {e}"))?;
            let mut intro = Intro::palette_from(&old);
            if let Some(ms) = self.intro_ms {
                intro = intro.duration_ms(ms);
            }
            if let Some(ease) = self.ease {
                intro = intro.ease(ease);
            }
            if let Some(trigger) = &self.trigger {
                intro = intro.trigger(trigger.clone());
            }
            style = style.intro(intro);
        } else if self.intro.is_some() || animated {
            let mut intro = Intro::new(self.intro.unwrap_or(Preset::Settle));
            if let Some(ms) = self.intro_ms {
                intro = intro.duration_ms(ms);
            }
            if let Some(order) = self.stagger {
                intro = intro.stagger(order);
            }
            if let Some(ease) = self.ease {
                intro = intro.ease(ease);
            }
            if let Some(groups) = self.groups {
                intro = intro.groups(groups);
            }
            if let Some(trigger) = &self.trigger {
                intro = intro.trigger(trigger.clone());
            }
            style = style.intro(intro);
        }
        for &idle in &self.idle {
            style = style.idle(idle);
        }
        if let Some(ms) = self.theme_transition {
            style = style.theme_transition(ms);
        }
        if let Some(title) = &self.title {
            style = style.title(title.clone());
        }
        let is_url = text.starts_with("https://") || text.starts_with("http://");
        match &self.label {
            Some(label) => style = style.label(label.clone()),
            None if is_url => style = style.label(text),
            None => {}
        }
        if let Some(target) = self.target {
            style = style.target_size(target);
        }
        Ok(style)
    }
}

struct Args {
    text: String,
    builder: QrBuilder,
    pinned: bool,
    style: StyleArgs,
    output: Option<PathBuf>,
    gallery: Option<PathBuf>,
    check: bool,
    verify: bool,
    frame_at: Option<u32>,
    plan: bool,
    show_palette: bool,
}

fn parse_args() -> Result<Option<Args>, Failure> {
    use lexopt::prelude::*;

    let mut parser = lexopt::Parser::from_env();
    let mut text = None;
    let mut ecc = Ecc::M;
    let mut version = None;
    let mut style = StyleArgs::default();
    let (mut output, mut gallery, mut check, mut verify) = (None, None, false, false);
    let (mut frame_at, mut plan, mut show_palette) = (None, false, false);
    while let Some(arg) = parser.next()? {
        match arg {
            Short('h') | Long("help") => {
                print!("{HELP}");
                return Ok(None);
            }
            Short('o') | Long("output") => output = Some(PathBuf::from(parser.value()?)),
            Long("gallery") => gallery = Some(PathBuf::from(parser.value()?)),
            Long("check") => check = true,
            Long("verify") => verify = true,
            Long("plan") => plan = true,
            Long("show-palette") => show_palette = true,
            Long("frame-at") => {
                let v = parser.value()?.string()?;
                frame_at = Some(v.parse().map_err(|_| {
                    Failure::Usage(format!("--frame-at: {v:?} isn't a whole number of ms"))
                })?);
            }
            Long("pretty") => style.pretty = true,
            Long("allow-inverted") => style.allow_inverted = true,
            Long("no-logo-backdrop") => style.no_logo_backdrop = true,
            Long("keep-modules") => style.keep_modules = true,
            Long("no-steer") => style.no_steer = true,
            Long("ecc") => {
                ecc = options::ecc(&parser.value()?.string()?).map_err(Failure::Usage)?
            }
            Long("version") => {
                let v = parser.value()?.string()?;
                version = Some(
                    v.parse()
                        .ok()
                        .and_then(Version::new)
                        .ok_or_else(|| Failure::Usage(format!("version {v:?} isn't 1-40")))?,
                );
            }
            Long(flag) => {
                let flag = flag.to_string();
                let value = parser.value()?.string()?;
                if !style
                    .set(&flag, &value)
                    .map_err(|e| Failure::Usage(format!("--{flag}: {e}")))?
                {
                    return Err(Failure::Usage(format!("unknown option --{flag}")));
                }
            }
            Value(v) if text.is_none() => text = Some(v.string()?),
            _ => return Err(arg.unexpected().into()),
        }
    }
    let text = text.ok_or_else(|| Failure::Usage("missing TEXT; see --help".to_string()))?;
    let mut builder = QrCode::builder().ecc(ecc);
    if let Some(v) = version {
        builder = builder.version(v);
    }
    Ok(Some(Args {
        text,
        builder,
        pinned: version.is_some(),
        style,
        output,
        gallery,
        check,
        verify,
        frame_at,
        plan,
        show_palette,
    }))
}

/// Prints what extraction found, and the flags that reproduce the palette.
fn print_palette(out: &mut dyn Write, report: &PaletteReport) -> io::Result<()> {
    writeln!(out, "palette: {} colors found", report.swatches.len())?;
    for (i, s) in report.swatches.iter().enumerate() {
        writeln!(
            out,
            "  {i:>2}  {:<24} {:>5.1}%",
            s.color.to_string(),
            s.share * 100.0
        )?;
    }
    let mut flags = Vec::new();
    for c in &report.choices {
        let why = match c.origin {
            Origin::Swatch(i) => format!("color {i}"),
            Origin::Tinted(Some(i)) => format!("near-white tinted by color {i}"),
            Origin::Tinted(None) => "neutral".to_string(),
            Origin::Darkened { swatch, delta_l } => {
                format!("color {swatch} darkened from contrast {delta_l:.2}")
            }
            _ => "derived".to_string(),
        };
        writeln!(
            out,
            "  {:<11} {:<24} {why}",
            c.token.name(),
            c.color.to_string()
        )?;
        flags.push(match c.token {
            Token::Bg | Token::Fg => format!("--{} '{}'", c.token.name(), c.color),
            t => format!("--token '{}={}'", t.name(), c.color),
        });
    }
    writeln!(out, "  as flags: {}", flags.join(" "))
}

fn print_report(out: &mut dyn Write, report: &ScanReport) -> io::Result<()> {
    if report.ok() {
        writeln!(
            out,
            "ok: renders; lowest contrast {:.2}",
            report.min_contrast
        )?;
    }
    for e in &report.errors {
        writeln!(out, "error: {e}")?;
    }
    for w in &report.warnings {
        writeln!(out, "warning: {w}")?;
    }
    if report.ecc_budget_used > 0.0 {
        writeln!(
            out,
            "error-correction budget used: {:.0}%",
            report.ecc_budget_used * 100.0
        )?;
    }
    if let Some(s) = report.steering {
        let share = |n: usize| n as f64 * 100.0 / s.code_modules as f64;
        writeln!(
            out,
            "steering: {} of {} modules steerable ({:.0}%); {:.0}% follow the picture",
            s.steerable_modules,
            s.code_modules,
            share(s.steerable_modules),
            share(s.matched_modules)
        )?;
    }
    Ok(())
}

/// Decodes the render, when built with the `verify` feature.
#[cfg(feature = "verify")]
fn verify(style: &Style, qr: &QrMatrix, text: &str) -> Result<String, String> {
    let report = style.check(qr);
    let bar = freeqrent_verify::Bar::for_report(&report);
    freeqrent_verify::verify_style(style, qr, text).map_err(|e| e.to_string())?;
    let mut verdict = if bar.rqrr {
        "decodes with ZXing and rqrr".to_string()
    } else {
        "decodes with ZXing".to_string()
    };
    if style.intro_options().is_some() {
        let total = style.plan(qr).map_err(|e| e.to_string())?.total_ms;
        let at =
            freeqrent_verify::scannable_after(style, qr, text, 50).map_err(|e| e.to_string())?;
        verdict.push_str(&format!("; scannable from {at} ms of the {total} ms intro"));
    }
    Ok(verdict)
}

#[cfg(not(feature = "verify"))]
fn verify(_: &Style, _: &QrMatrix, _: &str) -> Result<String, String> {
    Err("built without the `verify` feature".to_string())
}

fn run() -> Result<(), Failure> {
    let Some(args) = parse_args()? else {
        return Ok(());
    };
    if let Some(path) = &args.gallery {
        let page = gallery::page(&args.text, &args.builder, args.pinned, &args.style);
        return fs::write(path, page)
            .map_err(|e| Failure::Failed(format!("{}: {e}", path.display())));
    }

    if args.show_palette {
        match args.style.extract().map_err(Failure::Failed)? {
            Some((_, report)) => print_palette(&mut io::stderr(), &report),
            None => writeln!(io::stderr(), "palette: no --palette-from-* given"),
        }
        .map_err(|e| Failure::Failed(e.to_string()))?;
    }
    let style = args.style.build(&args.text).map_err(Failure::Usage)?;
    let qr = args
        .style
        .encoder(args.builder, args.pinned)
        .encode_text_for(&args.text, &style)
        .map_err(|e| Failure::Failed(e.to_string()))?;
    let report = style.check(&qr);
    if args.check {
        print_report(&mut io::stdout(), &report).map_err(|e| Failure::Failed(e.to_string()))?;
        return if report.ok() {
            Ok(())
        } else {
            Err(Failure::Failed("the code would be refused".to_string()))
        };
    }
    let svg = match (args.plan, args.frame_at) {
        (true, _) => style.plan(&qr).map(|p| p.to_json()),
        (false, Some(t)) => style.render_frame(&qr, t),
        (false, None) => style.render(&qr),
    }
    .map_err(|e| Failure::Failed(e.to_string()))?;
    for w in &report.warnings {
        eprintln!("warning: {w}");
    }
    if args.verify {
        let verdict = verify(&style, &qr, &args.text).map_err(Failure::Failed)?;
        eprintln!("verified: {verdict}");
    }
    match &args.output {
        Some(path) => {
            fs::write(path, svg).map_err(|e| Failure::Failed(format!("{}: {e}", path.display())))
        }
        None => io::stdout()
            .write_all(svg.as_bytes())
            .map_err(|e| Failure::Failed(e.to_string())),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Failed(e)) => {
            eprintln!("freeqrent: {e}");
            ExitCode::from(1)
        }
        Err(Failure::Usage(e)) => {
            eprintln!("freeqrent: {e}");
            ExitCode::from(2)
        }
    }
}
