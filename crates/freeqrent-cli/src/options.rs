//! Turning command-line values into style options. Each parser takes the
//! text after the flag, e.g. `dots:0.8` for `--shape`.

use freeqrent::{
    AlignmentShape, Axis, Corner, CubicBezier, Ease, Ecc, Emphasis, EyeShape, Frame, FrameRing,
    LogoShape, ModuleShape, Preset, RingShape, StaggerOrder, TimingStyle, Trigger,
};

/// Splits `name:param` and parses the parameter as a number.
fn named(value: &str) -> Result<(&str, Option<f32>), String> {
    match value.split_once(':') {
        None => Ok((value, None)),
        Some((name, param)) => {
            let number = param
                .parse::<f32>()
                .map_err(|_| format!("{param:?} in {value:?} isn't a number"))?;
            Ok((name, Some(number)))
        }
    }
}

fn no_param(name: &str, param: Option<f32>) -> Result<(), String> {
    match param {
        None => Ok(()),
        Some(_) => Err(format!("{name} takes no parameter")),
    }
}

/// `square[:gap]`, `rounded[:radius]`, `dots[:size]`, `liquid[:smoothness]`,
/// `bars-v[:radius]`, `bars-h[:radius]`.
pub fn shape(value: &str) -> Result<ModuleShape, String> {
    let (name, param) = named(value)?;
    Ok(match name {
        "square" => ModuleShape::Square {
            gap: param.unwrap_or(0.0),
        },
        "rounded" => ModuleShape::Rounded {
            radius: param.unwrap_or(0.35),
        },
        "dots" => ModuleShape::Dots {
            size: param.unwrap_or(0.85),
        },
        "liquid" => ModuleShape::Liquid {
            smoothness: param.unwrap_or(0.8),
        },
        "bars-v" | "bars-h" => ModuleShape::Bars {
            axis: if name == "bars-v" {
                Axis::Vertical
            } else {
                Axis::Horizontal
            },
            radius: param.unwrap_or(0.5),
        },
        _ => {
            return Err(format!(
                "unknown shape {name:?}: try square, rounded, dots, liquid, bars-v or bars-h"
            ));
        }
    })
}

/// `square`, `rounded[:radius]`, `squircle`, `leaf`.
pub fn ring(value: &str) -> Result<RingShape, String> {
    let (name, param) = named(value)?;
    Ok(match name {
        "square" => {
            no_param(name, param)?;
            RingShape::Square
        }
        "rounded" => RingShape::Rounded(param.unwrap_or(1.5)),
        "squircle" => {
            no_param(name, param)?;
            RingShape::Squircle
        }
        "leaf" => {
            no_param(name, param)?;
            RingShape::Leaf
        }
        _ => {
            return Err(format!(
                "unknown ring {name:?}: try square, rounded, squircle or leaf"
            ));
        }
    })
}

/// `square`, `rounded[:radius]`, `circle`, `squircle`, `leaf`.
pub fn eye(value: &str) -> Result<EyeShape, String> {
    let (name, param) = named(value)?;
    let plain = |shape| no_param(name, param).map(|()| shape);
    match name {
        "square" => plain(EyeShape::Square),
        "rounded" => Ok(EyeShape::Rounded(param.unwrap_or(0.75))),
        "circle" => plain(EyeShape::Circle),
        "squircle" => plain(EyeShape::Squircle),
        "leaf" => plain(EyeShape::Leaf),
        _ => Err(format!(
            "unknown eye {name:?}: try square, rounded, circle, squircle or leaf"
        )),
    }
}

/// The eye that goes with a ring when only the ring is given.
fn matching_eye(ring: RingShape) -> EyeShape {
    match ring {
        RingShape::Square => EyeShape::Square,
        RingShape::Rounded(r) => EyeShape::Rounded((r / 2.0).min(1.5)),
        RingShape::Squircle => EyeShape::Squircle,
        RingShape::Leaf => EyeShape::Leaf,
    }
}

/// `RING[/EYE]`, e.g. `rounded:2/circle`.
pub fn finder(value: &str) -> Result<(RingShape, EyeShape), String> {
    match value.split_once('/') {
        None => {
            let r = ring(value)?;
            Ok((r, matching_eye(r)))
        }
        Some((r, e)) => Ok((ring(r)?, eye(e)?)),
    }
}

/// `CORNER=RING[/EYE]` with CORNER one of `top-left`, `top-right`,
/// `bottom-left`.
pub fn corner(value: &str) -> Result<(Corner, RingShape, EyeShape), String> {
    let (name, shapes) = value
        .split_once('=')
        .ok_or_else(|| format!("expected CORNER=RING[/EYE], got {value:?}"))?;
    let corner = match name {
        "top-left" => Corner::TopLeft,
        "top-right" => Corner::TopRight,
        "bottom-left" => Corner::BottomLeft,
        _ => {
            return Err(format!(
                "unknown corner {name:?}: try top-left, top-right or bottom-left"
            ));
        }
    };
    let (ring, eye) = finder(shapes)?;
    Ok((corner, ring, eye))
}

/// `match` or `RING[/EYE]` with alignment-sized radii.
pub fn alignment(value: &str) -> Result<AlignmentShape, String> {
    if value == "match" {
        return Ok(AlignmentShape::Match);
    }
    let (outer, eye) = match value.split_once('/') {
        None => {
            let r = ring(value)?;
            let e = match r {
                RingShape::Rounded(_) => EyeShape::Circle,
                other => matching_eye(other),
            };
            (r, e)
        }
        Some((r, e)) => (ring(r)?, eye(e)?),
    };
    Ok(AlignmentShape::Own { outer, eye })
}

/// `none`, `accent`, `outline`.
pub fn emphasis(value: &str) -> Result<Emphasis, String> {
    match value {
        "none" => Ok(Emphasis::None),
        "accent" => Ok(Emphasis::Accent),
        "outline" => Ok(Emphasis::Outline),
        _ => Err(format!(
            "unknown emphasis {value:?}: try none, accent or outline"
        )),
    }
}

/// `plain`, `dotted`, `dashed`.
pub fn timing(value: &str) -> Result<TimingStyle, String> {
    match value {
        "plain" => Ok(TimingStyle::Plain),
        "dotted" => Ok(TimingStyle::Dotted),
        "dashed" => Ok(TimingStyle::Dashed),
        _ => Err(format!(
            "unknown timing {value:?}: try plain, dotted or dashed"
        )),
    }
}

/// `square`, `rounded[:radius]`, `circle[:density]`, `none`. The ring of
/// a circle frame comes from `--frame-ring`.
pub fn frame(value: &str, ring: FrameRing) -> Result<Frame, String> {
    let (name, param) = named(value)?;
    match name {
        "square" => no_param(name, param).map(|()| Frame::Square),
        "rounded" => Ok(Frame::RoundedSquare(param.unwrap_or(2.0))),
        "circle" => Ok(Frame::Circle {
            filler_density: param.unwrap_or(0.5),
            ring,
        }),
        "none" => no_param(name, param).map(|()| Frame::None),
        _ => Err(format!(
            "unknown frame {name:?}: try square, rounded, circle or none"
        )),
    }
}

/// `none`, `solid`, `dashed`.
pub fn frame_ring(value: &str) -> Result<FrameRing, String> {
    match value {
        "none" => Ok(FrameRing::None),
        "solid" => Ok(FrameRing::Solid),
        "dashed" => Ok(FrameRing::Dashed),
        _ => Err(format!(
            "unknown frame ring {value:?}: try none, solid or dashed"
        )),
    }
}

/// `square`, `rounded[:radius]`, `circle`.
pub fn logo_shape(value: &str) -> Result<LogoShape, String> {
    let (name, param) = named(value)?;
    match name {
        "square" => no_param(name, param).map(|()| LogoShape::Square),
        "rounded" => Ok(LogoShape::Rounded(param.unwrap_or(1.0))),
        "circle" => no_param(name, param).map(|()| LogoShape::Circle),
        _ => Err(format!(
            "unknown logo shape {name:?}: try square, rounded or circle"
        )),
    }
}

/// An intro preset name.
pub fn preset(value: &str) -> Result<Preset, String> {
    Preset::ALL
        .into_iter()
        .find(|p| p.name() == value)
        .ok_or_else(|| {
            let names: Vec<&str> = Preset::ALL.iter().map(|p| p.name()).collect();
            format!("unknown intro {value:?}: try {}", names.join(", "))
        })
}

/// `radial`, `linear`, `diagonal`, `random[:seed]`, `polar`.
pub fn stagger(value: &str) -> Result<StaggerOrder, String> {
    let (name, seed) = match value.split_once(':') {
        Some((name, seed)) => (
            name,
            Some(
                seed.parse::<u64>()
                    .map_err(|_| format!("seed {seed:?} isn't a whole number"))?,
            ),
        ),
        None => (value, None),
    };
    let order = match name {
        "radial" => StaggerOrder::Radial,
        "linear" => StaggerOrder::Linear,
        "diagonal" => StaggerOrder::Diagonal,
        "polar" => StaggerOrder::Polar,
        "random" => return Ok(StaggerOrder::Random(seed.unwrap_or(0))),
        _ => {
            return Err(format!(
                "unknown stagger {name:?}: try radial, linear, diagonal, random[:seed] or polar"
            ));
        }
    };
    match seed {
        None => Ok(order),
        Some(_) => Err(format!("{name} takes no seed")),
    }
}

/// `quart`, `quint`, `expo`, or `x1,y1,x2,y2`.
pub fn ease(value: &str) -> Result<Ease, String> {
    match value {
        "quart" => Ok(Ease::Quart),
        "quint" => Ok(Ease::Quint),
        "expo" => Ok(Ease::Expo),
        _ => {
            let parts: Vec<f32> = value
                .split(',')
                .map(|p| p.trim().parse::<f32>())
                .collect::<Result<_, _>>()
                .map_err(|_| format!("ease {value:?}: try quart, quint, expo or x1,y1,x2,y2"))?;
            match parts[..] {
                [x1, y1, x2, y2] => Ok(Ease::Custom(CubicBezier::new(x1, y1, x2, y2))),
                _ => Err(format!("ease {value:?} needs four numbers")),
            }
        }
    }
}

/// `load`, `hover`, `class:NAME`.
pub fn trigger(value: &str) -> Result<Trigger, String> {
    match value.split_once(':') {
        None if value == "load" => Ok(Trigger::Load),
        None if value == "hover" => Ok(Trigger::Hover),
        Some(("class", name)) if !name.is_empty() => Ok(Trigger::Class(name.to_string())),
        _ => Err(format!(
            "unknown trigger {value:?}: try load, hover or class:NAME"
        )),
    }
}

/// A number for `--flag`.
pub fn number(flag: &str, value: &str) -> Result<f32, String> {
    value
        .parse()
        .map_err(|_| format!("--{flag}: {value:?} isn't a number"))
}

/// `L`, `M`, `Q`, `H` (either case).
pub fn ecc(value: &str) -> Result<Ecc, String> {
    match value.to_ascii_uppercase().as_str() {
        "L" => Ok(Ecc::L),
        "M" => Ok(Ecc::M),
        "Q" => Ok(Ecc::Q),
        "H" => Ok(Ecc::H),
        _ => Err(format!("unknown ECC level {value:?}: try L, M, Q or H")),
    }
}

/// `NAME=VALUE`, split at the first `=`.
pub fn token(value: &str) -> Result<(String, String), String> {
    value
        .split_once('=')
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .ok_or_else(|| format!("expected NAME=VALUE, got {value:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes() {
        assert_eq!(shape("square").unwrap(), ModuleShape::SQUARE);
        assert_eq!(shape("dots:0.7").unwrap(), ModuleShape::Dots { size: 0.7 });
        assert_eq!(
            shape("bars-v").unwrap(),
            ModuleShape::Bars {
                axis: Axis::Vertical,
                radius: 0.5
            }
        );
        assert_eq!(
            shape("liquid").unwrap(),
            ModuleShape::Liquid { smoothness: 0.8 }
        );
        assert!(shape("dots:big").is_err());
        assert!(shape("hex").is_err());
    }

    #[test]
    fn finders_and_corners() {
        assert_eq!(
            finder("rounded:2/circle").unwrap(),
            (RingShape::Rounded(2.0), EyeShape::Circle)
        );
        assert_eq!(
            finder("squircle").unwrap(),
            (RingShape::Squircle, EyeShape::Squircle)
        );
        assert_eq!(
            finder("rounded:2").unwrap(),
            (RingShape::Rounded(2.0), EyeShape::Rounded(1.0))
        );
        assert!(finder("leaf:2").is_err());
        assert_eq!(
            corner("top-right=leaf/circle").unwrap(),
            (Corner::TopRight, RingShape::Leaf, EyeShape::Circle)
        );
        assert!(corner("middle=square").is_err());
    }

    #[test]
    fn the_rest() {
        assert_eq!(alignment("match").unwrap(), AlignmentShape::Match);
        assert_eq!(
            alignment("rounded:2.5").unwrap(),
            AlignmentShape::Own {
                outer: RingShape::Rounded(2.5),
                eye: EyeShape::Circle
            }
        );
        assert_eq!(emphasis("outline").unwrap(), Emphasis::Outline);
        assert_eq!(timing("dotted").unwrap(), TimingStyle::Dotted);
        assert_eq!(ecc("q").unwrap(), Ecc::Q);
        assert_eq!(
            frame("circle:0.4", FrameRing::Dashed).unwrap(),
            Frame::Circle {
                filler_density: 0.4,
                ring: FrameRing::Dashed
            }
        );
        assert_eq!(
            frame("rounded", FrameRing::None).unwrap(),
            Frame::RoundedSquare(2.0)
        );
        assert!(frame("none:1", FrameRing::None).is_err());
        assert!(frame_ring("dotted").is_err());
        assert_eq!(logo_shape("rounded:2").unwrap(), LogoShape::Rounded(2.0));
        assert!(logo_shape("circle:1").is_err());
        assert_eq!(preset("lock-on").unwrap(), Preset::LockOn);
        assert!(preset("bloom").is_err());
        assert_eq!(stagger("random:7").unwrap(), StaggerOrder::Random(7));
        assert!(stagger("radial:1").is_err());
        assert_eq!(
            ease("0.2,1,0.4,1").unwrap(),
            Ease::Custom(CubicBezier::new(0.2, 1.0, 0.4, 1.0))
        );
        assert_eq!(trigger("class:in").unwrap(), Trigger::Class("in".into()));
        assert!(trigger("class:").is_err());
        assert_eq!(
            token("fg=oklch(32% .08 265)").unwrap(),
            ("fg".to_string(), "oklch(32% .08 265)".to_string())
        );
    }
}
