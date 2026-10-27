// The API beyond conformance: typed and plain styles agree with JSON text,
// errors carry their fields, bytes payloads, results parse, and workers.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { Worker } from 'node:worker_threads';

import * as fq from '../dist/node.mjs';
import { rgbaPng } from './fixtures/png.mjs';

const TEXT = 'https://example.com/js';
const MARK =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><circle cx="5" cy="5" r="4" fill="#c2410c"/></svg>';

// A 12×12 picture: navy on the left, warm white on the right.
function picture() {
  const data = new Uint8ClampedArray(12 * 12 * 4);
  for (let i = 0; i < 144; i++) {
    const left = i % 12 < 6;
    data.set(left ? [20, 30, 80, 255] : [250, 248, 244, 255], i * 4);
  }
  return { width: 12, height: 12, data };
}

/** @type {import('../dist/index.d.mts').Style} */
const typed = {
  preset: 'ink',
  modules: { kind: 'dots', size: 0.9 },
  finders: { ring: 'squircle', eye: 'circle', corners: { 'top-right': { ring: 'leaf' } } },
  alignment: { shape: { kind: 'own', ring: 'rounded', eye: 'circle' }, emphasis: 'accent' },
  palette: { fg: '#1d3b72', 'finder.eye': 'oklch(55% .2 30)' },
  frame: { kind: 'rounded', radius: 1.5 },
  border: { kind: 'double', width: 1 },
  shadow: 1,
  textures: [{ kind: 'grain', amount: 0.05 }],
  caption: { text: 'menu', font: 'mono', bold: false },
  logo: { svg: MARK, frame: { kind: 'ring', width: 0.6 } },
  intro: { preset: 'ripple', stagger: { kind: 'random', seed: 3 }, ease: 'expo', trigger: { kind: 'every', ms: 5000 } },
  idle: ['finder-pulse'],
  seed: 42,
  encoding: { ecc: 'Q' },
};

test('a typed style, its JSON text and the default style', () => {
  const svg = fq.render(TEXT, typed);
  assert.ok(svg.startsWith('<svg'));
  assert.equal(fq.render(TEXT, JSON.stringify(typed)), svg);
  assert.equal(fq.toStyleJson(typed), JSON.stringify(typed));
  assert.equal(fq.render(TEXT), fq.render(TEXT, {}));
  assert.equal(fq.render(TEXT, null), fq.render(TEXT, '{}'));
  // Keys left undefined are left out, like JSON.stringify.
  assert.equal(fq.render(TEXT, { ...typed, title: undefined }), svg);
});

test('bare names and tagged objects are the same variant', () => {
  assert.equal(fq.render(TEXT, { modules: 'dots' }), fq.render(TEXT, { modules: { kind: 'dots' } }));
  assert.equal(fq.render(TEXT, { frame: 'none' }), fq.render(TEXT, { frame: { kind: 'none' } }));
});

test('a bigint seed is written exactly', () => {
  const big = 18446744073709551615n;
  assert.equal(fq.toStyleJson({ seed: big }), '{"seed":18446744073709551615}');
  assert.equal(fq.render(TEXT, { seed: big }), fq.render(TEXT, '{"seed":18446744073709551615}'));
  assert.equal(fq.render(TEXT, { seed: 7n }), fq.render(TEXT, { seed: 7 }));
});

test('bytes in a style are sent as base64, and ImageData as an image', () => {
  const png = rgbaPng(picture());
  const asBytes = fq.render(TEXT, { logo: { raster: new Uint8Array(png) }, encoding: { ecc: 'H' } });
  const asText = fq.render(TEXT, { logo: { raster: png.toString('base64') }, encoding: { ecc: 'H' } });
  assert.equal(asBytes, asText);

  const p = picture();
  const rgba = Buffer.from(p.data.buffer).toString('base64');
  const doc = `{"halftone":{"image":{"width":12,"height":12,"rgba":"${rgba}"},"clarity":0.4}}`;
  assert.equal(fq.render(TEXT, { halftone: { image: p, clarity: 0.4 } }), fq.render(TEXT, doc));
  assert.equal(fq.render(TEXT, { halftone: { image: { width: 12, height: 12, rgba: p.data } } }), fq.render(TEXT, { halftone: { image: p } }));
});

test('bytes payloads', () => {
  const bytes = new Uint8Array([0, 1, 2, 0xfe, 0xff, 0x51, 0x52]);
  const svg = fq.render(bytes, {});
  assert.ok(svg.startsWith('<svg'));
  // Any view of the same bytes, and an ArrayBuffer, is the same payload.
  assert.equal(fq.render(Buffer.from(bytes), {}), svg);
  assert.equal(fq.render(new Uint8ClampedArray(bytes), {}), svg);
  assert.equal(fq.render(new DataView(bytes.buffer), {}), svg);
  assert.equal(fq.render(bytes.buffer, {}), svg);
  const sub = new Uint8Array([9, 9, ...bytes, 9]).subarray(2, 2 + bytes.length);
  assert.equal(fq.render(sub, {}), svg);
  // Byte mode isn't text mode: "HELLO" as text is alphanumeric.
  const hello = new TextEncoder().encode('HELLO');
  assert.notEqual(fq.check(hello, {}).code.version, undefined);
  assert.notEqual(fq.render(hello, {}), fq.render('HELLO', {}));
  assert.equal(fq.check(bytes).ok, true);
});

test('check, plan, renderFrame and the palette functions parse', () => {
  const report = fq.check(TEXT, { logo: { svg: MARK } });
  assert.equal(report.ok, true);
  assert.equal(typeof report.logo_scale, 'number');
  assert.deepEqual(Object.keys(report.code), ['version', 'ecc', 'size', 'mask']);
  const refused = fq.check(TEXT, { palette: { fg: '#eeeeee' } });
  assert.equal(refused.ok, false);
  assert.equal(refused.errors[0].code, 'contrast_too_low');
  assert.equal(refused.errors[0].detail.token, 'fg');

  const plan = fq.plan(TEXT, { intro: 'ripple' });
  assert.deepEqual(plan.tracks[0].ease, [0.22, 1, 0.36, 1]);
  assert.equal(plan.totalMs, 1200);
  assert.equal(plan.reducedMotion, 'final-state');
  // Every plan the suite has parses, numbers below one included.
  for (const intro of ['settle', 'ripple', 'lock-on', 'sweep', 'raster', 'iris', 'assemble']) {
    const p = fq.plan(TEXT, { intro, logo: { svg: MARK, frame: 'orbit' }, frame: 'circle', idle: ['logo-turn'] });
    assert.ok(p.layers.length > 3, intro);
  }

  const start = fq.renderFrame(TEXT, { intro: 'ripple' }, 0);
  const end = fq.renderFrame(TEXT, { intro: 'ripple' }, 10_000);
  assert.notEqual(start, end);
  assert.equal(fq.renderFrame(TEXT, {}, 0), fq.render(TEXT, {}));

  const css = ':root { --brand: #1d3b72; --accent: #c2410c; --paper: #f8f5ee; }';
  const fromCss = fq.paletteFromCss(css);
  assert.equal(typeof fromCss.palette.bg, 'string');
  assert.ok(fromCss.choices.every((c) => typeof c.origin.kind === 'string'));
  assert.ok(fq.render(TEXT, { palette: fromCss.palette }).startsWith('<svg'));
  assert.deepEqual(fq.paletteFromCss(css, '{}'), fromCss);

  const html = '<meta name="theme-color" content="#0f3d3e"><body style="background:#fdf6e3;color:#073642">';
  assert.ok(fq.paletteFromHtml(html).swatches.length > 0);

  const fromImage = fq.paletteFromImage(picture(), { clusters: 3, seed: 1n });
  assert.ok(fromImage.swatches.length >= 2);
  const p = picture();
  assert.deepEqual(fq.paletteFromImage({ width: 12, height: 12, rgba: new Uint8Array(p.data.buffer) }, { clusters: 3, seed: 1 }), fromImage);
});

test('presets and version', () => {
  assert.deepEqual(fq.presets(), ['classic', 'ink', 'candy', 'terminal', 'sunset', 'bubble', 'ribbon', 'leaf']);
  const pkg = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8'));
  assert.equal(fq.version(), pkg.version);
  for (const preset of fq.presets()) assert.ok(fq.render(TEXT, { preset }).startsWith('<svg'));
});

function fails(f, code, check) {
  assert.throws(f, (e) => {
    assert.ok(e instanceof fq.FreeQrentError, `${e}`);
    assert.ok(e instanceof Error);
    assert.equal(e.name, 'FreeQrentError');
    assert.equal(e.code, code, e.message);
    assert.equal(typeof e.message, 'string');
    assert.ok(e.message.length > 0);
    assert.equal(typeof e.detail, 'object');
    if (check) check(e.detail, e);
    return true;
  });
}

test('errors carry code, message and parsed detail', () => {
  fails(() => fq.render(TEXT, { intro: { trigger: { kind: 'every', ms: 'soon' } } }), 'invalid_style', (d) => {
    assert.equal(d.path, 'intro.trigger.ms');
    assert.equal(typeof d.reason, 'string');
  });
  fails(() => fq.render(TEXT, '{"modules": '), 'invalid_style', (d) => {
    assert.equal(typeof d.offset, 'number');
  });
  fails(() => fq.render(TEXT, { colour: 'red' }), 'invalid_style', (d) => assert.equal(d.path, 'colour'));
  fails(() => fq.render(TEXT, { shadow: NaN }), 'invalid_style', (d) => assert.equal(d.path, 'shadow'));
  fails(() => fq.render(TEXT, { preset: 'ink', art: { image: picture() } }), 'invalid_style');
  fails(() => fq.render(TEXT, { logo: { svg: 'not svg' } }), 'invalid_logo', (d) => {
    assert.equal(d.path, 'logo.svg');
    assert.equal(d.reason, 'not_svg');
  });
  fails(() => fq.render(TEXT, { halftone: { image: { width: 4, height: 4, rgba: new Uint8Array(3) } } }), 'invalid_image', (d) => {
    assert.equal(d.expected, 64);
    assert.equal(d.actual, 3);
  });
  fails(() => fq.render('x'.repeat(3000), { encoding: { max_version: 10 } }), 'data_too_long', (d) => {
    assert.equal(d.max_version, 10);
    assert.ok(d.bits_needed > d.bits_available);
    assert.equal(typeof d.ecc, 'string');
  });
  fails(() => fq.render(TEXT, { encoding: { min_version: 20, max_version: 10 } }), 'invalid_version_range');
  fails(() => fq.render(TEXT, { palette: { fg: '#eeeeee' } }), 'contrast_too_low', (d) => {
    assert.equal(d.token, 'fg');
    assert.equal(typeof d.delta_l, 'number');
  });
  fails(() => fq.render(TEXT, { palette: { fg: '#ffffff', bg: '#000000' } }), 'inverted_colors');
  fails(() => fq.render(TEXT, { quiet_zone: 0 }), 'quiet_zone_too_small');
  fails(() => fq.render(TEXT, { modules: { kind: 'dots', size: 5 } }), 'option_out_of_range', (d) => {
    assert.equal(typeof d.option, 'string');
    assert.equal(d.value, 5);
    assert.equal(typeof d.min, 'number');
    assert.equal(typeof d.max, 'number');
  });
  fails(() => fq.render(TEXT, { logo: { svg: MARK, scale: 0.4 }, encoding: { version: 4, ecc: 'L', boost_ecc: false } }), 'logo_too_large', (d) => {
    assert.equal(typeof d.max_scale, 'number');
  });
  fails(() => fq.render(TEXT, { logo: { svg: MARK }, idle: ['logo-turn'] }), 'turn_needs_round_frame');
  fails(() => fq.render(TEXT, { intro: { preset: 'ripple', duration_ms: 1200, trigger: { kind: 'every', ms: 1500 } } }), 'replay_too_soon');
  fails(() => fq.render(TEXT, { shadow: 1, palette: { bg: 'transparent' } }), 'shadow_needs_background');
  fails(() => fq.paletteFromCss('body { margin: 0 }'), 'no_colors');
  fails(() => fq.paletteFromCss(':root{--a:#123}', '{"clusters": "many"}'), 'invalid_style', (d) => {
    assert.equal(d.path, 'clusters');
  });
});

test('bad arguments are invalid_argument errors', () => {
  fails(() => fq.render(42, {}), 'invalid_argument', (d) => assert.equal(typeof d.reason, 'string'));
  fails(() => fq.render('\ud800 lone', {}), 'invalid_argument');
  fails(() => fq.render(TEXT, 5), 'invalid_argument');
  fails(() => fq.render(TEXT, [1, 2]), 'invalid_argument');
  fails(() => fq.renderFrame(TEXT, {}, -1), 'invalid_argument');
  fails(() => fq.renderFrame(TEXT, {}, 1.5), 'invalid_argument');
  fails(() => fq.paletteFromImage({ width: 2, height: 2 }), 'invalid_argument');
  fails(() => fq.paletteFromImage({ width: -1, height: 2, data: new Uint8Array(8) }), 'invalid_argument');
  fails(() => fq.paletteFromCss(null), 'invalid_argument');
  // Surrogate pairs are fine.
  assert.ok(fq.render('emoji 😀', {}).startsWith('<svg'));
});

test('init resolves, and the library keeps working after an error', async () => {
  await fq.init();
  await fq.init();
  assert.throws(() => fq.render(TEXT, { palette: { fg: '#eee' } }));
  assert.ok(fq.render(TEXT).startsWith('<svg'));
});

test('workers render the same bytes in parallel', async () => {
  const styles = [{}, { preset: 'candy', intro: 'ripple' }, typed, { preset: 'terminal', modules: 'liquid' }];
  const expected = styles.map((s) => fq.render(TEXT, s));
  const worker = (style) =>
    new Promise((resolve, reject) => {
      const w = new Worker(new URL('./fixtures/worker.mjs', import.meta.url), {
        workerData: { text: TEXT, style: JSON.stringify(style), rounds: 5 },
      });
      w.once('message', resolve);
      w.once('error', reject);
    });
  const results = await Promise.all([...styles, ...styles].map(worker));
  results.forEach((svgs, i) => {
    for (const svg of svgs) assert.equal(svg, expected[i % styles.length]);
  });
});
