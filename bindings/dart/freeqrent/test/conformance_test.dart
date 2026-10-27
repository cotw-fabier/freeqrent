// The cross-language conformance suite (conformance/cases.json, SPEC
// §10.3): every case's style and options re-serialized with Dart's own
// JSON writer, run through the binding, and held to the expected SHA-256
// and length of the result, or the expected error code.
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:crypto/crypto.dart';
import 'package:freeqrent/freeqrent.dart';
import 'package:test/test.dart';

Object payloadOf(Map<String, Object?> p) => p.containsKey('bytes')
    ? Uint8List.fromList(base64Decode(p['bytes'] as String))
    : p['text'] as String;

String run(Map<String, Object?> c) {
  final style = jsonEncode(c['style'] ?? const {});
  final opts = jsonEncode(c['opts'] ?? const {});
  switch (c['fn']) {
    case 'render':
      return FreeQrent.render(
        payloadOf(c['payload'] as Map<String, Object?>),
        style: style,
      );
    case 'render_frame':
      return FreeQrent.renderFrame(
        payloadOf(c['payload'] as Map<String, Object?>),
        c['t_ms'] as int,
        style: style,
      );
    case 'check':
      return FreeQrent.checkJson(
        payloadOf(c['payload'] as Map<String, Object?>),
        style: style,
      );
    case 'plan':
      return FreeQrent.planJson(
        payloadOf(c['payload'] as Map<String, Object?>),
        style: style,
      );
    case 'palette_from_css':
      return FreeQrent.paletteFromCssJson(c['text'] as String, options: opts);
    case 'palette_from_html':
      return FreeQrent.paletteFromHtmlJson(c['text'] as String, options: opts);
    case 'palette_from_image':
      final image = c['image'] as Map<String, Object?>;
      return FreeQrent.paletteFromImageJson(
        base64Decode(image['rgba'] as String),
        image['width'] as int,
        image['height'] as int,
        options: opts,
      );
    case 'presets':
      return FreeQrent.presetsJson();
  }
  throw StateError('unknown fn ${c['fn']}');
}

void main() {
  final suite =
      jsonDecode(File('../../../conformance/cases.json').readAsStringSync())
          as Map<String, Object?>;
  final cases = (suite['cases'] as List).cast<Map<String, Object?>>();

  test('the suite has every case', () {
    expect(cases, hasLength(77));
    expect(suite['version'], FreeQrent.version);
  });

  for (final c in cases) {
    test('${c['fn']}: ${c['name']}', () {
      final expect_ = c['expect'] as Map<String, Object?>;
      final error = expect_['error'];
      if (error != null) {
        expect(
          () => run(c),
          throwsA(
            isA<FreeQrentException>().having((e) => e.code, 'code', error),
          ),
        );
        return;
      }
      final bytes = utf8.encode(run(c));
      expect(bytes.length, expect_['length'], reason: 'length');
      expect(
        sha256.convert(bytes).toString(),
        expect_['sha256'],
        reason: 'sha256',
      );
    });
  }
}
