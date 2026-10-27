// A small PNG writer for the tests, so they need no image files: every
// color type and bit depth, each filter type, and Adam7 interlacing.

import { deflateSync } from 'node:zlib';

const CRC = new Int32Array(256).map((_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c;
});

function crc32(bytes) {
  let c = -1;
  for (const b of bytes) c = CRC[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ -1) >>> 0;
}

function chunk(type, data) {
  const out = Buffer.alloc(12 + data.length);
  out.writeUInt32BE(data.length, 0);
  out.write(type, 4, 'latin1');
  Buffer.from(data).copy(out, 8);
  out.writeUInt32BE(crc32(out.subarray(4, 8 + data.length)), 8 + data.length);
  return out;
}

const CHANNELS = { 0: 1, 2: 3, 3: 1, 4: 2, 6: 4 };

function paeth(a, b, c) {
  const p = a + b - c;
  const pa = Math.abs(p - a);
  const pb = Math.abs(p - b);
  const pc = Math.abs(p - c);
  return pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
}

// Filtered scanlines for `samples` (one array of sample values per row).
function scanlines(rows, width, channels, depth, filterFor) {
  const bpp = Math.max(1, (channels * depth) >> 3);
  const stride = Math.ceil((width * channels * depth) / 8);
  const out = [];
  let prev = new Uint8Array(stride);
  rows.forEach((samples, y) => {
    const line = new Uint8Array(stride);
    if (depth === 16) {
      samples.forEach((v, i) => {
        line[2 * i] = v >> 8;
        line[2 * i + 1] = v & 0xff;
      });
    } else if (depth === 8) {
      line.set(samples);
    } else {
      samples.forEach((v, i) => {
        const bit = i * depth;
        line[bit >> 3] |= v << (8 - depth - (bit & 7));
      });
    }
    const filter = filterFor(y);
    const f = new Uint8Array(stride);
    for (let i = 0; i < stride; i++) {
      const a = i >= bpp ? line[i - bpp] : 0;
      const b = prev[i];
      const c = i >= bpp ? prev[i - bpp] : 0;
      const pred = [0, a, b, (a + b) >> 1, paeth(a, b, c)][filter];
      f[i] = (line[i] - pred) & 0xff;
    }
    out.push(Uint8Array.of(filter), f);
    prev = line;
  });
  return Buffer.concat(out);
}

const ADAM7 = [
  [0, 0, 8, 8],
  [4, 0, 8, 8],
  [0, 4, 4, 8],
  [2, 0, 4, 4],
  [0, 2, 2, 4],
  [1, 0, 2, 2],
  [0, 1, 1, 2],
];

/**
 * A PNG of `pixels` (rows of per-pixel sample arrays, in the color type's
 * own samples: [g], [r, g, b], [index], [g, a] or [r, g, b, a]).
 */
export function encodePng(width, height, pixels, opts = {}) {
  const { colorType = 6, depth = 8, interlace = false, palette, trns } = opts;
  const channels = CHANNELS[colorType];
  const filterFor = opts.filter === undefined ? (y) => y % 5 : () => opts.filter;
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = depth;
  ihdr[9] = colorType;
  ihdr[12] = interlace ? 1 : 0;
  const rowsOf = (xs, ys) => ys.map((y) => xs.flatMap((x) => pixels[y][x]));
  let data;
  if (!interlace) {
    data = scanlines(rowsOf([...Array(width).keys()], [...Array(height).keys()]), width, channels, depth, filterFor);
  } else {
    const parts = [];
    for (const [x0, y0, dx, dy] of ADAM7) {
      const xs = [];
      for (let x = x0; x < width; x += dx) xs.push(x);
      const ys = [];
      for (let y = y0; y < height; y += dy) ys.push(y);
      if (xs.length === 0 || ys.length === 0) continue;
      parts.push(scanlines(rowsOf(xs, ys), xs.length, channels, depth, filterFor));
    }
    data = Buffer.concat(parts);
  }
  const chunks = [chunk('IHDR', ihdr)];
  if (palette) chunks.push(chunk('PLTE', Buffer.from(palette.flat())));
  if (trns) chunks.push(chunk('tRNS', Buffer.from(trns)));
  // Split the data over two IDAT chunks, as encoders may.
  const z = deflateSync(data);
  const half = z.length >> 1;
  chunks.push(chunk('IDAT', z.subarray(0, half)), chunk('IDAT', z.subarray(half)), chunk('IEND', Buffer.alloc(0)));
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), ...chunks]);
}

/** An 8-bit RGBA PNG from `{width, height, data}`. */
export function rgbaPng({ width, height, data }) {
  const rows = [];
  for (let y = 0; y < height; y++) {
    const row = [];
    for (let x = 0; x < width; x++) {
      const i = (y * width + x) * 4;
      row.push([data[i], data[i + 1], data[i + 2], data[i + 3]]);
    }
    rows.push(row);
  }
  return encodePng(width, height, rows);
}
