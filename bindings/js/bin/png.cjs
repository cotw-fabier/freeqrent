// A PNG decoder in plain JavaScript over node:zlib, so the npm bin reads
// pictures without a dependency. It decodes what freeqrent-image does:
// every color type and bit depth, palettes and tRNS transparency, and
// Adam7 interlacing, into 8-bit RGBA. Pixels are taken as sRGB (gamma and
// ICC chunks are ignored), 16-bit samples keep their high byte, and low
// bit depths of gray are scaled to 0-255. JPEG isn't decoded here.
'use strict';

const { inflateSync } = require('node:zlib');

const SIGNATURE = [137, 80, 78, 71, 13, 10, 26, 10];
const CHANNELS = { 0: 1, 2: 3, 3: 1, 4: 2, 6: 4 };
const DEPTHS = { 0: [1, 2, 4, 8, 16], 2: [8, 16], 3: [1, 2, 4, 8], 4: [8, 16], 6: [8, 16] };
const ADAM7 = [
  [0, 0, 8, 8],
  [4, 0, 8, 8],
  [0, 4, 4, 8],
  [2, 0, 4, 4],
  [0, 2, 2, 4],
  [1, 0, 2, 2],
  [0, 1, 1, 2],
];
// freeqrent-image's limits.
const MAX_SIDE = 16384;
const MAX_PIXELS = 40_000_000;

class PngError extends Error {}

function isPng(bytes) {
  return bytes.length >= 8 && SIGNATURE.every((b, i) => bytes[i] === b);
}

function isJpeg(bytes) {
  return bytes.length >= 3 && bytes[0] === 0xff && bytes[1] === 0xd8 && bytes[2] === 0xff;
}

function paeth(a, b, c) {
  const p = a + b - c;
  const pa = Math.abs(p - a);
  const pb = Math.abs(p - b);
  const pc = Math.abs(p - c);
  return pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
}

// Unfilters one pass of `height` rows, `stride` bytes each, from `data` at
// `at`; returns the rows and where the pass ended.
function unfilter(data, at, height, stride, bpp) {
  const rows = [];
  let prev = new Uint8Array(stride);
  for (let y = 0; y < height; y++) {
    if (at + 1 + stride > data.length) throw new PngError('the image data ends early');
    const filter = data[at];
    const line = data.slice(at + 1, at + 1 + stride);
    at += 1 + stride;
    for (let i = 0; i < stride; i++) {
      const a = i >= bpp ? line[i - bpp] : 0;
      const b = prev[i];
      const c = i >= bpp ? prev[i - bpp] : 0;
      switch (filter) {
        case 0:
          break;
        case 1:
          line[i] = (line[i] + a) & 0xff;
          break;
        case 2:
          line[i] = (line[i] + b) & 0xff;
          break;
        case 3:
          line[i] = (line[i] + ((a + b) >> 1)) & 0xff;
          break;
        case 4:
          line[i] = (line[i] + paeth(a, b, c)) & 0xff;
          break;
        default:
          throw new PngError(`unknown filter ${filter}`);
      }
    }
    rows.push(line);
    prev = line;
  }
  return [rows, at];
}

// The sample at index `i` of a row, at its own depth.
function sample(line, i, depth) {
  switch (depth) {
    case 8:
      return line[i];
    case 16:
      return (line[2 * i] << 8) | line[2 * i + 1];
    default: {
      const bit = i * depth;
      return (line[bit >> 3] >> (8 - depth - (bit & 7))) & ((1 << depth) - 1);
    }
  }
}

/** Decodes a PNG into `{width, height, data}` with 8-bit RGBA rows. */
function decodePng(bytes) {
  if (!isPng(bytes)) throw new PngError('not a PNG file');
  let at = 8;
  let header = null;
  let palette = null;
  let trns = null;
  const idat = [];
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  while (at + 8 <= bytes.length) {
    const length = view.getUint32(at);
    const type = String.fromCharCode(...bytes.subarray(at + 4, at + 8));
    const body = bytes.subarray(at + 8, at + 8 + length);
    if (body.length !== length) throw new PngError(`the ${type} chunk ends early`);
    at += 12 + length;
    if (type === 'IHDR') {
      if (length !== 13) throw new PngError('a malformed IHDR');
      const v = new DataView(body.buffer, body.byteOffset, 13);
      header = {
        width: v.getUint32(0),
        height: v.getUint32(4),
        depth: body[8],
        colorType: body[9],
        interlace: body[12],
      };
      if (body[10] !== 0 || body[11] !== 0 || body[12] > 1) throw new PngError('an unknown compression, filter or interlace method');
    } else if (type === 'PLTE') {
      palette = body;
    } else if (type === 'tRNS') {
      trns = body;
    } else if (type === 'IDAT') {
      idat.push(body);
    } else if (type === 'IEND') {
      break;
    }
  }
  if (header === null) throw new PngError('no IHDR chunk');
  const { width, height, depth, colorType, interlace } = header;
  if (!(colorType in CHANNELS) || !DEPTHS[colorType].includes(depth)) {
    throw new PngError(`color type ${colorType} at ${depth} bits isn't a PNG format`);
  }
  if (width === 0 || height === 0) throw new PngError('an empty image');
  if (width > MAX_SIDE || height > MAX_SIDE || width * height > MAX_PIXELS) {
    throw new PngError(`${width}x${height} is too large`);
  }
  if (colorType === 3 && palette === null) throw new PngError('a palette image without a palette');
  if (idat.length === 0) throw new PngError('no image data');
  let data;
  try {
    data = inflateSync(Buffer.concat(idat));
  } catch (e) {
    throw new PngError(`the image data doesn't inflate: ${e.message}`);
  }

  const channels = CHANNELS[colorType];
  const bpp = Math.max(1, (channels * depth) >> 3);
  const out = new Uint8Array(width * height * 4);
  const scale = depth < 8 ? 255 / ((1 << depth) - 1) : 1;
  const eight = (v) => (depth === 16 ? v >> 8 : depth < 8 ? v * scale : v);
  const key = trns !== null && (colorType === 0 || colorType === 2)
    ? colorType === 0
      ? [(trns[0] << 8) | trns[1]]
      : [(trns[0] << 8) | trns[1], (trns[2] << 8) | trns[3], (trns[4] << 8) | trns[5]]
    : null;

  const put = (line, i, x, y) => {
    const o = (y * width + x) * 4;
    const s = (c) => sample(line, i * channels + c, depth);
    switch (colorType) {
      case 0: {
        const g = s(0);
        const v = eight(g);
        out[o] = out[o + 1] = out[o + 2] = v;
        out[o + 3] = key !== null && g === key[0] ? 0 : 255;
        break;
      }
      case 2: {
        const r = s(0);
        const g = s(1);
        const b = s(2);
        out[o] = eight(r);
        out[o + 1] = eight(g);
        out[o + 2] = eight(b);
        out[o + 3] = key !== null && r === key[0] && g === key[1] && b === key[2] ? 0 : 255;
        break;
      }
      case 3: {
        const index = s(0);
        if (3 * index + 2 >= palette.length) throw new PngError(`palette index ${index} is out of range`);
        out[o] = palette[3 * index];
        out[o + 1] = palette[3 * index + 1];
        out[o + 2] = palette[3 * index + 2];
        out[o + 3] = trns !== null && index < trns.length ? trns[index] : 255;
        break;
      }
      case 4: {
        const v = eight(s(0));
        out[o] = out[o + 1] = out[o + 2] = v;
        out[o + 3] = eight(s(1));
        break;
      }
      default:
        out[o] = eight(s(0));
        out[o + 1] = eight(s(1));
        out[o + 2] = eight(s(2));
        out[o + 3] = eight(s(3));
    }
  };

  let pos = 0;
  const passes = interlace ? ADAM7 : [[0, 0, 1, 1]];
  for (const [x0, y0, dx, dy] of passes) {
    const w = Math.ceil((width - x0) / dx);
    const h = Math.ceil((height - y0) / dy);
    if (w <= 0 || h <= 0) continue;
    const stride = Math.ceil((w * channels * depth) / 8);
    const [rows, next] = unfilter(data, pos, h, stride, bpp);
    pos = next;
    rows.forEach((line, j) => {
      for (let i = 0; i < w; i++) put(line, i, x0 + i * dx, y0 + j * dy);
    });
  }
  return { width, height, data: out };
}

module.exports = { decodePng, isPng, isJpeg, PngError };
