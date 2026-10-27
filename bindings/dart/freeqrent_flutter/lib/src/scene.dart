// A plan prepared for drawing: every layer's path parsed into a ui.Path
// once, and the geometry the painter needs (the code's square, the card,
// the logo's boxes, the effect and mask ranges, the grain).
import 'dart:convert';
import 'dart:math' as math;
import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:freeqrent/freeqrent.dart';

import 'svg_path.dart';

/// The logo options a plan doesn't carry, from the Style document.
class LogoOptions {
  /// Options as the core defaults them.
  const LogoOptions({this.backdrop = true, this.padding = 0.5, this.raster});

  /// Reads them from a Style document's `logo`.
  factory LogoOptions.fromStyle(Object? style) {
    try {
      final doc = FreeQrent.styleJson(style);
      final logo = (_decode(doc) as Map?)?['logo'];
      if (logo is! Map) return const LogoOptions();
      final raster = logo['raster'];
      return LogoOptions(
        backdrop: logo['backdrop'] as bool? ?? true,
        padding: (logo['padding'] as num?)?.toDouble() ?? 0.5,
        raster: raster is String ? _base64(raster) : null,
      );
    } on Object {
      return const LogoOptions();
    }
  }

  /// Whether the plate is painted behind the logo.
  final bool backdrop;

  /// Space between the plate (or the frame's window) and the logo.
  final double padding;

  /// A raster logo's file bytes.
  final Uint8List? raster;
}

Object? _decode(String json) => jsonDecode(json);

Uint8List _base64(String s) =>
    base64Decode(s.padRight((s.length + 3) ~/ 4 * 4, '='));

/// A plan with its paths parsed.
class QrScene {
  /// Prepares [plan] for drawing.
  QrScene(this.plan, {this.logo = const LogoOptions()})
    : paths = [
        for (final l in plan.layers)
          parseSvgPath(l.path)
            ..fillType = l.evenOdd
                ? ui.PathFillType.evenOdd
                : ui.PathFillType.nonZero,
      ] {
    final finders = [
      for (var i = 0; i < plan.layers.length; i++)
        if (plan.layers[i].role == PlanRole.finderOuter) boundsOf(paths[i]),
    ];
    code = finders.isEmpty
        ? ui.Rect.fromLTWH(0, 0, plan.viewBox.x, plan.viewBox.x)
        : finders.reduce((a, b) => a.expandToInclude(b));
    final bg = plan.layers.indexWhere((l) => l.role == PlanRole.background);
    final bgBounds = bg < 0 ? ui.Rect.zero : boundsOf(paths[bg]);
    card = bgBounds.isEmpty
        ? plan.viewBox.x
        : math.max(bgBounds.width, bgBounds.height);
    logoIndex = plan.layers.indexWhere((l) => l.role == PlanRole.logo);
    final art = plan.layers.indexWhere((l) => l.role == PlanRole.logoArt);
    final ink = plan.layers.indexWhere((l) => l.role == PlanRole.logoInk);
    if (logoIndex >= 0) {
      final plate = boundsOf(paths[logoIndex]);
      window = art >= 0 ? paths[art] : paths[logoIndex];
      artIndex = art >= 0 ? art : null;
      if (ink >= 0) {
        content = boundsOf(paths[ink]);
      } else {
        final w = art >= 0 ? boundsOf(paths[art]) : plate;
        content = w.deflate(logo.padding);
      }
    }
    final fx = [
      for (var i = 0; i < plan.layers.length; i++)
        if (plan.layers[i].role.takesEffect) i,
    ];
    if (plan.effect != null && fx.isNotEmpty) fxRange = (fx.first, fx.last);
    final r = plan.reveal;
    if (r != null) maskRange = (r.first, r.last);
    final bgPaint = AnimationPlan.paintIn('bg', plan.palette);
    final fgPaint = AnimationPlan.paintIn('fg', plan.palette);
    inverted = _lightness(bgPaint) < _lightness(fgPaint);
  }

  /// The plan.
  final AnimationPlan plan;

  /// What the plan doesn't say about the logo.
  final LogoOptions logo;

  /// Each layer's path, in viewBox units.
  final List<ui.Path> paths;

  /// The code's square (finder to finder): the span of its gradients.
  late final ui.Rect code;

  /// The card's side: the span of the background's gradient.
  late final double card;

  /// The logo's layer, or -1.
  late final int logoIndex;

  /// The logo's window (its clip) and where its content sits.
  ui.Path? window;

  /// The box the logo's picture is fitted into.
  ui.Rect? content;

  /// The layer that moves the logo's picture inside its window.
  int? artIndex;

  /// The first and last layer the effect filters.
  (int, int)? fxRange;

  /// The first and last layer the reveal mask covers.
  (int, int)? maskRange;

  /// Light ink on a dark ground.
  late final bool inverted;

  ui.Picture? _grain;

  /// The grain's speckles over the card, made once: seeded noise in the
  /// ink's lightness, up to [amount] opaque (an approximation of the SVG's
  /// fractal-noise filter).
  ui.Picture grain(double amount) {
    final cached = _grain;
    if (cached != null) return cached;
    final recorder = ui.PictureRecorder();
    final canvas = ui.Canvas(recorder);
    const cell = 1 / 3;
    final n = (plan.viewBox.x / cell).ceil();
    final m = (plan.viewBox.y / cell).ceil();
    final rng = math.Random(0x9e3779b9 ^ plan.layers.length ^ n);
    const levels = 4;
    final buckets = List.generate(levels, (_) => <double>[]);
    for (var y = 0; y < m; y++) {
      for (var x = 0; x < n; x++) {
        final noise = rng.nextDouble();
        if (noise <= 0.5) continue;
        final level = ((noise - 0.5) * 2 * levels).floor().clamp(0, levels - 1);
        buckets[level].addAll([x * cell + cell / 2, y * cell + cell / 2]);
      }
    }
    final ink = inverted
        ? const ui.Color(0xffffffff)
        : const ui.Color(0xff000000);
    for (var i = 0; i < levels; i++) {
      final paint = ui.Paint()
        ..color = ink.withValues(alpha: amount * (i + 1) / levels)
        ..strokeWidth = cell
        ..strokeCap = ui.StrokeCap.square;
      canvas.drawRawPoints(
        ui.PointMode.points,
        Float32List.fromList(buckets[i]),
        paint,
      );
    }
    return _grain = recorder.endRecording();
  }

  /// Frees the cached grain.
  void dispose() {
    _grain?.dispose();
    _grain = null;
  }
}

double _lightness(CssPaint p) => switch (p) {
  CssSolid(:final color) => color.oklab[0],
  CssGradient(:final stops) =>
    stops.map((s) => s.color.oklab[0]).reduce((a, b) => a + b) / stops.length,
};
