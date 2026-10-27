/// freeQRent for Dart: styled and animated SVG QR codes that always scan,
/// over the Rust core's C ABI (built by the package's Native Assets hook).
///
/// ```dart
/// import 'package:freeqrent/freeqrent.dart';
///
/// final svg = FreeQrent.render(
///   'https://example.com',
///   style: QrStyle(
///     preset: QrPreset.ink,
///     modules: QrModuleShape.dots(size: 0.9),
///     intro: QrIntro.preset(preset: QrIntroName.ripple),
///   ),
/// );
/// ```
library;

export 'src/api.dart';
export 'src/color.dart' show CssColor, CssPaint, CssSolid, CssGradient, CssStop;
export 'src/errors.dart';
export 'src/plan.dart';
export 'src/results.dart';
export 'src/style.g.dart';
