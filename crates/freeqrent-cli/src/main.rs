//! `freeqrent`: render styled QR codes as SVG from the command line, check
//! and verify them, or write a gallery page of styles for a payload.

mod gallery;
mod options;

use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use freeqrent::{
    AlignmentShape, AlignmentStyle, Corner, Ecc, Emphasis, EyeShape, FinderStyle, ModuleShape,
    Palette, QrBuilder, QrCode, QrMatrix, RingShape, ScanReport, Style, TargetSize, TimingStyle,
    Version,
};

const HELP: &str = "\
freeqrent: styled SVG QR codes

Usage: freeqrent [options] <TEXT>

Encoding:
  --ecc L|M|Q|H            minimum error correction (default M; raised while it fits)
  --version N              pin the symbol version (1-40)

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
  --quiet-zone N           modules of margin (default 4)
  --allow-inverted         permit light modules on a dark background
  --target-px N            displayed size in device pixels (warns when too small)
  --target-mm N            printed size in millimeters (warns when too small)

Output:
  -o, --output FILE        write the SVG here (default: stdout)
  --pretty                 one element per line
  --title TEXT             add a <title>
  --label TEXT             aria-label (default: TEXT when it's a URL, else \"QR code\")
  --check                  print the scan report instead of the SVG
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
            "title" => self.title = Some(value.to_string()),
            "label" => self.label = Some(value.to_string()),
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// The style these flags describe, labeled for `text`.
    pub fn build(&self, text: &str) -> Result<Style, String> {
        let palette =
            Palette::from_tokens(self.tokens.iter().map(|(k, v)| (k.as_str(), v.as_str())))
                .map_err(|e| e.to_string())?;
        let (ring, eye) = self.finder.unwrap_or((RingShape::Square, EyeShape::Square));
        let mut finders = FinderStyle::new(ring, eye);
        for &(corner, ring, eye) in &self.corners {
            finders = finders.corner(corner, ring, eye);
        }
        let default = AlignmentStyle::default();
        let mut style = Style::new()
            .modules(self.shape.unwrap_or_default())
            .finders(finders)
            .alignment(AlignmentStyle {
                shape: self.alignment.unwrap_or(default.shape),
                emphasis: self.emphasis.unwrap_or(default.emphasis),
            })
            .timing(self.timing.unwrap_or_default())
            .palette(palette)
            .quiet_zone(self.quiet_zone.unwrap_or(4))
            .allow_inverted(self.allow_inverted)
            .minify(!self.pretty);
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
    style: StyleArgs,
    output: Option<PathBuf>,
    gallery: Option<PathBuf>,
    check: bool,
    verify: bool,
}

fn parse_args() -> Result<Option<Args>, Failure> {
    use lexopt::prelude::*;

    let mut parser = lexopt::Parser::from_env();
    let mut text = None;
    let mut ecc = Ecc::M;
    let mut version = None;
    let mut style = StyleArgs::default();
    let (mut output, mut gallery, mut check, mut verify) = (None, None, false, false);
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
            Long("pretty") => style.pretty = true,
            Long("allow-inverted") => style.allow_inverted = true,
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
        style,
        output,
        gallery,
        check,
        verify,
    }))
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
    Ok(())
}

/// Decodes the render, when built with the `verify` feature.
#[cfg(feature = "verify")]
fn verify(style: &Style, qr: &QrMatrix, text: &str) -> Result<String, String> {
    let report = style.check(qr);
    let bar = freeqrent_verify::Bar::for_report(&report);
    freeqrent_verify::verify_style(style, qr, text).map_err(|e| e.to_string())?;
    Ok(if bar.rqrr {
        "decodes with ZXing and rqrr".to_string()
    } else {
        "decodes with ZXing".to_string()
    })
}

#[cfg(not(feature = "verify"))]
fn verify(_: &Style, _: &QrMatrix, _: &str) -> Result<String, String> {
    Err("built without the `verify` feature".to_string())
}

fn run() -> Result<(), Failure> {
    let Some(args) = parse_args()? else {
        return Ok(());
    };
    let qr = args
        .builder
        .encode_text(&args.text)
        .map_err(|e| Failure::Failed(e.to_string()))?;

    if let Some(path) = &args.gallery {
        let page = gallery::page(&args.text, &qr, &args.style);
        return fs::write(path, page)
            .map_err(|e| Failure::Failed(format!("{}: {e}", path.display())));
    }

    let style = args.style.build(&args.text).map_err(Failure::Usage)?;
    let report = style.check(&qr);
    if args.check {
        print_report(&mut io::stdout(), &report).map_err(|e| Failure::Failed(e.to_string()))?;
        return if report.ok() {
            Ok(())
        } else {
            Err(Failure::Failed("the code would be refused".to_string()))
        };
    }
    let svg = style
        .render(&qr)
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
