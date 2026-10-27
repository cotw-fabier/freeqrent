// Errors carry their code, message and parsed detail (SPEC §10.1).
import 'dart:ffi';
import 'dart:typed_data';

import 'package:ffi/ffi.dart';
import 'package:freeqrent/freeqrent.dart';
import 'package:freeqrent/src/ffi.dart' as c;
import 'package:test/test.dart';

const url = 'https://example.com/conformance';

FreeQrentException caught(void Function() f) {
  try {
    f();
  } on FreeQrentException catch (e) {
    return e;
  }
  fail('no FreeQrentException');
}

void main() {
  test('invalid_style names the path and the reason', () {
    final e = caught(
      () => FreeQrent.render(
        url,
        style: '{"intro":{"trigger":{"kind":"every","ms":"x"}}}',
      ),
    );
    expect(e.code, 'invalid_style');
    expect(e.detail['path'], 'intro.trigger.ms');
    expect(e.detail['reason'], isA<String>());
    expect(e.message, isNotEmpty);
    expect(e.toString(), contains('invalid_style'));
  });

  test('text that is not JSON has an offset', () {
    final e = caught(() => FreeQrent.render(url, style: '{"modules":'));
    expect(e.code, 'invalid_style');
    expect(e.detail['offset'], isA<int>());
  });

  test('a style that is not a document is refused before the call', () {
    expect(() => FreeQrent.render(url, style: 42), throwsArgumentError);
    expect(() => FreeQrent.render(3.5), throwsArgumentError);
  });

  test('invalid_logo says why', () {
    final e = caught(
      () => FreeQrent.render(
        url,
        style: const QrStyle(logo: QrLogo(svg: '<html/>')),
      ),
    );
    expect(e.code, 'invalid_logo');
    expect(e.detail['path'], 'logo.svg');
    expect(e.detail['reason'], 'not_svg');
  });

  test('invalid_image counts the bytes', () {
    final e = caught(() => FreeQrent.paletteFromImage(Uint8List(5), 2, 2));
    expect(e.code, 'invalid_image');
    expect(e.detail['expected'], 16);
    expect(e.detail['actual'], 5);
    final h = caught(
      () => FreeQrent.render(
        url,
        style: QrStyle(
          halftone: QrHalftone(
            image: QrImage(width: 2, height: 2, rgba: Uint8List(3)),
          ),
        ),
      ),
    );
    expect(h.code, 'invalid_image');
    expect(h.detail['path'], 'halftone.image');
  });

  test('data_too_long carries the bits', () {
    final e = caught(
      () => FreeQrent.render(
        'x' * 3000,
        style: const QrStyle(encoding: QrEncoding(ecc: QrEcc.h)),
      ),
    );
    expect(e.code, 'data_too_long');
    expect(
      e.detail['bits_needed'],
      greaterThan(e.detail['bits_available'] as int),
    );
    expect(e.detail['ecc'], 'H');
    expect(e.detail.containsKey('max_version'), isTrue);
  });

  test('invalid_version_range', () {
    final e = caught(
      () => FreeQrent.render(
        url,
        style: const QrStyle(
          encoding: QrEncoding(minVersion: 9, maxVersion: 3),
        ),
      ),
    );
    expect(e.code, 'invalid_version_range');
  });

  test('render errors carry their fields', () {
    final contrast = caught(
      () => FreeQrent.render(url, style: '{"palette":{"fg":"#ccc"}}'),
    );
    expect(contrast.code, 'contrast_too_low');
    expect(contrast.detail['token'], 'fg');
    expect(contrast.detail['delta_l'], isA<num>());

    final range = caught(
      () => FreeQrent.render(
        url,
        style: const QrStyle(modules: QrModuleShape.dots(size: 3)),
      ),
    );
    expect(range.code, 'option_out_of_range');
    expect(range.detail.keys, containsAll(['option', 'value', 'min', 'max']));

    final shadow = caught(
      () => FreeQrent.render(
        url,
        style: const QrStyle(frame: QrFrame.none(), shadow: 1),
      ),
    );
    expect(shadow.code, 'shadow_needs_background');

    final inverted = caught(
      () => FreeQrent.render(
        url,
        style: const QrStyle(palette: {QrToken.bg: '#000', QrToken.fg: '#fff'}),
      ),
    );
    expect(inverted.code, 'inverted_colors');

    final logo = caught(
      () => FreeQrent.render(
        url,
        style: const QrStyle(
          logo: QrLogo(
            svg: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 1"/>',
            scale: 0.45,
          ),
          encoding: QrEncoding(ecc: QrEcc.l, version: 5),
        ),
      ),
    );
    expect(logo.code, anyOf('logo_too_large', 'ecc_budget_exceeded'));
  });

  test('no_colors', () {
    final e = caught(() => FreeQrent.paletteFromCss(''));
    expect(e.code, 'no_colors');
  });

  test('every error round-trips through toJson', () {
    final e = caught(() => FreeQrent.render(url, style: '{"shape":"dots"}'));
    final again = FreeQrentException.fromJson(e.toJson());
    expect(again.code, e.code);
    expect(again.message, e.message);
    expect(again.detail, e.detail);
  });

  test('invalid_argument comes back from the C ABI itself', () {
    // A NULL payload with a length: only reachable below the Dart API.
    using((arena) {
      final out = arena<Pointer<Char>>();
      final len = arena<Size>();
      final code = c.freeqrentRender(nullptr, 3, c.kindText, nullptr, out, len);
      expect(code, c.errorCode);
      final text = out.value.cast<Utf8>().toDartString(length: len.value);
      c.freeqrentFree(out.value);
      final e = FreeQrentException.parse(text);
      expect(e.code, 'invalid_argument');
      expect(
        c.freeqrentRender(nullptr, 0, c.kindText, nullptr, nullptr, nullptr),
        c.invalidCode,
      );
    });
  });
}
