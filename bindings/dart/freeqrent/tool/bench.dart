// Medians of the Dart calls, FFI crossing included:
//
//   dart run tool/bench.dart
import 'package:freeqrent/freeqrent.dart';

double median(void Function() f, {int runs = 200}) {
  for (var i = 0; i < 20; i++) {
    f();
  }
  final times = <int>[];
  final sw = Stopwatch();
  for (var i = 0; i < runs; i++) {
    sw
      ..reset()
      ..start();
    f();
    sw.stop();
    times.add(sw.elapsedMicroseconds);
  }
  times.sort();
  return times[runs ~/ 2] / 1000;
}

void main() {
  const v10 = QrStyle(
    modules: QrModuleShape.rounded(),
    finders: QrFinders(ring: QrRing.squircle()),
    palette: {QrToken.fg: 'linear-gradient(135deg, #1d2b64, #50004d)'},
    encoding: QrEncoding(version: 10),
  );
  const liquid = QrStyle(
    modules: QrModuleShape.liquid(),
    intro: QrIntro.preset(preset: QrIntroName.ripple),
    encoding: QrEncoding(version: 10),
  );
  const url = 'https://example.com/bench';
  final json = FreeQrent.styleJson(v10);
  print('version            ${median(() => FreeQrent.version, runs: 2000)} ms');
  print('render default     ${median(() => FreeQrent.render(url))} ms');
  print(
    'render v10 styled  ${median(() => FreeQrent.render(url, style: json))} ms',
  );
  print(
    '  (typed style)    ${median(() => FreeQrent.render(url, style: v10))} ms',
  );
  print(
    'render v10 liquid  ${median(() => FreeQrent.render(url, style: liquid))} ms',
  );
  print(
    'plan v10 liquid    ${median(() => FreeQrent.planJson(url, style: liquid))} ms',
  );
  print(
    '  (parsed)         ${median(() => FreeQrent.plan(url, style: liquid))} ms',
  );
  print(
    'check v10          ${median(() => FreeQrent.check(url, style: json))} ms',
  );
}
