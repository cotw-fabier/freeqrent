// The CLI's built-in samples (crates/freeqrent-cli/src/sample.rs and
// main.rs), so `sample` works for the bin's picture, logo and palette
// flags too.
'use strict';

/** `--logo sample`. */
const SAMPLE_LOGO =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect width="24" height="24" rx="6" fill="#c2410c"/><path d="M7 6h10v4h-6v2h5v4h-5v3H7z" fill="#fff7ed"/></svg>';

/** `--palette-from-css sample`. */
const SAMPLE_CSS = `:root { --paper: #f6f1e7; --ink: #23201c; --brand: #c2410c }
body { background: var(--paper); color: var(--ink) }
a, .button { color: var(--brand) }`;

/** `--palette-from-css sample-dark`: a dark-first site, for --allow-inverted. */
const SAMPLE_DARK_CSS = `:root { --paper: #12161c; --ink: #e8ecef; --brand: #3fa796 }
body { background: var(--paper); color: var(--ink) }
a, .button { color: var(--brand) }`;

const SIDE = 160;

function mix(a, b, t) {
  t = Math.min(Math.max(t, 0), 1);
  return [0, 1, 2].map((i) => a[i] + (b[i] - a[i]) * t);
}

// Rust's f64::round: halves away from zero.
const round = (v) => Math.sign(v) * Math.round(Math.abs(v));

/**
 * `--halftone sample`: a sunset over layered hills, the same drawing as the
 * CLI's (sample.rs). It uses sin and pow, so its pixels are as exact as the
 * platform's math library, like the CLI's own.
 */
function samplePicture() {
  const TOP = [38, 48, 104];
  const GLOW = [244, 150, 92];
  const SUN = [255, 226, 150];
  const RIDGES = [
    [[150, 88, 112], 0.62, 0.05, 2.1],
    [[84, 52, 92], 0.72, 0.07, 3.3],
    [[30, 24, 48], 0.84, 0.06, 5.2],
  ];
  const data = new Uint8Array(SIDE * SIDE * 4);
  for (let y = 0; y < SIDE; y++) {
    for (let x = 0; x < SIDE; x++) {
      const u = x / SIDE;
      const v = y / SIDE;
      let c = mix(TOP, GLOW, Math.pow(v / 0.7, 1.6));
      const dx = u - 0.62;
      const dy = v - 0.56;
      const r = Math.sqrt(dx * dx + dy * dy);
      if (r < 0.12) c = SUN;
      else if (r < 0.2) c = mix(SUN, c, (r - 0.12) / 0.08);
      for (const [ridge, base, height, waves] of RIDGES) {
        const top = base - height * (0.5 + 0.5 * Math.sin(u * waves * 3.0 + base * 9.0));
        if (v > top) c = ridge;
      }
      const o = (y * SIDE + x) * 4;
      data[o] = round(c[0]);
      data[o + 1] = round(c[1]);
      data[o + 2] = round(c[2]);
      data[o + 3] = 255;
    }
  }
  return { width: SIDE, height: SIDE, data };
}

module.exports = { SAMPLE_LOGO, SAMPLE_CSS, SAMPLE_DARK_CSS, samplePicture };
