//! `Style` (SPEC §5): everything about how a matrix is drawn, as plain
//! data. Building a style never fails; [`Style::check`] reports problems and
//! [`Style::render`] refuses to draw a code that might not scan.

use alloc::string::String;
#[cfg(feature = "animate")]
use alloc::vec::Vec;
use core::fmt;

#[cfg(feature = "animate")]
use crate::animate::{Idle, Intro};
use crate::check::{self, RenderError, ScanReport};
#[cfg(feature = "halftone")]
use crate::halftone::Halftone;
use crate::logo::Logo;
use crate::matrix::QrMatrix;
use crate::palette::Palette;
use crate::scanbox::{AlignmentStyle, FinderStyle, TimingStyle};
use crate::svg;

/// Direction of [`ModuleShape::Bars`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Axis {
    /// Runs along rows.
    Horizontal,
    /// Runs along columns.
    Vertical,
}

/// How data modules are drawn (SPEC §7.1). Sizes are fractions of a module.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ModuleShape {
    /// Squares; horizontal runs merge when `gap` is 0. `gap` is 0–0.25
    /// (at 0.3, rqrr loses codes at 6 px per module).
    Square {
        /// Space between neighboring squares.
        gap: f32,
    },
    /// Squares whose corners are rounded where no neighbor touches them.
    /// `radius` is 0–0.5.
    Rounded {
        /// Corner radius.
        radius: f32,
    },
    /// Circles of diameter `size`, 0.6–1.0 (below 0.75 raises a warning).
    Dots {
        /// Diameter.
        size: f32,
    },
    /// Neighboring modules flow into one blob per connected region, with
    /// every corner rounded and concave inner corners filled ("gooey").
    /// `smoothness` is 0–1: 0 gives the square outline, 1 rounds each
    /// corner across half a module.
    Liquid {
        /// How far each corner's curve reaches, in half-modules.
        smoothness: f32,
    },
    /// Runs along `axis` merged into rounded bars. `radius` is 0–0.5.
    Bars {
        /// Which way runs merge.
        axis: Axis,
        /// Corner radius at the bar ends.
        radius: f32,
    },
}

impl ModuleShape {
    /// Plain merged squares, the default.
    pub const SQUARE: ModuleShape = ModuleShape::Square { gap: 0.0 };
}

impl Default for ModuleShape {
    fn default() -> ModuleShape {
        ModuleShape::SQUARE
    }
}

/// The outer shape of the code (SPEC §7.4).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Frame {
    /// A square background, the default.
    #[default]
    Square,
    /// A square background with rounded corners; the radius is 0 to the
    /// quiet zone width, in modules.
    RoundedSquare(f32),
    /// A round code: the quiet-zone square inscribed in a circle, the
    /// space between them filled with decorative modules in the `filler`
    /// token.
    Circle {
        /// The share of filler cells drawn, 0–1. Cells are picked by the
        /// seeded generator (`Style::seed`).
        filler_density: f32,
        /// A ring around the circle in the `frame` token.
        ring: FrameRing,
    },
    /// No background element: the host page shows through. Contrast is
    /// still judged against `bg`.
    None,
}

/// The ring around [`Frame::Circle`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FrameRing {
    /// No ring.
    #[default]
    None,
    /// A solid ring half a module wide.
    Solid,
    /// A dashed ring half a module wide.
    Dashed,
}

/// How large a code will be displayed, across the whole symbol including
/// the quiet zone.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TargetSize {
    /// On screen, in device pixels.
    Pixels(u32),
    /// In print, in millimeters.
    Millimeters(f32),
}

/// A complete description of how to draw a code. Cheap to clone; every
/// field is plain data.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub(crate) modules: ModuleShape,
    pub(crate) finders: FinderStyle,
    pub(crate) alignment: AlignmentStyle,
    pub(crate) timing: TimingStyle,
    pub(crate) palette: Palette,
    pub(crate) quiet_zone: u8,
    pub(crate) allow_inverted: bool,
    pub(crate) minify: bool,
    pub(crate) title: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) label: Option<String>,
    pub(crate) id_prefix: Option<String>,
    pub(crate) target: Option<TargetSize>,
    pub(crate) frame: Frame,
    pub(crate) seed: Option<u64>,
    pub(crate) logo: Option<Logo>,
    #[cfg(feature = "halftone")]
    pub(crate) halftone: Option<Halftone>,
    #[cfg(feature = "animate")]
    pub(crate) intro: Option<Intro>,
    #[cfg(feature = "animate")]
    pub(crate) idle: Vec<Idle>,
    #[cfg(feature = "animate")]
    pub(crate) theme_transition: Option<u32>,
}

impl Default for Style {
    fn default() -> Style {
        Style {
            modules: ModuleShape::default(),
            finders: FinderStyle::default(),
            alignment: AlignmentStyle::default(),
            timing: TimingStyle::default(),
            palette: Palette::default(),
            quiet_zone: 4,
            allow_inverted: false,
            minify: true,
            title: None,
            description: None,
            label: None,
            id_prefix: None,
            target: None,
            frame: Frame::Square,
            seed: None,
            logo: None,
            #[cfg(feature = "halftone")]
            halftone: None,
            #[cfg(feature = "animate")]
            intro: None,
            #[cfg(feature = "animate")]
            idle: Vec::new(),
            #[cfg(feature = "animate")]
            theme_transition: None,
        }
    }
}

impl Style {
    /// Square modules and finders, the default palette, a 4-module quiet
    /// zone.
    pub fn new() -> Style {
        Style::default()
    }

    /// Sets the data module shape.
    pub fn modules(mut self, shape: ModuleShape) -> Style {
        self.modules = shape;
        self
    }

    /// Sets the finder shapes (SPEC §7.2).
    pub fn finders(mut self, finders: FinderStyle) -> Style {
        self.finders = finders;
        self
    }

    /// Sets how alignment patterns are drawn.
    pub fn alignment(mut self, alignment: AlignmentStyle) -> Style {
        self.alignment = alignment;
        self
    }

    /// Sets how the timing patterns are drawn.
    pub fn timing(mut self, timing: TimingStyle) -> Style {
        self.timing = timing;
        self
    }

    /// Sets the color tokens.
    pub fn palette(mut self, palette: Palette) -> Style {
        self.palette = palette;
        self
    }

    /// Quiet zone width in modules. The default and the standard's minimum
    /// is 4; 2 and 3 render with a warning, less is refused.
    pub fn quiet_zone(mut self, modules: u8) -> Style {
        self.quiet_zone = modules;
        self
    }

    /// Allows light modules on a dark background (with a warning, since
    /// some older scanners can't read them).
    pub fn allow_inverted(mut self, allow: bool) -> Style {
        self.allow_inverted = allow;
        self
    }

    /// Compact output (the default) or one element per line.
    pub fn minify(mut self, minify: bool) -> Style {
        self.minify = minify;
        self
    }

    /// Adds a `<title>` element.
    pub fn title(mut self, title: impl Into<String>) -> Style {
        self.title = Some(title.into());
        self
    }

    /// Adds a `<desc>` element.
    pub fn description(mut self, description: impl Into<String>) -> Style {
        self.description = Some(description.into());
        self
    }

    /// The `aria-label` (default "QR code"). Pass the URL for a link code.
    pub fn label(mut self, label: impl Into<String>) -> Style {
        self.label = Some(label.into());
        self
    }

    /// Prefix for element IDs (gradients). By default it's derived from the
    /// code and palette, so different codes inlined in one page don't
    /// collide. Characters other than ASCII letters, digits, `-` and `_`
    /// are dropped.
    pub fn id_prefix(mut self, prefix: impl Into<String>) -> Style {
        self.id_prefix = Some(prefix.into());
        self
    }

    /// How big the code will be shown, so [`check`](Style::check) can warn
    /// when modules get too small to scan (SPEC §7.8).
    pub fn target_size(mut self, size: TargetSize) -> Style {
        self.target = Some(size);
        self
    }

    /// Sets the frame: square (default), rounded, round with filler, or
    /// none (SPEC §7.4).
    pub fn frame(mut self, frame: Frame) -> Style {
        self.frame = frame;
        self
    }

    /// Seeds the generator behind decorative choices such as filler
    /// modules. By default the seed is a hash of the code's modules, so
    /// the same payload and options always give the same output.
    pub fn seed(mut self, seed: u64) -> Style {
        self.seed = Some(seed);
        self
    }

    /// Puts a logo in the middle (SPEC §7.3). The modules it covers are
    /// counted against the error-correction budget; encode with
    /// [`QrBuilder::encode_text_for`](crate::QrBuilder::encode_text_for)
    /// to get a code big enough for it.
    pub fn logo(mut self, logo: Logo) -> Style {
        self.logo = Some(logo);
        self
    }

    /// Builds the code from a picture (SPEC §7.7). Encode with
    /// [`QrBuilder::encode_text_for`](crate::QrBuilder::encode_text_for)
    /// so the free data bits are steered to match it.
    #[cfg(feature = "halftone")]
    pub fn halftone(mut self, halftone: Halftone) -> Style {
        self.halftone = Some(halftone);
        self
    }

    /// The picture, if any.
    #[cfg(feature = "halftone")]
    pub fn halftone_options(&self) -> Option<&Halftone> {
        self.halftone.as_ref()
    }

    /// Plays an intro when the code appears (SPEC §8). The document
    /// without its animation is still the finished code.
    #[cfg(feature = "animate")]
    pub fn intro(mut self, intro: impl Into<Intro>) -> Style {
        self.intro = Some(intro.into());
        self
    }

    /// Adds an idle loop (SPEC §8.3). Loops are endless, start when the
    /// intro ends, and stop under reduced motion; the static document is
    /// unchanged.
    #[cfg(feature = "animate")]
    pub fn idle(mut self, idle: Idle) -> Style {
        if !self.idle.contains(&idle) {
            self.idle.push(idle);
        }
        self
    }

    /// The idle loops.
    #[cfg(feature = "animate")]
    pub fn idle_loops(&self) -> &[Idle] {
        &self.idle
    }

    /// Gives every solid paint a `transition: fill` of `ms`, so a page
    /// that changes the token custom properties on an inline code (see
    /// [`Palette::to_css_vars`](crate::Palette::to_css_vars)) re-themes it
    /// smoothly (SPEC §6.5). Off under reduced motion; 0–10000 ms.
    #[cfg(feature = "animate")]
    pub fn theme_transition(mut self, ms: u32) -> Style {
        self.theme_transition = Some(ms);
        self
    }

    /// The intro, if any.
    #[cfg(feature = "animate")]
    pub fn intro_options(&self) -> Option<&Intro> {
        self.intro.as_ref()
    }

    /// The logo, if any.
    pub fn logo_options(&self) -> Option<&Logo> {
        self.logo.as_ref()
    }

    /// The frame.
    pub fn frame_style(&self) -> Frame {
        self.frame
    }

    /// The data module shape.
    pub fn module_shape(&self) -> ModuleShape {
        self.modules
    }

    /// The finder shapes.
    pub fn finder_style(&self) -> FinderStyle {
        self.finders
    }

    /// The alignment pattern style.
    pub fn alignment_style(&self) -> AlignmentStyle {
        self.alignment
    }

    /// The timing pattern style.
    pub fn timing_style(&self) -> TimingStyle {
        self.timing
    }

    /// The color tokens.
    pub fn colors(&self) -> &Palette {
        &self.palette
    }

    /// The quiet zone width in modules.
    pub fn quiet_zone_modules(&self) -> u8 {
        self.quiet_zone
    }

    /// Checks the style against `qr` without rendering (SPEC §9).
    pub fn check(&self, qr: &QrMatrix) -> ScanReport {
        check::check(self, qr)
    }

    /// Renders `qr` to an SVG string, or fails if [`check`](Style::check)
    /// finds an error.
    pub fn render(&self, qr: &QrMatrix) -> Result<String, RenderError> {
        let mut out = String::new();
        self.render_to(qr, &mut out)?;
        Ok(out)
    }

    /// Renders into any [`fmt::Write`] sink without building a `String`.
    /// Nothing is written when the check fails.
    pub fn render_to<W: fmt::Write + ?Sized>(
        &self,
        qr: &QrMatrix,
        out: &mut W,
    ) -> Result<(), RenderError> {
        let report = self.check(qr);
        if let Some(error) = report.errors.into_iter().next() {
            return Err(error);
        }
        svg::write(self, qr, None, out).map_err(|_| RenderError::Write)
    }

    /// The animation as data (SPEC §8.5): layers and tracks, for players
    /// other than CSS. Fails like [`render`](Style::render).
    #[cfg(feature = "animate")]
    pub fn plan(&self, qr: &QrMatrix) -> Result<crate::AnimationPlan, RenderError> {
        let report = self.check(qr);
        if let Some(error) = report.errors.into_iter().next() {
            return Err(error);
        }
        Ok(crate::animate::plan(self, qr))
    }

    /// Renders the animation stopped at `t_ms` from its trigger, as a
    /// static SVG with no animation CSS: for thumbnails, video frames and
    /// testing when a code becomes scannable. From the plan's `total_ms`
    /// on it looks like the finished code.
    #[cfg(feature = "animate")]
    pub fn render_frame(&self, qr: &QrMatrix, t_ms: u32) -> Result<String, RenderError> {
        let report = self.check(qr);
        if let Some(error) = report.errors.into_iter().next() {
            return Err(error);
        }
        let mut out = String::new();
        svg::write(self, qr, Some(t_ms), &mut out).map_err(|_| RenderError::Write)?;
        Ok(out)
    }

    /// Renders into a [`std::io::Write`] sink.
    #[cfg(feature = "std")]
    pub fn render_io<W: std::io::Write>(
        &self,
        qr: &QrMatrix,
        mut out: W,
    ) -> Result<(), RenderError> {
        let svg = self.render(qr)?;
        out.write_all(svg.as_bytes())
            .map_err(|_| RenderError::Write)
    }
}
