// The AnimationPlan (SPEC §8.5) as Dart data, and its evaluation: where
// each layer is at a time t, the same arithmetic the core uses for its
// frames (crates/freeqrent/src/animate.rs, `progress`, `segment`, `sample`,
// `pose`), so a native player lands on the same poses as render_frame.
import 'dart:convert';
import 'dart:math' as math;

import 'color.dart';

/// A point or size in viewBox units (modules).
typedef PlanPoint = ({double x, double y});

PlanPoint _point(Object? v) {
  final l = v as List;
  return (x: (l[0] as num).toDouble(), y: (l[1] as num).toDouble());
}

double _d(Object? v, [double fallback = 0]) =>
    v == null ? fallback : (v as num).toDouble();

/// What a layer is (`LayerRole`).
enum PlanRole {
  background('background'),
  shadow('shadow'),
  grain('grain'),
  caption('caption'),
  scanlines('scanlines'),
  border('border'),
  ring('ring'),
  filler('filler'),
  data('data'),
  timing('timing'),
  alignment('alignment'),
  finderOuter('finder-outer'),
  finderEye('finder-eye'),
  logo('logo'),
  shimmer('shimmer'),
  halftone('halftone'),
  logoFrame('logo-frame'),
  logoInk('logo-ink'),
  logoGlint('logo-glint'),
  logoArt('logo-art'),
  unknown('');

  const PlanRole(this.json);

  /// The role's name in the plan.
  final String json;

  /// The role named [name], or [unknown] for one added after this package.
  static PlanRole of(String name) =>
      values.firstWhere((r) => r.json == name, orElse: () => unknown);

  /// The frame's chrome: drawn around the code, never filtered.
  bool get isChrome => const {
    background,
    shadow,
    grain,
    caption,
    scanlines,
    border,
  }.contains(this);

  /// A part of the logo.
  bool get isLogo =>
      const {logo, logoFrame, logoInk, logoGlint, logoArt}.contains(this);

  /// Whether the plan's effect filters this layer (the core's
  /// `takes_effects`): the code, not the chrome, the logo or the shimmer.
  bool get takesEffect => !isChrome && !isLogo && this != shimmer;
}

/// A logo frame stroked along its path.
final class PlanOutline {
  PlanOutline._(this.width, this.length, this.dashes, this.dashOffset);

  factory PlanOutline._fromJson(Map<String, Object?> j) => PlanOutline._(
    _d(j['width']),
    _d(j['length']),
    [for (final d in j['dashes'] as List) _d(d)],
    _d(j['dashOffset']),
  );

  /// Stroke width, in viewBox units.
  final double width;

  /// The path's length: what a `draw` of 1 draws.
  final double length;

  /// Dash and gap lengths (empty for a solid stroke).
  final List<double> dashes;

  /// Where the dash pattern starts along the path.
  final double dashOffset;
}

/// One drawn layer, in paint order.
final class PlanLayer {
  PlanLayer._fromJson(Map<String, Object?> j)
    : roleName = j['role'] as String,
      role = PlanRole.of(j['role'] as String),
      token = j['token'] as String,
      color = j['color'] as String?,
      cells = j['cells'] == null ? null : _point(j['cells']),
      stroke = j['stroke'] == null ? null : _d(j['stroke']),
      outline = j['outline'] == null
          ? null
          : PlanOutline._fromJson(j['outline'] as Map<String, Object?>),
      opacity = _d(j['opacity'], 1),
      evenOdd = j['evenodd'] == true,
      fillOpacity = _d(j['fillOpacity'], 1),
      group = j['group'] as int?,
      corner = j['corner'] as String?,
      origin = _point(j['origin']),
      path = j['path'] as String;

  /// The role as the plan names it.
  final String roleName;

  /// What it is.
  final PlanRole role;

  /// Its paint's token, looked up in [AnimationPlan.palette].
  final String token;

  /// A fixed color replacing the token's paint (a picture's colors).
  final String? color;

  /// For a picture's sub-cells: the path is in fifths of a module from this
  /// point.
  final PlanPoint? cells;

  /// For a picture's sub-cells: stroke this wide (in the path's units, butt
  /// caps) instead of filling.
  final double? stroke;

  /// For a stroked logo frame.
  final PlanOutline? outline;

  /// Opacity at rest.
  final double opacity;

  /// Whether the path fills even-odd.
  final bool evenOdd;

  /// The share of its paint the layer keeps (uneven ink).
  final double fillOpacity;

  /// The stagger group, for split layers.
  final int? group;

  /// The finder corner, for finder layers.
  final String? corner;

  /// Transform origin, in viewBox units.
  final PlanPoint origin;

  /// SVG path data in viewBox units.
  final String path;
}

/// A keyframe property.
enum PlanProperty {
  opacity('opacity'),
  scale('scale'),
  translateX('translateX'),
  translateY('translateY'),
  fill('fill'),
  reveal('reveal'),
  rotate('rotate'),
  draw('draw'),
  scaleY('scaleY'),
  unknown('');

  const PlanProperty(this.json);

  /// The property's name in the plan.
  final String json;

  /// The property named [name], or [unknown].
  static PlanProperty of(String name) =>
      values.firstWhere((p) => p.json == name, orElse: () => unknown);
}

/// One property's value at one point of a track.
final class PlanKey {
  PlanKey._fromJson(Map<String, Object?> j)
    : offset = _d(j['offset']),
      property = PlanProperty.of(j['property'] as String),
      value = j['value'] is String ? null : _d(j['value']),
      color = j['value'] is String
          ? CssColor.parse(j['value'] as String)
          : null;

  /// Where in the track, 0–1.
  final double offset;

  /// The property.
  final PlanProperty property;

  /// A number value.
  final double? value;

  /// A color value (for [PlanProperty.fill]).
  final CssColor? color;
}

/// A cubic Bézier timing curve, as CSS `cubic-bezier()`.
final class PlanEase {
  /// The curve through these control points.
  const PlanEase(this.x1, this.y1, this.x2, this.y2);

  /// The control points.
  final double x1, y1, x2, y2;

  /// The eased progress at [x] (0–1), by the core's bisection.
  double at(double x) {
    if (x <= 0) return 0;
    if (x >= 1) return 1;
    double bez(double a, double b, double t) {
      final u = 1 - t;
      return 3 * u * u * t * a + 3 * u * t * t * b + t * t * t;
    }

    var lo = 0.0, hi = 1.0;
    for (var i = 0; i < 48; i++) {
      final mid = (lo + hi) / 2;
      if (bez(x1, x2, mid) < x) {
        lo = mid;
      } else {
        hi = mid;
      }
    }
    return bez(y1, y2, (lo + hi) / 2);
  }
}

/// One animation of one layer, or of the reveal mask.
final class PlanTrack {
  PlanTrack._(
    this.layer,
    this.delayMs,
    this.durationMs,
    this.ease,
    this.iterations,
    this.keys,
  );

  factory PlanTrack._fromJson(Map<String, Object?> j) {
    final e = [for (final v in j['ease'] as List) _d(v)];
    return PlanTrack._(
      j['target'] is Map ? (j['target'] as Map)['layer'] as int : null,
      j['delayMs'] as int,
      j['durationMs'] as int,
      PlanEase(e[0], e[1], e[2], e[3]),
      j['iterations'] as int?,
      [
        for (final k in j['keys'] as List)
          PlanKey._fromJson(k as Map<String, Object?>),
      ],
    );
  }

  /// The layer it moves, or null for the reveal mask.
  final int? layer;

  /// Whether it moves the reveal mask.
  bool get isReveal => layer == null;

  /// Start, from the trigger.
  final int delayMs;

  /// One iteration's length.
  final int durationMs;

  /// Applied between neighboring keyframes.
  final PlanEase ease;

  /// Iterations, or null for an endless loop.
  final int? iterations;

  /// The keyframes.
  final List<PlanKey> keys;

  /// Where the track is at [t] ms, 0–1 within its iteration, or null when
  /// it doesn't touch its target: a finite track shows its first keyframe
  /// while it waits, an endless one nothing until it starts.
  double? progress(double t) {
    final dur = math.max(durationMs, 1).toDouble();
    final local = t - delayMs;
    final n = iterations;
    if (n != null && local >= dur * n) return null;
    if (n != null && local < 0) return 0;
    if (n == null && local < 0) return null;
    return (local % dur) / dur;
  }

  (T, T, double)? _segment<T>(List<(double, T)> keys, T natural, double p) {
    if (keys.isEmpty) return null;
    if (keys.first.$1 > 0) keys.insert(0, (0, natural));
    if (keys.last.$1 < 1) keys.add((1, natural));
    var i = keys.length - 2;
    for (var w = 0; w + 1 < keys.length; w++) {
      if (p <= keys[w + 1].$1) {
        i = w;
        break;
      }
    }
    final (o0, v0) = keys[i];
    final (o1, v1) = keys[i + 1];
    final local = o1 > o0 ? (p - o0) / (o1 - o0) : 1.0;
    return (v0, v1, ease.at(local));
  }

  /// The value of [property] at [t] ms, if the track touches it.
  double? sample(PlanProperty property, double natural, double t) {
    final p = progress(t);
    if (p == null) return null;
    final points = <(double, double)>[
      for (final k in keys)
        if (k.property == property && k.value != null) (k.offset, k.value!),
    ];
    final s = _segment(points, natural, p);
    if (s == null) return null;
    final (a, b, e) = s;
    return a + (b - a) * e;
  }

  /// The fill at [t] ms, if the track animates it: mixed in 8-bit sRGB, as
  /// browsers and the core's frames do.
  CssColor? sampleFill(CssColor natural, double t) {
    final p = progress(t);
    if (p == null) return null;
    final points = <(double, CssColor)>[
      for (final k in keys)
        if (k.property == PlanProperty.fill && k.color != null)
          (k.offset, k.color!),
    ];
    final s = _segment(points, natural, p);
    if (s == null) return null;
    final (a, b, e) = s;
    return CssColor.lerpSrgb8(a, b, e, natural.alpha);
  }
}

/// A layer's state at one moment.
final class PlanPose {
  /// A pose.
  const PlanPose({
    this.opacity = 1,
    this.scale = 1,
    this.scaleY = 1,
    this.rotate = 0,
    this.translateX = 0,
    this.translateY = 0,
    this.draw = 1,
    this.fill,
  });

  /// Opacity, 0–1.
  final double opacity;

  /// Uniform scale about the layer's origin.
  final double scale;

  /// Vertical scale on top of [scale].
  final double scaleY;

  /// Clockwise turn about the origin, degrees.
  final double rotate;

  /// Offset, in modules.
  final double translateX, translateY;

  /// Share of an outline drawn.
  final double draw;

  /// A fill replacing the token's paint, while a fill track moves it.
  final CssColor? fill;

  /// The 2D affine transform `[a, b, c, d, e, f]` (SVG `matrix()`):
  /// scale, turn, then translate, about [origin].
  List<double> matrix(PlanPoint origin) {
    final rad = rotate * math.pi / 180;
    final sin = math.sin(rad), cos = math.cos(rad);
    final s = scale, sy = scale * scaleY;
    final a = s * cos, b = s * sin, c = -sy * sin, d = sy * cos;
    final ox = origin.x, oy = origin.y;
    return [
      a,
      b,
      c,
      d,
      ox + translateX - a * ox - c * oy,
      oy + translateY - b * ox - d * oy,
    ];
  }

  /// Whether the transform is the identity.
  bool get still =>
      scale == 1 &&
      scaleY == 1 &&
      rotate == 0 &&
      translateX == 0 &&
      translateY == 0;
}

/// The plan's reveal mask (Sweep, Raster, Iris).
final class PlanReveal {
  PlanReveal._fromJson(Map<String, Object?> j)
    : iris = j['shape'] == 'iris',
      from = j['from'] == null ? null : _point(j['from']),
      to = j['to'] == null ? null : _point(j['to']),
      center = j['center'] == null ? null : _point(j['center']),
      radius = _d(j['radius']),
      feather = _d(j['feather']),
      first = (j['layers'] as List)[0] as int,
      last = (j['layers'] as List)[1] as int;

  /// An iris (a growing circle) rather than a wipe.
  final bool iris;

  /// A wipe's edge, from and to, in viewBox units.
  final PlanPoint? from, to;

  /// An iris's center.
  final PlanPoint? center;

  /// The iris's radius at which everything shows.
  final double radius;

  /// The soft edge's width.
  final double feather;

  /// The first and last layer it covers.
  final int first, last;

  /// How far the edge travels from closed to open.
  double get travel {
    if (iris) return radius + feather;
    final dx = to!.x - from!.x, dy = to!.y - from!.y;
    return math.sqrt(dx * dx + dy * dy) + feather;
  }
}

/// What starts a plan.
final class PlanTrigger {
  PlanTrigger._(this.kind, this.className, this.everyMs);

  factory PlanTrigger._fromJson(Object? j) => switch (j) {
    'hover' => PlanTrigger._('hover', null, null),
    {'class': final String c} => PlanTrigger._('class', c, null),
    {'every': final int ms} => PlanTrigger._('every', null, ms),
    _ => PlanTrigger._('load', null, null),
  };

  /// `load`, `hover`, `class` or `every`.
  final String kind;

  /// The class name, for `class`.
  final String? className;

  /// The replay period, for `every`.
  final int? everyMs;
}

/// The plan's effect on the code's layers.
final class PlanEffect {
  PlanEffect._fromJson(Map<String, Object?> j)
    : kind = j['kind'] as String,
      radius = _d(j['radius']),
      strength = _d(j['strength']),
      depth = _d(j['depth']),
      roughness = _d(j['roughness']);

  /// `glow`, `emboss` or `sketch`.
  final String kind;

  /// A glow's blur radius and halo opacity, in modules and 0–1.
  final double radius, strength;

  /// An emboss's depth, in modules.
  final double depth;

  /// A sketch's wobble, in modules.
  final double roughness;
}

/// A texture.
final class PlanTexture {
  PlanTexture._fromJson(Map<String, Object?> j)
    : kind = j['kind'] as String,
      amount = _d(j['amount'] ?? j['opacity'] ?? j['fade']);

  /// `grain`, `scanlines` or `ink`.
  final String kind;

  /// Its amount (grain), opacity (scanlines) or fade (ink).
  final double amount;
}

/// The caption line.
final class PlanCaption {
  PlanCaption._fromJson(Map<String, Object?> j)
    : text = j['text'] as String,
      size = _d(j['size']),
      font = j['font'] as String,
      bold = j['bold'] == true,
      tracking = j['tracking'] as int,
      center = _point(j['center']);

  /// The text.
  final String text;

  /// Its size, in modules.
  final double size;

  /// `sans`, `serif` or `mono`.
  final String font;

  /// Whether it's bold.
  final bool bold;

  /// Letter spacing, in hundredths of the size.
  final int tracking;

  /// Where the line's middle sits.
  final PlanPoint center;
}

/// A style's animation as data (SPEC §8.5): the static layers and the
/// tracks that move them, from `FreeQrent.plan`.
final class AnimationPlan {
  AnimationPlan._fromJson(this.json)
    : viewBox = _point(json['viewBox']),
      palette = (json['palette'] as Map).cast<String, String>(),
      layers = [
        for (final l in json['layers'] as List)
          PlanLayer._fromJson(l as Map<String, Object?>),
      ],
      tracks = [
        for (final t in json['tracks'] as List)
          PlanTrack._fromJson(t as Map<String, Object?>),
      ],
      reveal = json['reveal'] == null
          ? null
          : PlanReveal._fromJson(json['reveal'] as Map<String, Object?>),
      totalMs = json['totalMs'] as int,
      trigger = PlanTrigger._fromJson(json['trigger']),
      reducedMotion = json['reducedMotion'] as String,
      effect = json['effect'] == null
          ? null
          : PlanEffect._fromJson(json['effect'] as Map<String, Object?>),
      caption = json['caption'] == null
          ? null
          : PlanCaption._fromJson(json['caption'] as Map<String, Object?>),
      textures = [
        for (final t in (json['textures'] as List?) ?? const [])
          PlanTexture._fromJson(t as Map<String, Object?>),
      ];

  /// Parses the plan JSON `FreeQrent.planJson` returns.
  factory AnimationPlan.parse(String text) => AnimationPlan._fromJson(
    jsonDecode(text) as Map<String, Object?>,
  );

  /// The parsed JSON, for fields this model doesn't name.
  final Map<String, Object?> json;

  /// The viewBox's width and height, in modules.
  final PlanPoint viewBox;

  /// The set tokens' CSS paints; others fall back (see [paintOf]).
  final Map<String, String> palette;

  /// The layers, in paint order.
  final List<PlanLayer> layers;

  /// The tracks; later ones win on the same property.
  final List<PlanTrack> tracks;

  /// The reveal mask, for presets that wipe the code in.
  final PlanReveal? reveal;

  /// When the last finite track ends.
  final int totalMs;

  /// What starts the tracks.
  final PlanTrigger trigger;

  /// What to show with motion off: always `final-state`.
  final String reducedMotion;

  /// The effect on the code's layers.
  final PlanEffect? effect;

  /// The caption.
  final PlanCaption? caption;

  /// The textures. Ink shows as layers' fill opacity.
  final List<PlanTexture> textures;

  /// Whether any track loops forever (idle loops).
  bool get loops => tracks.any((t) => t.iterations == null);

  /// The paint of [token], through the core's fallbacks (`fg` for most
  /// tokens, `finder.outer` for alignment and the corner rings, `bg` for
  /// the logo's backdrop; `filler` is `fg` at 35%), from [palette] or the
  /// plan's own.
  CssPaint paintOf(String token, [Map<String, String>? palette]) =>
      paintIn(token, palette ?? this.palette);

  /// [paintOf] over any palette.
  static CssPaint paintIn(String token, Map<String, String> palette) {
    final own = palette[token];
    if (own != null) return CssPaint.parse(own);
    final fallback = switch (token) {
      'bg' => null,
      'fg' => null,
      'alignment' => 'finder.outer',
      'logo.backdrop' => 'bg',
      _
          when token.startsWith('finder.') &&
              token.endsWith('.outer') &&
              token != 'finder.outer' =>
        'finder.outer',
      _
          when token.startsWith('finder.') &&
              token.endsWith('.eye') &&
              token != 'finder.eye' =>
        'finder.eye',
      _ => 'fg',
    };
    if (fallback == null) {
      return CssSolid(CssColor.parse(token == 'bg' ? '#fefcf8' : '#17161c'));
    }
    final paint = paintIn(fallback, palette);
    if (token != 'filler') return paint;
    return switch (paint) {
      CssSolid(:final color) => CssSolid(color.withAlpha(color.alpha * 0.35)),
      CssGradient(:final radial, :final angle, :final stops) => CssGradient(
        radial: radial,
        angle: angle,
        stops: [
          for (final s in stops)
            (color: s.color.withAlpha(s.color.alpha * 0.35), offset: s.offset),
        ],
      ),
    };
  }

  /// Layer [i]'s pose at [t] ms from the trigger (the core's `pose`): its
  /// rest state, with every track on it applied in order.
  PlanPose poseAt(int i, double t, {Map<String, String>? palette}) {
    final layer = layers[i];
    final rest = layer.role == PlanRole.logoInk ? 0.0 : 1.0;
    var opacity = rest, scale = 1.0, scaleY = 1.0, rotate = 0.0;
    var tx = 0.0, ty = 0.0, draw = 1.0;
    CssColor? fill;
    final natural = switch (paintOf(layer.token, palette)) {
      CssSolid(:final color) => color,
      CssGradient() => null,
    };
    for (final track in tracks) {
      if (track.layer != i) continue;
      opacity = track.sample(PlanProperty.opacity, rest, t) ?? opacity;
      scale = track.sample(PlanProperty.scale, 1, t) ?? scale;
      rotate = track.sample(PlanProperty.rotate, 0, t) ?? rotate;
      scaleY = track.sample(PlanProperty.scaleY, 1, t) ?? scaleY;
      draw = track.sample(PlanProperty.draw, 1, t) ?? draw;
      tx = track.sample(PlanProperty.translateX, 0, t) ?? tx;
      ty = track.sample(PlanProperty.translateY, 0, t) ?? ty;
      if (natural != null) {
        final c = track.sampleFill(natural, t);
        if (c != null) {
          final same = c.srgb8.join() == natural.srgb8.join();
          fill = same ? null : c;
        }
      }
    }
    return PlanPose(
      opacity: opacity,
      scale: scale,
      scaleY: scaleY,
      rotate: rotate,
      translateX: tx,
      translateY: ty,
      draw: draw,
      fill: fill,
    );
  }

  /// How far the reveal mask is open at [t] ms, 0–1 (1 without a mask).
  double revealAt(double t) {
    for (final track in tracks.reversed) {
      if (!track.isReveal) continue;
      final v = track.sample(PlanProperty.reveal, 1, t);
      if (v != null) return v;
    }
    return 1;
  }
}
