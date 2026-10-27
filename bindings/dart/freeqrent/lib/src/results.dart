// What check and the palette functions return, as Dart data (the schema's
// Report and PaletteResult).
import 'style.g.dart';

double? _maybe(Object? v) => v == null ? null : (v as num).toDouble();

/// One of a report's errors or warnings: `{code, message, detail}`.
final class ScanIssue {
  ScanIssue._fromJson(Map<String, Object?> j)
    : code = j['code'] as String,
      message = j['message'] as String,
      detail = (j['detail'] as Map?)?.cast<String, Object?>() ?? const {};

  /// The stable code, like `low_contrast` or `contrast_too_low`.
  final String code;

  /// What it means, in English.
  final String message;

  /// Its fields.
  final Map<String, Object?> detail;

  @override
  String toString() => '$code: $message';
}

/// The code a report is about.
final class CodeInfo {
  CodeInfo._fromJson(Map<String, Object?> j)
    : version = j['version'] as int,
      ecc = QrEcc.values.firstWhere((e) => e.json == j['ecc']),
      size = j['size'] as int,
      mask = j['mask'] as int;

  /// The QR version, 1–40.
  final int version;

  /// The error correction level.
  final QrEcc ecc;

  /// Modules a side.
  final int size;

  /// The mask pattern, 0–7.
  final int mask;
}

/// How a picture steered the code (halftone and art mode).
final class Steering {
  Steering._fromJson(Map<String, Object?> j)
    : steerableModules = j['steerable_modules'] as int,
      codeModules = j['code_modules'] as int,
      matchedModules = j['matched_modules'] as int;

  /// Modules the solver could choose.
  final int steerableModules;

  /// All the code's modules.
  final int codeModules;

  /// Modules that match the picture.
  final int matchedModules;
}

/// A style's scan report (`Style::check`): whether it renders, and what
/// to watch.
final class ScanReport {
  /// Parses the report JSON.
  ScanReport.fromJson(this.json)
    : ok = json['ok'] as bool,
      errors = [
        for (final e in json['errors'] as List)
          ScanIssue._fromJson(e as Map<String, Object?>),
      ],
      warnings = [
        for (final e in json['warnings'] as List)
          ScanIssue._fromJson(e as Map<String, Object?>),
      ],
      minContrast = _maybe(json['min_contrast']),
      eccBudgetUsed = _maybe(json['ecc_budget_used']),
      logoScale = _maybe(json['logo_scale']),
      steering = json['steering'] == null
          ? null
          : Steering._fromJson(json['steering'] as Map<String, Object?>),
      code = CodeInfo._fromJson(json['code'] as Map<String, Object?>);

  /// The parsed JSON.
  final Map<String, Object?> json;

  /// True when the style renders (no errors).
  final bool ok;

  /// Why it would be refused.
  final List<ScanIssue> errors;

  /// What makes it harder to scan.
  final List<ScanIssue> warnings;

  /// The lowest lightness contrast between a token and the background.
  final double? minContrast;

  /// Share of the error-correction budget a logo uses.
  final double? eccBudgetUsed;

  /// The logo's scale, after fitting.
  final double? logoScale;

  /// A picture's steering numbers.
  final Steering? steering;

  /// The code drawn.
  final CodeInfo code;
}

/// A color the extraction found, with its share of the source.
typedef Swatch = ({String color, double share});

/// Where a token's color came from.
final class PaletteOrigin {
  PaletteOrigin._fromJson(Map<String, Object?> j)
    : kind = j['kind'] as String,
      swatch = j['swatch'] as int?,
      deltaL = _maybe(j['delta_l']);

  /// `swatch`, `tinted`, `darkened` or `lightened`.
  final String kind;

  /// The swatch it is (or was made from), by index.
  final int? swatch;

  /// How far it moved in lightness.
  final double? deltaL;
}

/// One token the extraction set.
typedef PaletteChoice = ({QrToken token, String color, PaletteOrigin origin});

/// An extracted palette (`Palette::from_image/css/html`). [palette] drops
/// straight into a style: `QrStyle(palette: result.palette)`.
final class PaletteResult {
  /// Parses the palette JSON.
  PaletteResult.fromJson(this.json)
    : palette = {
        for (final MapEntry(:key, :value)
            in (json['palette'] as Map).cast<String, String>().entries)
          QrToken.values.firstWhere((t) => t.json == key): value,
      },
      swatches = [
        for (final s in json['swatches'] as List)
          (
            color: (s as Map)['color'] as String,
            share: (s['share'] as num).toDouble(),
          ),
      ],
      choices = [
        for (final c in json['choices'] as List)
          (
            token: QrToken.values.firstWhere(
              (t) => t.json == (c as Map)['token'],
            ),
            color: (c as Map)['color'] as String,
            origin: PaletteOrigin._fromJson(
              (c['origin'] as Map).cast<String, Object?>(),
            ),
          ),
      ];

  /// The parsed JSON.
  final Map<String, Object?> json;

  /// The tokens' paints.
  final Map<QrToken, String> palette;

  /// The colors found, most common first.
  final List<Swatch> swatches;

  /// Each token's color and where it came from.
  final List<PaletteChoice> choices;
}
