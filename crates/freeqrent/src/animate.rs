//! Animation (SPEC §8): intros as data first. A style's intro becomes an
//! [`AnimationPlan`] of layers and tracks; the SVG writer turns the tracks
//! into CSS keyframes, and `render_frame` evaluates them at a fixed time.
//!
//! The document without its animation is the finished code (§8.2). Every
//! keyframe list leaves out its end, so each track ends on the element's
//! own value, and all of it sits in the `@supports` block that renderers
//! without CSS animation skip.
//!
//! Layers are split into stagger groups of whole pieces: a piece is a
//! 4-connected region of one layer (or a single module for shapes whose
//! modules never touch). Every module shape depends only on 4-neighbors in
//! its own layer, so a group of whole pieces has exactly the outline those
//! pieces have in the static drawing, and no two touching edges end up in
//! different elements, which would show anti-aliasing seams.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::{self, Write};

use crate::color::Color;
use crate::geometry::{Grid, Path};
use crate::math;
use crate::matrix::{Corner, QrMatrix};
use crate::num::write_scaled;
use crate::palette::{Palette, Token};
use crate::rng::SplitMix64;
use crate::scene::{self, Canvas, LayerRole, Part};
use crate::style::{ModuleShape, Style};
use crate::svg::{self, Element, Pose};

/// A cubic Bézier timing curve from (0, 0) to (1, 1), as in CSS
/// `cubic-bezier(x1, y1, x2, y2)`. `x1` and `x2` are 0–1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicBezier {
    /// First control point, x.
    pub x1: f32,
    /// First control point, y.
    pub y1: f32,
    /// Second control point, x.
    pub x2: f32,
    /// Second control point, y.
    pub y2: f32,
}

impl CubicBezier {
    /// No easing.
    pub const LINEAR: CubicBezier = CubicBezier::new(0.0, 0.0, 1.0, 1.0);

    /// A curve through the given control points.
    pub const fn new(x1: f32, y1: f32, x2: f32, y2: f32) -> CubicBezier {
        CubicBezier { x1, y1, x2, y2 }
    }

    /// The eased progress at time fraction `x` (0–1).
    pub fn at(self, x: f64) -> f64 {
        if x <= 0.0 {
            return 0.0;
        }
        if x >= 1.0 {
            return 1.0;
        }
        let (x1, y1, x2, y2) = (
            self.x1 as f64,
            self.y1 as f64,
            self.x2 as f64,
            self.y2 as f64,
        );
        let bez = |a: f64, b: f64, t: f64| {
            let u = 1.0 - t;
            3.0 * u * u * t * a + 3.0 * u * t * t * b + t * t * t
        };
        // x(t) is monotonic for x1, x2 in 0–1, so bisection always finds t.
        let (mut lo, mut hi) = (0.0, 1.0);
        for _ in 0..48 {
            let mid = (lo + hi) / 2.0;
            if bez(x1, x2, mid) < x {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        bez(y1, y2, (lo + hi) / 2.0)
    }
}

/// Timing curves (SPEC §8.1): exponential ease-outs only.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Ease {
    /// `cubic-bezier(0.25, 1, 0.5, 1)`, refined. The default.
    Quart,
    /// `cubic-bezier(0.22, 1, 0.36, 1)`, snappier.
    Quint,
    /// `cubic-bezier(0.16, 1, 0.3, 1)`, confident.
    Expo,
    /// Any curve; `x1` and `x2` must be 0–1.
    Custom(CubicBezier),
}

impl Ease {
    /// The curve.
    pub fn curve(self) -> CubicBezier {
        match self {
            Ease::Quart => CubicBezier::new(0.25, 1.0, 0.5, 1.0),
            Ease::Quint => CubicBezier::new(0.22, 1.0, 0.36, 1.0),
            Ease::Expo => CubicBezier::new(0.16, 1.0, 0.3, 1.0),
            Ease::Custom(c) => c,
        }
    }
}

/// The order stagger groups play in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StaggerOrder {
    /// From the center outward.
    Radial,
    /// Top to bottom.
    Linear,
    /// From the top-left corner to the bottom-right.
    Diagonal,
    /// Shuffled by the given seed.
    Random(u64),
    /// Clockwise from twelve o'clock around the center.
    Polar,
}

/// What starts an intro.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Trigger {
    /// The document loading. The default.
    Load,
    /// The pointer over the code; the intro replays on each hover. Inline
    /// SVG only (an `<img>` gets no pointer events).
    Hover,
    /// An ancestor (or the inline `<svg>` itself) gaining this class, for
    /// scroll-into-view without script inside the SVG. Until the class is
    /// added the code waits in its intro state, so a page that never adds
    /// it shows nothing; reduced motion still shows the code.
    Class(String),
}

/// A preset intro (SPEC §8.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Preset {
    /// Groups fade in from 92% scale toward their own centers, in a
    /// shuffled stagger. The default.
    Settle,
    /// Groups fade in from the center outward, each growing from 80%
    /// about the center, so the code spreads like a ripple.
    Ripple,
    /// The finder rings close in from 135% and their eyes pop in 150 ms
    /// later, then the rest settles.
    LockOn,
    /// A soft-edged wipe from the top-left corner to the bottom-right.
    Sweep,
    /// A crisp wipe from top to bottom, like a printer head (left to
    /// right for vertical bars).
    Raster,
    /// A soft-edged circle opening from the center.
    Iris,
    /// Groups converge from seeded offsets of up to 3 modules.
    Assemble,
    /// Like a photo developing: the picture fades in faint, then its
    /// contrast resolves while the finders and boxes come in. Made for
    /// codes with a picture (SPEC §7.7); on others the data modules play
    /// the picture.
    Develop,
}

impl Preset {
    /// Every preset, in documentation order.
    pub const ALL: [Preset; 8] = [
        Preset::Settle,
        Preset::Ripple,
        Preset::LockOn,
        Preset::Sweep,
        Preset::Raster,
        Preset::Iris,
        Preset::Assemble,
        Preset::Develop,
    ];

    /// Whether the preset reveals the code through a moving mask instead
    /// of animating groups.
    pub fn is_reveal(self) -> bool {
        matches!(self, Preset::Sweep | Preset::Raster | Preset::Iris)
    }

    /// The preset's lowercase name, as the CLI spells it.
    pub fn name(self) -> &'static str {
        match self {
            Preset::Settle => "settle",
            Preset::Ripple => "ripple",
            Preset::LockOn => "lock-on",
            Preset::Sweep => "sweep",
            Preset::Raster => "raster",
            Preset::Iris => "iris",
            Preset::Assemble => "assemble",
            Preset::Develop => "develop",
        }
    }
}

/// How the logo and its frame come in (SPEC §8.1), on top of the code's
/// own intro. Each one stays on the logo's layers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LogoIntro {
    /// The logo appears with the code.
    None,
    /// The logo and frame scale 0.94 → 1 with a fade (400 ms). The default.
    Reveal,
    /// The logo and frame land from 112% (450 ms), opaque a third of the
    /// way in.
    Stamp,
    /// A ring frame draws itself around the plate, then the logo fades in
    /// (700 ms). Other frames fade in instead of drawing.
    Draw,
    /// The frame closes in from 130%, like a viewfinder locking on, then
    /// the logo fades in (550 ms).
    Lock,
    /// The logo shows first as a flat silhouette in `fg`, then blooms into
    /// its own colors (800 ms).
    Ink,
    /// The logo, plate and frame fade in as wide as the code, hold there
    /// for the first 15% of the intro, then shrink to their place over the
    /// rest while the code comes in beneath them. It always plays with the
    /// code over the whole intro: the timing doesn't apply.
    Shrink,
    /// The logo and frame turn in from a quarter turn back, growing from
    /// 90% as they fade in (600 ms). With a frame, the frame turns and the
    /// logo fades in inside it once it is a third of the way round.
    Spin,
}

impl LogoIntro {
    /// Every logo intro.
    pub const ALL: [LogoIntro; 8] = [
        LogoIntro::None,
        LogoIntro::Reveal,
        LogoIntro::Stamp,
        LogoIntro::Draw,
        LogoIntro::Lock,
        LogoIntro::Ink,
        LogoIntro::Shrink,
        LogoIntro::Spin,
    ];

    /// The intro's lowercase name, as the CLI spells it.
    pub fn name(self) -> &'static str {
        match self {
            LogoIntro::None => "none",
            LogoIntro::Reveal => "reveal",
            LogoIntro::Stamp => "stamp",
            LogoIntro::Draw => "draw",
            LogoIntro::Lock => "lock",
            LogoIntro::Ink => "ink",
            LogoIntro::Shrink => "shrink",
            LogoIntro::Spin => "spin",
        }
    }

    /// Its length, before it is capped to a share of the whole intro.
    fn ms(self) -> f64 {
        match self {
            LogoIntro::None => 0.0,
            LogoIntro::Reveal => LOGO_MS,
            LogoIntro::Stamp => 450.0,
            LogoIntro::Draw => 700.0,
            LogoIntro::Lock => 550.0,
            LogoIntro::Ink => 800.0,
            LogoIntro::Spin => 600.0,
            // The whole intro, whatever its length.
            LogoIntro::Shrink => f64::INFINITY,
        }
    }
}

/// When the logo's intro plays against the code's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LogoTiming {
    /// Last: the code finishes halfway through it. The default.
    After,
    /// First: the code starts halfway through it.
    Before,
    /// In the middle of the code's intro.
    With,
}

impl LogoTiming {
    /// Every timing.
    pub const ALL: [LogoTiming; 3] = [LogoTiming::After, LogoTiming::Before, LogoTiming::With];

    /// The timing's lowercase name, as the CLI spells it.
    pub fn name(self) -> &'static str {
        match self {
            LogoTiming::After => "after",
            LogoTiming::Before => "before",
            LogoTiming::With => "with",
        }
    }
}

/// A code's entrance: a preset and its timing. Build one with
/// [`Intro::new`] or from a [`Preset`].
#[derive(Clone, PartialEq)]
pub struct Intro {
    pub(crate) preset: Preset,
    /// False for a palette shift alone.
    pub(crate) entrance: bool,
    pub(crate) palette_from: Option<Palette>,
    pub(crate) duration_ms: u32,
    pub(crate) delay_ms: u32,
    pub(crate) ease: Option<Ease>,
    pub(crate) stagger: Option<StaggerOrder>,
    pub(crate) groups: u8,
    pub(crate) trigger: Trigger,
    pub(crate) logo: LogoIntro,
    pub(crate) logo_timing: LogoTiming,
    /// The frame's own intro and timing, when it is cut loose from the
    /// logo's; the timing defaults to the logo's.
    pub(crate) frame: Option<LogoIntro>,
    pub(crate) frame_timing: Option<LogoTiming>,
}

// By hand, so the logo's intro shows only when it isn't the default: the
// SVG writer hashes this text into the document's ID prefix, and codes
// made before logo intros existed keep theirs.
impl fmt::Debug for Intro {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = f.debug_struct("Intro");
        d.field("preset", &self.preset)
            .field("entrance", &self.entrance)
            .field("palette_from", &self.palette_from)
            .field("duration_ms", &self.duration_ms)
            .field("delay_ms", &self.delay_ms)
            .field("ease", &self.ease)
            .field("stagger", &self.stagger)
            .field("groups", &self.groups)
            .field("trigger", &self.trigger)
            .field("logo_reveal", &(self.logo != LogoIntro::None));
        if !matches!(self.logo, LogoIntro::None | LogoIntro::Reveal) {
            d.field("logo", &self.logo);
        }
        if self.logo_timing != LogoTiming::After {
            d.field("logo_timing", &self.logo_timing);
        }
        if let Some(frame) = self.frame {
            d.field("frame", &frame);
        }
        if let Some(timing) = self.frame_timing {
            d.field("frame_timing", &timing);
        }
        d.finish()
    }
}

impl From<Preset> for Intro {
    fn from(preset: Preset) -> Intro {
        Intro::new(preset)
    }
}

impl Intro {
    /// The preset with its defaults: 1200 ms in all, 12 groups, the
    /// preset's own curve and stagger, on load, with the logo revealed
    /// last.
    pub fn new(preset: Preset) -> Intro {
        Intro {
            preset,
            entrance: true,
            palette_from: None,
            duration_ms: 1200,
            delay_ms: 0,
            ease: None,
            stagger: None,
            groups: 12,
            trigger: Trigger::Load,
            logo: LogoIntro::Reveal,
            logo_timing: LogoTiming::After,
            frame: None,
            frame_timing: None,
        }
    }

    /// A palette shift (SPEC §6.5): every solid token cross-fades from its
    /// value in `old` to the style's, in OKLab, over 600 ms with ease-out
    /// quart; the code itself doesn't move. Tokens painted with a gradient
    /// or a different alpha in either palette switch at once. Under
    /// reduced motion the swap is instant.
    pub fn palette_from(old: &Palette) -> Intro {
        Intro {
            entrance: false,
            palette_from: Some(old.clone()),
            duration_ms: 600,
            ..Intro::new(Preset::Settle)
        }
    }

    /// The whole intro's length; the stagger is compressed to fit. Over
    /// 2000 ms raises a warning, since the code should scan soon after it
    /// appears. 100–10000.
    pub fn duration_ms(mut self, ms: u32) -> Intro {
        self.duration_ms = ms;
        self
    }

    /// Time before the intro starts, 0–10000 ms.
    pub fn delay_ms(mut self, ms: u32) -> Intro {
        self.delay_ms = ms;
        self
    }

    /// The timing curve, instead of the preset's own.
    pub fn ease(mut self, ease: Ease) -> Intro {
        self.ease = Some(ease);
        self
    }

    /// The stagger order, instead of the preset's own.
    pub fn stagger(mut self, order: StaggerOrder) -> Intro {
        self.stagger = Some(order);
        self
    }

    /// How many stagger groups, 2–16 (default 12). Each group is one
    /// element per layer.
    pub fn groups(mut self, groups: u8) -> Intro {
        self.groups = groups;
        self
    }

    /// What starts the intro.
    pub fn trigger(mut self, trigger: Trigger) -> Intro {
        self.trigger = trigger;
        self
    }

    /// Whether the logo scales in after the modules (default on): a
    /// shorthand for [`LogoIntro::Reveal`] or [`LogoIntro::None`].
    pub fn logo_reveal(mut self, reveal: bool) -> Intro {
        self.logo = if reveal {
            LogoIntro::Reveal
        } else {
            LogoIntro::None
        };
        self
    }

    /// How the logo and its frame come in (default [`LogoIntro::Reveal`]).
    pub fn logo(mut self, logo: LogoIntro) -> Intro {
        self.logo = logo;
        self
    }

    /// When the logo's intro plays (default [`LogoTiming::After`]).
    pub fn logo_timing(mut self, timing: LogoTiming) -> Intro {
        self.logo_timing = timing;
        self
    }

    /// Cuts the logo's frame loose from the logo's intro: the frame plays
    /// `frame` on its own, at [`Intro::frame_timing`] (default: the
    /// logo's timing), while the plate and logo play theirs as if they had
    /// no frame. So a frame can draw itself in as a Shrink lands, or spin
    /// in around a logo that only fades. `LogoIntro::None` here holds the
    /// frame still while the logo moves. No effect without a frame.
    pub fn frame(mut self, frame: LogoIntro) -> Intro {
        self.frame = Some(frame);
        self
    }

    /// When a cut-loose frame's intro plays against the code's. Setting it
    /// cuts the frame loose too, with the logo's own intro.
    pub fn frame_timing(mut self, timing: LogoTiming) -> Intro {
        self.frame_timing = Some(timing);
        self
    }

    /// The logo's intro.
    pub fn logo_intro(&self) -> LogoIntro {
        self.logo
    }

    /// The frame's own intro, when it is cut loose from the logo's.
    pub fn frame_intro(&self) -> Option<LogoIntro> {
        self.frame.or(self.frame_timing.map(|_| self.logo))
    }

    /// The entrance preset, or `None` for a palette shift.
    pub fn entrance(&self) -> Option<Preset> {
        self.entrance.then_some(self.preset)
    }

    /// The palette a palette shift starts from.
    pub fn palette_shift(&self) -> Option<&Palette> {
        self.palette_from.as_ref()
    }

    /// The whole length, in milliseconds.
    pub fn total_ms(&self) -> u32 {
        self.duration_ms
    }

    /// The number of stagger groups.
    pub fn group_count(&self) -> u8 {
        self.groups
    }

    /// The timing curve: the chosen one, or the preset's.
    pub fn curve(&self) -> CubicBezier {
        self.ease
            .unwrap_or(match self.preset {
                Preset::Settle | Preset::Sweep | Preset::Raster | Preset::Develop => Ease::Quart,
                Preset::Ripple => Ease::Quint,
                Preset::LockOn | Preset::Iris | Preset::Assemble => Ease::Expo,
            })
            .curve()
    }

    fn order(&self) -> StaggerOrder {
        self.stagger.unwrap_or(match self.preset {
            Preset::Ripple | Preset::Iris => StaggerOrder::Radial,
            Preset::Sweep => StaggerOrder::Diagonal,
            Preset::Raster => StaggerOrder::Linear,
            Preset::Settle | Preset::LockOn | Preset::Assemble | Preset::Develop => {
                StaggerOrder::Random(0)
            }
        })
    }
}

/// Idle loops (SPEC §8.3): opt-in, endless, starting when the intro ends,
/// and off under reduced motion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Idle {
    /// The finder eyes swell to 106% and back every 4 s.
    FinderPulse,
    /// A faint highlight band crosses the code every 6 s.
    Shimmer,
    /// The background's lightness drifts 2% away from the ink and back
    /// every 8 s, so contrast only ever grows.
    Breathe,
    /// A highlight crosses the logo every 7 s, clipped to its plate.
    LogoGlint,
    /// The logo's frame turns: an Orbit's moon goes round every 12 s,
    /// other frames once every 24 s. Only round frames may turn
    /// ([`RenderError::TurnNeedsRoundFrame`](crate::RenderError::TurnNeedsRoundFrame)).
    LogoTurn,
    /// Every 8 s the logo turns once in its plane (1.2 s, ease-in-out),
    /// inside a window that stays put, then rests.
    LogoSpin,
    /// Every 8 s the logo flips over once about its horizontal axis, like
    /// a coin (1 s), then rests. With [`Idle::LogoSpin`] the two take
    /// turns, half the loop apart.
    LogoFlip,
}

impl Idle {
    /// Every idle loop.
    pub const ALL: [Idle; 7] = [
        Idle::FinderPulse,
        Idle::Shimmer,
        Idle::Breathe,
        Idle::LogoGlint,
        Idle::LogoTurn,
        Idle::LogoSpin,
        Idle::LogoFlip,
    ];

    /// The loop's lowercase name, as the CLI spells it.
    pub fn name(self) -> &'static str {
        match self {
            Idle::FinderPulse => "finder-pulse",
            Idle::Shimmer => "shimmer",
            Idle::Breathe => "breathe",
            Idle::LogoGlint => "logo-glint",
            Idle::LogoTurn => "logo-turn",
            Idle::LogoSpin => "logo-spin",
            Idle::LogoFlip => "logo-flip",
        }
    }
}

/// What a keyframe sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Property {
    /// Opacity, 0–1.
    Opacity,
    /// Uniform scale about the layer's origin.
    Scale,
    /// Horizontal offset, in modules.
    TranslateX,
    /// Vertical offset, in modules.
    TranslateY,
    /// The fill color.
    Fill,
    /// How far the reveal mask has opened, 0–1 (1 shows everything).
    Reveal,
    /// Clockwise turn about the layer's origin, in degrees.
    Rotate,
    /// How much of an outline is drawn from its start, 0–1
    /// ([`PlanLayer::outline`]).
    Draw,
    /// Vertical scale on top of [`Property::Scale`], about the layer's
    /// origin; negative mirrors the layer top to bottom.
    ScaleY,
}

impl Property {
    fn is_transform(self) -> bool {
        matches!(
            self,
            Property::Scale
                | Property::TranslateX
                | Property::TranslateY
                | Property::Rotate
                | Property::ScaleY
        )
    }

    fn name(self) -> &'static str {
        match self {
            Property::Opacity => "opacity",
            Property::Scale => "scale",
            Property::TranslateX => "translateX",
            Property::TranslateY => "translateY",
            Property::Fill => "fill",
            Property::Reveal => "reveal",
            Property::Rotate => "rotate",
            Property::Draw => "draw",
            Property::ScaleY => "scaleY",
        }
    }
}

/// A keyframe value.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Value {
    /// A number.
    Number(f32),
    /// A color.
    Color(Color),
}

/// One property's value at one point of a track.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct Keyframe {
    /// Where in the track, 0–1. Offsets 0 and 1 left out of a track take
    /// the layer's natural value.
    pub offset: f32,
    /// The property.
    pub property: Property,
    /// Its value.
    pub value: Value,
}

/// What a track moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Target {
    /// A layer, by index into [`AnimationPlan::layers`].
    Layer(usize),
    /// The plan's [`Reveal`] mask.
    Reveal,
}

/// The shape of a reveal mask.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum RevealShape {
    /// A straight edge moving from `from` to `to`: at progress 0 the
    /// feather lies wholly before `from`, at 1 wholly past `to`.
    Wipe {
        /// Where the edge starts, in viewBox units.
        from: (f32, f32),
        /// Where it ends.
        to: (f32, f32),
    },
    /// A circle about `center` growing from nothing to `radius` plus the
    /// feather.
    Iris {
        /// The center, in viewBox units.
        center: (f32, f32),
        /// The radius at which everything is shown.
        radius: f32,
    },
}

/// A mask over a run of layers, opened by a [`Property::Reveal`] track
/// (presets Sweep, Raster and Iris).
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct Reveal {
    /// Its shape.
    pub shape: RevealShape,
    /// Width of the soft edge, in modules.
    pub feather: f32,
    /// The first and last layer it covers (inclusive).
    pub layers: (usize, usize),
}

impl Reveal {
    /// How far the mask's content moves between closed and open: the
    /// wipe's travel along its direction, or the iris's full radius.
    fn travel(&self) -> f64 {
        match self.shape {
            RevealShape::Wipe { from, to } => {
                let (dx, dy) = ((to.0 - from.0) as f64, (to.1 - from.1) as f64);
                math::sqrt(dx * dx + dy * dy) + self.feather as f64
            }
            RevealShape::Iris { radius, .. } => radius as f64 + self.feather as f64,
        }
    }
}

/// One animation of one layer.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Track {
    /// What it moves.
    pub target: Target,
    /// Start, from the trigger.
    pub delay_ms: u32,
    /// Length of one iteration.
    pub duration_ms: u32,
    /// Applied between each pair of neighboring keyframes, as CSS does.
    pub ease: CubicBezier,
    /// Iterations, or `None` for an endless loop. A finite track shows
    /// its first keyframe while it waits (CSS `backwards`); an endless one
    /// doesn't touch the layer until it starts.
    pub iterations: Option<u32>,
    /// The keyframes. Every track ends on the layer's natural value.
    pub keys: Vec<Keyframe>,
}

/// What a player shows when motion is off.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Fallback {
    /// The finished code (always, SPEC §8.2).
    FinalState,
}

/// One drawn layer of a plan.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct PlanLayer {
    /// What it is.
    pub role: LayerRole,
    /// Its paint, looked up in [`AnimationPlan::palette`].
    pub token: Token,
    /// A fixed color that replaces the token's paint: a picture's colors
    /// (the token then says only whether the band is dark, `fg`, or light,
    /// `bg`).
    pub color: Option<Color>,
    /// SVG path data in viewBox units (modules). For the logo, its plate;
    /// the logo's content is drawn from the style.
    pub path: String,
    /// For a picture's sub-cells: the path is in fifths of a module, from
    /// this point (the code's top-left corner) in viewBox units.
    pub cells: Option<(f32, f32)>,
    /// For a picture's sub-cells: the path is horizontal lines, stroked
    /// this wide (in the path's units, butt caps) instead of filled.
    pub stroke: Option<f32>,
    /// For a stroked logo frame: the path is stroked instead of filled.
    pub outline: Option<PlanOutline>,
    /// Its opacity at rest: 1, or 0 for a layer the intro only flashes
    /// ([`LayerRole::LogoInk`]).
    pub opacity: f32,
    /// Whether the path uses the even-odd fill rule.
    pub evenodd: bool,
    /// The stagger group, for split layers.
    pub group: Option<u8>,
    /// The finder corner, for a finder layer.
    pub corner: Option<Corner>,
    /// Scale origin, in viewBox units.
    pub origin: (f32, f32),
}

/// A logo frame stroked along its path (SPEC §7.3).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct PlanOutline {
    /// Stroke width, in viewBox units.
    pub width: f32,
    /// The path's length, in viewBox units: what [`Property::Draw`] 1
    /// draws.
    pub length: f32,
    /// Dash and gap lengths (empty for a solid stroke).
    pub dashes: Vec<f32>,
    /// Where the dash pattern starts along the path.
    pub dash_offset: f32,
}

/// A style's animation as data (SPEC §8.5): the static layers and the
/// tracks that move them. The SVG writer serializes it to CSS; other
/// players (the Flutter widget in Phase 2) read the same plan.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct AnimationPlan {
    /// Width and height of the viewBox, in modules.
    pub view_box: (f32, f32),
    /// The paints the layers' tokens resolve through.
    pub palette: Palette,
    /// The layers, in paint order.
    pub layers: Vec<PlanLayer>,
    /// The tracks.
    pub tracks: Vec<Track>,
    /// The reveal mask, for presets that wipe the code in.
    pub reveal: Option<Reveal>,
    /// When the last finite track ends, from the trigger.
    pub total_ms: u32,
    /// What starts the tracks.
    pub trigger: Trigger,
    /// What to show with motion off.
    pub reduced_motion: Fallback,
}

/// The animation part of a document: its tracks, by element index.
#[derive(Clone, Debug)]
pub(crate) struct Motion {
    pub(crate) tracks: Vec<Track>,
    pub(crate) origins: Vec<(f64, f64)>,
    /// Each element's dash length, for outlines that draw on.
    pub(crate) dashes: Vec<Option<f64>>,
    /// Elements drawn inside the element before them, which carries their
    /// motion: their tracks get no CSS of their own.
    pub(crate) joined: Vec<bool>,
    pub(crate) reveal: Option<Reveal>,
    pub(crate) total_ms: u32,
    pub(crate) trigger: Trigger,
    /// For a frame: the time it shows. Frames get poses, not CSS.
    pub(crate) at: Option<u32>,
}

// ------------------------------------------------------------- grouping

/// A unit of a layer that is never split: a region, a module, a finder,
/// an alignment pattern or the ring.
#[derive(Clone, Debug)]
enum Unit {
    /// Cells of a grid layer.
    Cells(Vec<(u16, u16)>),
    Corner(Corner),
    Center((u16, u16)),
    Ring,
}

/// A picture splits into square tiles of this many modules a side, each
/// one a piece of the stagger like a run of modules (cut on module edges,
/// so the strokes meet exactly).
const PICTURE_TILE: u16 = 3;

#[derive(Clone, Debug)]
struct Piece {
    layer: usize,
    unit: Unit,
    /// Bounding box in canvas modules: (x0, y0, x1, y1).
    bounds: (f64, f64, f64, f64),
    group: u8,
}

/// Whether neighboring modules of `shape` touch, so a region must stay in
/// one element.
fn touches(shape: &ModuleShape) -> bool {
    match *shape {
        ModuleShape::Square { gap } => crate::geometry::units(gap) == 0,
        ModuleShape::Dots { .. } => false,
        ModuleShape::Rounded { .. } | ModuleShape::Liquid { .. } | ModuleShape::Bars { .. } => true,
    }
}

/// The 4-connected regions of `grid` (or its single cells), row-major.
fn regions(grid: &Grid, size: usize, connected: bool) -> Vec<Vec<(u16, u16)>> {
    let mut seen = alloc::vec![false; size * size];
    let mut out = Vec::new();
    let mut stack = Vec::new();
    for start in 0..size * size {
        let (sx, sy) = ((start % size) as i64, (start / size) as i64);
        if seen[start] || !grid.get(sx, sy) {
            continue;
        }
        seen[start] = true;
        if !connected {
            out.push(alloc::vec![(sx as u16, sy as u16)]);
            continue;
        }
        let mut cells = Vec::new();
        stack.push((sx, sy));
        while let Some((x, y)) = stack.pop() {
            cells.push((x as u16, y as u16));
            for (nx, ny) in [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)] {
                if grid.get(nx, ny) {
                    let i = ny as usize * size + nx as usize;
                    if !seen[i] {
                        seen[i] = true;
                        stack.push((nx, ny));
                    }
                }
            }
        }
        cells.sort_unstable_by_key(|&(x, y)| (y, x));
        out.push(cells);
    }
    out
}

/// Every piece of the style's layers, grouped by `intro`'s order.
fn pieces(
    style: &Style,
    qr: &QrMatrix,
    canvas: &Canvas,
    intro: &Intro,
    layers: &[scene::Layer],
) -> Vec<Piece> {
    let o = canvas.origin as f64;
    let n = qr.size();
    let mut out = Vec::new();
    for (li, layer) in layers.iter().enumerate() {
        let mut push = |unit: Unit, bounds| {
            out.push(Piece {
                layer: li,
                unit,
                bounds,
                group: 0,
            })
        };
        let cells = |grid: &Grid, size: usize, offset: f64, connected: bool| {
            regions(grid, size, connected)
                .into_iter()
                .map(move |cells| {
                    let mut b = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
                    for &(x, y) in &cells {
                        let (x, y) = (x as f64 + offset, y as f64 + offset);
                        b = (b.0.min(x), b.1.min(y), b.2.max(x + 1.0), b.3.max(y + 1.0));
                    }
                    (cells, b)
                })
        };
        match &layer.part {
            Part::Modules(grid) => {
                for (c, b) in cells(grid, n as usize, o, touches(&style.modules)) {
                    push(Unit::Cells(c), b);
                }
            }
            Part::Filler(grid) => {
                for (c, b) in cells(grid, canvas.side as usize, 0.0, touches(&style.modules)) {
                    push(Unit::Cells(c), b);
                }
            }
            Part::Timing(grid) => {
                for (c, b) in cells(grid, n as usize, o, false) {
                    push(Unit::Cells(c), b);
                }
            }
            Part::FinderOuter(corners) | Part::FinderEye(corners) => {
                for &c in corners {
                    let (x, y) = scene::finder_origin(c, n);
                    let (x, y) = (o + x as f64, o + y as f64);
                    push(Unit::Corner(c), (x, y, x + 7.0, y + 7.0));
                }
            }
            Part::Alignment(centers) => {
                for &(cx, cy) in centers {
                    let (x, y) = (o + cx as f64 - 2.0, o + cy as f64 - 2.0);
                    push(Unit::Center((cx, cy)), (x, y, x + 5.0, y + 5.0));
                }
            }
            Part::Ring => {
                let s = canvas.side as f64;
                push(Unit::Ring, (0.0, 0.0, s, s));
            }
            Part::Halftone => {
                for ty in (0..n).step_by(PICTURE_TILE as usize) {
                    for tx in (0..n).step_by(PICTURE_TILE as usize) {
                        let (x1, y1) = ((tx + PICTURE_TILE).min(n), (ty + PICTURE_TILE).min(n));
                        let cells = (ty..y1).flat_map(|y| (tx..x1).map(move |x| (x, y)));
                        push(
                            Unit::Cells(cells.collect()),
                            (o + tx as f64, o + ty as f64, o + x1 as f64, o + y1 as f64),
                        );
                    }
                }
            }
        }
    }

    // Stagger keys from each piece's center, binned into equal ranges.
    let mid = canvas.side as f64 / 2.0;
    let seed = scene::seed(style, qr);
    let order = intro.order();
    let key = |p: &Piece| {
        let (x0, y0, x1, y1) = p.bounds;
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        match order {
            StaggerOrder::Radial if matches!(p.unit, Unit::Ring) => mid * core::f64::consts::SQRT_2,
            StaggerOrder::Radial => math::sqrt((cx - mid) * (cx - mid) + (cy - mid) * (cy - mid)),
            StaggerOrder::Linear => cy,
            StaggerOrder::Diagonal => cx + cy,
            StaggerOrder::Polar => {
                // Clockwise from twelve o'clock, 0 to 2π.
                let a = math::atan2(cx - mid, mid - cy);
                if a < 0.0 {
                    a + 2.0 * core::f64::consts::PI
                } else {
                    a
                }
            }
            StaggerOrder::Random(s) => {
                // From the center, so a finder's ring and eye share a group.
                let h = ((cx * 2.0) as u64) << 32 | (cy * 2.0) as u64;
                SplitMix64::new(seed ^ s ^ h.wrapping_mul(0x9e37_79b9_7f4a_7c15)).next_f64()
            }
        }
    };
    let keys: Vec<f64> = out.iter().map(key).collect();
    let (lo, hi) = match order {
        StaggerOrder::Random(_) => (0.0, 1.0),
        _ => keys
            .iter()
            .fold((f64::MAX, f64::MIN), |(lo, hi), &k| (lo.min(k), hi.max(k))),
    };
    let k = intro.groups.clamp(1, 16) as f64;
    for (p, key) in out.iter_mut().zip(keys) {
        let t = if hi > lo { (key - lo) / (hi - lo) } else { 0.0 };
        p.group = (math::floor(t * k) as i64).clamp(0, k as i64 - 1) as u8;
    }
    out
}

/// How many elements `style`'s intro animates, without drawing them.
pub(crate) fn element_count(style: &Style, qr: &QrMatrix) -> usize {
    let Some(intro) = &style.intro else {
        return 0;
    };
    let canvas = scene::canvas(style, qr);
    let logo = svg::logo_elements(style, qr, &canvas).len();
    if intro.entrance().is_none_or(|p| p.is_reveal()) {
        return 1 + logo;
    }
    if intro.entrance() == Some(Preset::Develop) {
        return svg::static_elements(style, qr, &canvas)
            .iter()
            .filter(|e| e.role != LayerRole::Background)
            .count();
    }
    let layers = scene::layers(style, qr);
    let mut seen: Vec<(usize, u8, Option<Corner>)> = pieces(style, qr, &canvas, intro, &layers)
        .iter()
        .map(|p| (p.layer, p.group, corner_of(&p.unit)))
        .collect();
    seen.sort_unstable_by_key(|&(l, g, c)| (l, g, c.map(|c| c as u8)));
    seen.dedup();
    seen.len() + logo
}

fn corner_of(unit: &Unit) -> Option<Corner> {
    match unit {
        Unit::Corner(c) => Some(*c),
        _ => None,
    }
}

/// The split elements of an animated style: background, each layer's
/// groups in order (finders one corner each), then the logo.
fn split(style: &Style, qr: &QrMatrix, canvas: &Canvas, intro: &Intro) -> Vec<Element> {
    let layers = scene::layers(style, qr);
    let pieces = pieces(style, qr, canvas, intro, &layers);
    let mut elements = Vec::new();
    let mid = canvas.side as f64 / 2.0;
    if svg::draws_background(style) {
        let mut bg = Element::new(
            LayerRole::Background,
            Token::Bg,
            false,
            svg::background_path(style, canvas),
        );
        bg.origin = (mid, mid);
        elements.push(bg);
    }
    let n = qr.size() as usize;
    #[cfg(feature = "halftone")]
    let picture = svg::halftone_cells(style, qr);
    for (li, layer) in layers.iter().enumerate() {
        let mine: Vec<&Piece> = pieces.iter().filter(|p| p.layer == li).collect();
        // Finders stay one element per corner, so each scales about its
        // own center.
        let mut keys: Vec<(u8, Option<Corner>)> =
            mine.iter().map(|p| (p.group, corner_of(&p.unit))).collect();
        keys.sort_unstable_by_key(|&(g, c)| (g, c.map(|c| c as u8)));
        keys.dedup();
        for (group, corner) in keys {
            let chosen: Vec<&&Piece> = mine
                .iter()
                .filter(|p| p.group == group && corner_of(&p.unit) == corner)
                .collect();
            let b = chosen
                .iter()
                .fold((f64::MAX, f64::MAX, f64::MIN, f64::MIN), |b, p| {
                    (
                        b.0.min(p.bounds.0),
                        b.1.min(p.bounds.1),
                        b.2.max(p.bounds.2),
                        b.3.max(p.bounds.3),
                    )
                });
            #[cfg(feature = "halftone")]
            if let (Part::Halftone, Some(cells)) = (&layer.part, &picture) {
                // This group's tiles of the picture.
                let mut grid = Grid::new(n);
                for p in &chosen {
                    if let Unit::Cells(c) = &p.unit {
                        for &(x, y) in c {
                            grid.set(x as usize, y as usize);
                        }
                    }
                }
                for mut e in svg::halftone_strokes(cells, Some(&grid)) {
                    e.group = Some(group);
                    e.origin = ((b.0 + b.2) / 2.0, (b.1 + b.3) / 2.0);
                    elements.push(e);
                }
                continue;
            }
            if matches!(layer.part, Part::Halftone) {
                continue;
            }
            let part = match &layer.part {
                Part::Modules(_) | Part::Filler(_) | Part::Timing(_) => {
                    let size = if matches!(layer.part, Part::Filler(_)) {
                        canvas.side as usize
                    } else {
                        n
                    };
                    let mut grid = Grid::new(size);
                    for p in &chosen {
                        if let Unit::Cells(cells) = &p.unit {
                            for &(x, y) in cells {
                                grid.set(x as usize, y as usize);
                            }
                        }
                    }
                    match &layer.part {
                        Part::Modules(_) => Part::Modules(grid),
                        Part::Filler(_) => Part::Filler(grid),
                        _ => Part::Timing(grid),
                    }
                }
                Part::FinderOuter(_) => Part::FinderOuter(corner.into_iter().collect()),
                Part::FinderEye(_) => Part::FinderEye(corner.into_iter().collect()),
                Part::Alignment(_) => Part::Alignment(
                    chosen
                        .iter()
                        .filter_map(|p| match p.unit {
                            Unit::Center(c) => Some(c),
                            _ => None,
                        })
                        .collect(),
                ),
                Part::Ring => Part::Ring,
                Part::Halftone => Part::Halftone,
            };
            let mut path = Path::new();
            svg::draw_part(style, qr, canvas, &part, &mut path);
            if path.is_empty() {
                continue;
            }
            let mut e = Element::new(layer.role(), layer.token, layer.part.evenodd(), path);
            e.group = Some(group);
            e.corner = corner;
            e.origin = ((b.0 + b.2) / 2.0, (b.1 + b.3) / 2.0);
            elements.push(e);
        }
    }
    elements.extend(svg::logo_elements(style, qr, canvas));
    elements
}

// --------------------------------------------------------------- timing

/// Longest time one element animates.
const MAX_ELEMENT_MS: f64 = 800.0;
/// Develop: how faint the picture is when the rest starts coming in, and
/// when that is, as a share of the intro.
const DEVELOP_FAINT: f32 = 0.35;
const DEVELOP_FAINT_AT: f64 = 0.45;
/// The logo's reveal, at most.
const LOGO_MS: f64 = 400.0;

fn number(property: Property, offset: f32, value: f32) -> Keyframe {
    Keyframe {
        offset,
        property,
        value: Value::Number(value),
    }
}

/// When group `g` of `k` starts and how long it runs, staggered across
/// `span` ms from `start`.
fn stagger(start: f64, span: f64, k: f64, g: f64) -> (f64, f64) {
    let each = (span / 2.0).min(MAX_ELEMENT_MS);
    let step = if k > 1.0 {
        (span - each) / (k - 1.0)
    } else {
        0.0
    };
    (start + g * step, each)
}

/// How far each group of Assemble starts from home: a seeded direction,
/// 1.5–3 modules away.
fn scatter(seed: u64, group: u8) -> (f32, f32) {
    let mut rng = SplitMix64::new(seed ^ (group as u64 + 1).wrapping_mul(0xd1b5_4a32_d192_ed03));
    let angle = rng.next_f64() * 2.0 * core::f64::consts::PI;
    let reach = 1.5 + 1.5 * rng.next_f64();
    let (sin, cos) = math::sin_cos(angle);
    let k = |v: f64| (math::round(v * reach * 1000.0) / 1000.0) as f32;
    (k(cos), k(sin))
}

/// The tracks of `intro` for `elements`.
fn tracks(
    style: &Style,
    qr: &QrMatrix,
    intro: &Intro,
    seed: u64,
    elements: &[Element],
) -> Vec<Track> {
    let Some(preset) = intro.entrance() else {
        return Vec::new();
    };
    let total = intro.duration_ms as f64;
    let logo_intro = if style.logo.is_some() {
        intro.logo
    } else {
        LogoIntro::None
    };
    let has_frame = elements.iter().any(|e| e.role == LayerRole::LogoFrame);
    // A frame cut loose plays its own intro; the logo then plays as if it
    // had none.
    let cut = intro
        .frame_intro()
        .filter(|_| has_frame && style.logo.is_some());
    let length = |intro: LogoIntro| match intro {
        LogoIntro::None => 0.0,
        LogoIntro::Reveal => LOGO_MS.min(total * 0.4),
        LogoIntro::Shrink => total,
        other => other.ms().min(total * 0.6),
    };
    let logo_ms = length(logo_intro);
    let frame_ms = cut.map_or(0.0, length);
    let frame_timing = intro.frame_timing.unwrap_or(intro.logo_timing);
    // Where an intro of `ms` starts. Shrink always runs the whole intro.
    let start_of = |intro: LogoIntro, ms: f64, timing| match timing {
        _ if intro == LogoIntro::Shrink => 0.0,
        LogoTiming::After => total - ms,
        LogoTiming::Before => 0.0,
        LogoTiming::With => (total - ms) / 2.0,
    };
    let logo_at = start_of(logo_intro, logo_ms, intro.logo_timing);
    let frame_at = cut.map_or(0.0, |f| start_of(f, frame_ms, frame_timing));
    // The code's own stretch: it overlaps the logo's and the frame's
    // intros by half.
    let (mut code_start, mut code_end) = (0.0f64, total);
    for (intro, ms, timing) in [
        (logo_intro, logo_ms, intro.logo_timing),
        (cut.unwrap_or(LogoIntro::None), frame_ms, frame_timing),
    ] {
        match timing {
            _ if ms == 0.0 || intro == LogoIntro::Shrink => {}
            LogoTiming::After => code_end = code_end.min(total - ms / 2.0),
            LogoTiming::Before => code_start = code_start.max(ms / 2.0),
            LogoTiming::With => {}
        }
    }
    let window = code_end - code_start;
    let k = intro.groups.clamp(1, 16) as f64;
    let ease = intro.curve();
    let delay = intro.delay_ms as f64;
    let ms = |v: f64| math::round(v) as u32;
    let track = |target, at: f64, length: f64, keys| Track {
        target,
        delay_ms: ms(delay + at),
        // The end rounded, not the length, so nothing ends past the intro.
        duration_ms: ms(delay + at + length) - ms(delay + at),
        ease,
        iterations: Some(1),
        keys,
    };
    let fade_scale = |scale| {
        alloc::vec![
            number(Property::Opacity, 0.0, 0.0),
            number(Property::Scale, 0.0, scale),
        ]
    };

    // Shrink starts the plate as wide as the code.
    let plate = 2.0 * plate_half(style, qr);
    let wide = if plate > 0.0 {
        (qr.size() as f64 / plate).max(1.0) as f32
    } else {
        1.0
    };
    let logo_track = |i: usize, e: &Element| {
        let drawable = e.outline.as_ref().is_some_and(|o| o.dashes.is_empty());
        let (intro, start, ms) = match cut {
            Some(f) if e.role == LayerRole::LogoFrame => (f, frame_at, frame_ms),
            _ => (logo_intro, logo_at, logo_ms),
        };
        logo_keys(intro, e.role, has_frame && cut.is_none(), drawable, wide)
            .map(|(at, length, keys)| track(Target::Layer(i), start + at * ms, length * ms, keys))
    };

    let mut tracks = Vec::new();
    if preset.is_reveal() {
        tracks.push(track(
            Target::Reveal,
            code_start,
            window,
            alloc::vec![number(Property::Reveal, 0.0, 0.0)],
        ));
    }
    if preset == Preset::Develop {
        // The picture: the halftone, or the data modules of a code without
        // one.
        let pictured = elements.iter().any(|e| e.role == LayerRole::Halftone);
        let picture = |role| {
            role == LayerRole::Halftone
                || (!pictured && matches!(role, LayerRole::Data | LayerRole::Filler))
        };
        let faint = code_start + window * DEVELOP_FAINT_AT;
        for (i, e) in elements.iter().enumerate() {
            let target = Target::Layer(i);
            match e.role {
                LayerRole::Background | LayerRole::Shimmer => {}
                role if role.is_logo() => tracks.extend(logo_track(i, e)),
                role if picture(role) => tracks.push(track(
                    target,
                    code_start,
                    window,
                    alloc::vec![
                        number(Property::Opacity, 0.0, 0.0),
                        number(Property::Opacity, DEVELOP_FAINT_AT as f32, DEVELOP_FAINT),
                    ],
                )),
                _ => tracks.push(track(
                    target,
                    faint,
                    code_end - faint,
                    alloc::vec![number(Property::Opacity, 0.0, 0.0)],
                )),
            }
        }
        return tracks;
    }
    // LockOn's finders take the first stretch; the rest settles after.
    let finder_ms = (total * 0.4).min(500.0);
    let (start, span) = match preset {
        Preset::LockOn => (code_start + finder_ms * 0.6, window - finder_ms * 0.6),
        _ => (code_start, window),
    };
    for (i, e) in elements.iter().enumerate() {
        let target = Target::Layer(i);
        match e.role {
            LayerRole::Background => {}
            role if role.is_logo() => tracks.extend(logo_track(i, e)),
            _ if preset.is_reveal() => {}
            LayerRole::FinderOuter | LayerRole::FinderEye if preset == Preset::LockOn => {
                let corner = scene::CORNERS
                    .iter()
                    .position(|&c| Some(c) == e.corner)
                    .unwrap_or(0) as f64;
                tracks.push(if e.role == LayerRole::FinderOuter {
                    track(
                        target,
                        code_start + corner * 60.0,
                        finder_ms,
                        fade_scale(1.35),
                    )
                } else {
                    track(
                        target,
                        code_start + 150.0 + corner * 60.0,
                        finder_ms * 0.8,
                        fade_scale(0.0),
                    )
                });
            }
            _ => {
                let g = e.group.unwrap_or(0);
                let (at, length) = stagger(start, span, k, g as f64);
                let keys = match preset {
                    Preset::Ripple => fade_scale(0.8),
                    Preset::Assemble => {
                        let (dx, dy) = scatter(seed, g);
                        alloc::vec![
                            number(Property::Opacity, 0.0, 0.0),
                            number(Property::TranslateX, 0.0, dx),
                            number(Property::TranslateY, 0.0, dy),
                        ]
                    }
                    _ => fade_scale(0.92),
                };
                tracks.push(track(target, at, length, keys));
            }
        }
    }
    tracks
}

/// The keyframes a logo layer of `role` gets from `intro`: where its track
/// starts and how long it runs, as shares of the logo's intro, and its
/// keys. `drawable` says the layer is an outline that can draw on; `wide`
/// is the scale at which the plate is as wide as the code.
fn logo_keys(
    intro: LogoIntro,
    role: LayerRole,
    has_frame: bool,
    drawable: bool,
    wide: f32,
) -> Option<(f64, f64, Vec<Keyframe>)> {
    let fade = |scale: f32| {
        alloc::vec![
            number(Property::Opacity, 0.0, 0.0),
            number(Property::Scale, 0.0, scale),
        ]
    };
    let fade_in = || alloc::vec![number(Property::Opacity, 0.0, 0.0)];
    let frame = role == LayerRole::LogoFrame;
    match (intro, role) {
        // The content moves inside the plate's layer, with it.
        (_, LayerRole::LogoGlint | LayerRole::LogoArt) | (LogoIntro::None, _) => None,
        (LogoIntro::Ink, LayerRole::LogoInk) => Some((
            0.0,
            1.0,
            // Rests at 0: in to full by 30%, then out.
            alloc::vec![number(Property::Opacity, 0.3, 1.0)],
        )),
        (_, LayerRole::LogoInk) => None,
        (LogoIntro::Shrink, _) => Some((
            0.0,
            1.0,
            // In fast at full width, so it never pops; a beat there so it
            // reads as the whole code; then down, decelerating.
            alloc::vec![
                number(Property::Opacity, 0.0, 0.0),
                number(Property::Opacity, 0.08, 1.0),
                number(Property::Scale, 0.0, wide),
                number(Property::Scale, 0.15, wide),
            ],
        )),
        (LogoIntro::Reveal, _) => Some((0.0, 1.0, fade(0.94))),
        (LogoIntro::Stamp, _) => Some((
            0.0,
            1.0,
            alloc::vec![
                number(Property::Opacity, 0.0, 0.0),
                number(Property::Opacity, 0.35, 1.0),
                number(Property::Scale, 0.0, 1.12),
            ],
        )),
        (LogoIntro::Draw, _) if frame && drawable => {
            Some((0.0, 0.7, alloc::vec![number(Property::Draw, 0.0, 0.0)]))
        }
        (LogoIntro::Draw, _) if frame => Some((0.0, 0.6, fade(0.94))),
        (LogoIntro::Draw, _) if has_frame => Some((0.4, 0.6, fade(0.94))),
        (LogoIntro::Draw, _) => Some((0.0, 1.0, fade(0.94))),
        (LogoIntro::Lock, _) if frame => Some((0.0, 0.65, fade(1.3))),
        (LogoIntro::Lock, _) if has_frame => Some((0.45, 0.55, fade_in())),
        (LogoIntro::Lock, _) => Some((0.0, 1.0, fade(1.2))),
        (LogoIntro::Ink, _) if frame => Some((0.0, 0.4, fade(0.94))),
        (LogoIntro::Ink, _) => Some((0.25, 0.1, fade_in())),
        (LogoIntro::Spin, _) if has_frame && !frame => Some((0.35, 0.65, fade(0.94))),
        (LogoIntro::Spin, _) => Some((
            0.0,
            1.0,
            // Opaque by 40%, so the turn's slow end is the part you see.
            alloc::vec![
                number(Property::Opacity, 0.0, 0.0),
                number(Property::Opacity, 0.4, 1.0),
                number(Property::Rotate, 0.0, -90.0),
                number(Property::Scale, 0.0, 0.9),
            ],
        )),
    }
}

/// The reveal mask of a wiping preset over `elements`: everything but the
/// background, and the logo when it has its own reveal.
fn reveal(style: &Style, intro: &Intro, canvas: &Canvas, elements: &[Element]) -> Option<Reveal> {
    let preset = intro.entrance().filter(|p| p.is_reveal())?;
    let s = canvas.side as f32;
    let first = elements
        .iter()
        .position(|e| e.role != LayerRole::Background)?;
    // A logo with no intro of its own still leaves the mask when its frame
    // has one, and then shows from the start.
    let logo_apart =
        (intro.logo != LogoIntro::None || intro.frame_intro().is_some()) && style.logo.is_some();
    let last = elements
        .iter()
        .rposition(|e| !(e.role.is_logo() && logo_apart))?;
    if last < first {
        return None;
    }
    let vertical_bars = matches!(
        style.modules,
        ModuleShape::Bars {
            axis: crate::style::Axis::Vertical,
            ..
        }
    );
    let (shape, feather) = match preset {
        Preset::Sweep => (
            RevealShape::Wipe {
                from: (0.0, 0.0),
                to: (s, s),
            },
            s * 0.25,
        ),
        Preset::Raster => (
            RevealShape::Wipe {
                from: (0.0, 0.0),
                to: if vertical_bars { (s, 0.0) } else { (0.0, s) },
            },
            0.5,
        ),
        _ => (
            RevealShape::Iris {
                center: (s / 2.0, s / 2.0),
                radius: s * core::f32::consts::FRAC_1_SQRT_2,
            },
            s * 0.15,
        ),
    };
    Some(Reveal {
        shape,
        feather,
        layers: (first, last),
    })
}

/// The palette fade's keyframe colors: OKLab interpolation sampled at
/// evenly spaced times through `ease`, so a linear track between them
/// follows the curve. The end is left out (the element's own value).
fn fade_stops(old: Color, new: Color, ease: CubicBezier) -> Vec<(f32, Color)> {
    let (a, b) = (old.to_oklab(), new.to_oklab());
    (0..FADE_STOPS)
        .map(|i| {
            let t = i as f64 / FADE_STOPS as f64;
            let e = ease.at(t);
            let mix = |j: usize| a[j] as f64 + (b[j] as f64 - a[j] as f64) * e;
            (
                t as f32,
                Color::from_oklab(mix(0), mix(1), mix(2)).opaque_srgb(),
            )
        })
        .collect()
}

/// Keyframes per palette fade.
const FADE_STOPS: usize = 8;

/// A token's solid color in `palette`, if it resolves to one.
fn solid(palette: &Palette, token: Token) -> Option<Color> {
    match palette.resolve(token) {
        crate::Paint::Solid(c) => Some(c),
        crate::Paint::Gradient(_) => None,
    }
}

/// The cross-fades of a palette shift: one linear track per element whose
/// token changes between two solid colors of the same alpha.
fn palette_tracks(style: &Style, intro: &Intro, elements: &[Element]) -> Vec<Track> {
    let Some(old) = &intro.palette_from else {
        return Vec::new();
    };
    let backdrop = style.logo.as_ref().is_some_and(|l| l.backdrop);
    let ease = intro.curve();
    let mut tracks = Vec::new();
    for (i, e) in elements.iter().enumerate() {
        // A picture's strokes take the new palette at once: their paint is
        // a stroke, and fill tracks don't reach it.
        if matches!(
            e.role,
            LayerRole::Shimmer | LayerRole::Halftone | LayerRole::LogoGlint | LayerRole::LogoArt
        ) || e.outline.is_some()
            || (e.role == LayerRole::Logo && !backdrop)
        {
            continue;
        }
        let (Some(a), Some(b)) = (solid(old, e.token), solid(&style.palette, e.token)) else {
            continue;
        };
        if a.alpha() != b.alpha() || a.opaque_srgb() == b.opaque_srgb() {
            continue;
        }
        tracks.push(Track {
            target: Target::Layer(i),
            delay_ms: intro.delay_ms,
            duration_ms: intro.duration_ms,
            ease: CubicBezier::LINEAR,
            iterations: Some(1),
            keys: fade_stops(a, b, ease)
                .into_iter()
                .map(|(offset, c)| Keyframe {
                    offset,
                    property: Property::Fill,
                    value: Value::Color(c),
                })
                .collect(),
        });
    }
    tracks
}

/// Loops are symmetric, not entrances: ease-in-out sine.
const LOOP_EASE: CubicBezier = CubicBezier::new(0.37, 0.0, 0.63, 1.0);
/// The shimmer band's width, as a share of the canvas.
const SHIMMER_BAND: f64 = 0.6;
/// The shimmer band's peak opacity.
pub(crate) const SHIMMER_ALPHA: f64 = 0.18;
/// How far Breathe moves the background's lightness.
const BREATHE_L: f64 = 0.02;

/// Breathe's far point: the background 2% lighter (or darker, under
/// light ink) in OKLab lightness.
pub(crate) fn breathe_color(style: &Style) -> Option<Color> {
    let bg = solid(&style.palette, Token::Bg)?;
    let [l, a, b] = bg.to_oklab();
    let away = if style.palette.contrast(Token::Fg).inverted {
        -BREATHE_L
    } else {
        BREATHE_L
    };
    let far =
        Color::from_oklab((l as f64 + away).clamp(0.0, 1.0), a as f64, b as f64).opaque_srgb();
    (far != bg.opaque_srgb()).then_some(far)
}

/// How long the logo's glint takes to come round, and the frame to turn
/// (an Orbit's moon goes faster).
const GLINT_MS: u32 = 7000;
const TURN_MS: u32 = 24_000;
const ORBIT_MS: u32 = 12_000;
/// How often the logo spins or flips, and how long each takes.
const SPIN_EVERY_MS: u32 = 8000;
const SPIN_MS: f64 = 1200.0;
const FLIP_MS: f64 = 1000.0;
/// A spin starts and lands softly: ease-in-out quart.
const SPIN_EASE: CubicBezier = CubicBezier::new(0.76, 0.0, 0.24, 1.0);
/// Steps a spin or flip is sampled in, through [`SPIN_EASE`]; a flip's
/// height follows the cosine of its turn.
const FLIP_STEPS: usize = 12;

/// The keyframes of the logo's spin and flip in one loop: each property
/// keyed at every step, so the browser interpolates matching transforms.
/// The spin turns from −360° to 0 in its first stretch; the flip, half the
/// loop later (or first, alone), takes the height through cos 0–360°. The
/// steps carry the easing, so the track itself is linear.
fn spin_flip_keys(spin: bool, flip: bool) -> Vec<Keyframe> {
    let every = SPIN_EVERY_MS as f64;
    let flip_at = if spin { 0.5 } else { 0.0 };
    let mut keys = Vec::new();
    let mut key = |offset: f64, rotate: f64, height: f64| {
        let offset = (math::round(offset * 10_000.0) / 10_000.0) as f32;
        if spin {
            keys.push(number(Property::Rotate, offset, rotate as f32));
        }
        if flip {
            keys.push(number(
                Property::ScaleY,
                offset,
                (math::round(height * 1000.0) / 1000.0) as f32,
            ));
        }
    };
    if spin {
        let length = SPIN_MS / every;
        for i in 0..=FLIP_STEPS {
            let t = i as f64 / FLIP_STEPS as f64;
            let angle = math::round((SPIN_EASE.at(t) - 1.0) * 360_000.0) / 1000.0;
            key(t * length, angle, 1.0);
        }
    }
    if flip {
        let (start, length) = (flip_at, FLIP_MS / every);
        if spin {
            key(start, 0.0, 1.0);
        }
        for i in 1..=FLIP_STEPS {
            let t = i as f64 / FLIP_STEPS as f64;
            let turn = SPIN_EASE.at(t) * 2.0 * core::f64::consts::PI;
            key(start + t * length, 0.0, math::sin_cos(turn).1);
        }
    }
    keys
}

/// The idle loops, starting at `start` ms.
fn idle_tracks(
    style: &Style,
    qr: &QrMatrix,
    canvas: &Canvas,
    start: u32,
    elements: &[Element],
) -> Vec<Track> {
    let mut tracks = Vec::new();
    let endless = |target, ms, ease, keys| Track {
        target,
        delay_ms: start,
        duration_ms: ms,
        ease,
        iterations: None,
        keys,
    };
    for (i, e) in elements.iter().enumerate() {
        let target = Target::Layer(i);
        match e.role {
            LayerRole::FinderEye if style.idle.contains(&Idle::FinderPulse) => {
                tracks.push(endless(
                    target,
                    4000,
                    LOOP_EASE,
                    alloc::vec![
                        number(Property::Scale, 0.075, 1.06),
                        number(Property::Scale, 0.15, 1.0),
                    ],
                ));
            }
            LayerRole::Background if style.idle.contains(&Idle::Breathe) => {
                if let Some(far) = breathe_color(style) {
                    tracks.push(endless(
                        target,
                        8000,
                        LOOP_EASE,
                        alloc::vec![Keyframe {
                            offset: 0.5,
                            property: Property::Fill,
                            value: Value::Color(far),
                        }],
                    ));
                }
            }
            LayerRole::LogoGlint => {
                // Across the plate in the first fifth, then home unseen.
                let side =
                    svg::logo_plate(style, qr, canvas).map_or(0.0, |_| 2.0 * plate_half(style, qr));
                let across = (side * (1.0 + svg::GLINT_BAND + svg::GLINT_SLANT)) as f32;
                tracks.push(endless(
                    target,
                    GLINT_MS,
                    CubicBezier::LINEAR,
                    alloc::vec![
                        number(Property::TranslateX, 0.2, across),
                        number(Property::Opacity, 0.2, 1.0),
                        number(Property::Opacity, 0.21, 0.0),
                        number(Property::Opacity, 0.99, 0.0),
                    ],
                ));
            }
            LayerRole::LogoFrame if style.idle.contains(&Idle::LogoTurn) => {
                let orbit = style
                    .logo
                    .as_ref()
                    .is_some_and(|l| l.frame == Some(crate::LogoFrame::Orbit));
                tracks.push(endless(
                    target,
                    if orbit { ORBIT_MS } else { TURN_MS },
                    CubicBezier::LINEAR,
                    // A whole turn back to its own angle.
                    alloc::vec![number(Property::Rotate, 0.0, -360.0)],
                ));
            }
            LayerRole::LogoArt => {
                let (spin, flip) = (
                    style.idle.contains(&Idle::LogoSpin),
                    style.idle.contains(&Idle::LogoFlip),
                );
                if spin || flip {
                    tracks.push(endless(
                        target,
                        SPIN_EVERY_MS,
                        CubicBezier::LINEAR,
                        spin_flip_keys(spin, flip),
                    ));
                }
            }
            LayerRole::Shimmer => {
                // Across in the first quarter, then home unseen.
                let across = (canvas.side as f64 * (1.0 + SHIMMER_BAND)) as f32;
                tracks.push(endless(
                    target,
                    6000,
                    CubicBezier::LINEAR,
                    alloc::vec![
                        number(Property::TranslateX, 0.25, across),
                        number(Property::Opacity, 0.25, 1.0),
                        number(Property::Opacity, 0.26, 0.0),
                        number(Property::Opacity, 0.99, 0.0),
                    ],
                ));
            }
            _ => {}
        }
    }
    tracks
}

/// Half the side of the logo's plate, in modules.
fn plate_half(style: &Style, qr: &QrMatrix) -> f64 {
    style.logo.as_ref().map_or(0.0, |logo| {
        crate::logo::placement(logo, qr, crate::check::logo_scale(style, qr)).half
    })
}

/// The lowest contrast each read token reaches while it animates: along
/// a palette shift (sampled the way browsers interpolate between the
/// keyframes) and under the shimmer band. Tokens that don't move aren't
/// listed.
pub(crate) fn motion_contrast(style: &Style, qr: &QrMatrix) -> Vec<(Token, f32)> {
    let l = |c: [f64; 3]| crate::color::srgb_to_oklab(c)[0];
    let read: Vec<Token> = {
        let mut t: Vec<Token> = scene::layers(style, qr)
            .iter()
            .map(|l| l.token)
            .filter(|t| !matches!(t, Token::Filler | Token::Frame))
            .collect();
        t.dedup();
        t
    };
    let mut out: Vec<(Token, f32)> = Vec::new();
    let mut note = |token: Token, d: f64| match out.iter_mut().find(|(t, _)| *t == token) {
        Some((_, v)) => *v = v.min(d as f32),
        None => out.push((token, d as f32)),
    };
    let new = &style.palette;
    if let Some(intro) = style.intro.as_ref()
        && let Some(old) = &intro.palette_from
    {
        // Each token's color at time t, as the keyframes play it.
        let at = |token: Token, t: f64| -> Option<[f64; 3]> {
            let (a, b) = (solid(old, token)?, solid(new, token)?);
            if a.alpha() != b.alpha() {
                return Some(b.to_srgb());
            }
            let mut stops = fade_stops(a, b, intro.curve());
            stops.push((1.0, b.opaque_srgb()));
            let i = stops
                .windows(2)
                .position(|w| t <= w[1].0 as f64)
                .unwrap_or(stops.len() - 2);
            let ((o0, c0), (o1, c1)) = (stops[i], stops[i + 1]);
            let e = (t - o0 as f64) / (o1 - o0) as f64;
            let (c0, c1) = (c0.to_srgb(), c1.to_srgb());
            Some([0, 1, 2].map(|j| c0[j] + (c1[j] - c0[j]) * e))
        };
        for &token in &read {
            if solid(old, token) == solid(new, token)
                && solid(old, Token::Bg) == solid(new, Token::Bg)
            {
                continue;
            }
            for k in 0..=96 {
                let t = k as f64 / 96.0;
                if let (Some(c), Some(bg)) = (at(token, t), at(Token::Bg, t)) {
                    note(token, (l(bg) - l(c)).abs());
                }
            }
        }
    }
    if style.idle.contains(&Idle::Shimmer) {
        let lift = |c: [f64; 3]| c.map(|v| v + (1.0 - v) * SHIMMER_ALPHA);
        if let Some(bg) = solid(new, Token::Bg) {
            for &token in &read {
                if let Some(c) = solid(new, token) {
                    note(token, (l(lift(bg.to_srgb())) - l(lift(c.to_srgb()))).abs());
                }
            }
        }
    }
    out
}

/// The animated layout of `style`, or `None` when it doesn't animate. With
/// `at`, each element gets its pose at that time instead of a class.
pub(crate) fn build(
    style: &Style,
    qr: &QrMatrix,
    canvas: &Canvas,
    at: Option<u32>,
) -> Option<(Vec<Element>, Motion)> {
    if style.intro.is_none() && style.idle.is_empty() {
        return None;
    }
    let intro = style.intro.as_ref();
    let entrance = intro.and_then(Intro::entrance);
    let mut elements = match (intro, entrance) {
        // A reveal moves one mask and Develop fades whole layers, so the
        // layers stay whole.
        (Some(_), Some(p)) if p.is_reveal() || p == Preset::Develop => {
            svg::static_elements(style, qr, canvas)
        }
        (Some(intro), Some(_)) => split(style, qr, canvas, intro),
        // Pulsing eyes need a finder per element.
        _ if style.idle.contains(&Idle::FinderPulse) => {
            split(style, qr, canvas, &Intro::new(Preset::Settle).groups(1))
        }
        _ => svg::static_elements(style, qr, canvas),
    };
    if entrance == Some(Preset::Ripple) {
        let mid = canvas.side as f64 / 2.0;
        for e in elements.iter_mut().filter(|e| !e.role.is_logo()) {
            e.origin = (mid, mid);
        }
    }
    if style.idle.contains(&Idle::Shimmer) {
        let side = canvas.side * crate::geometry::UNIT;
        let band = (side as f64 * SHIMMER_BAND) as i64;
        let mut path = Path::new();
        path.rect(-band, 0, band, side);
        elements.push(Element::new(LayerRole::Shimmer, Token::Bg, false, path));
    }
    let mut tracks = Vec::new();
    if let Some(intro) = intro {
        tracks.extend(self::tracks(
            style,
            qr,
            intro,
            scene::seed(style, qr),
            &elements,
        ));
        tracks.extend(palette_tracks(style, intro, &elements));
    }
    let total_ms = tracks
        .iter()
        .filter_map(|t| Some(t.delay_ms + t.duration_ms * t.iterations?))
        .max()
        .unwrap_or(0);
    tracks.extend(idle_tracks(style, qr, canvas, total_ms, &elements));
    // An outline that draws on gets one dash as long as itself, and a gap
    // as long: moving the dashes back hides its end.
    for (i, e) in elements.iter_mut().enumerate() {
        let draws = tracks.iter().any(|t| {
            t.target == Target::Layer(i) && t.keys.iter().any(|k| k.property == Property::Draw)
        });
        if let Some(outline) = e.outline.as_mut().filter(|_| draws) {
            let dash = outline.length + 0.01;
            outline.dashes = alloc::vec![dash, dash];
        }
    }
    let motion = Motion {
        origins: elements.iter().map(|e| e.origin).collect(),
        dashes: elements
            .iter()
            .map(|e| e.outline.as_ref().and_then(|o| o.dashes.first().copied()))
            .collect(),
        joined: alloc::vec![false; elements.len()],
        reveal: intro.and_then(|i| reveal(style, i, canvas, &elements)),
        tracks,
        total_ms,
        trigger: intro.map_or(Trigger::Load, |i| i.trigger.clone()),
        at,
    };
    // A picture's strokes in one group move alike: live, they share one
    // animated group, so browsers move a group per tile set, not a stroke
    // per paint.
    let mut motion = motion;
    if at.is_none() {
        let same = |a: usize, b: usize| {
            let of = |i: usize| {
                motion
                    .tracks
                    .iter()
                    .filter(move |t| t.target == Target::Layer(i))
                    .map(|t| (t.delay_ms, t.duration_ms, t.ease, t.iterations, &t.keys))
            };
            of(a).eq(of(b)) && of(a).next().is_some()
        };
        for i in 1..elements.len() {
            let (prev, e) = (&elements[i - 1], &elements[i]);
            if e.role == LayerRole::Halftone
                && prev.role == LayerRole::Halftone
                && prev.group == e.group
                && same(i - 1, i)
            {
                motion.joined[i] = true;
            }
        }
    }
    for (i, e) in elements.iter_mut().enumerate() {
        let mine = motion
            .tracks
            .iter()
            .filter(|t| t.target == Target::Layer(i));
        if motion.joined[i] {
            e.joins = true;
            continue;
        }
        match at {
            Some(t) => {
                let natural = solid(&style.palette, e.token);
                let rest = rest(e.role);
                let pose = pose(mine, natural, rest, t);
                e.pose = (pose != rest).then_some(pose);
            }
            None => {
                if mine.count() > 0 {
                    e.anim = Some(i);
                }
            }
        }
    }
    Some((elements, motion))
}

// ----------------------------------------------------------- evaluation

/// Where `track` is at `t` ms, 0–1 within its iteration, or `None` when
/// it doesn't touch its target then.
fn progress(track: &Track, t: u32) -> Option<f64> {
    let dur = track.duration_ms.max(1) as f64;
    let local = t as f64 - track.delay_ms as f64;
    match track.iterations {
        Some(n) if local >= dur * n as f64 => None,
        Some(_) if local < 0.0 => Some(0.0),
        None if local < 0.0 => None,
        _ => Some((local % dur) / dur),
    }
}

/// The keyframes of `property` with the natural value filled in at 0 and
/// 1, and where in them progress `p` falls: the pair and the eased share
/// between them.
fn segment<T: Copy>(
    track: &Track,
    mut keys: Vec<(f64, T)>,
    natural: T,
    p: f64,
) -> Option<(T, T, f64)> {
    if keys.is_empty() {
        return None;
    }
    if keys[0].0 > 0.0 {
        keys.insert(0, (0.0, natural));
    }
    if keys[keys.len() - 1].0 < 1.0 {
        keys.push((1.0, natural));
    }
    let i = keys
        .windows(2)
        .position(|w| p <= w[1].0)
        .unwrap_or(keys.len() - 2);
    let ((o0, v0), (o1, v1)) = (keys[i], keys[i + 1]);
    let local = if o1 > o0 { (p - o0) / (o1 - o0) } else { 1.0 };
    Some((v0, v1, track.ease.at(local)))
}

/// The value of `property` on `track` at `t` ms, if the track touches it.
fn sample(track: &Track, property: Property, natural: f64, t: u32) -> Option<f64> {
    let p = progress(track, t)?;
    let keys = track
        .keys
        .iter()
        .filter(|k| k.property == property)
        .filter_map(|k| match k.value {
            Value::Number(v) => Some((k.offset as f64, v as f64)),
            Value::Color(_) => None,
        })
        .collect();
    let (a, b, e) = segment(track, keys, natural, p)?;
    Some(a + (b - a) * e)
}

/// The fill on `track` at `t` ms. Browsers interpolate hex colors in
/// gamma-encoded sRGB, and so does this.
fn sample_fill(track: &Track, natural: Color, t: u32) -> Option<Color> {
    let p = progress(track, t)?;
    let keys = track
        .keys
        .iter()
        .filter_map(|k| match k.value {
            Value::Color(c) if k.property == Property::Fill => Some((k.offset as f64, c)),
            _ => None,
        })
        .collect();
    let (a, b, e) = segment(track, keys, natural, p)?;
    let (a, b) = (a.to_srgb8(), b.to_srgb8());
    let mix = |i: usize| math::round(a[i] as f64 + (b[i] as f64 - a[i] as f64) * e) as u8;
    Some(Color::srgb(mix(0), mix(1), mix(2)).with_alpha(natural.alpha()))
}

/// A layer's pose at rest: natural, but invisible for the Ink silhouette.
fn rest(role: LayerRole) -> Pose {
    Pose {
        opacity: if role == LayerRole::LogoInk { 0.0 } else { 1.0 },
        ..Pose::NATURAL
    }
}

/// An element's pose at `t` from its tracks, over its pose at rest; later
/// tracks win, as in a CSS animation list.
fn pose<'a>(
    tracks: impl Iterator<Item = &'a Track>,
    fill: Option<Color>,
    rest: Pose,
    t: u32,
) -> Pose {
    let mut pose = rest;
    for track in tracks {
        if let Some(v) = sample(track, Property::Opacity, rest.opacity, t) {
            pose.opacity = v;
        }
        if let Some(v) = sample(track, Property::Scale, 1.0, t) {
            pose.scale = v;
        }
        if let Some(v) = sample(track, Property::Rotate, 0.0, t) {
            pose.rotate = v;
        }
        if let Some(v) = sample(track, Property::ScaleY, 1.0, t) {
            pose.scale_y = v;
        }
        if let Some(v) = sample(track, Property::Draw, 1.0, t) {
            pose.draw = v;
        }
        if let Some(v) = sample(track, Property::TranslateX, 0.0, t) {
            pose.translate.0 = v;
        }
        if let Some(v) = sample(track, Property::TranslateY, 0.0, t) {
            pose.translate.1 = v;
        }
        if let Some(c) = fill.and_then(|natural| sample_fill(track, natural, t)) {
            // At the natural color, the token's own paint stays.
            pose.fill = (Some(c.opaque_srgb()) != fill.map(Color::opaque_srgb)).then_some(c);
        }
    }
    pose
}

// ------------------------------------------------------------------ CSS

fn write_number<W: Write + ?Sized>(w: &mut W, v: f64) -> fmt::Result {
    write_scaled(w, math::round(v * 1000.0) as i64, 3)
}

/// A CSS identifier with everything outside `[A-Za-z0-9_-]` escaped (and
/// a leading digit), so a class name can't leave its selector.
fn write_ident<W: Write + ?Sized>(w: &mut W, name: &str) -> fmt::Result {
    for (i, c) in name.chars().enumerate() {
        let plain =
            c.is_ascii_alphabetic() || c == '_' || c == '-' || (c.is_ascii_digit() && i > 0);
        if plain {
            w.write_char(c)?;
        } else {
            write!(w, "\\{:x} ", c as u32)?;
        }
    }
    Ok(())
}

fn write_curve<W: Write + ?Sized>(w: &mut W, c: CubicBezier) -> fmt::Result {
    w.write_str("cubic-bezier(")?;
    for (i, v) in [c.x1, c.y1, c.x2, c.y2].into_iter().enumerate() {
        if i > 0 {
            w.write_char(',')?;
        }
        write_number(w, v as f64)?;
    }
    w.write_char(')')
}

/// One keyframe list as CSS rules, grouped by offset.
fn write_keyframes<W: Write + ?Sized>(w: &mut W, keys: &[Keyframe]) -> fmt::Result {
    // A list that turns or flips writes those functions in every rule, so
    // neighboring transforms match and interpolate by angle, not matrix.
    let turns = keys.iter().any(|k| k.property == Property::Rotate);
    let flips = keys.iter().any(|k| k.property == Property::ScaleY);
    let mut offsets: Vec<f32> = keys.iter().map(|k| k.offset).collect();
    offsets.sort_unstable_by(|a, b| a.total_cmp(b));
    offsets.dedup();
    for offset in offsets {
        write_number(w, offset as f64 * 100.0)?;
        w.write_str("%{")?;
        let at: Vec<&Keyframe> = keys.iter().filter(|k| k.offset == offset).collect();
        let num = |p: Property, natural: f64| {
            at.iter()
                .find(|k| k.property == p)
                .and_then(|k| match k.value {
                    Value::Number(v) => Some(v as f64),
                    Value::Color(_) => None,
                })
                .unwrap_or(natural)
        };
        let mut first = true;
        let mut sep = |w: &mut W| -> fmt::Result {
            if !first {
                w.write_char(';')?;
            }
            first = false;
            Ok(())
        };
        for k in &at {
            match (k.property, k.value) {
                (Property::Opacity, Value::Number(v)) => {
                    sep(w)?;
                    w.write_str("opacity:")?;
                    write_number(w, v as f64)?;
                }
                (Property::Fill, Value::Color(c)) => {
                    sep(w)?;
                    w.write_str("fill:")?;
                    c.opaque_srgb().write_opaque(w)?;
                }
                // By now a dash offset (see `css_keys`).
                (Property::Draw, Value::Number(v)) => {
                    sep(w)?;
                    w.write_str("stroke-dashoffset:")?;
                    write_number(w, v as f64)?;
                }
                _ => {}
            }
        }
        if at.iter().any(|k| k.property.is_transform()) {
            sep(w)?;
            w.write_str("transform:")?;
            let (tx, ty, s) = (
                num(Property::TranslateX, 0.0),
                num(Property::TranslateY, 0.0),
                num(Property::Scale, 1.0),
            );
            let mut parts = 0;
            if tx != 0.0 || ty != 0.0 {
                w.write_str("translate(")?;
                write_number(w, tx)?;
                w.write_str("px,")?;
                write_number(w, ty)?;
                w.write_str("px)")?;
                parts += 1;
            }
            let r = num(Property::Rotate, 0.0);
            if r != 0.0 || turns {
                if parts > 0 {
                    w.write_char(' ')?;
                }
                w.write_str("rotate(")?;
                write_number(w, r)?;
                w.write_str("deg)")?;
                parts += 1;
            }
            if flips {
                if parts > 0 {
                    w.write_char(' ')?;
                }
                w.write_str("scale(")?;
                write_number(w, s)?;
                w.write_char(',')?;
                write_number(w, s * num(Property::ScaleY, 1.0))?;
                w.write_char(')')?;
            } else if s != 1.0 || parts == 0 {
                if parts > 0 {
                    w.write_char(' ')?;
                }
                w.write_str("scale(")?;
                write_number(w, s)?;
                w.write_char(')')?;
            }
        }
        w.write_char('}')?;
    }
    Ok(())
}

/// What a shared animation rule says about one track: keyframes name,
/// duration, curve (as bits) and iterations.
type Signature = (usize, u32, [u32; 4], Option<u32>);

/// A track's keyframes as CSS animates them: a reveal's progress becomes
/// the mask content's transform, and how much of an outline is drawn its
/// dash offset (the [`Property::Draw`] value then holds the offset).
fn css_keys(track: &Track, reveal: Option<&Reveal>, dashes: &[Option<f64>]) -> Vec<Keyframe> {
    if let Target::Layer(i) = track.target
        && let Some(Some(dash)) = dashes.get(i)
    {
        return track
            .keys
            .iter()
            .map(|&k| match (k.property, k.value) {
                (Property::Draw, Value::Number(v)) => {
                    number(Property::Draw, k.offset, ((1.0 - v as f64) * dash) as f32)
                }
                _ => k,
            })
            .collect();
    }
    let (Target::Reveal, Some(r)) = (track.target, reveal) else {
        return track.keys.clone();
    };
    let travel = r.travel();
    track
        .keys
        .iter()
        .flat_map(|k| {
            let Value::Number(p) = k.value else {
                return alloc::vec![];
            };
            let p = p as f64;
            match r.shape {
                RevealShape::Wipe { .. } => alloc::vec![number(
                    Property::TranslateX,
                    k.offset,
                    (-(1.0 - p) * travel) as f32
                )],
                RevealShape::Iris { .. } => {
                    alloc::vec![number(Property::Scale, k.offset, p as f32)]
                }
            }
        })
        .collect()
}

impl Motion {
    /// The selector suffix of a track's element: `e{n}` or `r`.
    fn class(target: Target) -> (char, usize) {
        match target {
            Target::Layer(i) => ('e', i),
            Target::Reveal => ('r', 0),
        }
    }

    fn origin(&self, target: Target) -> (f64, f64) {
        match (target, &self.reveal) {
            (Target::Layer(i), _) => self.origins[i],
            (
                Target::Reveal,
                Some(Reveal {
                    shape: RevealShape::Iris { center, .. },
                    ..
                }),
            ) => (center.0 as f64, center.1 as f64),
            (Target::Reveal, _) => (0.0, 0.0),
        }
    }

    /// The keyframes, one rule per animated element (`{prefix}-e{n}`, and
    /// `{prefix}-r` for the reveal mask), the trigger, and the
    /// reduced-motion override (SPEC §8.1). Nothing for a frame.
    pub(crate) fn write_css<W: Write + ?Sized>(&self, w: &mut W, prefix: &str) -> fmt::Result {
        if self.at.is_some() {
            return Ok(());
        }
        // Joined elements move with the one before them.
        let tracks: Vec<&Track> = self
            .tracks
            .iter()
            .filter(|t| !matches!(t.target, Target::Layer(i) if self.joined[i]))
            .collect();
        // Identical keyframe lists are written once.
        let keys: Vec<Vec<Keyframe>> = tracks
            .iter()
            .map(|t| css_keys(t, self.reveal.as_ref(), &self.dashes))
            .collect();
        let mut lists: Vec<&[Keyframe]> = Vec::new();
        let name_of: Vec<usize> = keys
            .iter()
            .map(|k| match lists.iter().position(|l| *l == k.as_slice()) {
                Some(i) => i,
                None => {
                    lists.push(k);
                    lists.len() - 1
                }
            })
            .collect();
        for (i, keys) in lists.iter().enumerate() {
            write!(w, "@keyframes {prefix}-k{i}{{")?;
            write_keyframes(w, keys)?;
            w.write_char('}')?;
        }
        // Each target once, in order of its first track.
        let mut targets: Vec<Target> = Vec::new();
        for t in &tracks {
            if !targets.contains(&t.target) {
                targets.push(t.target);
            }
        }
        // Elements whose tracks differ only in delay share one rule; each
        // element then sets its delays and origin.
        let signature = |target: Target| -> Vec<Signature> {
            tracks
                .iter()
                .zip(&name_of)
                .filter(|(t, _)| t.target == target)
                .map(|(t, &name)| {
                    let e = t.ease;
                    (
                        name,
                        t.duration_ms,
                        [e.x1, e.y1, e.x2, e.y2].map(f32::to_bits),
                        t.iterations,
                    )
                })
                .collect()
        };
        let mut shared: Vec<(Vec<Signature>, Vec<Target>)> = Vec::new();
        for &target in &targets {
            let sig = signature(target);
            match shared.iter_mut().find(|(s, _)| *s == sig) {
                Some((_, members)) => members.push(target),
                None => shared.push((sig, alloc::vec![target])),
            }
        }
        for (sig, members) in &shared {
            for (i, &target) in members.iter().enumerate() {
                if i > 0 {
                    w.write_char(',')?;
                }
                if let Trigger::Hover = self.trigger {
                    write!(w, ".{prefix}:hover ")?;
                }
                let (c, n) = Motion::class(target);
                write!(w, ".{prefix}-{c}")?;
                if c == 'e' {
                    write!(w, "{n}")?;
                }
            }
            w.write_str("{animation:")?;
            for (j, &(name, duration, ease, iterations)) in sig.iter().enumerate() {
                if j > 0 {
                    w.write_char(',')?;
                }
                write!(w, "{prefix}-k{name} {duration}ms ")?;
                let [x1, y1, x2, y2] = ease.map(f32::from_bits);
                write_curve(w, CubicBezier::new(x1, y1, x2, y2))?;
                match iterations {
                    Some(1) => w.write_str(" backwards")?,
                    Some(n) => write!(w, " {n} backwards")?,
                    None => w.write_str(" infinite")?,
                }
            }
            if let Trigger::Class(_) = self.trigger {
                w.write_str(";animation-play-state:paused")?;
            }
            w.write_str(";transform-box:view-box}")?;
        }
        for &target in &targets {
            let (c, n) = Motion::class(target);
            write!(w, ".{prefix}-{c}")?;
            if c == 'e' {
                write!(w, "{n}")?;
            }
            w.write_str("{animation-delay:")?;
            let mine = tracks.iter().filter(|t| t.target == target);
            for (j, t) in mine.enumerate() {
                if j > 0 {
                    w.write_char(',')?;
                }
                write!(w, "{}ms", t.delay_ms)?;
            }
            let (ox, oy) = self.origin(target);
            w.write_str(";transform-origin:")?;
            write_number(w, ox)?;
            w.write_str("px ")?;
            write_number(w, oy)?;
            w.write_str("px}")?;
        }
        if self.reveal.is_some() {
            write!(w, ".{prefix}-m{{mask:url(#{prefix}-reveal)}}")?;
        }
        if let Trigger::Class(class) = &self.trigger {
            for (i, &target) in targets.iter().enumerate() {
                w.write_str(if i == 0 { "." } else { ",." })?;
                write_ident(w, class)?;
                let (c, n) = Motion::class(target);
                write!(w, " .{prefix}-{c}")?;
                if c == 'e' {
                    write!(w, "{n}")?;
                }
            }
            w.write_str("{animation-play-state:running}")?;
        }
        write!(
            w,
            "@media (prefers-reduced-motion:reduce){{.{prefix} *{{animation:none!important;transition:none!important}}}}"
        )
    }

    /// Whether the root gets the prefix class (live animation only).
    pub(crate) fn live(&self) -> bool {
        self.at.is_none()
    }

    /// The reveal's progress in a frame, or `None` live.
    fn reveal_progress(&self) -> Option<f64> {
        let t = self.at?;
        Some(
            self.tracks
                .iter()
                .filter(|tr| tr.target == Target::Reveal)
                .rev()
                .find_map(|tr| sample(tr, Property::Reveal, 1.0, t))
                .unwrap_or(1.0),
        )
    }

    /// The layers the reveal mask covers, when it is drawn: always live
    /// (the stylesheet applies it), and in frames until it is fully open.
    pub(crate) fn reveal_layers(&self) -> Option<(usize, usize)> {
        let open = self.reveal_progress().is_some_and(|p| p >= 1.0);
        self.reveal.filter(|_| !open).map(|r| r.layers)
    }

    /// The reveal's `<mask>`: white wherever the code shows. Its natural
    /// state covers the whole canvas, so without CSS nothing is hidden.
    pub(crate) fn write_mask<W: Write + ?Sized>(
        &self,
        w: &mut W,
        prefix: &str,
        side: i64,
    ) -> fmt::Result {
        let Some(r) = &self.reveal else {
            return Ok(());
        };
        let progress = self.reveal_progress();
        let n = |w: &mut W, v: f64| write_number(w, v);
        write!(
            w,
            "<mask id=\"{prefix}-reveal\" maskUnits=\"userSpaceOnUse\" x=\"0\" y=\"0\" width=\"{side}\" height=\"{side}\">"
        )?;
        // The element that moves: a class live, a fixed transform in a frame.
        let moving = |w: &mut W, transform: &dyn Fn(&mut W, f64) -> fmt::Result| -> fmt::Result {
            match progress {
                None => write!(w, " class=\"{prefix}-r\""),
                Some(p) if p >= 1.0 => Ok(()),
                Some(p) => {
                    w.write_str(" transform=\"")?;
                    transform(w, p)?;
                    w.write_char('"')
                }
            }
        };
        let f = r.feather as f64;
        match r.shape {
            RevealShape::Wipe { from, to } => {
                let (dx, dy) = ((to.0 - from.0) as f64, (to.1 - from.1) as f64);
                let length = math::sqrt(dx * dx + dy * dy);
                let angle = math::atan2(dy, dx) * 180.0 / core::f64::consts::PI;
                let (reach, back) = (2.0 * side as f64, length + 2.0 * f + side as f64);
                w.write_str("<linearGradient id=\"")?;
                write!(
                    w,
                    "{prefix}-feather\"><stop offset=\"0\" stop-color=\"#fff\"/><stop offset=\"1\" stop-color=\"#fff\" stop-opacity=\"0\"/></linearGradient><g transform=\"translate("
                )?;
                n(w, from.0 as f64)?;
                w.write_char(' ')?;
                n(w, from.1 as f64)?;
                w.write_str(") rotate(")?;
                n(w, angle)?;
                w.write_str(")\"><g")?;
                let travel = r.travel();
                moving(w, &|w, p| {
                    w.write_str("translate(")?;
                    write_number(w, -(1.0 - p) * travel)?;
                    w.write_str(" 0)")
                })?;
                w.write_str("><rect x=\"")?;
                n(w, -back)?;
                w.write_str("\" y=\"")?;
                n(w, -reach)?;
                w.write_str("\" width=\"")?;
                n(w, back + length)?;
                w.write_str("\" height=\"")?;
                n(w, 2.0 * reach)?;
                w.write_str("\" fill=\"#fff\"/><rect x=\"")?;
                n(w, length)?;
                w.write_str("\" y=\"")?;
                n(w, -reach)?;
                w.write_str("\" width=\"")?;
                n(w, f)?;
                w.write_str("\" height=\"")?;
                n(w, 2.0 * reach)?;
                write!(w, "\" fill=\"url(#{prefix}-feather)\"/></g></g>")?;
            }
            RevealShape::Iris { center, radius } => {
                let outer = radius as f64 + f;
                write!(w, "<radialGradient id=\"{prefix}-iris\"><stop offset=\"")?;
                n(w, radius as f64 / outer)?;
                w.write_str("\" stop-color=\"#fff\"/><stop offset=\"1\" stop-color=\"#fff\" stop-opacity=\"0\"/></radialGradient><circle")?;
                let (cx, cy) = (center.0 as f64, center.1 as f64);
                moving(w, &|w, p| {
                    w.write_str("matrix(")?;
                    for (i, v) in [p, 0.0, 0.0, p, cx * (1.0 - p), cy * (1.0 - p)]
                        .into_iter()
                        .enumerate()
                    {
                        if i > 0 {
                            w.write_char(' ')?;
                        }
                        write_number(w, v)?;
                    }
                    w.write_char(')')
                })?;
                w.write_str(" cx=\"")?;
                n(w, cx)?;
                w.write_str("\" cy=\"")?;
                n(w, cy)?;
                w.write_str("\" r=\"")?;
                n(w, outer)?;
                write!(w, "\" fill=\"url(#{prefix}-iris)\"/>")?;
            }
        }
        w.write_str("</mask>")
    }
}

// ----------------------------------------------------------------- plan

/// The plan of `style` for `qr` (SPEC §8.5). A style without an intro
/// gives its static layers and no tracks.
pub(crate) fn plan(style: &Style, qr: &QrMatrix) -> AnimationPlan {
    let canvas = scene::canvas(style, qr);
    let (elements, motion) = match build(style, qr, &canvas, None) {
        Some((e, m)) => (e, Some(m)),
        None => (svg::static_elements(style, qr, &canvas), None),
    };
    let side = canvas.side as f32;
    let layers = elements
        .into_iter()
        .map(|e| PlanLayer {
            role: e.role,
            token: e.token,
            color: e.color,
            path: String::from(e.path.data()),
            cells: e
                .stroke
                .map(|_| (canvas.origin as f32, canvas.origin as f32)),
            stroke: e.stroke.map(f32::from),
            outline: e.outline.as_ref().map(|o| PlanOutline {
                width: o.width as f32 / crate::geometry::UNIT as f32,
                length: o.length as f32,
                dashes: o.dashes.iter().map(|&d| d as f32).collect(),
                dash_offset: o.offset as f32,
            }),
            opacity: rest(e.role).opacity as f32,
            evenodd: e.evenodd,
            group: e.group,
            corner: e.corner,
            origin: (e.origin.0 as f32, e.origin.1 as f32),
        })
        .collect();
    let (tracks, reveal, total_ms, trigger) = match motion {
        Some(m) => (m.tracks, m.reveal, m.total_ms, m.trigger),
        None => (Vec::new(), None, 0, Trigger::Load),
    };
    AnimationPlan {
        view_box: (side, side),
        palette: style.palette.clone(),
        layers,
        tracks,
        reveal,
        total_ms,
        trigger,
        reduced_motion: Fallback::FinalState,
    }
}

// ----------------------------------------------------------------- JSON

/// A JSON string literal.
fn json_str<W: Write + ?Sized>(w: &mut W, s: &str) -> fmt::Result {
    w.write_char('"')?;
    for c in s.chars() {
        match c {
            '"' => w.write_str("\\\"")?,
            '\\' => w.write_str("\\\\")?,
            c if (c as u32) < 0x20 => write!(w, "\\u{:04x}", c as u32)?,
            c => w.write_char(c)?,
        }
    }
    w.write_char('"')
}

fn json_num<W: Write + ?Sized>(w: &mut W, v: f32) -> fmt::Result {
    if v.is_finite() {
        write_number(w, v as f64)
    } else {
        w.write_char('0')
    }
}

fn role_name(role: LayerRole) -> &'static str {
    match role {
        LayerRole::Background => "background",
        LayerRole::Ring => "ring",
        LayerRole::Filler => "filler",
        LayerRole::Data => "data",
        LayerRole::Timing => "timing",
        LayerRole::Alignment => "alignment",
        LayerRole::FinderOuter => "finder-outer",
        LayerRole::FinderEye => "finder-eye",
        LayerRole::Logo => "logo",
        LayerRole::Shimmer => "shimmer",
        LayerRole::Halftone => "halftone",
        LayerRole::LogoFrame => "logo-frame",
        LayerRole::LogoInk => "logo-ink",
        LayerRole::LogoGlint => "logo-glint",
        LayerRole::LogoArt => "logo-art",
    }
}

fn corner_name(corner: Corner) -> &'static str {
    match corner {
        Corner::TopLeft => "top-left",
        Corner::TopRight => "top-right",
        Corner::BottomLeft => "bottom-left",
    }
}

impl AnimationPlan {
    /// The plan as compact JSON, with the same bytes for the same plan.
    /// Numbers have at most three decimals; paints are CSS strings keyed
    /// by token name; path data is SVG path syntax.
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        self.write_json(&mut out).expect("writing to a String");
        out
    }

    fn write_json<W: Write + ?Sized>(&self, w: &mut W) -> fmt::Result {
        w.write_str("{\"viewBox\":[")?;
        json_num(w, self.view_box.0)?;
        w.write_char(',')?;
        json_num(w, self.view_box.1)?;
        w.write_str("],\"palette\":{")?;
        for (i, (token, paint)) in self.palette.tokens().enumerate() {
            if i > 0 {
                w.write_char(',')?;
            }
            json_str(w, token.name())?;
            w.write_char(':')?;
            let mut css = String::new();
            write!(css, "{paint}")?;
            json_str(w, &css)?;
        }
        w.write_str("},\"layers\":[")?;
        for (i, l) in self.layers.iter().enumerate() {
            if i > 0 {
                w.write_char(',')?;
            }
            w.write_str("{\"role\":")?;
            json_str(w, role_name(l.role))?;
            w.write_str(",\"token\":")?;
            json_str(w, l.token.name())?;
            if let Some(c) = l.color {
                w.write_str(",\"color\":")?;
                let mut css = String::new();
                write!(css, "{c}")?;
                json_str(w, &css)?;
            }
            if let Some((x, y)) = l.cells {
                w.write_str(",\"cells\":[")?;
                json_num(w, x)?;
                w.write_char(',')?;
                json_num(w, y)?;
                w.write_char(']')?;
            }
            if let Some(width) = l.stroke {
                w.write_str(",\"stroke\":")?;
                json_num(w, width)?;
            }
            if let Some(o) = &l.outline {
                w.write_str(",\"outline\":{\"width\":")?;
                json_num(w, o.width)?;
                w.write_str(",\"length\":")?;
                json_num(w, o.length)?;
                w.write_str(",\"dashes\":[")?;
                for (j, &d) in o.dashes.iter().enumerate() {
                    if j > 0 {
                        w.write_char(',')?;
                    }
                    json_num(w, d)?;
                }
                w.write_str("],\"dashOffset\":")?;
                json_num(w, o.dash_offset)?;
                w.write_char('}')?;
            }
            if l.opacity != 1.0 {
                w.write_str(",\"opacity\":")?;
                json_num(w, l.opacity)?;
            }
            if l.evenodd {
                w.write_str(",\"evenodd\":true")?;
            }
            if let Some(g) = l.group {
                write!(w, ",\"group\":{g}")?;
            }
            if let Some(c) = l.corner {
                w.write_str(",\"corner\":")?;
                json_str(w, corner_name(c))?;
            }
            w.write_str(",\"origin\":[")?;
            json_num(w, l.origin.0)?;
            w.write_char(',')?;
            json_num(w, l.origin.1)?;
            w.write_str("],\"path\":")?;
            json_str(w, &l.path)?;
            w.write_char('}')?;
        }
        w.write_str("],\"tracks\":[")?;
        for (i, t) in self.tracks.iter().enumerate() {
            if i > 0 {
                w.write_char(',')?;
            }
            match t.target {
                Target::Layer(i) => write!(w, "{{\"target\":{{\"layer\":{i}}}")?,
                Target::Reveal => w.write_str("{\"target\":\"reveal\"")?,
            }
            write!(
                w,
                ",\"delayMs\":{},\"durationMs\":{},\"ease\":[",
                t.delay_ms, t.duration_ms
            )?;
            for (j, v) in [t.ease.x1, t.ease.y1, t.ease.x2, t.ease.y2]
                .into_iter()
                .enumerate()
            {
                if j > 0 {
                    w.write_char(',')?;
                }
                json_num(w, v)?;
            }
            w.write_str("],\"iterations\":")?;
            match t.iterations {
                Some(n) => write!(w, "{n}")?,
                None => w.write_str("null")?,
            }
            w.write_str(",\"keys\":[")?;
            for (j, k) in t.keys.iter().enumerate() {
                if j > 0 {
                    w.write_char(',')?;
                }
                w.write_str("{\"offset\":")?;
                json_num(w, k.offset)?;
                w.write_str(",\"property\":")?;
                json_str(w, k.property.name())?;
                w.write_str(",\"value\":")?;
                match k.value {
                    Value::Number(v) => json_num(w, v)?,
                    Value::Color(c) => {
                        let mut css = String::new();
                        write!(css, "{c}")?;
                        json_str(w, &css)?;
                    }
                }
                w.write_char('}')?;
            }
            w.write_str("]}")?;
        }
        w.write_char(']')?;
        if let Some(r) = &self.reveal {
            w.write_str(",\"reveal\":{\"shape\":")?;
            let pair = |w: &mut W, (x, y): (f32, f32)| -> fmt::Result {
                w.write_char('[')?;
                json_num(w, x)?;
                w.write_char(',')?;
                json_num(w, y)?;
                w.write_char(']')
            };
            match r.shape {
                RevealShape::Wipe { from, to } => {
                    w.write_str("\"wipe\",\"from\":")?;
                    pair(w, from)?;
                    w.write_str(",\"to\":")?;
                    pair(w, to)?;
                }
                RevealShape::Iris { center, radius } => {
                    w.write_str("\"iris\",\"center\":")?;
                    pair(w, center)?;
                    w.write_str(",\"radius\":")?;
                    json_num(w, radius)?;
                }
            }
            w.write_str(",\"feather\":")?;
            json_num(w, r.feather)?;
            write!(w, ",\"layers\":[{},{}]}}", r.layers.0, r.layers.1)?;
        }
        write!(w, ",\"totalMs\":{},\"trigger\":", self.total_ms)?;
        match &self.trigger {
            Trigger::Load => w.write_str("\"load\"")?,
            Trigger::Hover => w.write_str("\"hover\"")?,
            Trigger::Class(c) => {
                w.write_str("{\"class\":")?;
                json_str(w, c)?;
                w.write_char('}')?;
            }
        }
        w.write_str(",\"reducedMotion\":\"final-state\"}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Ecc, QrCode};

    #[test]
    fn curves_ease_out() {
        let quart = Ease::Quart.curve();
        assert_eq!(quart.at(0.0), 0.0);
        assert!((quart.at(1.0) - 1.0).abs() < 1e-9);
        // Ease-outs are ahead of linear everywhere in between.
        for i in 1..10 {
            let x = i as f64 / 10.0;
            assert!(quart.at(x) > x, "{x}");
        }
        assert!((CubicBezier::LINEAR.at(0.3) - 0.3).abs() < 1e-9);
    }

    #[test]
    fn groups_partition_the_modules() {
        let qr = QrCode::encode("https://example.com/menu", Ecc::M).unwrap();
        for shape in [
            ModuleShape::SQUARE,
            ModuleShape::Dots { size: 0.9 },
            ModuleShape::Liquid { smoothness: 1.0 },
        ] {
            let style = Style::new().modules(shape).intro(Preset::Settle);
            let canvas = scene::canvas(&style, &qr);
            let layers = scene::layers(&style, &qr);
            let pieces = pieces(&style, &qr, &canvas, style.intro.as_ref().unwrap(), &layers);
            // Every data module is in exactly one piece.
            let Part::Modules(grid) = &layers[0].part else {
                panic!("data first");
            };
            let mut count = 0;
            for p in pieces.iter().filter(|p| p.layer == 0) {
                if let Unit::Cells(cells) = &p.unit {
                    for &(x, y) in cells {
                        assert!(grid.get(x as i64, y as i64));
                        count += 1;
                    }
                }
            }
            let dark = (0..qr.size() as i64)
                .flat_map(|y| (0..qr.size() as i64).map(move |x| (x, y)))
                .filter(|&(x, y)| grid.get(x, y))
                .count();
            assert_eq!(count, dark);
            let groups: Vec<u8> = pieces.iter().map(|p| p.group).collect();
            assert!(groups.iter().all(|&g| g < 12));
            assert!(groups.contains(&0) && groups.contains(&11));
        }
    }

    #[test]
    fn regions_are_four_connected() {
        let mut grid = Grid::new(4);
        for (x, y) in [(0, 0), (1, 0), (1, 1), (2, 2), (3, 2)] {
            grid.set(x, y);
        }
        let r = regions(&grid, 4, true);
        assert_eq!(
            r,
            alloc::vec![
                alloc::vec![(0, 0), (1, 0), (1, 1)],
                alloc::vec![(2, 2), (3, 2)]
            ]
        );
        assert_eq!(regions(&grid, 4, false).len(), 5);
    }

    #[test]
    fn stagger_fits_the_duration() {
        let qr = QrCode::encode("https://example.com/menu", Ecc::M).unwrap();
        for ms in [600, 1200, 2000] {
            let style = Style::new().intro(Intro::new(Preset::Settle).duration_ms(ms));
            let plan = style.plan(&qr).unwrap();
            assert_eq!(plan.total_ms, ms);
            assert!(plan.tracks.iter().all(|t| t.duration_ms <= 800));
            let first = plan.tracks.iter().map(|t| t.delay_ms).min().unwrap();
            assert_eq!(first, 0);
        }
    }

    #[test]
    fn frames_end_on_the_static_state() {
        let qr = QrCode::encode("https://example.com/menu", Ecc::M).unwrap();
        let style = Style::new().intro(Preset::Settle);
        let plan = style.plan(&qr).unwrap();
        let end = style.render_frame(&qr, plan.total_ms).unwrap();
        assert!(!end.contains("opacity=") && !end.contains("transform="));
        let start = style.render_frame(&qr, 0).unwrap();
        assert!(start.contains("opacity=\"0\""));
        assert!(!start.contains("@keyframes"));
    }

    #[test]
    fn css_is_inside_supports_and_has_no_end_keyframes() {
        let qr = QrCode::encode("HELLO", Ecc::M).unwrap();
        let svg = Style::new().intro(Preset::Settle).render(&qr).unwrap();
        let style = &svg[svg.find("<style>").unwrap()..svg.find("</style>").unwrap()];
        let supports = style.find("@supports").unwrap();
        assert!(style.find("@keyframes").unwrap() > supports);
        assert!(style.contains("@media (prefers-reduced-motion:reduce)"));
        assert!(!style.contains("100%{") && !style.contains("to{"));
        assert!(style.contains("{0%{opacity:0;transform:scale(.92)}}"));
        assert!(svg.contains("class=\"qr qr"));
    }

    #[test]
    fn class_triggers_are_escaped() {
        let qr = QrCode::encode("HELLO", Ecc::M).unwrap();
        let intro = Intro::new(Preset::Settle).trigger(Trigger::Class(String::from("1</style>")));
        let svg = Style::new().intro(intro).render(&qr).unwrap();
        assert!(!svg.contains("1</style>"));
        assert!(svg.contains(".\\31 \\3c \\2f style\\3e  ."));
        assert!(svg.contains("animation-play-state:paused"));
    }

    #[test]
    fn reveals_mask_through_the_stylesheet() {
        let qr = QrCode::encode("HELLO", Ecc::M).unwrap();
        for preset in [Preset::Sweep, Preset::Raster, Preset::Iris] {
            let style = Style::new().intro(preset);
            let svg = style.render(&qr).unwrap();
            // Only CSS applies the mask; the document alone is unmasked.
            assert!(!svg.contains(" mask=\""), "{preset:?}");
            assert!(svg.contains("-m{mask:url(#"), "{preset:?}");
            assert_eq!(svg.matches("<mask ").count(), 1);
            let plan = style.plan(&qr).unwrap();
            assert_eq!(plan.tracks[0].target, Target::Reveal);
            assert_eq!(plan.reveal.unwrap().layers.0, 1, "background stays out");
            // Frames carry the mask until it's fully open.
            assert!(style.render_frame(&qr, 100).unwrap().contains(" mask=\""));
            assert!(
                !style
                    .render_frame(&qr, plan.total_ms)
                    .unwrap()
                    .contains("<mask")
            );
        }
    }

    #[test]
    fn lock_on_starts_with_the_finders() {
        let qr = QrCode::encode("https://example.com/menu", Ecc::M).unwrap();
        let plan = Style::new().intro(Preset::LockOn).plan(&qr).unwrap();
        let start = |role| {
            plan.tracks
                .iter()
                .filter(|t| matches!(t.target, Target::Layer(i) if plan.layers[i].role == role))
                .map(|t| t.delay_ms)
                .min()
                .unwrap()
        };
        assert_eq!(start(LayerRole::FinderOuter), 0);
        assert_eq!(start(LayerRole::FinderEye), 150);
        // The rest waits for 60% of the finders' 480 ms.
        assert_eq!(start(LayerRole::Data), 288);
    }

    #[test]
    fn assemble_offsets_are_seeded_and_bounded() {
        for g in 0..16 {
            let (dx, dy) = scatter(7, g);
            let r = math::sqrt((dx * dx + dy * dy) as f64);
            assert!((1.49..=3.01).contains(&r), "{r}");
            assert_eq!(scatter(7, g), (dx, dy));
        }
        assert_ne!(scatter(7, 0), scatter(8, 0));
    }

    #[test]
    fn palette_shifts_fade_fills_and_end_on_the_tokens() {
        let qr = QrCode::encode("HELLO", Ecc::M).unwrap();
        let old = Palette::from_tokens([("fg", "#7a1f10")]).unwrap();
        let style = Style::new().intro(Intro::palette_from(&old));
        let svg = style.render(&qr).unwrap();
        let css = &svg[..svg.find("</style>").unwrap()];
        assert!(css.contains("{0%{fill:#7a1f10}12.5%{fill:"));
        assert!(!css.contains("100%"));
        // bg didn't change, so it has no track; fg and the fg-colored
        // finders and timing fade.
        let plan = style.plan(&qr).unwrap();
        assert!(plan.tracks.iter().all(|t| {
            plan.layers[match t.target {
                Target::Layer(i) => i,
                Target::Reveal => unreachable!(),
            }]
            .token
                != Token::Bg
        }));
        assert_eq!(plan.total_ms, 600);
        let mid = style.render_frame(&qr, 300).unwrap();
        assert!(!mid.contains("fill=\"#17161c\"") && !mid.contains("fill=\"#7a1f10\""));
        assert_eq!(
            style
                .render_frame(&qr, 600)
                .unwrap()
                .matches("fill=\"#17161c\"")
                .count(),
            Style::new()
                .render(&qr)
                .unwrap()
                .matches("fill=\"#17161c\"")
                .count()
        );
    }

    #[test]
    fn theme_transitions_are_opt_in_and_respect_reduced_motion() {
        let qr = QrCode::encode("HELLO", Ecc::M).unwrap();
        let svg = Style::new().theme_transition(400).render(&qr).unwrap();
        assert!(svg.contains(";transition:fill 400ms cubic-bezier(.25,1,.5,1)}"));
        assert!(svg.contains("{transition:none}}"));
        assert!(!Style::new().render(&qr).unwrap().contains("transition"));
    }

    #[test]
    fn breathe_moves_away_from_the_ink() {
        let light = Style::new().palette(Palette::from_tokens([("bg", "#e8e4dc")]).unwrap());
        let far = breathe_color(&light).unwrap();
        assert!(far.to_oklab()[0] > Color::srgb(0xe8, 0xe4, 0xdc).to_oklab()[0]);
        let dark = Style::new()
            .allow_inverted(true)
            .palette(Palette::from_tokens([("bg", "#202024"), ("fg", "#f0f0f0")]).unwrap());
        let far = breathe_color(&dark).unwrap();
        assert!(far.to_oklab()[0] < Color::srgb(0x20, 0x20, 0x24).to_oklab()[0]);
        // Already white: nothing to breathe.
        let white = Style::new().palette(Palette::from_tokens([("bg", "#fff")]).unwrap());
        assert_eq!(breathe_color(&white), None);
    }

    #[test]
    fn idle_loops_are_endless_and_start_after_the_intro() {
        let qr = QrCode::encode("HELLO", Ecc::M).unwrap();
        let style = Style::new()
            .intro(Preset::Settle)
            .idle(Idle::FinderPulse)
            .idle(Idle::Shimmer);
        let plan = style.plan(&qr).unwrap();
        let loops: Vec<&Track> = plan
            .tracks
            .iter()
            .filter(|t| t.iterations.is_none())
            .collect();
        assert_eq!(loops.len(), 4, "three eyes and the band");
        assert!(loops.iter().all(|t| t.delay_ms == plan.total_ms));
        let svg = style.render(&qr).unwrap();
        assert!(svg.contains(" infinite"));
        assert!(!svg.contains(" infinite backwards"));
        // The band rests left of the canvas.
        assert!(svg.contains("fill=\"url(#") && svg.contains("d=\"m-17.4 0h17.4v29h-17.4z\""));
    }

    fn framed(frame: crate::LogoFrame) -> Style {
        let logo = crate::Logo::svg(r#"<svg viewBox="0 0 1 1"><rect width="1" height="1"/></svg>"#)
            .unwrap()
            .scale(0.12)
            .shape(crate::LogoShape::Circle)
            .frame(frame);
        Style::new().logo(logo)
    }

    fn framed_code(style: &Style) -> QrMatrix {
        QrCode::builder()
            .encode_text_for("https://example.com", style)
            .unwrap()
    }

    #[test]
    fn the_default_logo_intro_keeps_old_prefixes() {
        // The Debug text is hashed into ID prefixes (svg::id_prefix).
        let text = alloc::format!("{:?}", Intro::new(Preset::Settle));
        assert!(text.ends_with("logo_reveal: true }"), "{text}");
        let off = alloc::format!("{:?}", Intro::new(Preset::Settle).logo_reveal(false));
        assert!(off.ends_with("logo_reveal: false }"), "{off}");
        let ink = alloc::format!(
            "{:?}",
            Intro::new(Preset::Settle)
                .logo(LogoIntro::Ink)
                .logo_timing(LogoTiming::Before)
        );
        assert!(ink.ends_with("logo: Ink, logo_timing: Before }"), "{ink}");
    }

    #[test]
    fn logo_timings_place_the_logo() {
        let style = framed(crate::LogoFrame::Ring);
        let qr = framed_code(&style);
        let logo_start = |timing| {
            let plan = style
                .clone()
                .intro(
                    Intro::new(Preset::Settle)
                        .logo(LogoIntro::Stamp)
                        .logo_timing(timing),
                )
                .plan(&qr)
                .unwrap();
            let logo = plan
                .layers
                .iter()
                .position(|l| l.role == LayerRole::Logo)
                .unwrap();
            let t = plan
                .tracks
                .iter()
                .find(|t| t.target == Target::Layer(logo))
                .unwrap();
            (t.delay_ms, t.delay_ms + t.duration_ms, plan.total_ms)
        };
        assert_eq!(logo_start(LogoTiming::After), (750, 1200, 1200));
        assert_eq!(logo_start(LogoTiming::Before), (0, 450, 1200));
        assert_eq!(logo_start(LogoTiming::With), (375, 825, 1200));
    }

    #[test]
    fn ink_draw_and_turn_reach_the_document() {
        let ring = framed(crate::LogoFrame::Ring);
        let qr = framed_code(&ring);
        let ink = ring
            .clone()
            .intro(Intro::new(Preset::Settle).logo(LogoIntro::Ink));
        let plan = ink.plan(&qr).unwrap();
        let silhouette = plan
            .layers
            .iter()
            .find(|l| l.role == LayerRole::LogoInk)
            .unwrap();
        assert_eq!(silhouette.opacity, 0.0);
        let svg = ink.render(&qr).unwrap();
        assert!(svg.contains("mask-type=\"alpha\"") && svg.contains("-logo-art\""));
        assert!(svg.contains("opacity=\"0\""));

        let draw = ring
            .clone()
            .intro(Intro::new(Preset::Settle).logo(LogoIntro::Draw));
        let svg = draw.render(&qr).unwrap();
        assert!(svg.contains("stroke-dasharray=") && svg.contains("stroke-dashoffset:"));
        let plan = draw.plan(&qr).unwrap();
        let frame = plan
            .layers
            .iter()
            .find(|l| l.role == LayerRole::LogoFrame)
            .unwrap();
        let outline = frame.outline.as_ref().unwrap();
        assert_eq!(outline.dashes.len(), 2);
        assert!(outline.dashes[0] > outline.length);
        assert!(plan.to_json().contains("\"outline\":{\"width\":"));

        let turn = ring.idle(Idle::LogoTurn).idle(Idle::LogoGlint);
        let svg = turn.render(&qr).unwrap();
        assert!(svg.contains("rotate(-360deg)") && svg.contains("-glint-clip"));
        // A frame between turns is rotated about the plate's center.
        let plan = turn.plan(&qr).unwrap();
        let frame = turn.render_frame(&qr, plan.total_ms + 6000).unwrap();
        assert!(frame.contains("transform=\"matrix("));
    }

    #[test]
    fn shrink_starts_as_wide_as_the_code() {
        let ring = framed(crate::LogoFrame::Ring);
        let qr = framed_code(&ring);
        for timing in LogoTiming::ALL {
            let style = ring.clone().intro(
                Intro::new(Preset::Settle)
                    .logo(LogoIntro::Shrink)
                    .logo_timing(timing),
            );
            let plan = style.plan(&qr).unwrap();
            let plate = 2.0 * plate_half(&style, &qr);
            for (i, layer) in plan.layers.iter().enumerate() {
                let mine: Vec<&Track> = plan
                    .tracks
                    .iter()
                    .filter(|t| t.target == Target::Layer(i))
                    .collect();
                if !layer.role.is_logo() {
                    // The code plays over the whole intro too.
                    assert!(mine.iter().all(|t| t.delay_ms + t.duration_ms <= 1200));
                    continue;
                }
                let [track] = mine[..] else {
                    panic!("{:?}: {mine:?}", layer.role)
                };
                assert_eq!((track.delay_ms, track.duration_ms), (0, 1200), "{timing:?}");
                let wide = track
                    .keys
                    .iter()
                    .find(|k| k.property == Property::Scale)
                    .unwrap();
                let Value::Number(v) = wide.value else {
                    unreachable!()
                };
                assert!((v as f64 * plate - qr.size() as f64).abs() < 1e-3);
            }
        }
    }

    #[test]
    fn a_frame_cut_loose_plays_its_own_intro() {
        let ring = framed(crate::LogoFrame::Ring);
        let qr = framed_code(&ring);
        let span = |intro: Intro, role| {
            let plan = ring.clone().intro(intro).plan(&qr).unwrap();
            let i = plan.layers.iter().position(|l| l.role == role).unwrap();
            let t = plan
                .tracks
                .iter()
                .find(|t| t.target == Target::Layer(i))
                .unwrap()
                .clone();
            (t.delay_ms, t.delay_ms + t.duration_ms, t.keys)
        };
        // The logo shrinks over the whole intro; the ring draws in last.
        let cut = Intro::new(Preset::Settle)
            .logo(LogoIntro::Shrink)
            .frame(LogoIntro::Draw)
            .frame_timing(LogoTiming::After);
        let (start, end, _) = span(cut.clone(), LayerRole::Logo);
        assert_eq!((start, end), (0, 1200));
        let (start, end, keys) = span(cut.clone(), LayerRole::LogoFrame);
        assert_eq!((start, end), (500, 990));
        assert!(keys.iter().all(|k| k.property == Property::Draw));
        // Followed, the ring would shrink with the logo instead.
        let (start, _, keys) = span(
            Intro::new(Preset::Settle).logo(LogoIntro::Shrink),
            LayerRole::LogoFrame,
        );
        assert_eq!(start, 0);
        assert!(keys.iter().any(|k| k.property == Property::Scale));
        // Cut loose, a Draw logo no longer waits for its frame.
        let (start, _, _) = span(
            Intro::new(Preset::Settle)
                .logo(LogoIntro::Draw)
                .frame(LogoIntro::None),
            LayerRole::Logo,
        );
        assert_eq!(start, 1200 - 700);
        // The code makes room for both: the frame first, the logo last.
        let both = Intro::new(Preset::Settle)
            .logo(LogoIntro::Stamp)
            .frame(LogoIntro::Spin)
            .frame_timing(LogoTiming::Before);
        let style = ring.clone().intro(both.clone());
        let plan = style.plan(&qr).unwrap();
        let code: Vec<&Track> = plan
            .tracks
            .iter()
            .filter(|t| match t.target {
                Target::Layer(i) => !plan.layers[i].role.is_logo(),
                _ => false,
            })
            .collect();
        let first = code.iter().map(|t| t.delay_ms).min().unwrap();
        let last = code
            .iter()
            .map(|t| t.delay_ms + t.duration_ms)
            .max()
            .unwrap();
        assert_eq!((first, last), (300, 1200 - 225));
        let (start, end, keys) = span(both.clone(), LayerRole::LogoFrame);
        assert_eq!((start, end), (0, 600));
        assert!(keys.contains(&number(Property::Rotate, 0.0, -90.0)));
        let (start, _, _) = span(both, LayerRole::Logo);
        assert_eq!(start, 1200 - 450);
        // The timing alone cuts the frame loose with the logo's intro.
        let timed = Intro::new(Preset::Settle).frame_timing(LogoTiming::Before);
        assert_eq!(timed.frame_intro(), Some(LogoIntro::Reveal));
        assert_eq!(Intro::new(Preset::Settle).frame_intro(), None);
        let text = alloc::format!("{:?}", Intro::new(Preset::Settle).frame(LogoIntro::Spin));
        assert!(text.ends_with("frame: Spin }"), "{text}");
    }

    #[test]
    fn spin_and_flip_move_the_art_alone() {
        let style = framed(crate::LogoFrame::Ring);
        let qr = framed_code(&style);
        let still = style.render(&qr).unwrap();
        for idles in [
            &[Idle::LogoSpin][..],
            &[Idle::LogoFlip],
            &[Idle::LogoSpin, Idle::LogoFlip],
        ] {
            let moving = idles.iter().fold(style.clone(), |s, &i| s.idle(i));
            let plan = moving.plan(&qr).unwrap();
            let art = plan
                .layers
                .iter()
                .position(|l| l.role == LayerRole::LogoArt)
                .unwrap();
            assert_eq!(plan.layers[art - 1].role, LayerRole::Logo);
            let [track] = &plan.tracks[..] else {
                panic!("{:?}", plan.tracks)
            };
            assert_eq!(track.target, Target::Layer(art));
            assert_eq!((track.iterations, track.duration_ms), (None, 8000));
            // Every key ends where it started, at rest.
            let at = |p: Property, offset: f32| {
                track
                    .keys
                    .iter()
                    .rfind(|k| k.property == p && k.offset <= offset)
                    .map(|k| k.value)
            };
            if idles.contains(&Idle::LogoSpin) {
                assert_eq!(at(Property::Rotate, 0.0), Some(Value::Number(-360.0)));
                assert_eq!(at(Property::Rotate, 1.0), Some(Value::Number(0.0)));
            }
            if idles.contains(&Idle::LogoFlip) {
                assert_eq!(at(Property::ScaleY, 1.0), Some(Value::Number(1.0)));
                let lowest = track
                    .keys
                    .iter()
                    .filter_map(|k| match (k.property, k.value) {
                        (Property::ScaleY, Value::Number(v)) => Some(v),
                        _ => None,
                    })
                    .fold(1.0f32, f32::min);
                assert!(lowest < -0.9, "{lowest}");
            }
            let svg = moving.render(&qr).unwrap();
            assert!(svg.contains("-logo-clip)\"><g class="));
            // At rest it is the still picture's logo, plate apart.
            let frame = moving.render_frame(&qr, 7000).unwrap();
            assert!(!frame.contains("matrix("));
            assert_eq!(
                still.matches("<image").count() + still.matches("<svg").count(),
                frame.matches("<image").count() + frame.matches("<svg").count()
            );
            // Mid-move it turns or squashes.
            let mid = moving.render_frame(&qr, 400).unwrap();
            assert!(mid.contains("transform=\"matrix("), "{idles:?}");
        }
        let both = style.idle(Idle::LogoSpin).idle(Idle::LogoFlip);
        let svg = both.render(&qr).unwrap();
        assert!(svg.contains("rotate(0deg) scale(1,"));
    }

    #[test]
    fn json_is_deterministic_and_complete() {
        let qr = QrCode::encode("HELLO", Ecc::M).unwrap();
        let style = Style::new().intro(Preset::Settle);
        let a = style.plan(&qr).unwrap().to_json();
        assert_eq!(a, style.plan(&qr).unwrap().to_json());
        assert!(a.starts_with("{\"viewBox\":[29,29],\"palette\":{\"bg\":\"#fefcf8\""));
        assert!(a.contains("\"role\":\"finder-eye\",\"token\":\"finder.eye\""));
        assert!(a.ends_with(",\"trigger\":\"load\",\"reducedMotion\":\"final-state\"}"));
        let still = Style::new().plan(&qr).unwrap();
        assert!(still.tracks.is_empty());
    }
}
