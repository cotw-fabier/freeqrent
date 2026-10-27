// SPEC §10.4: the widget's final frame against the static SVG rendered by
// resvg (test/goldens/*.png, from goldens/), compared module by module.
import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter_test/flutter_test.dart';
import 'package:freeqrent_flutter/freeqrent_flutter.dart';

import 'support.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  final payload = goldenSuite['payload'] as String;
  final scale = (goldenSuite['px_per_module'] as num).toDouble();
  final cases = (goldenSuite['cases'] as List).cast<Map<String, Object?>>();

  for (final c in cases) {
    final name = c['name'] as String;
    test('final frame of $name matches resvg', () async {
      final style = c['style'];
      final plan = FreeQrent.plan(payload, style: style);
      final scene = QrScene(plan, logo: LogoOptions.fromStyle(style));
      final reference = await Gray.png(File('test/goldens/$name.png'));
      final vb = ui.Size(plan.viewBox.x, plan.viewBox.y);
      expect(reference.width, (vb.width * scale).ceil());

      // The logo's picture comes from outside the plan, and a caption's
      // glyphs from the platform's fonts: neither is compared.
      final plate = scene.logoIndex >= 0 ? scene.paths[scene.logoIndex].getBounds().inflate(0.5) : null;
      final caption = plan.caption;
      bool skip(double x, double y) =>
          (plate != null && plate.contains(ui.Offset(x, y))) ||
          (caption != null && (y - caption.center.y).abs() < caption.size);

      // FREEQRENT_DUMP=1 writes the widget's frames to test/failures/.
      final dump = Platform.environment['FREEQRENT_DUMP'] == null ? null : 'test/failures/$name';
      for (final (label, frame) in [
        ('at rest', await draw(scene, atRest: true, scale: scale, dumpTo: dump == null ? null : '$dump-rest.png')),
        (
          'at the end of the intro',
          await draw(scene, timeMs: plan.totalMs.toDouble(), scale: scale, dumpTo: dump == null ? null : '$dump-end.png'),
        ),
      ]) {
        final diff = compareModules(frame, reference, vb, scale: scale, skip: skip);
        // ignore: avoid_print
        print('$name, $label: $diff');
        expect(diff.mismatched, isEmpty, reason: '$name, $label: $diff');
        // Away from the centers too, the colors agree closely.
        expect(diff.meanDelta, lessThan(6), reason: '$name, $label: $diff');
      }
      scene.dispose();
    });
  }
}
