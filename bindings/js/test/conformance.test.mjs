// The conformance suite (SPEC §10.3): every case in conformance/cases.json
// through the npm package. Styles and options are re-serialized by the
// package's own JSON writer (toStyleJson), and each result's UTF-8 bytes
// must have the case's SHA-256 and length, or the call must fail with its
// error code.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';

import * as fq from '../dist/node.mjs';

const suite = JSON.parse(
  readFileSync(new URL('../../../conformance/cases.json', import.meta.url), 'utf8'),
);

function payload(p) {
  return 'text' in p ? p.text : new Uint8Array(Buffer.from(p.bytes, 'base64'));
}

function image(i) {
  return { width: i.width, height: i.height, data: new Uint8Array(Buffer.from(i.rgba, 'base64')) };
}

// Each case through the public API: the text result, and the parsed one
// where the function parses.
function run(c) {
  switch (c.fn) {
    case 'render':
      return [fq.render(payload(c.payload), c.style)];
    case 'render_frame':
      return [fq.renderFrame(payload(c.payload), c.style, c.t_ms)];
    case 'check':
      return [fq.raw.check(payload(c.payload), c.style), fq.check(payload(c.payload), c.style)];
    case 'plan':
      return [fq.raw.plan(payload(c.payload), c.style), fq.plan(payload(c.payload), c.style)];
    case 'palette_from_css':
      return [fq.raw.paletteFromCss(c.text, c.opts), fq.paletteFromCss(c.text, c.opts)];
    case 'palette_from_html':
      return [fq.raw.paletteFromHtml(c.text, c.opts), fq.paletteFromHtml(c.text, c.opts)];
    case 'palette_from_image':
      return [fq.raw.paletteFromImage(image(c.image), c.opts), fq.paletteFromImage(image(c.image), c.opts)];
    case 'presets':
      return [fq.raw.presets(), fq.presets()];
    default:
      throw new Error(`unknown fn ${c.fn}`);
  }
}

const sha256 = (text) => createHash('sha256').update(text, 'utf8').digest('hex');

test('the suite has every case', () => {
  assert.equal(suite.version, fq.version());
  assert.equal(suite.cases.length, 77);
});

for (const c of suite.cases) {
  test(`conformance: ${c.name}`, () => {
    if ('error' in c.expect) {
      assert.throws(
        () => run(c),
        (e) => {
          assert.ok(e instanceof fq.FreeQrentError, `${e}`);
          assert.equal(e.code, c.expect.error, e.message);
          return true;
        },
      );
      return;
    }
    const [text, parsed] = run(c);
    assert.equal(sha256(text), c.expect.sha256);
    assert.equal(Buffer.byteLength(text, 'utf8'), c.expect.length);
    if (parsed !== undefined) assert.equal(typeof parsed, 'object');
    if (c.fn === 'palette_from_css' || c.fn === 'palette_from_html' || c.fn === 'palette_from_image') {
      // The result's palette drops straight into a style (a dark one with
      // inverted colors allowed).
      assert.ok(fq.render('palette', { palette: parsed.palette, allow_inverted: true }).startsWith('<svg'));
    }
  });
}

test('JSON text and objects give the same bytes for every rendered case', () => {
  for (const c of suite.cases) {
    if (c.fn !== 'render' || 'error' in c.expect) continue;
    const p = payload(c.payload);
    assert.equal(fq.render(p, JSON.stringify(c.style)), fq.render(p, c.style), c.name);
  }
});
