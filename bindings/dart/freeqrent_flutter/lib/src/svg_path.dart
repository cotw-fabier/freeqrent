// SVG path data (the plan's `path`, SPEC §8.5) into a dart:ui Path: every
// command of SVG 1.1, absolute and relative, with the compact number forms
// the core writes (`-.5`, `1.5.5`, flags without separators).
import 'dart:math' as math;
import 'dart:ui' as ui;

/// Parses SVG path data into a [ui.Path].
ui.Path parseSvgPath(String data) {
  final p = _Parser(data);
  final path = ui.Path();
  var x = 0.0, y = 0.0; // current point
  var sx = 0.0, sy = 0.0; // subpath start
  double? cx, cy; // last control point, for S and T
  String? last;
  String? cmd;
  while (true) {
    p.skip();
    if (p.done) break;
    if (p.isCommand) {
      cmd = p.command();
    } else if (cmd == null) {
      throw FormatException('path data starts without a command', data);
    } else if (cmd == 'm') {
      cmd = 'l';
    } else if (cmd == 'M') {
      cmd = 'L';
    }
    final rel = cmd.toLowerCase() == cmd;
    final ox = rel ? x : 0.0, oy = rel ? y : 0.0;
    switch (cmd.toLowerCase()) {
      case 'm':
        x = ox + p.number();
        y = oy + p.number();
        path.moveTo(x, y);
        sx = x;
        sy = y;
        cx = cy = null;
      case 'l':
        x = ox + p.number();
        y = oy + p.number();
        path.lineTo(x, y);
        cx = cy = null;
      case 'h':
        x = ox + p.number();
        path.lineTo(x, y);
        cx = cy = null;
      case 'v':
        y = oy + p.number();
        path.lineTo(x, y);
        cx = cy = null;
      case 'c':
        final x1 = ox + p.number(), y1 = oy + p.number();
        final x2 = ox + p.number(), y2 = oy + p.number();
        x = ox + p.number();
        y = oy + p.number();
        path.cubicTo(x1, y1, x2, y2, x, y);
        cx = x2;
        cy = y2;
      case 's':
        final smooth = last != null && 'cCsS'.contains(last);
        final x1 = smooth ? 2 * x - cx! : x, y1 = smooth ? 2 * y - cy! : y;
        final x2 = ox + p.number(), y2 = oy + p.number();
        x = ox + p.number();
        y = oy + p.number();
        path.cubicTo(x1, y1, x2, y2, x, y);
        cx = x2;
        cy = y2;
      case 'q':
        final x1 = ox + p.number(), y1 = oy + p.number();
        x = ox + p.number();
        y = oy + p.number();
        path.quadraticBezierTo(x1, y1, x, y);
        cx = x1;
        cy = y1;
      case 't':
        final smooth = last != null && 'qQtT'.contains(last);
        final x1 = smooth ? 2 * x - cx! : x, y1 = smooth ? 2 * y - cy! : y;
        x = ox + p.number();
        y = oy + p.number();
        path.quadraticBezierTo(x1, y1, x, y);
        cx = x1;
        cy = y1;
      case 'a':
        final rx = p.number(), ry = p.number(), rot = p.number();
        final large = p.flag(), sweep = p.flag();
        x = ox + p.number();
        y = oy + p.number();
        if (rx == 0 || ry == 0) {
          path.lineTo(x, y);
        } else {
          path.arcToPoint(
            ui.Offset(x, y),
            radius: ui.Radius.elliptical(rx.abs(), ry.abs()),
            rotation: rot,
            largeArc: large,
            clockwise: sweep,
          );
        }
        cx = cy = null;
      case 'z':
        path.close();
        x = sx;
        y = sy;
        cx = cy = null;
      default:
        throw FormatException('unknown path command $cmd', data);
    }
    last = cmd;
    if (cmd.toLowerCase() == 'z') cmd = null;
  }
  return path;
}

class _Parser {
  _Parser(this.s);

  final String s;
  int i = 0;

  bool get done => i >= s.length;

  void skip() {
    while (i < s.length && (s.codeUnitAt(i) <= 0x20 || s[i] == ',')) {
      i++;
    }
  }

  bool get isCommand {
    final c = s.codeUnitAt(i);
    return (c >= 0x41 && c <= 0x5a || c >= 0x61 && c <= 0x7a) &&
        c != 0x65 && // e
        c != 0x45; // E
  }

  String command() => s[i++];

  bool flag() {
    skip();
    final c = s[i++];
    if (c != '0' && c != '1') throw FormatException('bad arc flag', s, i - 1);
    return c == '1';
  }

  double number() {
    skip();
    final start = i;
    if (i < s.length && (s[i] == '-' || s[i] == '+')) i++;
    var dot = false;
    while (i < s.length) {
      final c = s[i];
      if (c == '.') {
        if (dot) break;
        dot = true;
      } else if (c.codeUnitAt(0) < 0x30 || c.codeUnitAt(0) > 0x39) {
        break;
      }
      i++;
    }
    if (i < s.length && (s[i] == 'e' || s[i] == 'E')) {
      final save = i;
      i++;
      if (i < s.length && (s[i] == '-' || s[i] == '+')) i++;
      final digits = i;
      while (i < s.length &&
          s.codeUnitAt(i) >= 0x30 &&
          s.codeUnitAt(i) <= 0x39) {
        i++;
      }
      if (i == digits) i = save;
    }
    final text = s.substring(start, i);
    final v = double.tryParse(text);
    if (v == null) throw FormatException('bad number "$text"', s, start);
    return v;
  }
}

/// The bounds of [path], or an empty rect at the origin for an empty path.
ui.Rect boundsOf(ui.Path path) {
  final b = path.getBounds();
  return b.isFinite ? b : ui.Rect.zero;
}

/// [path] cut into dashes of [pattern] (dash, gap, …) starting [offset]
/// along it, as SVG's `stroke-dasharray` and `stroke-dashoffset`.
ui.Path dashPath(ui.Path path, List<double> pattern, double offset) {
  if (pattern.isEmpty || pattern.every((d) => d <= 0)) return path;
  final total = pattern.fold(0.0, (a, b) => a + b);
  final out = ui.Path();
  for (final metric in path.computeMetrics()) {
    // Start inside the pattern where the offset puts the path's start.
    var phase = offset % total;
    if (phase < 0) phase += total;
    var index = 0;
    while (phase >= pattern[index]) {
      phase -= pattern[index];
      index = (index + 1) % pattern.length;
    }
    var at = 0.0;
    var remaining = pattern[index] - phase;
    while (at < metric.length) {
      final end = math.min(metric.length, at + remaining);
      if (index.isEven)
        out.addPath(metric.extractPath(at, end), ui.Offset.zero);
      at = end;
      index = (index + 1) % pattern.length;
      remaining = pattern[index];
    }
  }
  return out;
}
