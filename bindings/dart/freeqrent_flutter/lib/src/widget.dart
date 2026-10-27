// FreeQrentAnimated (SPEC §10.4): an animated code drawn by Flutter from
// the core's AnimationPlan, played by an AnimationController.
import 'dart:async';
import 'dart:ui' as ui;

import 'package:flutter/scheduler.dart';
import 'package:flutter/widgets.dart';
import 'package:freeqrent/freeqrent.dart';

import 'painter.dart';
import 'scene.dart';

/// When the intro starts.
enum FreeQrentTrigger {
  /// As the plan says: `load` plays on build, `hover` when a pointer
  /// enters, `every` on build and again every period; `class` waits for
  /// the controller (a page's CSS class has no Flutter counterpart).
  auto,

  /// As soon as the widget is built.
  build,

  /// When at least [FreeQrentAnimated.visibleFraction] of the widget is on
  /// screen (checked after layout and as its scrollable scrolls).
  visible,

  /// Only through [FreeQrentController.play].
  manual,
}

/// Plays, replays and stops a [FreeQrentAnimated].
class FreeQrentController extends ChangeNotifier {
  _FreeQrentAnimatedState? _state;

  /// Starts the intro from the beginning.
  void play() => _state?._play();

  /// Shows the finished code and stops (idle loops stop too).
  void finish() => _state?._finish();

  /// Back to before the intro, waiting for a trigger.
  void reset() => _state?._reset();

  /// Whether the intro has been started.
  bool get started => _state?._started ?? false;

  /// Milliseconds since the intro started.
  double get elapsedMs => _state?._clock.value ?? 0;

  void _changed() => notifyListeners();
}

/// An animated freeQRent code, drawn natively: the plan's paths are parsed
/// into `ui.Path`s once and a `CustomPainter` plays the plan's tracks
/// (opacity, scale, turn, offsets, fills, the reveal mask, drawn-on
/// outlines), its effect, textures and caption, and its idle loops.
///
/// With motion off (`MediaQuery.disableAnimations`) it shows the finished
/// code, which is the static code (SPEC §8.2). Changing [palette] tweens
/// the colors in OKLab over [paletteDuration].
class FreeQrentAnimated extends StatefulWidget {
  /// The code for [payload] (a `String` or `Uint8List`) in [style] (a
  /// `QrStyle`, a `Map` or JSON text), planned with `FreeQrent.plan`.
  const FreeQrentAnimated({
    super.key,
    required Object this.payload,
    this.style,
    this.trigger = FreeQrentTrigger.auto,
    this.controller,
    this.palette,
    this.paletteDuration = const Duration(milliseconds: 300),
    this.paletteCurve = Curves.easeOutCubic,
    this.logo,
    this.size,
    this.visibleFraction = 0.5,
    this.semanticLabel,
    this.onComplete,
  }) : plan = null;

  /// A code from a plan made elsewhere (`FreeQrent.plan`, or a plan's JSON
  /// sent from a server). [style] is optional: it only supplies the logo's
  /// options the plan doesn't carry (backdrop, padding, a raster picture).
  const FreeQrentAnimated.fromPlan(
    AnimationPlan this.plan, {
    super.key,
    this.style,
    this.trigger = FreeQrentTrigger.auto,
    this.controller,
    this.palette,
    this.paletteDuration = const Duration(milliseconds: 300),
    this.paletteCurve = Curves.easeOutCubic,
    this.logo,
    this.size,
    this.visibleFraction = 0.5,
    this.semanticLabel,
    this.onComplete,
  }) : payload = null;

  /// What the code carries.
  final Object? payload;

  /// The Style document.
  final Object? style;

  /// A ready plan.
  final AnimationPlan? plan;

  /// When the intro starts.
  final FreeQrentTrigger trigger;

  /// Plays and resets the intro from outside.
  final FreeQrentController? controller;

  /// Token paints over the plan's palette; a change tweens in OKLab.
  final Map<QrToken, String>? palette;

  /// How long a palette change takes.
  final Duration paletteDuration;

  /// Its curve.
  final Curve paletteCurve;

  /// The logo's picture, drawn in the logo's box (an SVG logo needs one,
  /// for example from flutter_svg; a raster logo in the style is used when
  /// this is null).
  final ImageProvider? logo;

  /// The width; the height follows the code's aspect. Null fills the
  /// available width (or 240 without a bound).
  final double? size;

  /// How much of the widget must be on screen for [FreeQrentTrigger.visible].
  final double visibleFraction;

  /// The label for screen readers (the style's label when null).
  final String? semanticLabel;

  /// Called when the intro has finished (not for endless idle loops).
  final VoidCallback? onComplete;

  @override
  State<FreeQrentAnimated> createState() => _FreeQrentAnimatedState();
}

/// Time in milliseconds, running until [end] (forever when null).
class _Clock extends Simulation {
  _Clock(this.end);

  final double? end;

  @override
  double x(double time) => time * 1000;

  @override
  double dx(double time) => 1000;

  @override
  bool isDone(double time) => end != null && time * 1000 >= end!;
}

class _FreeQrentAnimatedState extends State<FreeQrentAnimated>
    with TickerProviderStateMixin {
  late AnimationController _clock = AnimationController.unbounded(vsync: this);
  late final AnimationController _blend = AnimationController(
    vsync: this,
    value: 1,
  );
  late QrScene _scene;
  bool _started = false;
  bool _finished = false;
  Map<String, String> _palette = const {};
  Map<String, String>? _paletteFrom;
  ui.Image? _logoImage;
  ImageStream? _logoStream;
  ImageStreamListener? _logoListener;
  ScrollPosition? _scroll;
  Timer? _replay;

  AnimationPlan get _plan => _scene.plan;

  @override
  void initState() {
    super.initState();
    _scene = _makeScene();
    _palette = _merged();
    widget.controller?._state = this;
    _clock.addStatusListener(_onStatus);
    if (_startsOnBuild) _play();
  }

  QrScene _makeScene() {
    final plan =
        widget.plan ?? FreeQrent.plan(widget.payload!, style: widget.style);
    return QrScene(plan, logo: LogoOptions.fromStyle(widget.style));
  }

  Map<String, String> _merged() => {
    ..._plan.palette,
    for (final MapEntry(:key, :value) in (widget.palette ?? const {}).entries)
      key.json: value,
  };

  bool get _startsOnBuild => switch (widget.trigger) {
    FreeQrentTrigger.build => true,
    FreeQrentTrigger.auto =>
      _plan.trigger.kind == 'load' || _plan.trigger.kind == 'every',
    _ => false,
  };

  void _onStatus(AnimationStatus status) {
    if (status == AnimationStatus.completed && !_plan.loops) {
      widget.onComplete?.call();
      final every = _plan.trigger.everyMs;
      if (widget.trigger == FreeQrentTrigger.auto &&
          every != null &&
          every > _plan.totalMs) {
        _replay?.cancel();
        _replay = Timer(Duration(milliseconds: every - _plan.totalMs), () {
          if (mounted) _play();
        });
      }
    }
  }

  void _play() {
    _replay?.cancel();
    _started = true;
    _finished = false;
    _clock.value = 0;
    if (_plan.tracks.isNotEmpty) {
      _clock.animateWith(_Clock(_plan.loops ? null : _plan.totalMs.toDouble()));
    }
    widget.controller?._changed();
    if (mounted) setState(() {});
  }

  void _finish() {
    _replay?.cancel();
    _clock.stop();
    _started = true;
    _finished = true;
    widget.controller?._changed();
    if (mounted) setState(() {});
  }

  void _reset() {
    _replay?.cancel();
    _clock.stop();
    _clock.value = 0;
    _started = false;
    _finished = false;
    widget.controller?._changed();
    if (mounted) setState(() {});
    _scheduleVisibilityCheck();
  }

  @override
  void didUpdateWidget(FreeQrentAnimated old) {
    super.didUpdateWidget(old);
    if (old.controller != widget.controller) {
      if (old.controller?._state == this) old.controller!._state = null;
      widget.controller?._state = this;
    }
    final replanned =
        old.plan != widget.plan ||
        old.payload != widget.payload ||
        !_sameStyle(old.style, widget.style);
    if (replanned) {
      _scene.dispose();
      _scene = _makeScene();
      _palette = _merged();
      _paletteFrom = null;
      _resolveLogo();
      if (_startsOnBuild) {
        _play();
      } else {
        _reset();
      }
      return;
    }
    final next = _merged();
    if (!_sameMap(next, _palette)) {
      _paletteFrom = _palette;
      _palette = next;
      _blend
        ..duration = widget.paletteDuration
        ..value = 0;
      if (_reducedMotion) {
        _blend.value = 1;
      } else {
        _blend.forward();
      }
    }
    if (old.logo != widget.logo) _resolveLogo();
  }

  bool _sameStyle(Object? a, Object? b) {
    if (identical(a, b)) return true;
    try {
      return FreeQrent.styleJson(a) == FreeQrent.styleJson(b);
    } on Object {
      return false;
    }
  }

  bool _sameMap(Map<String, String> a, Map<String, String> b) =>
      a.length == b.length && a.entries.every((e) => b[e.key] == e.value);

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _resolveLogo();
    final scroll = Scrollable.maybeOf(context)?.position;
    if (scroll != _scroll) {
      _scroll?.removeListener(_checkVisible);
      _scroll = scroll;
      _scroll?.addListener(_checkVisible);
    }
    _scheduleVisibilityCheck();
  }

  void _resolveLogo() {
    final provider =
        widget.logo ??
        (_scene.logo.raster == null ? null : MemoryImage(_scene.logo.raster!));
    final stream = provider?.resolve(createLocalImageConfiguration(context));
    if (stream?.key == _logoStream?.key && stream != null) return;
    if (_logoListener != null) _logoStream?.removeListener(_logoListener!);
    _logoStream = stream;
    if (stream == null) {
      _logoImage = null;
      return;
    }
    _logoListener = ImageStreamListener((info, _) {
      if (mounted) setState(() => _logoImage = info.image);
    });
    stream.addListener(_logoListener!);
  }

  bool get _watchesVisibility => widget.trigger == FreeQrentTrigger.visible;

  void _scheduleVisibilityCheck() {
    if (!_watchesVisibility || _started) return;
    SchedulerBinding.instance.addPostFrameCallback((_) => _checkVisible());
  }

  void _checkVisible() {
    if (!mounted || !_watchesVisibility || _started) return;
    final box = context.findRenderObject();
    if (box is! RenderBox || !box.attached || !box.hasSize || box.size.isEmpty)
      return;
    final rect = box.localToGlobal(Offset.zero) & box.size;
    final screen = Offset.zero & MediaQuery.sizeOf(context);
    final seen = rect.intersect(screen);
    if (seen.width <= 0 || seen.height <= 0) return;
    final share = seen.width * seen.height / (rect.width * rect.height);
    if (share >= widget.visibleFraction - 1e-9) _play();
  }

  bool get _reducedMotion =>
      MediaQuery.maybeDisableAnimationsOf(context) ?? false;

  @override
  void dispose() {
    _replay?.cancel();
    _scroll?.removeListener(_checkVisible);
    if (_logoListener != null) _logoStream?.removeListener(_logoListener!);
    if (widget.controller?._state == this) widget.controller!._state = null;
    _clock.dispose();
    _blend.dispose();
    _scene.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    _scheduleVisibilityCheck();
    final reduced = _reducedMotion;
    final vb = _plan.viewBox;
    Widget paint = AnimatedBuilder(
      animation: Listenable.merge([_clock, _blend]),
      builder: (context, _) {
        final t = widget.paletteCurve.transform(_blend.value.clamp(0.0, 1.0));
        final every = _plan.trigger.everyMs;
        var time = _clock.value;
        if (_plan.loops &&
            every != null &&
            every > 0 &&
            widget.trigger == FreeQrentTrigger.auto) {
          time %= every;
        }
        return CustomPaint(
          size: Size.infinite,
          painter: QrScenePainter(
            scene: _scene,
            timeMs: _started ? time : 0,
            atRest: reduced || _finished || _plan.tracks.isEmpty,
            palette: PaletteBlend(_palette, from: _paletteFrom, t: t),
            logoImage: _logoImage,
          ),
        );
      },
    );
    if (widget.trigger == FreeQrentTrigger.auto &&
        _plan.trigger.kind == 'hover') {
      paint = MouseRegion(onEnter: (_) => _play(), child: paint);
    }
    final label = widget.semanticLabel ?? _labelOf(widget.style) ?? 'QR code';
    return Semantics(
      label: label,
      image: true,
      child: LayoutBuilder(
        builder: (context, constraints) {
          final width =
              widget.size ??
              (constraints.hasBoundedWidth ? constraints.maxWidth : 240.0);
          var height = width * vb.y / vb.x;
          var w = width;
          if (constraints.hasBoundedHeight && height > constraints.maxHeight) {
            height = constraints.maxHeight;
            w = height * vb.x / vb.y;
          }
          return SizedBox(width: w, height: height, child: paint);
        },
      ),
    );
  }

  String? _labelOf(Object? style) {
    try {
      final doc = FreeQrent.styleJson(style);
      final m = RegExp(r'"label"\s*:\s*"((?:[^"\\]|\\.)*)"').firstMatch(doc);
      return m?.group(1);
    } on Object {
      return null;
    }
  }
}
