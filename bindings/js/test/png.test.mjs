// The bin's PNG decoder (bin/png.cjs) against pictures whose pixels are
// known: every color type and bit depth, palettes, tRNS, all five filters
// and Adam7 interlacing.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';

import { encodePng } from './fixtures/png.mjs';

const { decodePng, PngError } = createRequire(import.meta.url)('../bin/png.cjs');

const W = 13;
const H = 11;

// A sample value in 0..max for pixel (x, y), channel c.
const value = (x, y, c, max) => (x * 37 + y * 101 + c * 59 + ((x * y) % 7)) % (max + 1);

function expect(colorType, depth, opts = {}) {
  const max = (1 << depth) - 1;
  const to8 = (v) => (depth === 16 ? v >> 8 : depth < 8 ? (v * 255) / max : v);
  const rows = [];
  const rgba = new Uint8Array(W * H * 4);
  for (let y = 0; y < H; y++) {
    const row = [];
    for (let x = 0; x < W; x++) {
      const o = (y * W + x) * 4;
      let px;
      if (colorType === 0) {
        const g = value(x, y, 0, max);
        px = [g];
        rgba.set([to8(g), to8(g), to8(g), opts.trns !== undefined && g === opts.key ? 0 : 255], o);
      } else if (colorType === 2) {
        const [r, g, b] = [0, 1, 2].map((c) => value(x, y, c, max));
        px = [r, g, b];
        rgba.set([to8(r), to8(g), to8(b), 255], o);
      } else if (colorType === 3) {
        const i = value(x, y, 0, Math.min(max, opts.palette.length - 1));
        px = [i];
        const alpha = opts.trns !== undefined && i < opts.trns.length ? opts.trns[i] : 255;
        rgba.set([...opts.palette[i], alpha], o);
      } else if (colorType === 4) {
        const [g, a] = [0, 1].map((c) => value(x, y, c, max));
        px = [g, a];
        rgba.set([to8(g), to8(g), to8(g), to8(a)], o);
      } else {
        const s = [0, 1, 2, 3].map((c) => value(x, y, c, max));
        px = s;
        rgba.set(s.map(to8), o);
      }
      row.push(px);
    }
    rows.push(row);
  }
  return { rows, rgba };
}

const FORMATS = [
  [0, 1],
  [0, 2],
  [0, 4],
  [0, 8],
  [0, 16],
  [2, 8],
  [2, 16],
  [3, 1],
  [3, 2],
  [3, 4],
  [3, 8],
  [4, 8],
  [4, 16],
  [6, 8],
  [6, 16],
];

const PALETTE = Array.from({ length: 200 }, (_, i) => [(i * 7) % 256, (i * 13) % 256, (i * 29) % 256]);

for (const [colorType, depth] of FORMATS) {
  for (const interlace of [false, true]) {
    test(`color type ${colorType}, ${depth} bits${interlace ? ', interlaced' : ''}`, () => {
      const opts = colorType === 3 ? { palette: PALETTE.slice(0, Math.min(200, 1 << depth)) } : {};
      const { rows, rgba } = expect(colorType, depth, opts);
      const png = encodePng(W, H, rows, { colorType, depth, interlace, palette: opts.palette });
      const out = decodePng(png);
      assert.equal(out.width, W);
      assert.equal(out.height, H);
      assert.deepEqual(out.data, rgba);
    });
  }
}

test('tRNS: a gray key, and palette alphas', () => {
  const gray = expect(0, 8, { trns: true, key: 40 });
  const png = encodePng(W, H, gray.rows, { colorType: 0, depth: 8, trns: [0, 40] });
  assert.deepEqual(decodePng(png).data, gray.rgba);
  assert.ok(gray.rgba.some((v, i) => i % 4 === 3 && v === 0));

  const palette = PALETTE.slice(0, 16);
  const trns = [0, 128, 255, 7];
  const indexed = expect(3, 4, { palette, trns });
  const png2 = encodePng(W, H, indexed.rows, { colorType: 3, depth: 4, palette, trns });
  assert.deepEqual(decodePng(png2).data, indexed.rgba);
});

test('every filter type alone', () => {
  for (let filter = 0; filter < 5; filter++) {
    const { rows, rgba } = expect(6, 8);
    assert.deepEqual(decodePng(encodePng(W, H, rows, { filter })).data, rgba, `filter ${filter}`);
  }
});

test('broken files are refused', () => {
  const { rows } = expect(6, 8);
  const png = encodePng(W, H, rows);
  assert.throws(() => decodePng(Buffer.from('not a png')), PngError);
  assert.throws(() => decodePng(png.subarray(0, 60)), PngError);
  const huge = Buffer.from(png);
  huge.writeUInt32BE(20000, 16);
  assert.throws(() => decodePng(huge), /too large/);
});
