// Shared by the widget tests: drawing a scene to grayscale pixels, and
// comparing two grayscale images module by module.
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:freeqrent_flutter/freeqrent_flutter.dart';

/// A grayscale image.
class Gray {
  Gray(this.width, this.height, this.pixels);

  final int width, height;
  final Uint8List pixels;

  int at(int x, int y) =>
      pixels[y.clamp(0, height - 1) * width + x.clamp(0, width - 1)];

  /// Rec. 601 luma over white, as freeqrent-verify rasterizes.
  static Gray fromRgba(int width, int height, ByteData straight) {
    final out = Uint8List(width * height);
    for (var i = 0; i < out.length; i++) {
      final a = straight.getUint8(i * 4 + 3);
      int over(int c) => (c * a / 255).round() + 255 - a;
      final r = over(straight.getUint8(i * 4));
      final g = over(straight.getUint8(i * 4 + 1));
      final b = over(straight.getUint8(i * 4 + 2));
      out[i] = (r * 299 + g * 587 + b * 114 + 500) ~/ 1000;
    }
    return Gray(width, height, out);
  }

  /// Decodes a PNG with the engine's codec.
  static Future<Gray> png(File file) async {
    final codec = await ui.instantiateImageCodec(file.readAsBytesSync());
    final frame = await codec.getNextFrame();
    final data = await frame.image.toByteData(
      format: ui.ImageByteFormat.rawStraightRgba,
    );
    final g = Gray.fromRgba(frame.image.width, frame.image.height, data!);
    frame.image.dispose();
    return g;
  }
}

/// The golden cases.
final goldenSuite =
    jsonDecode(File('test/goldens/cases.json').readAsStringSync())
        as Map<String, Object?>;

/// [scene] drawn at [scale] pixels a module, at [timeMs] (or at rest).
Future<Gray> draw(
  QrScene scene, {
  double timeMs = 0,
  bool atRest = false,
  double scale = 8,
  Map<String, String>? palette,
  String? dumpTo,
}) async {
  final vb = scene.plan.viewBox;
  final w = (vb.x * scale).ceil(), h = (vb.y * scale).ceil();
  final recorder = ui.PictureRecorder();
  final canvas = ui.Canvas(recorder);
  QrScenePainter(
    scene: scene,
    timeMs: timeMs,
    atRest: atRest,
    palette: PaletteBlend(palette ?? scene.plan.palette),
  ).paint(canvas, ui.Size(vb.x * scale, vb.y * scale));
  final picture = recorder.endRecording();
  final image = await picture.toImage(w, h);
  final data = await image.toByteData(
    format: ui.ImageByteFormat.rawStraightRgba,
  );
  if (dumpTo != null) {
    final png = await image.toByteData(format: ui.ImageByteFormat.png);
    File(dumpTo)
      ..parent.createSync(recursive: true)
      ..writeAsBytesSync(png!.buffer.asUint8List());
  }
  image.dispose();
  picture.dispose();
  return Gray.fromRgba(w, h, data!);
}

/// How two renders of a code differ, module by module.
class ModuleDiff {
  ModuleDiff(this.compared, this.mismatched, this.meanDelta, this.worst);

  /// Modules compared.
  final int compared;

  /// Modules whose centers fall on opposite sides of the threshold.
  final List<(int, int)> mismatched;

  /// Mean absolute luma difference at module centers.
  final double meanDelta;

  /// Largest luma difference at a module center.
  final int worst;

  @override
  String toString() =>
      '${mismatched.length}/$compared modules differ (mean Δ ${meanDelta.toStringAsFixed(1)}, worst $worst)${mismatched.isEmpty ? '' : ': ${mismatched.take(8).toList()}'}';
}

/// Compares [a] with [reference] at every module center of the viewBox
/// (each center averaged over 3×3 pixels), skipping modules [skip] says
/// to. A module is dark where it's below the middle of the reference's
/// range.
ModuleDiff compareModules(
  Gray a,
  Gray reference,
  ui.Size viewBox, {
  double scale = 8,
  bool Function(double x, double y)? skip,
}) {
  var lo = 255, hi = 0;
  for (final p in reference.pixels) {
    if (p < lo) lo = p;
    if (p > hi) hi = p;
  }
  final mid = (lo + hi) / 2;
  final bad = <(int, int)>[];
  var n = 0, sum = 0, worst = 0;
  for (var y = 0; y < viewBox.height.floor(); y++) {
    for (var x = 0; x < viewBox.width.floor(); x++) {
      if (skip != null && skip(x + 0.5, y + 0.5)) continue;
      final px = ((x + 0.5) * scale).floor(), py = ((y + 0.5) * scale).floor();
      double mean(Gray g) {
        var s = 0;
        for (var dy = -1; dy <= 1; dy++) {
          for (var dx = -1; dx <= 1; dx++) {
            s += g.at(px + dx, py + dy);
          }
        }
        return s / 9;
      }

      final va = mean(a), vr = mean(reference);
      n++;
      final d = (va - vr).abs().round();
      sum += d;
      if (d > worst) worst = d;
      if ((va < mid) != (vr < mid)) bad.add((x, y));
    }
  }
  return ModuleDiff(n, bad, n == 0 ? 0 : sum / n, worst);
}
