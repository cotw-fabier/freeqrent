// Payloads (text and bytes), isolates, render_frame and the generated
// surface's freshness.
import 'dart:convert';
import 'dart:io';
import 'dart:isolate';
import 'dart:typed_data';

import 'package:freeqrent/freeqrent.dart';
import 'package:test/test.dart';

const url = 'https://example.com/conformance';

void main() {
  test('bytes go in byte mode', () {
    final bytes = Uint8List.fromList([0, 1, 2, 254, 255, 81, 82]);
    final svg = FreeQrent.render(bytes);
    expect(svg, startsWith('<svg'));
    // A List<int> is bytes too.
    expect(FreeQrent.render(bytes.toList()), svg);
    // Text that happens to be the same bytes is a different code: text is
    // segmented and gets an ECI header outside ASCII.
    final text = FreeQrent.render('Grüße');
    expect(
      FreeQrent.render(Uint8List.fromList(utf8.encode('Grüße'))),
      isNot(text),
    );
    expect(FreeQrent.check(bytes).code.version, 1);
    expect(
      FreeQrent.plan(
        bytes,
        style: const QrStyle(intro: QrIntro.preset(preset: QrIntroName.ripple)),
      ).tracks,
      isNotEmpty,
    );
    expect(FreeQrent.render(Uint8List(0)), startsWith('<svg'));
  });

  test('render_frame stops the intro', () {
    const style = QrStyle(
      preset: QrPreset.candy,
      intro: QrIntro.preset(preset: QrIntroName.ripple),
    );
    final mid = FreeQrent.renderFrame(url, 400, style: style);
    final end = FreeQrent.renderFrame(url, 60000, style: style);
    expect(mid, isNot(end));
    expect(end, isNot(contains('@keyframes')));
    expect(
      FreeQrent.renderFrame(url, 0, style: const QrStyle(preset: QrPreset.ink)),
      FreeQrent.render(url, style: const QrStyle(preset: QrPreset.ink)),
    );
    expect(() => FreeQrent.renderFrame(url, -1), throwsRangeError);
  });

  test('isolates agree', () async {
    const styles = [
      QrStyle(preset: QrPreset.sunset),
      QrStyle(
        modules: QrModuleShape.liquid(),
        intro: QrIntro.preset(preset: QrIntroName.settle),
      ),
      QrStyle(effect: QrEffect.sketch(), seed: 9),
    ];
    final here = [for (final s in styles) FreeQrent.render(url, style: s)];
    final there = await Future.wait([
      for (final s in styles) FreeQrent.renderAsync(url, style: s),
    ]);
    expect(there, here);
    // Many at once, each on its own isolate.
    final many = await Future.wait([
      for (var i = 0; i < 16; i++)
        Isolate.run(() => FreeQrent.render('$url/$i', style: styles[i % 3])),
    ]);
    for (var i = 0; i < 16; i++) {
      expect(many[i], FreeQrent.render('$url/$i', style: styles[i % 3]));
    }
    expect(
      await FreeQrent.renderFrameAsync(url, 300, style: styles[1]),
      FreeQrent.renderFrame(url, 300, style: styles[1]),
    );
    expect((await FreeQrent.checkAsync(url, style: styles[0])).ok, isTrue);
    expect(
      (await FreeQrent.planAsync(url, style: styles[1])).totalMs,
      FreeQrent.plan(url, style: styles[1]).totalMs,
    );
    await expectLater(
      FreeQrent.renderAsync(url, style: '{"shape":1}'),
      throwsA(
        isA<FreeQrentException>().having(
          (e) => e.code,
          'code',
          'invalid_style',
        ),
      ),
    );
  });

  test('the generated style surface is up to date with the schema', () {
    final r = Process.runSync(Platform.resolvedExecutable, [
      'run',
      'tool/generate.dart',
      '--check',
    ]);
    expect(r.exitCode, 0, reason: '${r.stdout}${r.stderr}');
  });

  test('colors convert like the core', () {
    expect(CssColor.parse('#17161c').srgb8, [0x17, 0x16, 0x1c]);
    expect(CssColor.parse('#abc').srgb8, [0xaa, 0xbb, 0xcc]);
    expect(CssColor.parse('rgb(1 2 3 / .5)').alpha, 0.5);
    expect(CssColor.parse('rgba(1,2,3,.5)').srgb8, [1, 2, 3]);
    expect(CssColor.parse('hsl(120, 100%, 25%)').srgb8, [0, 128, 0]);
    expect(CssColor.parse('oklch(62.8% .2577 29.23)').srgb8, [255, 0, 0]);
    expect(CssColor.parse('rebeccapurple').srgb8, [0x66, 0x33, 0x99]);
    // The stops the core's SVG carries for an OKLCH gradient.
    final g =
        CssPaint.parse(
              'radial-gradient(oklch(97% .02 300) 0%, oklch(90% .05 300) 100%)',
            )
            as CssGradient;
    expect(g.radial, isTrue);
    expect(g.stops.map((s) => s.color.srgb8).toList(), [
      [0xf7, 0xf3, 0xff],
      [0xe3, 0xd7, 0xfb],
    ]);
    final l =
        CssPaint.parse(
              'linear-gradient(135deg, #1d2b64 0%, oklch(30% .14 330) 100%)',
            )
            as CssGradient;
    expect(l.stops.last.color.srgb8, [0x50, 0x00, 0x4d]);
    final line = l.line(4, 4, 29);
    expect([line.x1, line.y1, line.x2, line.y2].map((v) => v.round()), [
      4,
      4,
      33,
      33,
    ]);
  });
}
