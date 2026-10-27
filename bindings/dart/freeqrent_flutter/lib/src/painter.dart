// Draws a QrScene at one moment: each layer's pose from the plan's tracks,
// painted the way the core's SVG writer paints it (see crates/freeqrent/
// src/svg.rs). What Flutter can't draw exactly is approximated, as listed
// in the package README: the sketch effect's displacement, the grain's
// noise and the caption's font.
import 'dart:math' as math;
import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:flutter/rendering.dart';
import 'package:freeqrent/freeqrent.dart';

import 'scene.dart';
import 'svg_path.dart';

/// A palette between two others, for the widget's OKLab transitions.
class PaletteBlend {
  /// [to], or the mix of [from] and [to] at [t] (0–1).
  const PaletteBlend(this.to, {this.from, this.t = 1});

  /// Where the palette is going (token name to CSS paint).
  final Map<String, String> to;

  /// Where it came from.
  final Map<String, String>? from;

  /// How far along, 0–1.
  final double t;

  /// The paint of [token].
  CssPaint paint(String token) {
    final target = AnimationPlan.paintIn(token, to);
    final source = from;
    if (source == null || t >= 1) return target;
    return lerpPaint(AnimationPlan.paintIn(token, source), target, t);
  }
}

/// Mixes two paints in OKLab: solid with solid, a gradient stop by stop
/// (a solid stands for a gradient of its color), or, for gradients of
/// different shapes, a switch at the middle.
CssPaint lerpPaint(CssPaint a, CssPaint b, double t) {
  if (a is CssSolid && b is CssSolid) {
    return CssSolid(CssColor.lerpOklab(a.color, b.color, t));
  }
  List<CssStop> stopsOf(CssPaint p, int n) => switch (p) {
    CssGradient(:final stops) => stops,
    CssSolid(:final color) => [
      for (var i = 0; i < n; i++)
        (color: color, offset: n == 1 ? 0 : i / (n - 1)),
    ],
  };
  final shape = (b is CssGradient ? b : a) as CssGradient;
  final n = shape.stops.length;
  final sa = stopsOf(a, n), sb = stopsOf(b, n);
  if (sa.length != sb.length ||
      (a is CssGradient &&
          b is CssGradient &&
          (a.radial != b.radial || a.angle != b.angle))) {
    return t < 0.5 ? a : b;
  }
  return CssGradient(
    radial: shape.radial,
    angle: shape.angle,
    stops: [
      for (var i = 0; i < n; i++)
        (
          color: CssColor.lerpOklab(sa[i].color, sb[i].color, t),
          offset: sa[i].offset + (sb[i].offset - sa[i].offset) * t,
        ),
    ],
  );
}

Color _color(CssColor c, [double alpha = 1]) =>
    Color(c.argb32).withValues(alpha: (c.alpha * alpha).clamp(0.0, 1.0));

/// Paints [scene] at [timeMs] from the trigger, or at rest (the finished
/// code, the reduced-motion state) when [atRest].
class QrScenePainter extends CustomPainter {
  /// A painter for one frame.
  QrScenePainter({
    required this.scene,
    required this.timeMs,
    required this.palette,
    this.atRest = false,
    this.logoImage,
    super.repaint,
  });

  /// What to draw.
  final QrScene scene;

  /// The moment, in ms from the trigger.
  final double timeMs;

  /// Show every layer at rest, ignoring [timeMs].
  final bool atRest;

  /// The paints.
  final PaletteBlend palette;

  /// The logo's picture, fitted into its content box.
  final ui.Image? logoImage;

  AnimationPlan get _plan => scene.plan;

  @override
  void paint(Canvas canvas, Size size) {
    final vb = _plan.viewBox;
    final s = math.min(size.width / vb.x, size.height / vb.y);
    canvas.save();
    canvas.translate((size.width - vb.x * s) / 2, (size.height - vb.y * s) / 2);
    canvas.scale(s);
    paintScene(canvas);
    canvas.restore();
  }

  PlanPose _pose(int i) {
    if (atRest) {
      return PlanPose(
        opacity: _plan.layers[i].role == PlanRole.logoInk ? 0 : 1,
      );
    }
    return _plan.poseAt(i, timeMs, palette: palette.to);
  }

  /// Draws the scene in viewBox units.
  void paintScene(Canvas canvas) {
    final layers = _plan.layers;
    final reveal = atRest ? 1.0 : _plan.revealAt(timeMs);
    final mask = reveal < 1 ? scene.maskRange : null;
    final fx = scene.fxRange;
    final viewRect = Rect.fromLTWH(0, 0, _plan.viewBox.x, _plan.viewBox.y);
    Canvas target = canvas;
    ui.PictureRecorder? recorder;
    for (var i = 0; i < layers.length; i++) {
      if (mask != null && i == mask.$1)
        canvas.saveLayer(viewRect.inflate(1), Paint());
      if (fx != null && i == fx.$1) {
        recorder = ui.PictureRecorder();
        target = Canvas(recorder);
      }
      _layer(target, i);
      if (fx != null && i == fx.$2 && recorder != null) {
        final picture = recorder.endRecording();
        _effect(canvas, picture, viewRect);
        picture.dispose();
        recorder = null;
        target = canvas;
      }
      if (mask != null && i == mask.$2) {
        _mask(canvas, reveal, viewRect);
        canvas.restore();
      }
    }
  }

  /// The effect's group: under the code, a glow (it blurred, faint) or an
  /// emboss (soft dark and light shadows); a sketch is drawn plain.
  void _effect(Canvas canvas, ui.Picture code, Rect bounds) {
    final e = _plan.effect!;
    switch (e.kind) {
      case 'glow':
        canvas.saveLayer(
          bounds,
          Paint()
            ..imageFilter = ui.ImageFilter.blur(
              sigmaX: e.radius,
              sigmaY: e.radius,
            )
            ..color = Color.fromRGBO(0, 0, 0, e.strength.clamp(0.0, 1.0)),
        );
        canvas.drawPicture(code);
        canvas.restore();
      case 'emboss':
        for (final (sign, color) in [
          (1.0, const Color.fromRGBO(0, 0, 0, 0.35)),
          if (!scene.inverted) (-1.0, const Color.fromRGBO(255, 255, 255, 0.7)),
        ]) {
          canvas.saveLayer(
            bounds,
            Paint()
              ..imageFilter = ui.ImageFilter.blur(
                sigmaX: e.depth,
                sigmaY: e.depth,
              )
              ..colorFilter = ColorFilter.mode(color, BlendMode.srcIn),
          );
          canvas.translate(sign * e.depth, sign * e.depth);
          canvas.drawPicture(code);
          canvas.restore();
        }
    }
    canvas.drawPicture(code);
  }

  /// The reveal mask over the masked layers' layer, at progress [p].
  void _mask(Canvas canvas, double p, Rect view) {
    final r = _plan.reveal!;
    canvas.saveLayer(view.inflate(1), Paint()..blendMode = BlendMode.dstIn);
    final f = r.feather;
    const white = Color(0xffffffff), clear = Color(0x00ffffff);
    if (r.iris) {
      final c = Offset(r.center!.x, r.center!.y);
      final outer = (r.radius + f) * p;
      if (outer > 0) {
        canvas.drawCircle(
          c,
          outer,
          Paint()
            ..shader = ui.Gradient.radial(
              c,
              outer,
              [white, white, clear],
              [0, r.radius / (r.radius + f), 1],
            ),
        );
      }
    } else {
      final from = r.from!, to = r.to!;
      final dx = to.x - from.x, dy = to.y - from.y;
      final length = math.sqrt(dx * dx + dy * dy);
      final side = math.max(view.width, view.height);
      final reach = 2 * side, back = length + 2 * f + side;
      canvas.save();
      canvas.translate(from.x, from.y);
      canvas.rotate(math.atan2(dy, dx));
      canvas.translate(-(1 - p) * r.travel, 0);
      canvas.drawRect(
        Rect.fromLTWH(-back, -reach, back + length, 2 * reach),
        Paint()..color = white,
      );
      if (f > 0) {
        canvas.drawRect(
          Rect.fromLTWH(length, -reach, f, 2 * reach),
          Paint()
            ..shader = ui.Gradient.linear(
              Offset(length, 0),
              Offset(length + f, 0),
              [white, clear],
            ),
        );
      }
      canvas.restore();
    }
    canvas.restore();
  }

  /// The paint of [token] (or a fixed [color]) as a fill, its gradient
  /// spanning the card for the background and the code's square otherwise,
  /// in the coordinates [local] maps viewBox points into.
  Paint _fill(
    String token, {
    CssColor? color,
    double alpha = 1,
    Offset Function(Offset)? local,
    double localScale = 1,
  }) {
    final paint = color != null ? CssSolid(color) : palette.paint(token);
    final p = Paint()..isAntiAlias = true;
    switch (paint) {
      case CssSolid(:final color):
        p.color = _color(color, alpha);
      case CssGradient(:final radial, :final stops):
        final box = _source(token) == 'bg'
            ? Rect.fromLTWH(0, 0, scene.card, scene.card)
            : scene.code;
        final map = local ?? (o) => o;
        final colors = [for (final s in stops) _color(s.color, alpha)];
        final offsets = [for (final s in stops) s.offset];
        if (radial) {
          p.shader = ui.Gradient.radial(
            map(box.center),
            box.width * math.sqrt1_2 * localScale,
            colors,
            offsets,
          );
        } else {
          final l = paint.line(box.left, box.top, box.width);
          p.shader = ui.Gradient.linear(
            map(Offset(l.x1, l.y1)),
            map(Offset(l.x2, l.y2)),
            colors,
            offsets,
          );
        }
    }
    return p;
  }

  /// The token whose paint [token] uses: itself if set, else its fallback's.
  String _source(String token) {
    var t = token;
    for (var i = 0; i < 4; i++) {
      if (palette.to.containsKey(t) || t == 'bg' || t == 'fg') return t;
      t = switch (t) {
        'alignment' => 'finder.outer',
        'logo.backdrop' => 'bg',
        _
            when t.startsWith('finder.') &&
                t.endsWith('.outer') &&
                t != 'finder.outer' =>
          'finder.outer',
        _
            when t.startsWith('finder.') &&
                t.endsWith('.eye') &&
                t != 'finder.eye' =>
          'finder.eye',
        _ => 'fg',
      };
    }
    return t;
  }

  CssColor? _fixed(PlanLayer l) =>
      l.color == null ? null : CssColor.parse(l.color!);

  void _layer(Canvas canvas, int i) {
    final layer = _plan.layers[i];
    if (layer.role == PlanRole.logoArt) return; // drawn inside the logo
    final pose = _pose(i);
    final opacity = layer.opacity * pose.opacity;
    // The ink silhouette rests invisible; anything faded out is skipped.
    if (opacity <= 0.001) return;
    canvas.save();
    if (!pose.still) canvas.transform(_matrix(pose.matrix(layer.origin)));
    final faded = opacity < 0.999;
    if (faded) {
      canvas.saveLayer(null, Paint()..color = Color.fromRGBO(0, 0, 0, opacity));
    }
    final path = scene.paths[i];
    final fill = pose.fill ?? _fixed(layer);
    switch (layer.role) {
      case PlanRole.logo:
        _logo(canvas, i, path, fill);
      case PlanRole.logoInk:
        _logoInk(canvas, path, layer, fill);
      case PlanRole.logoFrame when layer.outline != null:
        _outline(canvas, path, layer, pose, fill);
      case PlanRole.logoGlint:
        canvas.save();
        if (scene.logoIndex >= 0) canvas.clipPath(scene.paths[scene.logoIndex]);
        _sheen(canvas, path, 0.45);
        canvas.restore();
      case PlanRole.shimmer:
        _sheen(canvas, path, 0.18);
      case PlanRole.caption:
        _caption(canvas, layer, fill);
      case PlanRole.grain:
        final grain = _plan.textures
            .where((t) => t.kind == 'grain')
            .firstOrNull;
        canvas.save();
        canvas.clipPath(path);
        canvas.drawPicture(scene.grain(grain?.amount ?? 0.08));
        canvas.restore();
      case PlanRole.scanlines:
        _scanlines(canvas, path);
      case _ when layer.stroke != null && layer.cells != null:
        _subcells(canvas, path, layer, fill);
      default:
        canvas.drawPath(
          path,
          _fill(layer.token, color: fill, alpha: layer.fillOpacity),
        );
    }
    if (faded) canvas.restore();
    canvas.restore();
  }

  Float64List _matrix(List<double> m) => Float64List.fromList([
    m[0], m[1], 0, 0, //
    m[2], m[3], 0, 0,
    0, 0, 1, 0,
    m[4], m[5], 0, 1,
  ]);

  /// A picture's sub-cells: lines in fifths of a module from the code's
  /// corner, stroked with butt caps.
  void _subcells(Canvas canvas, Path path, PlanLayer layer, CssColor? fill) {
    final at = Offset(layer.cells!.x, layer.cells!.y);
    canvas.save();
    canvas.translate(at.dx, at.dy);
    canvas.scale(0.2);
    final paint =
        _fill(
            layer.token,
            color: fill,
            alpha: layer.fillOpacity,
            local: (o) => (o - at) * 5,
            localScale: 5,
          )
          ..style = PaintingStyle.stroke
          ..strokeWidth = layer.stroke!
          ..strokeCap = StrokeCap.butt;
    canvas.drawPath(path, paint);
    canvas.restore();
  }

  /// A logo frame stroked along its path, dashed, drawn on as far as the
  /// pose says.
  void _outline(
    Canvas canvas,
    Path path,
    PlanLayer layer,
    PlanPose pose,
    CssColor? fill,
  ) {
    final o = layer.outline!;
    var dashes = o.dashes;
    var offset = o.dashOffset;
    final draws = _plan.tracks.any(
      (t) =>
          t.layer == _plan.layers.indexOf(layer) &&
          t.keys.any((k) => k.property == PlanProperty.draw),
    );
    if (draws && dashes.isEmpty) {
      final d = o.length + 0.01;
      dashes = [d, d];
    }
    if (dashes.isNotEmpty) offset += (1 - pose.draw) * dashes.first;
    final paint = _fill(layer.token, color: fill)
      ..style = PaintingStyle.stroke
      ..strokeWidth = o.width;
    // SVG's dash offset moves the pattern back along the path.
    canvas.drawPath(
      dashes.isEmpty ? path : dashPath(path, dashes, offset),
      paint,
    );
  }

  /// A white sheen across [path]'s box, clear at the edges and [peak]
  /// opaque in the middle (the shimmer and the logo's glint).
  void _sheen(Canvas canvas, Path path, double peak) {
    final b = boundsOf(path);
    canvas.drawPath(
      path,
      Paint()
        ..shader = ui.Gradient.linear(
          b.centerLeft,
          b.centerRight,
          [
            const Color(0x00ffffff),
            Color.fromRGBO(255, 255, 255, peak),
            const Color(0x00ffffff),
          ],
          [0, 0.5, 1],
        ),
    );
  }

  /// Scanlines over the card: bands of the background, a sixth of a module
  /// tall every third.
  void _scanlines(Canvas canvas, Path path) {
    final t = _plan.textures.where((t) => t.kind == 'scanlines').firstOrNull;
    final bg = switch (palette.paint('bg')) {
      CssSolid(:final color) => color,
      CssGradient(:final stops) => stops.first.color,
    };
    canvas.save();
    canvas.clipPath(path);
    final b = boundsOf(path);
    canvas.saveLayer(
      b,
      Paint()..color = Color.fromRGBO(0, 0, 0, t?.amount ?? 0.15),
    );
    final solid = Paint()..color = _color(bg.withAlpha(1));
    const period = 0.333, band = 0.167;
    for (var y = 0.0; y < b.bottom; y += period) {
      canvas.drawRect(Rect.fromLTRB(b.left, y, b.right, y + band), solid);
    }
    canvas.restore();
    canvas.restore();
  }

  /// The caption: one line centered on its point, squeezed to fit the card.
  void _caption(Canvas canvas, PlanLayer layer, CssColor? fill) {
    final c = _plan.caption;
    if (c == null) return;
    final paint = _fill(layer.token, color: fill, alpha: layer.fillOpacity);
    final tp = TextPainter(
      text: TextSpan(
        text: c.text,
        style: TextStyle(
          fontSize: c.size,
          height: 1,
          fontWeight: c.bold ? FontWeight.w700 : FontWeight.w400,
          letterSpacing: c.tracking / 100 * c.size,
          fontFamily: switch (c.font) {
            'serif' => 'serif',
            'mono' => 'monospace',
            _ => null,
          },
          foreground: paint,
        ),
      ),
      textDirection: TextDirection.ltr,
      maxLines: 1,
    )..layout();
    final room = scene.card - 2;
    final squeeze = tp.width > room ? room / tp.width : 1.0;
    canvas.save();
    canvas.translate(c.center.x, c.center.y);
    canvas.scale(squeeze, 1);
    tp.paint(canvas, Offset(-tp.width / 2, -tp.height / 2));
    canvas.restore();
    tp.dispose();
  }

  /// The logo: its plate (when it has a backdrop), then its picture fitted
  /// into its content box, clipped to its window and moved by its art
  /// layer's pose.
  void _logo(Canvas canvas, int i, Path plate, CssColor? fill) {
    if (scene.logo.backdrop) {
      canvas.drawPath(plate, _fill(_plan.layers[i].token, color: fill));
    }
    final image = logoImage;
    final box = scene.content;
    if (image == null || box == null) return;
    canvas.save();
    canvas.clipPath(scene.window ?? plate);
    final art = scene.artIndex;
    if (art != null) {
      final pose = _pose(art);
      if (!pose.still)
        canvas.transform(_matrix(pose.matrix(_plan.layers[art].origin)));
    }
    paintImage(
      canvas: canvas,
      rect: box,
      image: image,
      fit: BoxFit.contain,
      filterQuality: FilterQuality.medium,
    );
    canvas.restore();
  }

  /// Ink's silhouette: the token's paint where the logo's picture is.
  void _logoInk(Canvas canvas, Path path, PlanLayer layer, CssColor? fill) {
    final image = logoImage;
    final box = scene.content;
    if (image == null || box == null) {
      canvas.drawPath(path, _fill(layer.token, color: fill));
      return;
    }
    canvas.saveLayer(boundsOf(path), Paint());
    paintImage(canvas: canvas, rect: box, image: image, fit: BoxFit.contain);
    canvas.drawPath(
      path,
      _fill(layer.token, color: fill)..blendMode = BlendMode.srcIn,
    );
    canvas.restore();
  }

  @override
  bool shouldRepaint(QrScenePainter old) =>
      old.scene != scene ||
      old.timeMs != timeMs ||
      old.atRest != atRest ||
      old.palette != palette ||
      old.logoImage != logoImage;
}
