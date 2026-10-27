// The npm `freeqrent` command (SPEC §10.5): the Rust CLI's flags, as
// bindings/SURFACE.md lists them, turned into a Style document and run
// through the WebAssembly build. The CLI's own tools (--verify, the gallery,
// the scan sheet) stay in the Rust CLI. Pictures are read as PNG by a
// decoder in plain JavaScript (png.cjs); JPEG files are refused.
'use strict';

const fs = require('node:fs');

const fq = require('../dist/node.cjs');
const { decodePng, isJpeg, isPng } = require('./png.cjs');
const { SAMPLE_CSS, SAMPLE_DARK_CSS, SAMPLE_LOGO, samplePicture } = require('./sample.cjs');

const HELP = `freeqrent (npm): styled SVG QR codes, through freeQRent's WebAssembly build

Usage: freeqrent [options] <TEXT>

Takes the Rust CLI's flags (see \`cargo install freeqrent-cli\` and its --help,
or bindings/SURFACE.md), and writes the same bytes. Pictures (--halftone,
--art, --palette-from-image, --clarity-map, --priority) are read as PNG or
\`sample\`; JPEG isn't decoded here. --verify, --gallery, --gallery-picture
and --scan-sheet are the Rust CLI's own tools.

Output:
  -o, --output FILE        write here (default: stdout)
  --check                  print the scan report as JSON instead of the SVG
  --plan                   write the animation plan as JSON instead
  --frame-at MS            write the intro stopped at MS as a still SVG
  --show-palette           print what --palette-from-* found, as flags
  -h, --help               show this help
`;

/** A bad command line (exit 2). */
class Usage extends Error {}
/** The code was refused or couldn't be written (exit 1). */
class Failed extends Error {}

const SWITCHES = new Set([
  'pretty',
  'allow-inverted',
  'dark',
  'no-logo-backdrop',
  'keep-modules',
  'no-steer',
  'caption-regular',
  'check',
  'plan',
  'show-palette',
  'help',
]);

const CLI_ONLY = {
  verify: false,
  gallery: true,
  'gallery-picture': true,
  'scan-sheet': true,
};

const VALUES = new Set([
  'output',
  'frame-at',
  'ecc',
  'version',
  'preset',
  'shape',
  'finder',
  'corner',
  'alignment',
  'emphasis',
  'timing',
  'token',
  'fg',
  'bg',
  'quiet-zone',
  'target-px',
  'target-mm',
  'frame',
  'frame-ring',
  'border',
  'shadow',
  'spread',
  'effect',
  'texture',
  'caption',
  'caption-size',
  'caption-font',
  'caption-tracking',
  'seed',
  'logo',
  'logo-scale',
  'logo-shape',
  'logo-padding',
  'logo-size',
  'logo-frame',
  'logo-frame-width',
  'logo-frame-color',
  'logo-moon',
  'logo-intro',
  'logo-timing',
  'logo-frame-intro',
  'logo-frame-timing',
  'intro',
  'intro-ms',
  'stagger',
  'ease',
  'groups',
  'trigger',
  'from-token',
  'from-fg',
  'from-bg',
  'idle',
  'theme-transition',
  'palette-from-image',
  'palette-from-css',
  'palette-from-html',
  'palette-clusters',
  'palette-contrast',
  'halftone',
  'art',
  'ink',
  'bands',
  'clarity',
  'clarity-map',
  'fit',
  'focal',
  'priority',
  'tracking-grid',
  'title',
  'label',
]);

/**
 * Splits the command line like the Rust CLI's lexopt: `--flag value`,
 * `--flag=value`, `-o FILE`, `-oFILE`, and `--` before positional words.
 */
function lex(argv) {
  const out = [];
  let i = 0;
  let positional = false;
  const value = (flag, inline) => {
    if (inline !== undefined) return inline;
    if (i >= argv.length) throw new Usage(`missing argument for option '--${flag}'`);
    return argv[i++];
  };
  while (i < argv.length) {
    const arg = argv[i++];
    if (positional || arg === '-' || !arg.startsWith('-')) {
      out.push({ text: arg });
    } else if (arg === '--') {
      positional = true;
    } else if (arg.startsWith('--')) {
      const eq = arg.indexOf('=');
      const flag = eq < 0 ? arg.slice(2) : arg.slice(2, eq);
      const inline = eq < 0 ? undefined : arg.slice(eq + 1);
      if (SWITCHES.has(flag) || CLI_ONLY[flag] === false) {
        if (inline !== undefined) throw new Usage(`unexpected argument for option '--${flag}'`);
        out.push({ flag });
      } else if (VALUES.has(flag) || CLI_ONLY[flag] === true) {
        out.push({ flag, value: value(flag, inline) });
      } else {
        throw new Usage(`unknown option --${flag}`);
      }
    } else if (arg === '-h') {
      out.push({ flag: 'help' });
    } else if (arg.startsWith('-o')) {
      out.push({ flag: 'output', value: arg.length > 2 ? arg.slice(2) : value('output') });
    } else {
      throw new Usage(`invalid option '${arg}'`);
    }
  }
  return out;
}

// Numbers as Rust parses an f32: decimal, with an optional exponent.
const NUMBER = /^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$/;
const WHOLE = /^\+?\d+$/;

function number(flag, v) {
  if (!NUMBER.test(v)) throw new Usage(`--${flag}: ${JSON.stringify(v)} isn't a number`);
  return Number(v);
}

function whole(flag, v, what = 'a whole number') {
  if (!WHOLE.test(v)) throw new Usage(`--${flag}: ${JSON.stringify(v)} isn't ${what}`);
  return Number(v);
}

// A u64, kept exact.
function u64(flag, v) {
  if (!WHOLE.test(v) || BigInt(v) > 18446744073709551615n) {
    throw new Usage(`--${flag}: ${JSON.stringify(v)} isn't a whole number`);
  }
  return BigInt(v);
}

/** `name[:number]`. */
function named(flag, v) {
  const colon = v.indexOf(':');
  if (colon < 0) return [v, undefined];
  return [v.slice(0, colon), number(flag, v.slice(colon + 1))];
}

function noParam(flag, name, param) {
  if (param !== undefined) throw new Usage(`--${flag}: ${name} takes no parameter`);
}

function oneOf(flag, v, names) {
  if (!names.includes(v)) throw new Usage(`--${flag}: unknown ${JSON.stringify(v)}: try ${names.join(', ')}`);
  return v;
}

function ring(flag, v) {
  const [name, r] = named(flag, v);
  if (name === 'rounded') return r === undefined ? 'rounded' : { kind: 'rounded', radius: r };
  oneOf(flag, name, ['square', 'rounded', 'squircle', 'leaf']);
  noParam(flag, name, r);
  return name;
}

function eye(flag, v) {
  const [name, r] = named(flag, v);
  if (name === 'rounded') return r === undefined ? 'rounded' : { kind: 'rounded', radius: r };
  oneOf(flag, name, ['square', 'rounded', 'circle', 'squircle', 'leaf']);
  noParam(flag, name, r);
  return name;
}

// `RING[/EYE]`; a ring alone brings its matching eye (the document does
// the same).
function ringAndEye(flag, v) {
  const slash = v.indexOf('/');
  if (slash < 0) return { ring: ring(flag, v) };
  return { ring: ring(flag, v.slice(0, slash)), eye: eye(flag, v.slice(slash + 1)) };
}

function shape(flag, v) {
  const [name, p] = named(flag, v);
  switch (name) {
    case 'square':
      return p === undefined ? 'square' : { kind: 'square', gap: p };
    case 'rounded':
      return p === undefined ? 'rounded' : { kind: 'rounded', radius: p };
    case 'dots':
      return p === undefined ? 'dots' : { kind: 'dots', size: p };
    case 'liquid':
      return p === undefined ? 'liquid' : { kind: 'liquid', smoothness: p };
    case 'bars-v':
    case 'bars-h': {
      const s = { kind: 'bars', axis: name === 'bars-v' ? 'vertical' : 'horizontal' };
      if (p !== undefined) s.radius = p;
      return s;
    }
    default:
      throw new Usage(`--${flag}: unknown shape ${JSON.stringify(name)}: try square, rounded, dots, liquid, bars-v or bars-h`);
  }
}

function frame(flag, v, frameRing) {
  const [name, p] = named(flag, v);
  switch (name) {
    case 'square':
    case 'none':
      noParam(flag, name, p);
      return name;
    case 'rounded':
      return p === undefined ? 'rounded' : { kind: 'rounded', radius: p };
    case 'circle': {
      if (p === undefined && frameRing === undefined) return 'circle';
      const f = { kind: 'circle' };
      if (p !== undefined) f.filler_density = p;
      if (frameRing !== undefined) f.ring = frameRing;
      return f;
    }
    default:
      throw new Usage(`--${flag}: unknown frame ${JSON.stringify(name)}: try square, rounded, circle or none`);
  }
}

function border(flag, v) {
  const [name, w] = named(flag, v);
  oneOf(flag, name, ['solid', 'double']);
  return w === undefined ? name : { kind: name, width: w };
}

function effect(flag, v) {
  const parts = v.split(':');
  const [name, a, b] = parts;
  if (name === 'glow' && (parts.length === 2 || parts.length === 3)) {
    return { kind: 'glow', radius: number(flag, a), strength: b === undefined ? 0.25 : number(flag, b) };
  }
  if (name === 'emboss' && parts.length === 2) return { kind: 'emboss', depth: number(flag, a) };
  if (name === 'sketch' && parts.length === 2) return { kind: 'sketch', roughness: number(flag, a) };
  throw new Usage(`--${flag}: unknown effect ${JSON.stringify(v)}: try glow:RADIUS[:STRENGTH], emboss:DEPTH or sketch:ROUGHNESS`);
}

function texture(flag, v) {
  const colon = v.indexOf(':');
  const keys = { grain: 'amount', scanlines: 'opacity', ink: 'fade' };
  const name = v.slice(0, colon);
  if (colon < 0 || !(name in keys)) {
    throw new Usage(`--${flag}: unknown texture ${JSON.stringify(v)}: try grain:AMOUNT, scanlines:OPACITY or ink:FADE`);
  }
  return { kind: name, [keys[name]]: number(flag, v.slice(colon + 1)) };
}

function logoShape(flag, v) {
  const [name, r] = named(flag, v);
  if (name === 'rounded') return r === undefined ? 'rounded' : { kind: 'rounded', radius: r };
  oneOf(flag, name, ['square', 'circle']);
  noParam(flag, name, r);
  return name;
}

function stagger(flag, v) {
  const colon = v.indexOf(':');
  const name = colon < 0 ? v : v.slice(0, colon);
  if (name === 'random') {
    return { kind: 'random', seed: colon < 0 ? 0 : u64(flag, v.slice(colon + 1)) };
  }
  oneOf(flag, name, ['radial', 'linear', 'diagonal', 'polar', 'random']);
  if (colon >= 0) throw new Usage(`--${flag}: ${name} takes no seed`);
  return name;
}

function ease(flag, v) {
  if (['quart', 'quint', 'expo'].includes(v)) return v;
  const parts = v.split(',').map((p) => p.trim());
  if (parts.length !== 4 || !parts.every((p) => NUMBER.test(p))) {
    throw new Usage(`--${flag}: ease ${JSON.stringify(v)}: try quart, quint, expo or x1,y1,x2,y2`);
  }
  const [x1, y1, x2, y2] = parts.map(Number);
  return { kind: 'custom', x1, y1, x2, y2 };
}

function trigger(flag, v) {
  if (v === 'load' || v === 'hover') return v;
  if (v.startsWith('class:') && v.length > 6) return { kind: 'class', name: v.slice(6) };
  if (v.startsWith('every:')) return { kind: 'every', ms: whole(flag, v.slice(6), 'milliseconds') };
  throw new Usage(`--${flag}: unknown trigger ${JSON.stringify(v)}: try load, hover, class:NAME or every:MS`);
}

function token(flag, v) {
  const eq = v.indexOf('=');
  if (eq < 0) throw new Usage(`--${flag}: expected NAME=VALUE, got ${JSON.stringify(v)}`);
  return [v.slice(0, eq), v.slice(eq + 1)];
}

function pair(flag, v) {
  const comma = v.indexOf(',');
  if (comma < 0) throw new Usage(`--${flag}: ${JSON.stringify(v)} isn't two numbers like 0.3,0.9`);
  return [number(flag, v.slice(0, comma).trim()), number(flag, v.slice(comma + 1).trim())];
}

function ink(flag, v) {
  const [name, n] = named(flag, v);
  if (name === 'mono') {
    noParam(flag, name, n);
    return 'mono';
  }
  if (name === 'color') {
    const colors = n === undefined ? 6 : n;
    if (!(colors >= 2 && colors <= 16) || !Number.isInteger(colors)) {
      throw new Usage(`--${flag}: color:${colors}: 2-16 colors per band`);
    }
    return { kind: 'color', colors };
  }
  throw new Usage(`--${flag}: unknown ink ${JSON.stringify(v)}: try mono or color[:N]`);
}

function readFile(flag, path, io) {
  try {
    return io.readFile(path);
  } catch (e) {
    throw new Usage(`--${flag} ${path}: ${e.message}`);
  }
}

/** A picture flag's pixels: a PNG file, or `sample`. */
function picture(flag, source, io) {
  if (source === 'sample') return samplePicture();
  const bytes = readFile(flag, source, io);
  if (isJpeg(bytes)) {
    throw new Usage(
      `--${flag} ${source}: JPEG isn't decoded by the npm bin (it carries no image decoder); convert it to PNG, or use the Rust CLI`,
    );
  }
  if (!isPng(bytes)) throw new Usage(`--${flag} ${source}: not a PNG file`);
  try {
    return decodePng(bytes);
  } catch (e) {
    throw new Usage(`--${flag} ${source}: ${e.message}`);
  }
}

// `-` reads standard input.
function readText(flag, source, io) {
  if (source === '-') return io.readStdin();
  return readFile(flag, source, io).toString('utf8');
}

/** A logo file as the document's `svg` or `raster`, like the CLI's load_logo. */
function logo(source, io) {
  if (source === 'sample') return { svg: SAMPLE_LOGO };
  const bytes = readFile('logo', source, io);
  const start = bytes.findIndex((b) => !(b === 0x20 || (b >= 0x09 && b <= 0x0d)));
  if (source.toLowerCase().endsWith('.svg') || (start >= 0 && bytes[start] === 0x3c)) {
    const svg = new TextDecoder('utf-8', { fatal: true });
    try {
      return { svg: svg.decode(bytes) };
    } catch {
      throw new Usage(`--logo ${source}: not UTF-8`);
    }
  }
  const sig = (s) => s.every((b, i) => bytes[i] === b);
  const raster =
    isPng(bytes) ||
    isJpeg(bytes) ||
    sig([0x47, 0x49, 0x46, 0x38]) ||
    (sig([0x52, 0x49, 0x46, 0x46]) && String.fromCharCode(...bytes.subarray(8, 12)) === 'WEBP');
  if (!raster) throw new Usage(`--logo ${source}: not an SVG, PNG, JPEG, GIF or WebP file`);
  return { raster: new Uint8Array(bytes) };
}

/**
 * The logo's colors, which the CLI hands to palette extraction so the
 * frame keeps apart from them: a PNG's swatches. GIF and WebP logos have
 * none (the CLI doesn't decode them either). An SVG's paints and a JPEG's
 * swatches can't be had here.
 */
function logoColors(source, logoDoc) {
  if (logoDoc.svg !== undefined) {
    throw new Usage(
      `--logo ${source} with --palette-from-*: an SVG logo's colors aren't exposed by the library's boundary yet, so the npm bin can't keep the extracted frame apart from them; use the Rust CLI, or give the palette with --fg/--bg/--token`,
    );
  }
  const bytes = logoDoc.raster;
  if (isPng(bytes)) return fq.paletteFromImage(decodePng(bytes)).swatches.map((s) => s.color);
  if (isJpeg(bytes)) {
    throw new Usage(`--logo ${source} with --palette-from-*: a JPEG logo isn't decoded by the npm bin; convert it to PNG`);
  }
  return [];
}

/**
 * The command line as `{text, style, palette, mode, ...}`: the Style
 * document the flags describe, and what to do with it. `io` reads files
 * and standard input.
 */
function parse(argv, io = nodeIo) {
  const s = {
    tokens: [],
    corners: [],
    textures: [],
    idle: [],
    fromTokens: [],
    switches: new Set(),
  };
  let text = undefined;
  for (const a of lex(argv)) {
    if (a.text !== undefined) {
      if (text !== undefined) throw new Usage(`unexpected argument ${JSON.stringify(a.text)}`);
      text = a.text;
      continue;
    }
    const { flag, value: v } = a;
    if (flag in CLI_ONLY) {
      throw new Usage(`--${flag} is the Rust CLI's own tool (cargo install freeqrent-cli); the npm bin leaves it out`);
    }
    if (SWITCHES.has(flag)) {
      s.switches.add(flag);
      continue;
    }
    switch (flag) {
      case 'output':
        s.output = v;
        break;
      case 'frame-at':
        s.frameAt = whole(flag, v, 'a whole number of ms');
        break;
      case 'ecc':
        s.ecc = oneOf(flag, v.toUpperCase(), ['L', 'M', 'Q', 'H']);
        break;
      case 'version': {
        const n = WHOLE.test(v) ? Number(v) : NaN;
        if (!(n >= 1 && n <= 40)) throw new Usage(`version ${JSON.stringify(v)} isn't 1-40`);
        s.version = n;
        break;
      }
      case 'preset':
        s.preset = oneOf(flag, v, fq.presets());
        break;
      case 'shape':
        s.shape = shape(flag, v);
        break;
      case 'finder':
        s.finder = ringAndEye(flag, v);
        break;
      case 'corner': {
        const eq = v.indexOf('=');
        if (eq < 0) throw new Usage(`--${flag}: expected CORNER=RING[/EYE], got ${JSON.stringify(v)}`);
        const corner = oneOf(flag, v.slice(0, eq), ['top-left', 'top-right', 'bottom-left']);
        s.corners.push([corner, ringAndEye(flag, v.slice(eq + 1))]);
        break;
      }
      case 'alignment':
        s.alignment = v === 'match' ? 'match' : { kind: 'own', ...ringAndEye(flag, v) };
        break;
      case 'emphasis':
        s.emphasis = oneOf(flag, v, ['none', 'accent', 'outline']);
        break;
      case 'timing':
        s.timing = oneOf(flag, v, ['plain', 'dotted', 'dashed']);
        break;
      case 'token':
        s.tokens.push(token(flag, v));
        break;
      case 'fg':
      case 'bg':
        s.tokens.push([flag, v]);
        break;
      case 'logo-frame-color':
        s.tokens.push(['logo.frame', v]);
        break;
      case 'quiet-zone':
        s.quietZone = whole(flag, v, 'a number 0-255');
        break;
      case 'target-px':
        s.target = { kind: 'pixels', px: whole(flag, v, 'a pixel count') };
        break;
      case 'target-mm':
        s.target = { kind: 'millimeters', mm: number(flag, v) };
        break;
      case 'frame':
        frame(flag, v, undefined);
        s.frame = v;
        break;
      case 'frame-ring':
        s.frameRing = oneOf(flag, v, ['none', 'solid', 'dashed']);
        break;
      case 'border':
        s.border = border(flag, v);
        break;
      case 'shadow':
        s.shadow = number(flag, v);
        break;
      case 'spread':
        s.spread = oneOf(flag, v, ['modules', 'regions']);
        break;
      case 'effect':
        s.effect = effect(flag, v);
        break;
      case 'texture':
        s.textures.push(texture(flag, v));
        break;
      case 'caption':
        s.caption = v;
        break;
      case 'caption-size':
        s.captionSize = number(flag, v);
        break;
      case 'caption-font':
        s.captionFont = oneOf(flag, v, ['sans', 'serif', 'mono']);
        break;
      case 'caption-tracking':
        s.captionTracking = whole(flag, v, 'a whole number 0-255');
        break;
      case 'seed':
        s.seed = u64(flag, v);
        break;
      case 'logo':
        s.logo = v;
        break;
      case 'logo-scale':
        s.logoScale = number(flag, v);
        break;
      case 'logo-shape':
        s.logoShape = logoShape(flag, v);
        break;
      case 'logo-padding':
        s.logoPadding = number(flag, v);
        break;
      case 'logo-size':
        s.logoSize = oneOf(flag, v, ['small', 'medium', 'large', 'largest']);
        break;
      case 'logo-frame': {
        const [name, w] = named(flag, v);
        s.logoFrame = [oneOf(flag, name, ['ring', 'beads', 'stamp', 'brackets', 'orbit']), w];
        break;
      }
      case 'logo-frame-width':
        s.logoFrameWidth = number(flag, v);
        break;
      case 'logo-moon':
        s.logoMoon = number(flag, v);
        break;
      case 'logo-intro':
      case 'logo-frame-intro': {
        const intro = oneOf(flag, v, ['none', 'reveal', 'stamp', 'draw', 'lock', 'ink', 'shrink', 'spin']);
        if (flag === 'logo-intro') s.logoIntro = intro;
        else s.logoFrameIntro = intro;
        break;
      }
      case 'logo-timing':
      case 'logo-frame-timing': {
        const when = oneOf(flag, v, ['after', 'before', 'with']);
        if (flag === 'logo-timing') s.logoTiming = when;
        else s.logoFrameTiming = when;
        break;
      }
      case 'intro':
        s.intro = oneOf(flag, v, ['settle', 'ripple', 'lock-on', 'sweep', 'raster', 'iris', 'assemble', 'develop']);
        break;
      case 'intro-ms':
        s.introMs = whole(flag, v, 'a whole number of ms');
        break;
      case 'stagger':
        s.stagger = stagger(flag, v);
        break;
      case 'ease':
        s.ease = ease(flag, v);
        break;
      case 'groups':
        s.groups = whole(flag, v, 'a number 2-16');
        break;
      case 'trigger':
        s.trigger = trigger(flag, v);
        break;
      case 'from-token':
        s.fromTokens.push(token(flag, v));
        break;
      case 'from-fg':
      case 'from-bg':
        s.fromTokens.push([flag.slice(5), v]);
        break;
      case 'idle':
        s.idle.push(oneOf(flag, v, ['finder-pulse', 'shimmer', 'breathe', 'logo-glint', 'logo-turn', 'logo-spin', 'logo-flip']));
        break;
      case 'theme-transition':
        s.themeTransition = whole(flag, v, 'a whole number of ms');
        break;
      case 'palette-from-image':
        s.paletteSource = { image: picture(flag, v, io) };
        break;
      case 'palette-from-css':
        s.paletteSource = {
          css: v === 'sample' ? SAMPLE_CSS : v === 'sample-dark' ? SAMPLE_DARK_CSS : readText(flag, v, io),
        };
        break;
      case 'palette-from-html':
        s.paletteSource = { html: readText(flag, v, io) };
        break;
      case 'palette-clusters':
        s.paletteClusters = whole(flag, v, 'a number 2-12');
        break;
      case 'palette-contrast':
        s.paletteContrast = number(flag, v);
        break;
      case 'halftone':
      case 'art':
        s.picture = picture(flag, v, io);
        s.art = flag === 'art';
        break;
      case 'ink':
        s.ink = ink(flag, v);
        break;
      case 'bands':
        s.bands = pair(flag, v);
        break;
      case 'clarity':
        s.clarity = number(flag, v);
        break;
      case 'clarity-map':
        s.clarity = picture(flag, v, io);
        break;
      case 'fit':
        s.fit = oneOf(flag, v, ['cover', 'contain']);
        break;
      case 'focal':
        s.focal = pair(flag, v);
        break;
      case 'priority':
        s.priority = picture(flag, v, io);
        break;
      case 'tracking-grid':
        s.trackingGrid = oneOf(flag, v, ['sparse', 'standard', 'dense']);
        break;
      case 'title':
        s.title = v;
        break;
      case 'label':
        s.label = v;
        break;
      default:
        throw new Usage(`unknown option --${flag}`);
    }
  }
  const has = (sw) => s.switches.has(sw);
  if (has('help')) return { help: true };
  if (text === undefined) throw new Usage('missing TEXT; see --help');
  const { style, palette } = build(s, text, io);
  return {
    text,
    style,
    palette,
    output: s.output,
    mode: has('check') ? 'check' : has('plan') ? 'plan' : s.frameAt !== undefined ? 'frame' : 'render',
    frameAt: s.frameAt,
    showPalette: has('show-palette'),
  };
}

/** The Style document the flags describe, as the Rust CLI builds its Style. */
function build(s, text, io) {
  const has = (sw) => s.switches.has(sw);
  const doc = {};
  if (s.preset !== undefined) doc.preset = s.preset;

  // Pictures.
  const pictureOptions =
    s.ink !== undefined ||
    s.bands !== undefined ||
    s.clarity !== undefined ||
    s.fit !== undefined ||
    s.focal !== undefined ||
    s.priority !== undefined ||
    has('no-steer');
  if (s.picture === undefined && pictureOptions) throw new Usage('picture options need --halftone or --art');
  const logoOptions =
    s.logoSize !== undefined ||
    s.logoFrame !== undefined ||
    s.logoFrameWidth !== undefined ||
    s.logoMoon !== undefined ||
    s.logoIntro !== undefined ||
    s.logoTiming !== undefined ||
    s.logoFrameIntro !== undefined ||
    s.logoFrameTiming !== undefined;
  if (s.logo === undefined && logoOptions) throw new Usage('logo options need --logo');
  if (s.picture !== undefined && s.art && s.preset !== undefined) {
    throw new Usage('--art sets the whole style; use --halftone with --preset');
  }

  if (s.shape !== undefined) doc.modules = s.shape;
  if (s.finder !== undefined || s.corners.length > 0) {
    doc.finders = { ...s.finder };
    if (s.corners.length > 0) {
      doc.finders.corners = {};
      for (const [corner, shapes] of s.corners) doc.finders.corners[corner] = shapes;
    }
  }
  if (s.alignment !== undefined || s.emphasis !== undefined) {
    doc.alignment = {};
    if (s.alignment !== undefined) doc.alignment.shape = s.alignment;
    if (s.emphasis !== undefined) doc.alignment.emphasis = s.emphasis;
  }
  if (s.timing !== undefined) doc.timing = s.timing;

  // Colors: what --palette-from-* found, then the tokens given.
  let palette = null;
  if (s.paletteSource !== undefined) {
    const opts = { allow_inverted: has('allow-inverted') };
    if (s.logo !== undefined) opts.logo_colors = logoColors(s.logo, logo(s.logo, io));
    try {
      if (s.paletteSource.image !== undefined) {
        if (s.paletteClusters !== undefined) opts.clusters = s.paletteClusters;
        if (s.paletteContrast !== undefined) opts.min_contrast = s.paletteContrast;
        palette = fq.paletteFromImage(s.paletteSource.image, opts);
      } else if (s.paletteSource.css !== undefined) {
        palette = fq.paletteFromCss(s.paletteSource.css, opts);
      } else {
        palette = fq.paletteFromHtml(s.paletteSource.html, opts);
      }
    } catch (e) {
      if (e instanceof fq.FreeQrentError) throw new Failed(`--palette-from-*: ${e.message}`);
      throw e;
    }
  }
  if (palette !== null || s.tokens.length > 0) {
    doc.palette = { ...(palette === null ? {} : palette.palette) };
    for (const [name, paint] of s.tokens) doc.palette[name] = paint;
  }
  if (has('dark')) doc.dark = true;
  if (s.quietZone !== undefined) doc.quiet_zone = s.quietZone;
  if (has('allow-inverted')) doc.allow_inverted = true;
  if (has('pretty')) doc.minify = false;

  if (s.frame !== undefined) doc.frame = frame('frame', s.frame, s.frameRing);
  if (s.border !== undefined) doc.border = s.border;
  if (s.shadow !== undefined) doc.shadow = s.shadow;
  if (s.spread !== undefined) doc.color_spread = s.spread;
  if (s.effect !== undefined) doc.effect = s.effect;
  if (s.textures.length > 0) doc.textures = s.textures;
  const captionOptions =
    s.captionSize !== undefined || s.captionFont !== undefined || s.captionTracking !== undefined || has('caption-regular');
  if (s.caption !== undefined) {
    if (captionOptions) {
      doc.caption = { text: s.caption };
      if (s.captionSize !== undefined) doc.caption.size = s.captionSize;
      if (s.captionFont !== undefined) doc.caption.font = s.captionFont;
      if (s.captionTracking !== undefined) doc.caption.tracking = s.captionTracking;
      if (has('caption-regular')) doc.caption.bold = false;
    } else {
      doc.caption = s.caption;
    }
  } else if (captionOptions) {
    throw new Usage('caption options need --caption');
  }
  if (s.seed !== undefined) doc.seed = s.seed;

  if (s.picture !== undefined) {
    const h = { image: s.picture };
    if (s.ink !== undefined) h.ink = s.ink;
    if (s.bands !== undefined) h.bands = s.bands;
    if (s.clarity !== undefined) h.clarity = s.clarity;
    if (s.fit !== undefined) h.fit = s.fit;
    if (s.focal !== undefined) h.focal = s.focal;
    if (s.priority !== undefined) h.priority = s.priority;
    if (has('no-steer')) h.steer = false;
    doc[s.art ? 'art' : 'halftone'] = h;
  }

  if (s.logo !== undefined) {
    const l = logo(s.logo, io);
    if (s.logoSize !== undefined) l.size = s.logoSize;
    if (s.logoScale !== undefined) l.scale = s.logoScale;
    if (s.logoFrame !== undefined) {
      const [name, w] = s.logoFrame;
      const width = w !== undefined ? w : s.logoFrameWidth;
      if (width === undefined && s.logoMoon === undefined) {
        l.frame = name;
      } else {
        l.frame = { kind: name };
        if (width !== undefined) l.frame.width = width;
        if (s.logoMoon !== undefined) l.frame.moon = s.logoMoon;
      }
    } else if (
      s.logoFrameWidth !== undefined ||
      s.logoMoon !== undefined ||
      s.logoFrameIntro !== undefined ||
      s.logoFrameTiming !== undefined
    ) {
      throw new Usage('--logo-frame-width, --logo-moon and --logo-frame-intro/timing need --logo-frame');
    }
    if (s.logoShape !== undefined) l.shape = s.logoShape;
    if (s.logoPadding !== undefined) l.padding = s.logoPadding;
    if (has('no-logo-backdrop')) l.backdrop = false;
    if (has('keep-modules')) l.clear_modules = false;
    doc.logo = l;
  }

  // Motion: a palette shift, or an intro (Settle when only its options
  // are given).
  const animated =
    s.introMs !== undefined ||
    s.stagger !== undefined ||
    s.ease !== undefined ||
    s.groups !== undefined ||
    s.trigger !== undefined ||
    s.logoIntro !== undefined ||
    s.logoTiming !== undefined ||
    s.logoFrameIntro !== undefined ||
    s.logoFrameTiming !== undefined;
  if (s.fromTokens.length > 0) {
    if (s.intro !== undefined) throw new Usage("--intro and --from-* can't be combined");
    const intro = { palette_from: Object.fromEntries(s.fromTokens) };
    if (s.introMs !== undefined) intro.duration_ms = s.introMs;
    if (s.ease !== undefined) intro.ease = s.ease;
    if (s.trigger !== undefined) intro.trigger = s.trigger;
    doc.intro = intro;
  } else if (s.intro !== undefined && !animated) {
    doc.intro = s.intro;
  } else if (animated) {
    const intro = {};
    if (s.intro !== undefined) intro.preset = s.intro;
    if (s.introMs !== undefined) intro.duration_ms = s.introMs;
    if (s.stagger !== undefined) intro.stagger = s.stagger;
    if (s.ease !== undefined) intro.ease = s.ease;
    if (s.groups !== undefined) intro.groups = s.groups;
    if (s.trigger !== undefined) intro.trigger = s.trigger;
    if (s.logoIntro !== undefined) intro.logo = s.logoIntro;
    if (s.logoTiming !== undefined) intro.logo_timing = s.logoTiming;
    if (s.logoFrameIntro !== undefined) intro.frame = s.logoFrameIntro;
    if (s.logoFrameTiming !== undefined) intro.frame_timing = s.logoFrameTiming;
    doc.intro = intro;
  }
  if (s.idle.length > 0) doc.idle = s.idle;
  if (s.themeTransition !== undefined) doc.theme_transition_ms = s.themeTransition;
  if (s.title !== undefined) doc.title = s.title;
  // The CLI labels a URL payload with itself; the document doesn't.
  if (s.label !== undefined) doc.label = s.label;
  else if (text.startsWith('https://') || text.startsWith('http://')) doc.label = text;
  if (s.target !== undefined) doc.target_size = s.target;

  if (s.ecc !== undefined || s.version !== undefined || s.trackingGrid !== undefined) {
    doc.encoding = {};
    if (s.ecc !== undefined) doc.encoding.ecc = s.ecc;
    if (s.version !== undefined) doc.encoding.version = s.version;
    if (s.trackingGrid !== undefined) doc.encoding.tracking_grid = s.trackingGrid;
  }
  return { style: doc, palette };
}

// A color's OKLab lightness, for --show-palette's dark check: `#rrggbb`
// as the library prints colors.
function lightness(css) {
  const m = /^#([0-9a-f]{6})/i.exec(css);
  if (!m) return 1;
  const lin = (i) => {
    const c = parseInt(m[1].slice(i, i + 2), 16) / 255;
    return c <= 0.04045 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
  };
  const [r, g, b] = [lin(0), lin(2), lin(4)];
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const mm = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  return 0.2104542553 * l + 0.793617785 * mm - 0.0040720468 * s;
}

/** What --palette-from-* found, laid out like the Rust CLI's --show-palette. */
function showPalette(result) {
  const lines = [`palette: ${result.swatches.length} colors found`];
  result.swatches.forEach((s, i) => {
    lines.push(`  ${String(i).padStart(2)}  ${s.color.padEnd(24)} ${(s.share * 100).toFixed(1).padStart(5)}%`);
  });
  const dark = result.choices.some((c) => c.token === 'bg' && lightness(c.color) < 0.5);
  const tint = dark ? 'near-black' : 'near-white';
  const flags = [];
  for (const c of result.choices) {
    const o = c.origin;
    const why =
      o.kind === 'swatch'
        ? `color ${o.swatch}`
        : o.kind === 'tinted'
          ? o.swatch === null
            ? 'neutral'
            : `${tint} tinted by color ${o.swatch}`
          : o.kind === 'darkened'
            ? `color ${o.swatch} darkened from contrast ${o.delta_l.toFixed(2)}`
            : o.kind === 'lightened'
              ? `color ${o.swatch} lightened from contrast ${o.delta_l.toFixed(2)}`
              : 'derived';
    lines.push(`  ${c.token.padEnd(11)} ${c.color.padEnd(24)} ${why}`);
    flags.push(c.token === 'bg' || c.token === 'fg' ? `--${c.token} '${c.color}'` : `--token '${c.token}=${c.color}'`);
  }
  if (dark) flags.push('--allow-inverted');
  lines.push(`  as flags: ${flags.join(' ')}`);
  return `${lines.join('\n')}\n`;
}

const nodeIo = {
  readFile: (path) => fs.readFileSync(path),
  readStdin: () => fs.readFileSync(0, 'utf8'),
  stdout: (text) => process.stdout.write(text),
  stderr: (text) => process.stderr.write(text),
  writeFile: (path, text) => fs.writeFileSync(path, text),
};

// Errors from the document (a bad flag value the library refused) are bad
// arguments; the rest mean the code was refused.
const USAGE_CODES = new Set(['invalid_style', 'invalid_logo', 'invalid_image', 'invalid_argument']);

/** Runs the command; returns the exit code (0 ok, 1 refused, 2 bad arguments). */
function main(argv, io = nodeIo) {
  try {
    const cmd = parse(argv, io);
    if (cmd.help) {
      io.stdout(HELP);
      return 0;
    }
    if (cmd.showPalette) {
      io.stderr(cmd.palette === null ? 'palette: no --palette-from-* given\n' : showPalette(cmd.palette));
    }
    let report;
    try {
      report = fq.check(cmd.text, cmd.style);
    } catch (e) {
      throw e instanceof fq.FreeQrentError && USAGE_CODES.has(e.code) ? new Usage(e.message) : new Failed(e.message);
    }
    if (cmd.mode === 'check') {
      io.stdout(`${JSON.stringify(report, null, 2)}\n`);
      return report.ok ? 0 : 1;
    }
    let out;
    try {
      if (cmd.mode === 'plan') out = fq.raw.plan(cmd.text, cmd.style);
      else if (cmd.mode === 'frame') out = fq.renderFrame(cmd.text, cmd.style, cmd.frameAt);
      else out = fq.render(cmd.text, cmd.style);
    } catch (e) {
      if (e instanceof fq.FreeQrentError) throw new Failed(e.message);
      throw e;
    }
    for (const w of report.warnings) io.stderr(`warning: ${w.message}\n`);
    if (cmd.output !== undefined) {
      try {
        io.writeFile(cmd.output, out);
      } catch (e) {
        throw new Failed(`${cmd.output}: ${e.message}`);
      }
    } else {
      io.stdout(out);
    }
    return 0;
  } catch (e) {
    if (e instanceof Usage) {
      io.stderr(`freeqrent: ${e.message}\n`);
      return 2;
    }
    if (e instanceof Failed) {
      io.stderr(`freeqrent: ${e.message}\n`);
      return 1;
    }
    throw e;
  }
}

module.exports = { main, parse, lex, HELP };
