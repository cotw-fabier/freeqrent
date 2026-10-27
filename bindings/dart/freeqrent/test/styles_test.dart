// Typed styles are the Style document: each one serializes to the JSON a
// hand-written document holds and renders the same bytes.
import 'dart:convert';
import 'dart:typed_data';

import 'package:freeqrent/freeqrent.dart';
import 'package:test/test.dart';

const url = 'https://example.com/conformance';
const logoSvg =
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><circle cx="5" cy="5" r="4" fill="#a3302a"/></svg>';

/// A 4×4 picture: a warm gradient.
QrImage picture() {
  final rgba = Uint8List(4 * 4 * 4);
  for (var i = 0; i < 16; i++) {
    rgba.setAll(i * 4, [250 - i * 12, 200 - i * 8, 160, 255]);
  }
  return QrImage(width: 4, height: 4, rgba: rgba);
}

void main() {
  // Each typed style, and the document it stands for.
  final pairs = <String, (QrStyle, Map<String, Object?>)>{
    'preset and shape': (
      const QrStyle(
        preset: QrPreset.ink,
        modules: QrModuleShape.dots(size: 0.9),
      ),
      {
        'preset': 'ink',
        'modules': {'kind': 'dots', 'size': 0.9},
      },
    ),
    'finders and corners': (
      const QrStyle(
        finders: QrFinders(
          ring: QrRing.squircle(),
          eye: QrEye.circle(),
          corners: QrFindersCorners(
            topRight: QrRingAndEye(ring: QrRing.leaf(), eye: QrEye.leaf()),
          ),
        ),
      ),
      {
        'finders': {
          'ring': 'squircle',
          'eye': 'circle',
          'corners': {
            'top-right': {'ring': 'leaf', 'eye': 'leaf'},
          },
        },
      },
    ),
    'alignment and timing': (
      const QrStyle(
        alignment: QrAlignment(
          shape: QrAlignmentShape.own(ring: QrRing.rounded(radius: 2.5)),
          emphasis: QrEmphasis.outline,
        ),
        timing: QrTiming.dashed,
        encoding: QrEncoding(minVersion: 7),
      ),
      {
        'alignment': {
          'shape': {
            'kind': 'own',
            'ring': {'kind': 'rounded', 'radius': 2.5},
          },
          'emphasis': 'outline',
        },
        'timing': 'dashed',
        'encoding': {'min_version': 7},
      },
    ),
    'palette, frame, border and shadow': (
      const QrStyle(
        palette: {
          QrToken.bg: 'oklch(97% .015 85)',
          QrToken.fg: 'linear-gradient(135deg, #1d2b64, oklch(30% .14 330))',
          QrToken.frame: '#b8860b',
        },
        frame: QrFrame.rounded(radius: 2),
        border: QrBorder.double(width: 1.5),
        shadow: 1.5,
      ),
      {
        'palette': {
          'bg': 'oklch(97% .015 85)',
          'fg': 'linear-gradient(135deg, #1d2b64, oklch(30% .14 330))',
          'frame': '#b8860b',
        },
        'frame': {'kind': 'rounded', 'radius': 2},
        'border': {'kind': 'double', 'width': 1.5},
        'shadow': 1.5,
      },
    ),
    'round frame and seed': (
      const QrStyle(
        modules: QrModuleShape.dots(size: 0.85),
        frame: QrFrame.circle(fillerDensity: 0.4, ring: QrFrameRing.dashed),
        seed: 7,
      ),
      {
        'modules': {'kind': 'dots', 'size': 0.85},
        'frame': {'kind': 'circle', 'filler_density': 0.4, 'ring': 'dashed'},
        'seed': 7,
      },
    ),
    'effect, textures and caption': (
      const QrStyle(
        effect: QrEffect.glow(radius: 0.4),
        palette: {QrToken.bg: '#0a0a0f', QrToken.fg: '#7dffb0'},
        allowInverted: true,
        textures: [QrTexture.scanlines(opacity: 0.25), QrTexture.grain()],
        caption: QrCaption(
          text: 'SCAN ME',
          font: QrCaptionFont.mono,
          bold: false,
        ),
        colorSpread: QrColorSpread.regions,
      ),
      {
        'effect': {'kind': 'glow', 'radius': 0.4},
        'palette': {'bg': '#0a0a0f', 'fg': '#7dffb0'},
        'allow_inverted': true,
        'textures': [
          {'kind': 'scanlines', 'opacity': 0.25},
          'grain',
        ],
        'caption': {'text': 'SCAN ME', 'font': 'mono', 'bold': false},
        'color_spread': 'regions',
      },
    ),
    'intro with options': (
      const QrStyle(
        modules: QrModuleShape.liquid(),
        intro: QrIntro.preset(
          preset: QrIntroName.settle,
          durationMs: 900,
          delayMs: 100,
          groups: 6,
          ease: QrEase.custom(x1: 0.2, y1: 1, x2: 0.4, y2: 1),
          stagger: QrStagger.random(seed: 3),
          trigger: QrTrigger.every(ms: 5000),
        ),
        idle: [QrIdle.finderPulse, QrIdle.shimmer],
        themeTransitionMs: 300,
      ),
      {
        'modules': 'liquid',
        'intro': {
          'preset': 'settle',
          'duration_ms': 900,
          'delay_ms': 100,
          'groups': 6,
          'ease': {'kind': 'custom', 'x1': 0.2, 'y1': 1, 'x2': 0.4, 'y2': 1},
          'stagger': {'kind': 'random', 'seed': 3},
          'trigger': {'kind': 'every', 'ms': 5000},
        },
        'idle': ['finder-pulse', 'shimmer'],
        'theme_transition_ms': 300,
      },
    ),
    'class trigger': (
      const QrStyle(
        intro: QrIntro.preset(
          preset: QrIntroName.iris,
          trigger: QrTrigger.cssClass(name: 'shown'),
        ),
      ),
      {
        'intro': {
          'preset': 'iris',
          'trigger': {'kind': 'class', 'name': 'shown'},
        },
      },
    ),
    'palette shift': (
      const QrStyle(
        preset: QrPreset.candy,
        intro: QrIntro.paletteShift(
          paletteFrom: {QrToken.fg: '#1d3b72'},
          durationMs: 800,
        ),
      ),
      {
        'preset': 'candy',
        'intro': {
          'palette_from': {'fg': '#1d3b72'},
          'duration_ms': 800,
        },
      },
    ),
    'logo': (
      const QrStyle(
        modules: QrModuleShape.liquid(),
        logo: QrLogo(
          svg: logoSvg,
          size: QrLogoSize.small,
          frame: QrLogoFrame.orbit(width: 0.8),
          shape: QrLogoShape.rounded(radius: 0.2),
        ),
        intro: QrIntro.preset(
          preset: QrIntroName.sweep,
          logo: QrLogoIntro.spin,
          logoTiming: QrLogoTiming.together,
        ),
      ),
      {
        'modules': 'liquid',
        'logo': {
          'svg': logoSvg,
          'size': 'small',
          'frame': {'kind': 'orbit', 'width': 0.8},
          'shape': {'kind': 'rounded', 'radius': 0.2},
        },
        'intro': {'preset': 'sweep', 'logo': 'spin', 'logo_timing': 'with'},
      },
    ),
    'text, size and encoding': (
      const QrStyle(
        title: 'Menu',
        label: 'https://example.com/menu',
        idPrefix: 'm',
        targetSize: QrTargetSize.millimeters(mm: 25),
        minify: false,
        quietZone: 5,
        encoding: QrEncoding(
          ecc: QrEcc.l,
          version: 10,
          mask: 3,
          boostEcc: false,
          eci: QrEci.never,
        ),
      ),
      {
        'title': 'Menu',
        'label': 'https://example.com/menu',
        'id_prefix': 'm',
        'target_size': {'kind': 'millimeters', 'mm': 25},
        'minify': false,
        'quiet_zone': 5,
        'encoding': {
          'ecc': 'L',
          'version': 10,
          'mask': 3,
          'boost_ecc': false,
          'eci': 'never',
        },
      },
    ),
  };

  for (final MapEntry(key: name, value: (typed, doc)) in pairs.entries) {
    test('typed $name is its document', () {
      expect(typed.toJson(), doc);
      expect(
        FreeQrent.render(url, style: typed),
        FreeQrent.render(url, style: jsonEncode(doc)),
      );
      // Maps and JSON text are documents too.
      expect(
        FreeQrent.render(url, style: doc),
        FreeQrent.render(url, style: typed),
      );
    });
  }

  test('pictures carry their pixels as base64', () {
    final image = picture();
    final typed = QrStyle(
      halftone: QrHalftone(
        image: image,
        ink: const QrHalftoneInk.color(colors: 4),
        bands: const [0.3, 0.8],
        clarity: const QrHalftoneClarity.amount(0.6),
        fit: QrFit.contain,
        focal: const [0.5, 0.4],
        steer: true,
      ),
    );
    final doc = {
      'halftone': {
        'image': {'width': 4, 'height': 4, 'rgba': base64Encode(image.rgba)},
        'ink': {'kind': 'color', 'colors': 4},
        'bands': [0.3, 0.8],
        'clarity': 0.6,
        'fit': 'contain',
        'focal': [0.5, 0.4],
        'steer': true,
      },
    };
    expect(typed.toJson(), doc);
    expect(
      FreeQrent.render(url, style: typed),
      FreeQrent.render(url, style: jsonEncode(doc)),
    );
    final art = QrStyle(
      art: QrHalftone(image: image, clarity: QrHalftoneClarity.map(image)),
    );
    expect(FreeQrent.render(url, style: art), startsWith('<svg'));
  });

  test('bare names are the same as their kind objects', () {
    expect(
      FreeQrent.render(
        url,
        style: const QrStyle(modules: QrModuleShape.rounded()),
      ),
      FreeQrent.render(url, style: '{"modules":"rounded"}'),
    );
    expect(
      FreeQrent.render(
        url,
        style: const QrStyle(intro: QrIntro.preset(preset: QrIntroName.ripple)),
      ),
      FreeQrent.render(url, style: '{"intro":"ripple"}'),
    );
    expect(
      FreeQrent.render(
        url,
        style: const QrStyle(caption: QrCaption(text: 'hi')),
      ),
      FreeQrent.render(url, style: '{"caption":"hi"}'),
    );
  });

  test('typed parts can sit inside a map', () {
    expect(
      FreeQrent.render(
        url,
        style: {'modules': const QrModuleShape.dots(size: 0.8)},
      ),
      FreeQrent.render(url, style: '{"modules":{"kind":"dots","size":0.8}}'),
    );
  });

  test('every preset name is typed', () {
    expect(FreeQrent.presets, [for (final p in QrPreset.values) p.json]);
  });

  test('a palette result drops into a style', () {
    final result = FreeQrent.paletteFromCss(
      ':root { --brand: #1d3b72; --accent: #c2410c; --paper: #f8f5ee; }',
    );
    expect(result.palette.keys, containsAll([QrToken.bg, QrToken.fg]));
    expect(result.swatches, isNotEmpty);
    expect(result.choices.first.origin.kind, isNotEmpty);
    final svg = FreeQrent.render(url, style: QrStyle(palette: result.palette));
    expect(svg, contains(result.palette[QrToken.fg]!.substring(0, 4)));

    final html = FreeQrent.paletteFromHtml(
      '<style>body{background:#fffaf0;color:#2b2118}</style>',
      options: const QrPaletteOpts(logoColors: ['#a3302a']),
    );
    expect(html.palette[QrToken.bg], isNotNull);

    final rgba = Uint8List(8 * 8 * 4);
    for (var i = 0; i < 64; i++) {
      rgba.setAll(i * 4, i < 32 ? [20, 30, 90, 255] : [245, 240, 225, 255]);
    }
    final image = FreeQrent.paletteFromImage(
      rgba,
      8,
      8,
      options: const QrPaletteOpts(clusters: 2, seed: 1),
    );
    expect(
      FreeQrent.check(url, style: QrStyle(palette: image.palette)).ok,
      isTrue,
    );
  });

  test('the report is typed', () {
    final clean = FreeQrent.check(
      url,
      style: const QrStyle(preset: QrPreset.ink),
    );
    expect(clean.ok, isTrue);
    expect(clean.code.ecc, isA<QrEcc>());
    expect(clean.code.size, clean.code.version * 4 + 17);
    final warned = FreeQrent.check(
      url,
      style: const QrStyle(
        preset: QrPreset.terminal,
        modules: QrModuleShape.dots(size: 0.7),
      ),
    );
    expect(warned.warnings.map((w) => w.code), contains('small_dots'));
    final refused = FreeQrent.check(url, style: '{"palette":{"fg":"#bbb"}}');
    expect(refused.ok, isFalse);
    expect(refused.errors.first.code, 'contrast_too_low');
    expect(refused.errors.first.detail['token'], 'fg');
  });

  test('the plan is typed', () {
    final plan = FreeQrent.plan(
      url,
      style: const QrStyle(
        preset: QrPreset.ink,
        intro: QrIntro.preset(preset: QrIntroName.ripple),
        idle: [QrIdle.shimmer],
        effect: QrEffect.glow(),
        textures: [QrTexture.ink()],
      ),
    );
    expect(plan.layers.first.role, PlanRole.background);
    expect(plan.tracks, isNotEmpty);
    expect(plan.loops, isTrue);
    expect(plan.effect?.kind, 'glow');
    expect(plan.textures.single.kind, 'ink');
    expect(plan.layers.any((l) => l.fillOpacity < 1), isTrue);
    expect(plan.totalMs, greaterThan(0));
    // At the end of the intro every layer is at rest.
    for (var i = 0; i < plan.layers.length; i++) {
      final pose = plan.poseAt(i, plan.totalMs.toDouble());
      if (plan.layers[i].role == PlanRole.shimmer) continue;
      expect(pose.opacity, plan.layers[i].opacity);
      expect(pose.still, isTrue);
    }
    // And at the start the data waits, hidden.
    final data = plan.layers.indexWhere((l) => l.role == PlanRole.data);
    expect(plan.poseAt(data, 0).opacity, lessThan(1));
  });
}
