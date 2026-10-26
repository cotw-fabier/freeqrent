//! `Style` (SPEC §5): everything about how a matrix is drawn, as plain
//! data. Building a style never fails; [`Style::check`] reports problems and
//! [`Style::render`] refuses to draw a code that might not scan.

use alloc::string::String;
use core::fmt;

use crate::check::{self, RenderError, ScanReport};
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
        svg::write(self, qr, out).map_err(|_| RenderError::Write)
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
