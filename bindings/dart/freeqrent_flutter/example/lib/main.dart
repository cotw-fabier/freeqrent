import 'package:flutter/material.dart';
import 'package:freeqrent_flutter/freeqrent_flutter.dart';

void main() => runApp(const ExampleApp());

class ExampleApp extends StatefulWidget {
  const ExampleApp({super.key});

  @override
  State<ExampleApp> createState() => _ExampleAppState();
}

class _ExampleAppState extends State<ExampleApp> {
  final _controller = FreeQrentController();
  var _warm = false;

  static const _styles = <String, QrStyle>{
    'Ripple': QrStyle(
      preset: QrPreset.candy,
      intro: QrIntro.preset(preset: QrIntroName.ripple),
    ),
    'Sweep, glow': QrStyle(
      palette: {QrToken.bg: '#0a0a0f', QrToken.fg: '#7dffb0'},
      allowInverted: true,
      effect: QrEffect.glow(radius: 0.4),
      intro: QrIntro.preset(preset: QrIntroName.sweep),
    ),
    'Settle, idle': QrStyle(
      preset: QrPreset.ribbon,
      intro: QrIntro.preset(preset: QrIntroName.settle),
      idle: [QrIdle.finderPulse, QrIdle.shimmer],
    ),
    'Iris, caption': QrStyle(
      palette: {QrToken.bg: '#ffd93d', QrToken.fg: '#000000'},
      border: QrBorder.solid(width: 1),
      shadow: 1.5,
      caption: QrCaption(text: 'SCAN ME'),
      intro: QrIntro.preset(preset: QrIntroName.iris),
    ),
  };

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'freeQRent',
      home: Scaffold(
        appBar: AppBar(
          title: Text('freeQRent ${FreeQrent.version}'),
          actions: [
            IconButton(
              tooltip: 'Replay',
              icon: const Icon(Icons.replay),
              onPressed: _controller.play,
            ),
            IconButton(
              tooltip: 'Palette',
              icon: const Icon(Icons.palette),
              onPressed: () => setState(() => _warm = !_warm),
            ),
          ],
        ),
        body: ListView(
          padding: const EdgeInsets.all(24),
          children: [
            for (final (i, MapEntry(key: name, value: style))
                in _styles.entries.indexed) ...[
              Text(name, style: Theme.of(context).textTheme.titleMedium),
              const SizedBox(height: 8),
              FreeQrentAnimated(
                payload: 'https://example.com/flutter',
                style: style,
                size: 260,
                controller: i == 0 ? _controller : null,
                trigger: i == 0
                    ? FreeQrentTrigger.build
                    : FreeQrentTrigger.visible,
                palette: i == 0 && _warm ? {QrToken.fg: '#7a1f1f'} : null,
              ),
              const SizedBox(height: 32),
            ],
          ],
        ),
      ),
    );
  }
}
