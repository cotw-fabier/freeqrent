// FreeQrentAnimated's behavior: reduced motion, triggers, the controller,
// idle loops and palette transitions.
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:freeqrent_flutter/freeqrent_flutter.dart';

import 'support.dart';

const url = 'https://example.com/flutter';
const ripple = QrStyle(
  preset: QrPreset.candy,
  intro: QrIntro.preset(preset: QrIntroName.ripple),
);

QrScenePainter painterOf(WidgetTester tester) {
  final paints = tester.widgetList<CustomPaint>(
    find.descendant(
      of: find.byType(FreeQrentAnimated),
      matching: find.byType(CustomPaint),
    ),
  );
  return paints.map((p) => p.painter).whereType<QrScenePainter>().single;
}

/// What the widget's painter draws now, as grayscale pixels.
Future<Gray> pixels(WidgetTester tester) async {
  final p = painterOf(tester);
  return (await tester.runAsync(
    () => draw(
      p.scene,
      timeMs: p.timeMs,
      atRest: p.atRest,
      palette: p.palette.to,
    ),
  ))!;
}

Widget host(Widget child, {bool reduceMotion = false}) => MediaQuery(
  data: MediaQueryData(
    size: const Size(800, 600),
    disableAnimations: reduceMotion,
  ),
  child: Directionality(
    textDirection: TextDirection.ltr,
    child: Center(child: child),
  ),
);

void main() {
  late Gray finished;
  late ui.Size viewBox;

  setUpAll(() async {
    final plan = FreeQrent.plan(url, style: ripple);
    viewBox = ui.Size(plan.viewBox.x, plan.viewBox.y);
    finished = await draw(QrScene(plan), atRest: true);
  });

  testWidgets('plays the intro on build and ends on the static code', (
    tester,
  ) async {
    await tester.pumpWidget(
      host(const FreeQrentAnimated(payload: url, style: ripple, size: 200)),
    );
    expect(painterOf(tester).atRest, isFalse);
    await tester.pump(const Duration(milliseconds: 300));
    final mid = await pixels(tester);
    expect(painterOf(tester).timeMs, closeTo(300, 20));
    expect(
      compareModules(mid, finished, viewBox).mismatched,
      isNotEmpty,
      reason: 'a mid-intro frame differs',
    );
    await tester.pumpAndSettle();
    final end = await pixels(tester);
    expect(compareModules(end, finished, viewBox).mismatched, isEmpty);
    expect(tester.getSize(find.byType(FreeQrentAnimated)).width, 200);
  });

  testWidgets('reduced motion shows the finished code at once', (tester) async {
    await tester.pumpWidget(
      host(
        const FreeQrentAnimated(payload: url, style: ripple),
        reduceMotion: true,
      ),
    );
    expect(painterOf(tester).atRest, isTrue);
    final now = await pixels(tester);
    expect(compareModules(now, finished, viewBox).mismatched, isEmpty);
    // Nothing is left ticking towards a frame that would differ.
    await tester.pump(const Duration(milliseconds: 100));
    expect(painterOf(tester).atRest, isTrue);
  });

  testWidgets('a manual trigger waits for the controller', (tester) async {
    final controller = FreeQrentController();
    var done = 0;
    await tester.pumpWidget(
      host(
        FreeQrentAnimated(
          payload: url,
          style: ripple,
          trigger: FreeQrentTrigger.manual,
          controller: controller,
          onComplete: () => done++,
        ),
      ),
    );
    await tester.pump(const Duration(seconds: 2));
    expect(controller.started, isFalse);
    expect(painterOf(tester).timeMs, 0);
    // Before its trigger the code waits in the intro's first frame.
    expect(
      compareModules(await pixels(tester), finished, viewBox).mismatched,
      isNotEmpty,
    );
    controller.play();
    await tester.pump();
    expect(controller.started, isTrue);
    await tester.pumpAndSettle();
    expect(done, 1);
    expect(
      compareModules(await pixels(tester), finished, viewBox).mismatched,
      isEmpty,
    );
    controller.reset();
    await tester.pump();
    expect(controller.started, isFalse);
    controller.finish();
    await tester.pump();
    expect(painterOf(tester).atRest, isTrue);
  });

  testWidgets('a visible trigger starts once the code is on screen', (
    tester,
  ) async {
    final controller = FreeQrentController();
    await tester.pumpWidget(
      MediaQuery(
        data: const MediaQueryData(size: Size(800, 600)),
        child: Directionality(
          textDirection: TextDirection.ltr,
          child: ListView(
            children: [
              const SizedBox(height: 1500),
              FreeQrentAnimated(
                payload: url,
                style: ripple,
                size: 200,
                trigger: FreeQrentTrigger.visible,
                controller: controller,
              ),
              const SizedBox(height: 1500),
            ],
          ),
        ),
      ),
    );
    await tester.pump();
    expect(controller.started, isFalse);
    await tester.drag(find.byType(ListView), const Offset(0, -1200));
    await tester.pump();
    expect(controller.started, isTrue);
    await tester.pumpAndSettle();
  });

  testWidgets('idle loops keep playing after the intro', (tester) async {
    const style = QrStyle(
      preset: QrPreset.ribbon,
      intro: QrIntro.preset(preset: QrIntroName.settle),
      idle: [QrIdle.finderPulse],
    );
    final plan = FreeQrent.plan(url, style: style);
    expect(plan.loops, isTrue);
    await tester.pumpWidget(
      host(const FreeQrentAnimated(payload: url, style: style)),
    );
    await tester.pump(Duration(milliseconds: plan.totalMs + 50));
    final times = <double>[];
    final poses = <String>{};
    final looping = plan.tracks.firstWhere((t) => t.iterations == null).layer!;
    for (var i = 0; i < 12; i++) {
      await tester.pump(const Duration(milliseconds: 250));
      final t = painterOf(tester).timeMs;
      times.add(t);
      final pose = plan.poseAt(looping, t);
      poses.add(
        '${pose.scale} ${pose.opacity} ${pose.matrix(plan.layers[looping].origin)}',
      );
    }
    expect(times.last, greaterThan(times.first));
    expect(poses.length, greaterThan(1), reason: 'the finders pulse');
    // With motion off the loops rest too.
    await tester.pumpWidget(
      host(
        const FreeQrentAnimated(payload: url, style: style),
        reduceMotion: true,
      ),
    );
    expect(painterOf(tester).atRest, isTrue);
  });

  testWidgets('a palette change tweens in OKLab', (tester) async {
    Widget code(String fg) => host(
      FreeQrentAnimated(
        payload: url,
        style: const QrStyle(preset: QrPreset.ink),
        palette: {QrToken.fg: fg},
        paletteDuration: const Duration(milliseconds: 400),
        paletteCurve: Curves.linear,
      ),
    );
    await tester.pumpWidget(code('#1d3b72'));
    await tester.pumpWidget(code('#7a1f1f'));
    await tester.pump(const Duration(milliseconds: 200));
    final blend = painterOf(tester).palette;
    expect(blend.t, closeTo(0.5, 0.05));
    final mid = (blend.paint('fg') as CssSolid).color;
    final expected = CssColor.lerpOklab(
      CssColor.parse('#1d3b72'),
      CssColor.parse('#7a1f1f'),
      blend.t,
    );
    expect(mid.srgb8, expected.srgb8);
    // Not the sRGB midpoint.
    expect(
      mid.srgb8,
      isNot(
        CssColor.lerpSrgb8(
          CssColor.parse('#1d3b72'),
          CssColor.parse('#7a1f1f'),
          blend.t,
          1,
        ).srgb8,
      ),
    );
    await tester.pumpAndSettle();
    final end = painterOf(tester).palette;
    expect(
      (end.paint('fg') as CssSolid).color.srgb8,
      CssColor.parse('#7a1f1f').srgb8,
    );
    // Tokens that fall back to fg move with it.
    expect(
      (end.paint('timing') as CssSolid).color.srgb8,
      CssColor.parse('#7a1f1f').srgb8,
    );
  });

  testWidgets('a plan from elsewhere plays the same', (tester) async {
    final plan = AnimationPlan.parse(FreeQrent.planJson(url, style: ripple));
    await tester.pumpWidget(
      host(FreeQrentAnimated.fromPlan(plan, semanticLabel: 'Menu')),
    );
    expect(find.bySemanticsLabel('Menu'), findsOneWidget);
    await tester.pumpAndSettle();
    expect(
      compareModules(await pixels(tester), finished, viewBox).mismatched,
      isEmpty,
    );
  });

  test('path data parses every command', () {
    final p = parseSvgPath(
      'M1 1h2v2H1zm4 0l1 1L7 1c0 1 1 1 1 0s1-1 1 0q1 1 2 0t2 0a1 1 0 0 1 2 0A1 1 0 1 0 17 1z m-.5.5h.5',
    );
    final b = p.getBounds();
    expect(b.left, 1);
    expect(b.right, closeTo(17, 1e-6));
  });
}
