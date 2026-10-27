// Text in, text out over the C ABI: every call allocates its inputs,
// reads the one string the library hands back and frees it, and turns an
// error string into a FreeQrentException.
import 'dart:convert';
import 'dart:ffi';
import 'dart:typed_data';

import 'package:ffi/ffi.dart';

import 'errors.dart';
import 'ffi.dart';

/// A payload's bytes and kind for the C ABI.
({Uint8List bytes, int kind}) payloadOf(Object payload) => switch (payload) {
  String text => (bytes: utf8.encode(text), kind: kindText),
  Uint8List bytes => (bytes: bytes, kind: kindBytes),
  List<int> bytes => (bytes: Uint8List.fromList(bytes), kind: kindBytes),
  _ => throw ArgumentError.value(
    payload,
    'payload',
    'must be a String (text) or a Uint8List (bytes)',
  ),
};

/// Runs one C call with an out-string and returns its text, or throws the
/// error it wrote.
String _call(
  Arena arena,
  int Function(Pointer<Pointer<Char>> out, Pointer<Size> outLen) f,
) {
  final out = arena<Pointer<Char>>();
  final outLen = arena<Size>();
  final code = f(out, outLen);
  if (code == invalidCode) {
    throw const FreeQrentException(
      'invalid_argument',
      'the library was called without an output pointer',
    );
  }
  final ptr = out.value;
  try {
    final text = ptr == nullptr
        ? ''
        : utf8.decode(ptr.cast<Uint8>().asTypedList(outLen.value));
    if (code == okCode) return text;
    throw FreeQrentException.parse(text);
  } finally {
    if (ptr != nullptr) freeqrentFree(ptr);
  }
}

Pointer<Char> _cString(Arena arena, String text) =>
    text.toNativeUtf8(allocator: arena).cast<Char>();

Pointer<Uint8> _buffer(Arena arena, Uint8List bytes) {
  if (bytes.isEmpty) return nullptr;
  final ptr = arena<Uint8>(bytes.length);
  ptr.asTypedList(bytes.length).setAll(0, bytes);
  return ptr;
}

typedef _PayloadCall =
    int Function(
      Pointer<Uint8> data,
      int len,
      int kind,
      Pointer<Char> style,
      Pointer<Pointer<Char>> out,
      Pointer<Size> outLen,
    );

String _withPayload(Object payload, String styleJson, _PayloadCall f) {
  final p = payloadOf(payload);
  return using((arena) {
    final data = _buffer(arena, p.bytes);
    final style = _cString(arena, styleJson);
    return _call(
      arena,
      (out, len) => f(data, p.bytes.length, p.kind, style, out, len),
    );
  });
}

/// `freeqrent_render`.
String renderText(Object payload, String styleJson) =>
    _withPayload(payload, styleJson, freeqrentRender);

/// `freeqrent_render_frame`.
String renderFrameText(Object payload, String styleJson, int tMs) {
  if (tMs < 0 || tMs > 0xffffffff) {
    throw RangeError.range(tMs, 0, 0xffffffff, 'tMs');
  }
  return _withPayload(
    payload,
    styleJson,
    (data, len, kind, style, out, outLen) =>
        freeqrentRenderFrame(data, len, kind, style, tMs, out, outLen),
  );
}

/// `freeqrent_check`.
String checkText(Object payload, String styleJson) =>
    _withPayload(payload, styleJson, freeqrentCheck);

/// `freeqrent_plan`.
String planText(Object payload, String styleJson) =>
    _withPayload(payload, styleJson, freeqrentPlan);

/// `freeqrent_palette_from_image`.
String paletteFromImageText(
  Uint8List rgba,
  int width,
  int height,
  String optsJson,
) {
  RangeError.checkValueInInterval(width, 0, 0xffffffff, 'width');
  RangeError.checkValueInInterval(height, 0, 0xffffffff, 'height');
  return using((arena) {
    final data = _buffer(arena, rgba);
    final opts = _cString(arena, optsJson);
    return _call(
      arena,
      (out, len) => freeqrentPaletteFromImage(
        data,
        rgba.length,
        width,
        height,
        opts,
        out,
        len,
      ),
    );
  });
}

/// `freeqrent_palette_from_css`.
String paletteFromCssText(String css, String optsJson) => using((arena) {
  final text = _cString(arena, css);
  final opts = _cString(arena, optsJson);
  return _call(
    arena,
    (out, len) => freeqrentPaletteFromCss(text, opts, out, len),
  );
});

/// `freeqrent_palette_from_html`.
String paletteFromHtmlText(String html, String optsJson) => using((arena) {
  final text = _cString(arena, html);
  final opts = _cString(arena, optsJson);
  return _call(
    arena,
    (out, len) => freeqrentPaletteFromHtml(text, opts, out, len),
  );
});

/// `freeqrent_presets`.
String presetsText() => using((arena) => _call(arena, freeqrentPresets));

/// `freeqrent_version`.
String versionText() => freeqrentVersion().cast<Utf8>().toDartString();
