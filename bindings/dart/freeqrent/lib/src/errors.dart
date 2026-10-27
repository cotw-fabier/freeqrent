import 'dart:convert';

/// An error from freeQRent (SPEC §10.1): a refused document, a payload that
/// doesn't fit, a style that wouldn't scan, or a palette source without
/// colors.
///
/// [code] is stable and names the failure (`invalid_style`,
/// `data_too_long`, `contrast_too_low`, `no_colors`, …); [message] is
/// English for people; [detail] holds the fields, such as `path` and
/// `reason` for `invalid_style` or `bits_needed` for `data_too_long`.
class FreeQrentException implements Exception {
  /// An error with these fields.
  const FreeQrentException(this.code, this.message, [this.detail = const {}]);

  /// Parses the `{code, message, detail}` object the library returns.
  factory FreeQrentException.fromJson(Map<String, Object?> json) {
    final detail = json['detail'];
    return FreeQrentException(
      json['code'] as String? ?? 'internal',
      json['message'] as String? ?? '',
      detail is Map<String, Object?> ? detail : const {},
    );
  }

  /// Parses the error JSON the C ABI writes.
  factory FreeQrentException.parse(String text) {
    try {
      return FreeQrentException.fromJson(
        jsonDecode(text) as Map<String, Object?>,
      );
    } on Object {
      return FreeQrentException('internal', 'unreadable error: $text');
    }
  }

  /// The stable error code.
  final String code;

  /// What went wrong, in English.
  final String message;

  /// The error's fields, as parsed JSON.
  final Map<String, Object?> detail;

  /// The error as the boundary writes it.
  Map<String, Object?> toJson() => {
    'code': code,
    'message': message,
    'detail': detail,
  };

  @override
  String toString() => 'FreeQrentException($code): $message';
}
