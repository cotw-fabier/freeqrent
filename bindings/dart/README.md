# freeQRent for Dart and Flutter

Two packages over the Rust core's C ABI (`crates/freeqrent-ffi`, SPEC §10.2):

| Package | What | For |
|---|---|---|
| [`freeqrent`](freeqrent/) | `dart:ffi` bindings, typed styles, the plan, report and palette models | any Dart: servers, CLIs, Flutter |
| [`freeqrent_flutter`](freeqrent_flutter/) | `FreeQrentAnimated`, the native animated widget (SPEC §10.4) | Flutter apps |

They're two packages because the widget needs the Flutter SDK and `freeqrent` doesn't: a Dart server or CLI that renders SVG can depend on `freeqrent` alone, without pulling in Flutter (a package that imports `package:flutter` can't be used from `dart run` or a plain `dart test` at all). `freeqrent_flutter` re-exports `freeqrent`, so a Flutter app imports one library.

## Install

Both are path dependencies until they're published (M9):

```yaml
dependencies:
  freeqrent_flutter:        # or freeqrent, without Flutter
    path: path/to/freeqrent/bindings/dart/freeqrent_flutter
```

The native library is compiled by `freeqrent`'s **Native Assets** build hook (`hook/build.dart`, with [`native_toolchain_rust`](https://pub.dev/packages/native_toolchain_rust)) for whatever you build: macOS, iOS, Android, Linux or Windows. You need [rustup](https://rustup.rs); the hook builds with the toolchain pinned in `freeqrent/rust/rust-toolchain.toml` (1.96.0) and rustup installs it on first use. `dart run`, `dart test`, `flutter run`, `flutter test` and `flutter build` all run the hook; nothing else is needed.

## Quick start

```dart
import 'package:freeqrent/freeqrent.dart';

final svg = FreeQrent.render(
  'https://example.com',
  style: const QrStyle(
    preset: QrPreset.ink,
    modules: QrModuleShape.dots(size: 0.9),
    intro: QrIntro.preset(preset: QrIntroName.ripple),
  ),
);
```

In Flutter, draw a static code with `flutter_svg` (`SvgPicture.string(svg)`), or an animated one natively:

```dart
import 'package:freeqrent_flutter/freeqrent_flutter.dart';

FreeQrentAnimated(
  payload: 'https://example.com',
  style: const QrStyle(
    preset: QrPreset.candy,
    intro: QrIntro.preset(preset: QrIntroName.ripple),
    idle: [QrIdle.shimmer],
  ),
  trigger: FreeQrentTrigger.visible,
  palette: dark ? {QrToken.fg: '#e8e2d0', QrToken.bg: '#0f1420'} : null,
)
```

## The API (`package:freeqrent`)

Payloads are a `String` (text, segmented into the shortest modes) or a `Uint8List` (bytes, byte mode). A style is a typed `QrStyle`, a `Map` (typed parts may sit inside it), JSON text, or null for the default.

| Call | Returns |
|---|---|
| `FreeQrent.render(payload, style: …)` | the SVG `String` |
| `FreeQrent.renderFrame(payload, tMs, style: …)` | the intro stopped at `tMs`, as a still SVG |
| `FreeQrent.check(payload, style: …)` | a `ScanReport` (`ok`, `errors`, `warnings`, `minContrast`, `code`, …); `checkJson` for the text |
| `FreeQrent.plan(payload, style: …)` | an `AnimationPlan` (SPEC §8.5), with `poseAt(layer, t)` and `revealAt(t)`; `planJson` for the text |
| `FreeQrent.paletteFromImage(rgba, w, h, options: …)`, `paletteFromCss(css)`, `paletteFromHtml(html)` | a `PaletteResult`; its `palette` drops into `QrStyle(palette: …)`. Options are `QrPaletteOpts`, a map or JSON |
| `FreeQrent.presets`, `FreeQrent.version` | the preset names, `"1.0.0"` |
| `renderAsync`, `renderFrameAsync`, `checkAsync`, `planAsync` | the same on a background isolate (`Isolate.run`) |

Calls are synchronous and safe from any isolate. Errors throw `FreeQrentException` with `code` (stable, SPEC §10.1: `invalid_style`, `data_too_long`, `contrast_too_low`, `no_colors`, …), `message` and `detail` (the parsed fields, like `path` and `reason`).

**Typed styles** are generated from `schema/freeqrent.schema.json`: every object a class with optional named fields, every set of names an enum (`QrPreset`, `QrToken`, `QrIdle`, `QrEcc`, …), and every `kind` union a sealed class with a factory per kind (`QrModuleShape.dots(size: 0.9)`, `QrFrame.circle(ring: QrFrameRing.dashed)`, `QrTrigger.every(ms: 5000)`). `toJson()` writes exactly the Style document; a kind with no options left out writes its bare name. Dart names: `snake_case` keys are `camelCase`, enum values are camel-cased (`lock-on` → `lockOn`, `L` → `l`), the `class` trigger is `QrTrigger.cssClass` and the `with` logo timing is `QrLogoTiming.together`. Integers are Dart `int`, so a `seed` is limited to 2^63−1 (the document takes any `u64`).

## `FreeQrentAnimated` (`package:freeqrent_flutter`)

The widget plans the code once (`FreeQrent.plan`, or `FreeQrentAnimated.fromPlan` for a plan made elsewhere), parses every layer's path into a `ui.Path`, and a `CustomPainter` draws each frame from the plan's tracks, driven by an `AnimationController`.

- **Triggers:** `FreeQrentTrigger.auto` follows the plan (`load` plays on build, `hover` on pointer enter, `every` replays each period, `class` waits for the controller); `build`, `visible` (once `visibleFraction` of it is on screen, checked after layout and as its scrollable scrolls, with no extra dependency) and `manual`. `FreeQrentController` has `play()`, `reset()` and `finish()`; `onComplete` fires when the intro ends. Idle loops run after it, forever.
- **Reduced motion:** with `MediaQuery.disableAnimations` it shows the finished code, which is the static code (SPEC §8.2), and palette changes jump.
- **Palette:** `palette` overrides tokens over the plan's palette; changing it tweens every token in OKLab over `paletteDuration` (gradients stop by stop).
- **Logos:** the plan carries the logo's plate and window, not its picture. A raster logo in the style is decoded and drawn; for an SVG logo pass `logo:` an `ImageProvider` (for example one rasterized with `flutter_svg`).

What it plays: every keyframe property the plan carries (opacity, scale, scaleY, rotate, translateX/Y, fill, reveal, draw), the reveal masks (wipes and irises, feathered), layers' `fillOpacity` (uneven ink), the `effect`, the `textures` and the `caption`. Drawn exactly with Flutter's own primitives: paths (even-odd included), gradients with the core's geometry and gamut-mapped OKLCH stops, transforms about each layer's origin, the masks, the glow (a Gaussian blur under the code, `ImageFilter.blur`), the emboss (two offset, blurred, tinted copies), scanlines, the shimmer and glint sheens, stroked and dashed logo frames drawing on, and picture sub-cells. **Approximated:** the *sketch* effect is drawn without its noise displacement (Flutter has no turbulence filter), so edges are straight; *grain* is seeded speckles over the card rather than SVG fractal noise; the *caption* uses the platform's sans, serif or monospace font, and its squeeze to fit assumes a one-module margin; an SVG logo needs an `ImageProvider` from you; `every` replays restart idle loops with the intro.

## Build from source and test

```sh
cd freeqrent
dart pub get
dart test                      # 112 tests: the 77 conformance cases, typed styles, errors, payloads, isolates
dart run tool/bench.dart       # timings across the FFI

cd ../freeqrent_flutter
flutter test                   # 34 tests: 26 resvg goldens and the widget's behavior
cargo run --release --manifest-path goldens/Cargo.toml   # rewrite the golden PNGs (after a core change)

cd example && flutter run      # macOS, iOS, Android
```

The goldens (`freeqrent_flutter/test/goldens/`) are each case's static SVG rasterized by resvg through `freeqrent-verify`, in grayscale at 8 px a module. The test draws the widget's frame at rest and at the end of the intro and compares them with the reference at every module center (3×3 pixels averaged); every module has to land on the same side of the middle gray, and the mean difference has to stay under 6 levels. `FREEQRENT_DUMP=1 flutter test test/golden_test.dart` writes the widget's frames to `test/failures/`.

## Regenerate the typed surface

```sh
cd freeqrent
dart run tool/generate.dart          # writes lib/src/style.g.dart from the schema
dart run tool/generate.dart --check  # what the test runs: fails when the file is stale
```

The generator stops with a message when the schema grows a shape it doesn't know or two types would share a name; the fix is usually a line in its hint tables.

## Dart on the web

Not built yet. `dart:ffi` doesn't exist on the web, so a web build would need the WASM boundary the npm package uses: a `freeqrent` implementation behind a conditional import (`if (dart.library.js_interop)`) that loads the wasm-bindgen module (or the raw `freeqrent_alloc`/`freeqrent_dealloc` exports of a `wasm32-unknown-unknown` build of `freeqrent-ffi`) with `dart:js_interop`, passing the same text in and out. The typed styles, models and `FreeQrentAnimated` are pure Dart and would work unchanged; only `lib/src/native.dart` needs a web twin, plus a way to ship the `.wasm` as a Flutter web asset.
