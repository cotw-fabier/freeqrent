// CSS colors and paints as the core writes them (crates/freeqrent/src/
// color.rs and palette.rs), for players that draw a plan themselves: hex,
// rgb(), hsl(), oklch(), oklab(), named colors and the core's linear and
// radial gradients. OKLCH and OKLab colors are gamut-mapped into sRGB the
// way the core does it (chroma reduced at constant lightness and hue), so a
// player paints the same 8-bit colors the SVG's fallbacks carry.
import 'dart:math' as math;

/// A color in gamma-encoded sRGB, 0–1 per channel, with alpha.
final class CssColor {
  /// A color from sRGB channels in 0–1.
  const CssColor(this.r, this.g, this.b, [this.alpha = 1]);

  /// A color from 8-bit sRGB channels.
  factory CssColor.rgb8(int r, int g, int b, [double alpha = 1]) =>
      CssColor(r / 255, g / 255, b / 255, alpha);

  /// The sRGB color for OKLab `l` (0–1), `a`, `b`, gamut-mapped.
  factory CssColor.oklab(double l, double a, double b, [double alpha = 1]) {
    final rgb = _gamutMap(l, a, b);
    return CssColor(rgb[0], rgb[1], rgb[2], alpha);
  }

  /// The sRGB color for OKLCH `l` (0–1), chroma `c` and hue `h` in degrees.
  factory CssColor.oklch(double l, double c, double h, [double alpha = 1]) {
    final rad = h * math.pi / 180;
    return CssColor.oklab(l, c * math.cos(rad), c * math.sin(rad), alpha);
  }

  /// Parses a CSS color (`#rgb[a]`, `#rrggbb[aa]`, `rgb[a]()`, `hsl[a]()`,
  /// `oklch()`, `oklab()`, a named color or `transparent`).
  factory CssColor.parse(String input) => _parseColor(input.trim());

  /// Fully transparent black.
  static const transparent = CssColor(0, 0, 0, 0);

  /// Red, green and blue, gamma-encoded, 0–1.
  final double r, g, b;

  /// Opacity, 0–1.
  final double alpha;

  /// The 8-bit sRGB channels (the core's `to_srgb8`).
  List<int> get srgb8 => [
    (r * 255).round().clamp(0, 255),
    (g * 255).round().clamp(0, 255),
    (b * 255).round().clamp(0, 255),
  ];

  /// The color as 0xAARRGGBB, for `dart:ui`'s `Color`.
  int get argb32 {
    final [r8, g8, b8] = srgb8;
    final a8 = (alpha * 255).round().clamp(0, 255);
    return a8 << 24 | r8 << 16 | g8 << 8 | b8;
  }

  /// OKLab `[L, a, b]`, ignoring alpha.
  List<double> get oklab {
    final l = [r, g, b].map(_toLinear).toList();
    return _linearToOklab(l[0], l[1], l[2]);
  }

  /// The same color with another alpha.
  CssColor withAlpha(double a) => CssColor(r, g, b, a);

  /// The color between [a] and [b] at [t], mixed in OKLab (alpha linearly).
  static CssColor lerpOklab(CssColor a, CssColor b, double t) {
    final x = a.oklab, y = b.oklab;
    double mix(double p, double q) => p + (q - p) * t;
    return CssColor.oklab(
      mix(x[0], y[0]),
      mix(x[1], y[1]),
      mix(x[2], y[2]),
      mix(a.alpha, b.alpha),
    );
  }

  /// The color between [a] and [b] at [t] the way the core's frames mix
  /// fills: 8-bit gamma-encoded sRGB, rounded, keeping [a]'s alpha.
  static CssColor lerpSrgb8(CssColor a, CssColor b, double t, double alpha) {
    final x = a.srgb8, y = b.srgb8;
    int mix(int i) => (x[i] + (y[i] - x[i]) * t).round();
    return CssColor.rgb8(mix(0), mix(1), mix(2), alpha);
  }

  @override
  bool operator ==(Object other) =>
      other is CssColor &&
      other.r == r &&
      other.g == g &&
      other.b == b &&
      other.alpha == alpha;

  @override
  int get hashCode => Object.hash(r, g, b, alpha);

  @override
  String toString() {
    final [r8, g8, b8] = srgb8;
    return 'CssColor(#${[r8, g8, b8].map((c) => c.toRadixString(16).padLeft(2, '0')).join()}, alpha $alpha)';
  }
}

/// A token's paint: a color or a gradient.
sealed class CssPaint {
  const CssPaint();

  /// Parses a CSS paint: a color or a `linear-gradient()` /
  /// `radial-gradient()`.
  factory CssPaint.parse(String input) {
    final s = input.trim();
    final lower = s.toLowerCase();
    if (lower.startsWith('linear-gradient(') ||
        lower.startsWith('radial-gradient(')) {
      return CssGradient._parse(s);
    }
    return CssSolid(CssColor.parse(s));
  }
}

/// A solid color.
final class CssSolid extends CssPaint {
  /// The paint of [color].
  const CssSolid(this.color);

  /// The color.
  final CssColor color;
}

/// A gradient stop: a color at an offset, 0–1.
typedef CssStop = ({CssColor color, double offset});

/// A gradient spanning a square, as the core draws it: linear along CSS
/// [angle] (degrees, 0 up, clockwise) or radial from the center to the
/// corners.
final class CssGradient extends CssPaint {
  /// A gradient.
  const CssGradient({
    required this.radial,
    this.angle = 180,
    required this.stops,
  });

  /// Radial (center to corners) rather than linear.
  final bool radial;

  /// The linear gradient's direction, in degrees.
  final double angle;

  /// Its stops, with every offset written out.
  final List<CssStop> stops;

  factory CssGradient._parse(String s) {
    final open = s.indexOf('(');
    final radial = s.substring(0, open).toLowerCase().startsWith('radial');
    if (!s.endsWith(')')) throw FormatException('unclosed gradient', s);
    final parts = _splitTop(s.substring(open + 1, s.length - 1), ',');
    var angle = 180.0;
    if (parts.isNotEmpty && !radial) {
      final first = parts.first.trim().toLowerCase();
      final a = _angle(first);
      if (a != null) {
        angle = a;
        parts.removeAt(0);
      } else if (first.startsWith('to ')) {
        angle = _toSide(first.substring(3));
        parts.removeAt(0);
      }
    }
    final raw = <({CssColor color, double? offset})>[];
    for (final p in parts) {
      final words = _splitTop(
        p.trim(),
        ' ',
      ).where((w) => w.isNotEmpty).toList();
      double? offset;
      if (words.length > 1 && words.last.endsWith('%')) {
        offset = double.parse(words.removeLast().replaceAll('%', '')) / 100;
      }
      raw.add((color: CssColor.parse(words.join(' ')), offset: offset));
    }
    if (raw.length < 2) throw FormatException('a gradient needs two stops', s);
    // Positions left out spread evenly between their neighbors, as in CSS.
    final offsets = raw.map((r) => r.offset).toList();
    offsets[0] ??= 0;
    offsets[offsets.length - 1] ??= 1;
    var last = 0;
    for (var i = 1; i < offsets.length; i++) {
      if (offsets[i] == null) continue;
      for (var j = last + 1; j < i; j++) {
        offsets[j] =
            offsets[last]! +
            (offsets[i]! - offsets[last]!) * (j - last) / (i - last);
      }
      last = i;
    }
    var prev = 0.0;
    final stops = <CssStop>[];
    for (var i = 0; i < raw.length; i++) {
      final o = math.max(prev, offsets[i]!.clamp(0.0, 1.0));
      stops.add((color: raw[i].color, offset: o));
      prev = o;
    }
    return CssGradient(radial: radial, angle: angle, stops: stops);
  }

  /// The linear gradient's end points over the square at [left], [top] with
  /// side [side], as the core's SVG writes them: long enough that the
  /// corners take the end colors.
  ({double x1, double y1, double x2, double y2}) line(
    double left,
    double top,
    double side,
  ) {
    final rad = angle * math.pi / 180;
    final sin = math.sin(rad), cos = math.cos(rad);
    final half = side / 2 * (sin.abs() + cos.abs());
    final cx = left + side / 2, cy = top + side / 2;
    final dx = sin * half, dy = -cos * half;
    return (x1: cx - dx, y1: cy - dy, x2: cx + dx, y2: cy + dy);
  }
}

double? _angle(String s) {
  final m = RegExp(
    r'^(-?[0-9.]+(?:e[-+]?[0-9]+)?)(deg|turn|rad|grad)$',
  ).firstMatch(s);
  if (m == null) return null;
  final v = double.parse(m.group(1)!);
  return switch (m.group(2)) {
    'turn' => v * 360,
    'rad' => v * 180 / math.pi,
    'grad' => v * 0.9,
    _ => v,
  };
}

double _toSide(String s) {
  final words = s.split(RegExp(r'\s+')).toSet();
  final up = words.contains('top'), down = words.contains('bottom');
  final left = words.contains('left'), right = words.contains('right');
  if (up && right) return 45;
  if (down && right) return 135;
  if (down && left) return 225;
  if (up && left) return 315;
  if (up) return 0;
  if (right) return 90;
  if (left) return 270;
  return 180;
}

/// Splits [s] on [sep] outside parentheses.
List<String> _splitTop(String s, String sep) {
  final out = <String>[];
  var depth = 0, start = 0;
  for (var i = 0; i < s.length; i++) {
    final c = s[i];
    if (c == '(') depth++;
    if (c == ')') depth--;
    if (depth == 0 && c == sep) {
      out.add(s.substring(start, i));
      start = i + 1;
    }
  }
  out.add(s.substring(start));
  return out;
}

// ------------------------------------------------------------- OKLab

double _toLinear(double c) =>
    c <= 0.04045 ? c / 12.92 : math.pow((c + 0.055) / 1.055, 2.4).toDouble();

double _toGamma(double c) =>
    c <= 0.0031308 ? c * 12.92 : 1.055 * math.pow(c, 1 / 2.4) - 0.055;

double _cbrt(double v) =>
    v < 0 ? -math.pow(-v, 1 / 3).toDouble() : math.pow(v, 1 / 3).toDouble();

List<double> _linearToOklab(double r, double g, double b) {
  final l = _cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  final m = _cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  final s = _cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  return [
    0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
    1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
    0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
  ];
}

List<double> _oklabToLinear(double l, double a, double b) {
  final l_ = l + 0.3963377774 * a + 0.2158037573 * b;
  final m_ = l - 0.1055613458 * a - 0.0638541728 * b;
  final s_ = l - 0.0894841775 * a - 1.2914855480 * b;
  final ll = l_ * l_ * l_, mm = m_ * m_ * m_, ss = s_ * s_ * s_;
  return [
    4.0767416621 * ll - 3.3077115913 * mm + 0.2309699292 * ss,
    -1.2684380046 * ll + 2.6097574011 * mm - 0.3413193965 * ss,
    -0.0041960863 * ll - 0.7034186147 * mm + 1.7076147010 * ss,
  ];
}

List<double> _gamutMap(double l, double a, double b) {
  const eps = 1e-6;
  l = l.clamp(0.0, 1.0);
  bool fits(List<double> rgb) => rgb.every((c) => c >= -eps && c <= 1 + eps);
  var rgb = _oklabToLinear(l, a, b);
  if (!fits(rgb)) {
    var lo = 0.0, hi = 1.0;
    for (var i = 0; i < 32; i++) {
      final mid = (lo + hi) / 2;
      if (fits(_oklabToLinear(l, a * mid, b * mid))) {
        lo = mid;
      } else {
        hi = mid;
      }
    }
    rgb = _oklabToLinear(l, a * lo, b * lo);
  }
  return [for (final c in rgb) _toGamma(c.clamp(0.0, 1.0))];
}

// ----------------------------------------------------------- parsing

CssColor _parseColor(String s) {
  if (s.isEmpty) throw const FormatException('empty color');
  if (s.startsWith('#')) return _parseHex(s.substring(1), s);
  final open = s.indexOf('(');
  if (open < 0) return _parseName(s);
  if (!s.endsWith(')')) throw FormatException('unclosed color', s);
  final name = s.substring(0, open).toLowerCase();
  final body = s.substring(open + 1, s.length - 1);
  final commas = body.contains(',');
  var main = body;
  String? alphaText;
  if (!commas && body.contains('/')) {
    final at = body.indexOf('/');
    main = body.substring(0, at);
    alphaText = body.substring(at + 1).trim();
  }
  final args = main
      .split(commas ? RegExp(r'\s*,\s*') : RegExp(r'\s+'))
      .map((a) => a.trim())
      .where((a) => a.isNotEmpty)
      .toList();
  if (commas && args.length == 4) alphaText = args.removeLast();
  if (args.length != 3) throw FormatException('a color takes three values', s);
  final alpha = alphaText == null
      ? 1.0
      : _num(alphaText, percent: 0.01).clamp(0.0, 1.0);
  switch (name) {
    case 'rgb' || 'rgba':
      final c = [
        for (final a in args) _num(a, percent: 2.55).round().clamp(0, 255),
      ];
      return CssColor.rgb8(c[0], c[1], c[2], alpha);
    case 'hsl' || 'hsla':
      final h = _hue(args[0], s);
      final sat = (_num(args[1]) / 100).clamp(0.0, 1.0);
      final light = (_num(args[2]) / 100).clamp(0.0, 1.0);
      final rgb = _hsl(
        h,
        sat,
        light,
      ).map((c) => (c * 255).round().clamp(0, 255)).toList();
      return CssColor.rgb8(rgb[0], rgb[1], rgb[2], alpha);
    case 'oklch':
      final l = _num(args[0], percent: 0.01);
      final c = math.max(0.0, _num(args[1], percent: 0.004));
      return CssColor.oklch(l, c, _hue(args[2], s), alpha);
    case 'oklab':
      return CssColor.oklab(
        _num(args[0], percent: 0.01),
        _num(args[1], percent: 0.004),
        _num(args[2], percent: 0.004),
        alpha,
      );
  }
  throw FormatException('unknown color function', s);
}

double _num(String a, {double percent = 1}) {
  if (a.toLowerCase() == 'none') return 0;
  if (a.endsWith('%'))
    return double.parse(a.substring(0, a.length - 1)) * percent;
  return double.parse(a);
}

double _hue(String a, String s) {
  if (a.toLowerCase() == 'none') return 0;
  return _angle(a.toLowerCase()) ??
      double.tryParse(a) ??
      (throw FormatException('bad hue', s));
}

List<double> _hsl(double h, double s, double l) {
  h = h - 360 * (h / 360).floorToDouble();
  final a = s * math.min(l, 1 - l);
  return [0.0, 8.0, 4.0].map((n) {
    var k = n + h / 30;
    k = k - 12 * (k / 12).floorToDouble();
    return l - a * math.min(k - 3, 9 - k).clamp(-1.0, 1.0);
  }).toList();
}

CssColor _parseHex(String hex, String s) {
  if (!RegExp(r'^[0-9a-fA-F]+$').hasMatch(hex))
    throw FormatException('bad hex color', s);
  int d(int i) => int.parse(hex[i], radix: 16) * 17;
  int p(int i) => int.parse(hex.substring(i, i + 2), radix: 16);
  return switch (hex.length) {
    3 => CssColor.rgb8(d(0), d(1), d(2)),
    4 => CssColor.rgb8(d(0), d(1), d(2), d(3) / 255),
    6 => CssColor.rgb8(p(0), p(2), p(4)),
    8 => CssColor.rgb8(p(0), p(2), p(4), p(6) / 255),
    _ => throw FormatException('bad hex color', s),
  };
}

CssColor _parseName(String s) {
  final lower = s.toLowerCase();
  if (lower == 'transparent') return CssColor.transparent;
  final rgb = _named[lower];
  if (rgb == null) throw FormatException('unknown color name', s);
  return CssColor.rgb8(rgb >> 16 & 0xff, rgb >> 8 & 0xff, rgb & 0xff);
}

/// CSS Color 4 named colors (the core's table).
const _named = <String, int>{
  'aliceblue': 0xf0f8ff,
  'antiquewhite': 0xfaebd7,
  'aqua': 0x00ffff,
  'aquamarine': 0x7fffd4,
  'azure': 0xf0ffff,
  'beige': 0xf5f5dc,
  'bisque': 0xffe4c4,
  'black': 0x000000,
  'blanchedalmond': 0xffebcd,
  'blue': 0x0000ff,
  'blueviolet': 0x8a2be2,
  'brown': 0xa52a2a,
  'burlywood': 0xdeb887,
  'cadetblue': 0x5f9ea0,
  'chartreuse': 0x7fff00,
  'chocolate': 0xd2691e,
  'coral': 0xff7f50,
  'cornflowerblue': 0x6495ed,
  'cornsilk': 0xfff8dc,
  'crimson': 0xdc143c,
  'cyan': 0x00ffff,
  'darkblue': 0x00008b,
  'darkcyan': 0x008b8b,
  'darkgoldenrod': 0xb8860b,
  'darkgray': 0xa9a9a9,
  'darkgreen': 0x006400,
  'darkgrey': 0xa9a9a9,
  'darkkhaki': 0xbdb76b,
  'darkmagenta': 0x8b008b,
  'darkolivegreen': 0x556b2f,
  'darkorange': 0xff8c00,
  'darkorchid': 0x9932cc,
  'darkred': 0x8b0000,
  'darksalmon': 0xe9967a,
  'darkseagreen': 0x8fbc8f,
  'darkslateblue': 0x483d8b,
  'darkslategray': 0x2f4f4f,
  'darkslategrey': 0x2f4f4f,
  'darkturquoise': 0x00ced1,
  'darkviolet': 0x9400d3,
  'deeppink': 0xff1493,
  'deepskyblue': 0x00bfff,
  'dimgray': 0x696969,
  'dimgrey': 0x696969,
  'dodgerblue': 0x1e90ff,
  'firebrick': 0xb22222,
  'floralwhite': 0xfffaf0,
  'forestgreen': 0x228b22,
  'fuchsia': 0xff00ff,
  'gainsboro': 0xdcdcdc,
  'ghostwhite': 0xf8f8ff,
  'gold': 0xffd700,
  'goldenrod': 0xdaa520,
  'gray': 0x808080,
  'green': 0x008000,
  'greenyellow': 0xadff2f,
  'grey': 0x808080,
  'honeydew': 0xf0fff0,
  'hotpink': 0xff69b4,
  'indianred': 0xcd5c5c,
  'indigo': 0x4b0082,
  'ivory': 0xfffff0,
  'khaki': 0xf0e68c,
  'lavender': 0xe6e6fa,
  'lavenderblush': 0xfff0f5,
  'lawngreen': 0x7cfc00,
  'lemonchiffon': 0xfffacd,
  'lightblue': 0xadd8e6,
  'lightcoral': 0xf08080,
  'lightcyan': 0xe0ffff,
  'lightgoldenrodyellow': 0xfafad2,
  'lightgray': 0xd3d3d3,
  'lightgreen': 0x90ee90,
  'lightgrey': 0xd3d3d3,
  'lightpink': 0xffb6c1,
  'lightsalmon': 0xffa07a,
  'lightseagreen': 0x20b2aa,
  'lightskyblue': 0x87cefa,
  'lightslategray': 0x778899,
  'lightslategrey': 0x778899,
  'lightsteelblue': 0xb0c4de,
  'lightyellow': 0xffffe0,
  'lime': 0x00ff00,
  'limegreen': 0x32cd32,
  'linen': 0xfaf0e6,
  'magenta': 0xff00ff,
  'maroon': 0x800000,
  'mediumaquamarine': 0x66cdaa,
  'mediumblue': 0x0000cd,
  'mediumorchid': 0xba55d3,
  'mediumpurple': 0x9370db,
  'mediumseagreen': 0x3cb371,
  'mediumslateblue': 0x7b68ee,
  'mediumspringgreen': 0x00fa9a,
  'mediumturquoise': 0x48d1cc,
  'mediumvioletred': 0xc71585,
  'midnightblue': 0x191970,
  'mintcream': 0xf5fffa,
  'mistyrose': 0xffe4e1,
  'moccasin': 0xffe4b5,
  'navajowhite': 0xffdead,
  'navy': 0x000080,
  'oldlace': 0xfdf5e6,
  'olive': 0x808000,
  'olivedrab': 0x6b8e23,
  'orange': 0xffa500,
  'orangered': 0xff4500,
  'orchid': 0xda70d6,
  'palegoldenrod': 0xeee8aa,
  'palegreen': 0x98fb98,
  'paleturquoise': 0xafeeee,
  'palevioletred': 0xdb7093,
  'papayawhip': 0xffefd5,
  'peachpuff': 0xffdab9,
  'peru': 0xcd853f,
  'pink': 0xffc0cb,
  'plum': 0xdda0dd,
  'powderblue': 0xb0e0e6,
  'purple': 0x800080,
  'rebeccapurple': 0x663399,
  'red': 0xff0000,
  'rosybrown': 0xbc8f8f,
  'royalblue': 0x4169e1,
  'saddlebrown': 0x8b4513,
  'salmon': 0xfa8072,
  'sandybrown': 0xf4a460,
  'seagreen': 0x2e8b57,
  'seashell': 0xfff5ee,
  'sienna': 0xa0522d,
  'silver': 0xc0c0c0,
  'skyblue': 0x87ceeb,
  'slateblue': 0x6a5acd,
  'slategray': 0x708090,
  'slategrey': 0x708090,
  'snow': 0xfffafa,
  'springgreen': 0x00ff7f,
  'steelblue': 0x4682b4,
  'tan': 0xd2b48c,
  'teal': 0x008080,
  'thistle': 0xd8bfd8,
  'tomato': 0xff6347,
  'turquoise': 0x40e0d0,
  'violet': 0xee82ee,
  'wheat': 0xf5deb3,
  'white': 0xffffff,
  'whitesmoke': 0xf5f5f5,
  'yellow': 0xffff00,
  'yellowgreen': 0x9acd32,
};
