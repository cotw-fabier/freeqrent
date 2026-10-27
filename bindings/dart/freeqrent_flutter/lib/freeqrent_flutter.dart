/// FreeQrentAnimated: freeQRent's animated QR codes drawn natively by
/// Flutter from the core's AnimationPlan (SPEC §10.4).
///
/// ```dart
/// FreeQrentAnimated(
///   payload: 'https://example.com',
///   style: const QrStyle(
///     preset: QrPreset.candy,
///     intro: QrIntro.preset(preset: QrIntroName.ripple),
///   ),
///   trigger: FreeQrentTrigger.visible,
/// )
/// ```
library;

export 'package:freeqrent/freeqrent.dart';

export 'src/painter.dart' show QrScenePainter, PaletteBlend, lerpPaint;
export 'src/scene.dart' show QrScene, LogoOptions;
export 'src/svg_path.dart' show parseSvgPath;
export 'src/widget.dart';
