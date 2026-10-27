import 'dart:convert';
import 'dart:isolate';
import 'dart:typed_data';

import 'native.dart';
import 'plan.dart';
import 'results.dart';
import 'style.g.dart';

/// freeQRent: styled and animated SVG QR codes that always scan.
///
/// Every function takes a payload, a [String] (text, segmented into the
/// shortest modes) or a [Uint8List] (bytes, in byte mode), and a style:
/// a typed [QrStyle], a `Map` that encodes to a Style document, JSON text,
/// or null for the default style. Failures throw a `FreeQrentException`
/// with a stable `code`.
///
/// Calls are synchronous and safe from any isolate; the `…Async` variants
/// run on a background isolate (`Isolate.run`) for large renders.
abstract final class FreeQrent {
  /// The Style document JSON for [style]: a [QrDocument], a `Map` or `List`
  /// (encoded with `jsonEncode`, so typed parts may sit inside), JSON text
  /// as it is, or `{}` for null.
  static String styleJson(Object? style) => switch (style) {
    null => '{}',
    String json => json,
    QrDocument doc => jsonEncode(doc.toJson()),
    Map() || List() => jsonEncode(style),
    _ => throw ArgumentError.value(
      style,
      'style',
      'must be a QrStyle, a Map, JSON text or null',
    ),
  };

  /// The SVG document for [payload], or why the style can't be drawn.
  static String render(Object payload, {Object? style}) =>
      renderText(payload, styleJson(style));

  /// The intro stopped at [tMs] milliseconds, as a still SVG with no
  /// animation (the finished code for a style without an intro).
  static String renderFrame(Object payload, int tMs, {Object? style}) =>
      renderFrameText(payload, styleJson(style), tMs);

  /// The scan report. A style [render] would refuse still gets one, with
  /// `ok` false; only a bad document or a payload that doesn't fit throws.
  static ScanReport check(Object payload, {Object? style}) =>
      ScanReport.fromJson(
        jsonDecode(checkJson(payload, style: style)) as Map<String, Object?>,
      );

  /// The scan report as the library writes it.
  static String checkJson(Object payload, {Object? style}) =>
      checkText(payload, styleJson(style));

  /// The animation plan (SPEC §8.5): the static layers and the tracks that
  /// move them, for players outside CSS such as `FreeQrentAnimated`.
  static AnimationPlan plan(Object payload, {Object? style}) =>
      AnimationPlan.parse(planJson(payload, style: style));

  /// The animation plan as the library writes it.
  static String planJson(Object payload, {Object? style}) =>
      planText(payload, styleJson(style));

  /// A palette from [width] × [height] RGBA pixels, row by row.
  /// [options] is a [QrPaletteOpts], a `Map`, JSON text or null.
  static PaletteResult paletteFromImage(
    Uint8List rgba,
    int width,
    int height, {
    Object? options,
  }) => _palette(paletteFromImageText(rgba, width, height, styleJson(options)));

  /// A palette from a style sheet's colors.
  static PaletteResult paletteFromCss(String css, {Object? options}) =>
      _palette(paletteFromCssText(css, styleJson(options)));

  /// A palette from a page's colors (its style sheets and inline styles).
  static PaletteResult paletteFromHtml(String html, {Object? options}) =>
      _palette(paletteFromHtmlText(html, styleJson(options)));

  static PaletteResult _palette(String json) =>
      PaletteResult.fromJson(jsonDecode(json) as Map<String, Object?>);

  /// [paletteFromImage]'s result as the library writes it.
  static String paletteFromImageJson(
    Uint8List rgba,
    int width,
    int height, {
    Object? options,
  }) => paletteFromImageText(rgba, width, height, styleJson(options));

  /// [paletteFromCss]'s result as the library writes it.
  static String paletteFromCssJson(String css, {Object? options}) =>
      paletteFromCssText(css, styleJson(options));

  /// [paletteFromHtml]'s result as the library writes it.
  static String paletteFromHtmlJson(String html, {Object? options}) =>
      paletteFromHtmlText(html, styleJson(options));

  /// The preset names, for `QrStyle(preset: …)` or `{"preset": name}`.
  static List<String> get presets =>
      (jsonDecode(presetsJson()) as List).cast<String>();

  /// The preset names as the library writes them (a JSON array).
  static String presetsJson() => presetsText();

  /// The library version, the same in every package (e.g. `1.0.0`).
  static String get version => versionText();

  /// [render] on a background isolate.
  static Future<String> renderAsync(Object payload, {Object? style}) {
    final json = styleJson(style);
    return Isolate.run(() => renderText(payload, json));
  }

  /// [renderFrame] on a background isolate.
  static Future<String> renderFrameAsync(
    Object payload,
    int tMs, {
    Object? style,
  }) {
    final json = styleJson(style);
    return Isolate.run(() => renderFrameText(payload, json, tMs));
  }

  /// [check] on a background isolate.
  static Future<ScanReport> checkAsync(Object payload, {Object? style}) {
    final json = styleJson(style);
    return Isolate.run(
      () => ScanReport.fromJson(
        jsonDecode(checkText(payload, json)) as Map<String, Object?>,
      ),
    );
  }

  /// [plan] on a background isolate.
  static Future<AnimationPlan> planAsync(Object payload, {Object? style}) {
    final json = styleJson(style);
    return Isolate.run(() => AnimationPlan.parse(planText(payload, json)));
  }
}
