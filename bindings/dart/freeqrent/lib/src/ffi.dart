// The C ABI of freeqrent-ffi (crates/freeqrent-ffi/include/freeqrent.h),
// bound with @Native to the library the build hook compiles. Nothing here
// is public: lib/src/native.dart wraps it.
@DefaultAsset('package:freeqrent/src/ffi.dart')
library;

import 'dart:ffi';

const int okCode = 0;
const int errorCode = 1;
const int invalidCode = 2;

const int kindText = 0;
const int kindBytes = 1;

@Native<
  Int32 Function(
    Pointer<Uint8>,
    Size,
    Int32,
    Pointer<Char>,
    Pointer<Pointer<Char>>,
    Pointer<Size>,
  )
>(symbol: 'freeqrent_render')
external int freeqrentRender(
  Pointer<Uint8> data,
  int len,
  int kind,
  Pointer<Char> styleJson,
  Pointer<Pointer<Char>> out,
  Pointer<Size> outLen,
);

@Native<
  Int32 Function(
    Pointer<Uint8>,
    Size,
    Int32,
    Pointer<Char>,
    Uint32,
    Pointer<Pointer<Char>>,
    Pointer<Size>,
  )
>(symbol: 'freeqrent_render_frame')
external int freeqrentRenderFrame(
  Pointer<Uint8> data,
  int len,
  int kind,
  Pointer<Char> styleJson,
  int tMs,
  Pointer<Pointer<Char>> out,
  Pointer<Size> outLen,
);

@Native<
  Int32 Function(
    Pointer<Uint8>,
    Size,
    Int32,
    Pointer<Char>,
    Pointer<Pointer<Char>>,
    Pointer<Size>,
  )
>(symbol: 'freeqrent_check')
external int freeqrentCheck(
  Pointer<Uint8> data,
  int len,
  int kind,
  Pointer<Char> styleJson,
  Pointer<Pointer<Char>> out,
  Pointer<Size> outLen,
);

@Native<
  Int32 Function(
    Pointer<Uint8>,
    Size,
    Int32,
    Pointer<Char>,
    Pointer<Pointer<Char>>,
    Pointer<Size>,
  )
>(symbol: 'freeqrent_plan')
external int freeqrentPlan(
  Pointer<Uint8> data,
  int len,
  int kind,
  Pointer<Char> styleJson,
  Pointer<Pointer<Char>> out,
  Pointer<Size> outLen,
);

@Native<
  Int32 Function(
    Pointer<Uint8>,
    Size,
    Uint32,
    Uint32,
    Pointer<Char>,
    Pointer<Pointer<Char>>,
    Pointer<Size>,
  )
>(symbol: 'freeqrent_palette_from_image')
external int freeqrentPaletteFromImage(
  Pointer<Uint8> rgba,
  int rgbaLen,
  int width,
  int height,
  Pointer<Char> optsJson,
  Pointer<Pointer<Char>> out,
  Pointer<Size> outLen,
);

@Native<
  Int32 Function(
    Pointer<Char>,
    Pointer<Char>,
    Pointer<Pointer<Char>>,
    Pointer<Size>,
  )
>(symbol: 'freeqrent_palette_from_css')
external int freeqrentPaletteFromCss(
  Pointer<Char> css,
  Pointer<Char> optsJson,
  Pointer<Pointer<Char>> out,
  Pointer<Size> outLen,
);

@Native<
  Int32 Function(
    Pointer<Char>,
    Pointer<Char>,
    Pointer<Pointer<Char>>,
    Pointer<Size>,
  )
>(symbol: 'freeqrent_palette_from_html')
external int freeqrentPaletteFromHtml(
  Pointer<Char> html,
  Pointer<Char> optsJson,
  Pointer<Pointer<Char>> out,
  Pointer<Size> outLen,
);

@Native<Int32 Function(Pointer<Pointer<Char>>, Pointer<Size>)>(
  symbol: 'freeqrent_presets',
)
external int freeqrentPresets(Pointer<Pointer<Char>> out, Pointer<Size> outLen);

@Native<Pointer<Char> Function()>(symbol: 'freeqrent_version', isLeaf: true)
external Pointer<Char> freeqrentVersion();

@Native<Void Function(Pointer<Char>)>(symbol: 'freeqrent_free', isLeaf: true)
external void freeqrentFree(Pointer<Char> s);
