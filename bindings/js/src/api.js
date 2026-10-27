// freeQRent's JavaScript API over the WebAssembly boundary (SPEC §10.1).
//
// This file is a fragment, not a module: scripts/build.mjs puts it in one
// scope with wasm-bindgen's glue (whose functions are `wbg.render` and so
// on) and a loader that defines `ready()`, then adds the ESM or CJS
// exports. Keep it free of `import`, `export` and `require`.

/** An error from the library: `code` is stable, `detail` holds its fields. */
class FreeQrentError extends Error {
  constructor(code, message, detail) {
    super(message);
    this.name = 'FreeQrentError';
    this.code = code;
    this.detail = detail === undefined ? {} : detail;
  }
}

const utf8 = new TextEncoder();

function argumentError(reason) {
  return new FreeQrentError('invalid_argument', reason, { reason });
}

// The error JSON the Rust side throws as a string.
function fromWasm(thrown) {
  if (typeof thrown === 'string') {
    let e;
    try {
      e = JSON.parse(thrown);
    } catch {
      return new FreeQrentError('internal', thrown, {});
    }
    return new FreeQrentError(e.code, e.message, e.detail);
  }
  if (thrown instanceof WebAssembly.RuntimeError) {
    // A trap (a Rust panic, with panic=abort) leaves the instance in an
    // unknown state; start a fresh one for the next call.
    wbg.reset();
    return new FreeQrentError('internal', `freeqrent panicked: ${thrown.message}`, {});
  }
  return thrown;
}

function call(f, args) {
  ready();
  try {
    return f(...args);
  } catch (thrown) {
    throw fromWasm(thrown);
  }
}

const B64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';

/** Standard base64 with padding. */
function base64(bytes) {
  let out = '';
  let i = 0;
  const parts = [];
  for (; i + 2 < bytes.length; i += 3) {
    const n = (bytes[i] << 16) | (bytes[i + 1] << 8) | bytes[i + 2];
    out += B64[n >> 18] + B64[(n >> 12) & 63] + B64[(n >> 6) & 63] + B64[n & 63];
    if (out.length > 65536) {
      parts.push(out);
      out = '';
    }
  }
  const rest = bytes.length - i;
  if (rest === 1) {
    const n = bytes[i] << 16;
    out += B64[n >> 18] + B64[(n >> 12) & 63] + '==';
  } else if (rest === 2) {
    const n = (bytes[i] << 16) | (bytes[i + 1] << 8);
    out += B64[n >> 18] + B64[(n >> 12) & 63] + B64[(n >> 6) & 63] + '=';
  }
  parts.push(out);
  return parts.join('');
}

function isBytes(v) {
  return v instanceof Uint8Array || v instanceof Uint8ClampedArray;
}

function asBytes(v) {
  if (v instanceof Uint8Array) return v;
  if (v instanceof Uint8ClampedArray || ArrayBuffer.isView(v)) {
    return new Uint8Array(v.buffer, v.byteOffset, v.byteLength);
  }
  if (v instanceof ArrayBuffer) return new Uint8Array(v);
  return null;
}

// `{width, height, data}` (ImageData, or anything shaped like it) as the
// document's `{width, height, rgba}`.
function isImageLike(v) {
  return (
    v !== null &&
    typeof v === 'object' &&
    !('rgba' in v) &&
    typeof v.width === 'number' &&
    typeof v.height === 'number' &&
    isBytes(v.data)
  );
}

// Whether `s` is Unicode: no lone surrogates, which UTF-8 can't carry.
function wellFormed(s) {
  if (typeof s.isWellFormed === 'function') return s.isWellFormed();
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    if (c >= 0xd800 && c <= 0xdbff) {
      const d = s.charCodeAt(i + 1);
      if (!(d >= 0xdc00 && d <= 0xdfff)) return false;
      i++;
    } else if (c >= 0xdc00 && c <= 0xdfff) {
      return false;
    }
  }
  return true;
}

function writeJson(v, path, out) {
  switch (typeof v) {
    case 'string':
      out.push(JSON.stringify(v));
      return;
    case 'number':
      if (!Number.isFinite(v)) {
        throw new FreeQrentError('invalid_style', `${path || 'style'}: ${v} isn't a finite number`, {
          path,
          reason: `${v} isn't a finite number`,
        });
      }
      out.push(JSON.stringify(v));
      return;
    case 'bigint':
      out.push(v.toString());
      return;
    case 'boolean':
      out.push(v ? 'true' : 'false');
      return;
    case 'object':
      break;
    default:
      throw new FreeQrentError('invalid_style', `${path || 'style'}: a ${typeof v} isn't JSON`, {
        path,
        reason: `a ${typeof v} isn't JSON`,
      });
  }
  if (v === null) {
    out.push('null');
    return;
  }
  if (isBytes(v)) {
    out.push(`"${base64(v)}"`);
    return;
  }
  if (Array.isArray(v)) {
    out.push('[');
    v.forEach((item, i) => {
      if (i > 0) out.push(',');
      writeJson(item === undefined ? null : item, `${path}[${i}]`, out);
    });
    out.push(']');
    return;
  }
  if (isImageLike(v)) {
    v = { width: v.width, height: v.height, rgba: v.data };
  }
  out.push('{');
  let first = true;
  for (const key of Object.keys(v)) {
    const item = v[key];
    if (item === undefined || typeof item === 'function') continue;
    if (!first) out.push(',');
    first = false;
    out.push(JSON.stringify(key), ':');
    writeJson(item, path ? `${path}.${key}` : key, out);
  }
  out.push('}');
}

/**
 * A style (a typed or plain object, JSON text, or nothing) as the Style
 * document's JSON text. Byte arrays become base64, `{width, height, data}`
 * becomes an image, and a `bigint` seed is written exactly.
 */
function toStyleJson(style) {
  if (style === undefined || style === null) return '{}';
  if (typeof style === 'string') return style;
  if (typeof style !== 'object' || Array.isArray(style)) {
    throw argumentError('the style is an object, JSON text or nothing');
  }
  const out = [];
  writeJson(style, '', out);
  return out.join('');
}

function optsJson(opts) {
  if (opts === undefined || opts === null) return '';
  if (typeof opts === 'string') return opts;
  if (typeof opts !== 'object' || Array.isArray(opts)) {
    throw argumentError('palette options are an object, JSON text or nothing');
  }
  const out = [];
  writeJson(opts, '', out);
  return out.join('');
}

// [bytes, isText] for wasm.
function payloadArgs(payload) {
  if (typeof payload === 'string') {
    if (!wellFormed(payload)) {
      throw argumentError('the payload text has a lone surrogate, so it isn\'t Unicode');
    }
    return [utf8.encode(payload), true];
  }
  const bytes = asBytes(payload);
  if (bytes === null) {
    throw argumentError('the payload is a string or a Uint8Array');
  }
  return [bytes, false];
}

function tMsArg(tMs) {
  if (!Number.isInteger(tMs) || tMs < 0 || tMs > 0xffffffff) {
    throw argumentError('tMs is a whole number of milliseconds, 0 to 2^32 - 1');
  }
  return tMs;
}

function imageArgs(image) {
  if (image === null || typeof image !== 'object') {
    throw argumentError('the image is {width, height, data}');
  }
  const { width, height } = image;
  const pixels = asBytes(image.data !== undefined ? image.data : image.rgba);
  if (pixels === null) {
    throw argumentError("the image's data is a Uint8Array or Uint8ClampedArray of RGBA");
  }
  for (const [name, n] of [['width', width], ['height', height]]) {
    if (!Number.isInteger(n) || n < 0 || n > 0xffffffff) {
      throw argumentError(`the image's ${name} is a whole number of pixels`);
    }
  }
  return [pixels, width, height];
}

function textArg(text, what) {
  if (typeof text !== 'string') throw argumentError(`the ${what} is a string`);
  return text;
}

/**
 * The boundary's results as the library writes them, byte for byte: the
 * text to hash, cache or pass on. The functions below parse them.
 */
const raw = Object.freeze({
  render(payload, style) {
    return call(wbg.render, [...payloadArgs(payload), toStyleJson(style)]);
  },
  renderFrame(payload, style, tMs) {
    return call(wbg.render_frame, [...payloadArgs(payload), toStyleJson(style), tMsArg(tMs)]);
  },
  check(payload, style) {
    return call(wbg.check, [...payloadArgs(payload), toStyleJson(style)]);
  },
  plan(payload, style) {
    return call(wbg.plan, [...payloadArgs(payload), toStyleJson(style)]);
  },
  paletteFromImage(image, opts) {
    return call(wbg.palette_from_image, [...imageArgs(image), optsJson(opts)]);
  },
  paletteFromCss(css, opts) {
    return call(wbg.palette_from_css, [textArg(css, 'style sheet'), optsJson(opts)]);
  },
  paletteFromHtml(html, opts) {
    return call(wbg.palette_from_html, [textArg(html, 'page'), optsJson(opts)]);
  },
  presets() {
    return call(wbg.presets, []);
  },
});

/** The SVG for `payload` (text, or bytes for byte mode) in `style`. */
function render(payload, style) {
  return raw.render(payload, style);
}

/** The intro stopped at `tMs` milliseconds, as a still SVG. */
function renderFrame(payload, style, tMs) {
  return raw.renderFrame(payload, style, tMs);
}

/** The scan report, parsed. A refused style still has one, with `ok` false. */
function check(payload, style) {
  return JSON.parse(raw.check(payload, style));
}

/** The animation plan (SPEC §8.5), parsed. */
function plan(payload, style) {
  return JSON.parse(raw.plan(payload, style));
}

/**
 * A palette from pixels: `{width, height, data}` with 8-bit RGBA rows
 * (an `ImageData` works as is), or `{width, height, rgba}`.
 */
function paletteFromImage(image, opts) {
  return JSON.parse(raw.paletteFromImage(image, opts));
}

/** A palette from a style sheet's colors. */
function paletteFromCss(css, opts) {
  return JSON.parse(raw.paletteFromCss(css, opts));
}

/** A palette from a page's colors: theme-color, style sheets, style attributes. */
function paletteFromHtml(html, opts) {
  return JSON.parse(raw.paletteFromHtml(html, opts));
}

/** The preset names. */
function presets() {
  return JSON.parse(raw.presets());
}

/** The library version, the same in every package. */
function version() {
  return call(wbg.version, []);
}
