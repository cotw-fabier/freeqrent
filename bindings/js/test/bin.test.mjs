// The npm bin (bin/freeqrent.js). Every equivalence line of
// bindings/SURFACE.md, `FLAGS => [plan | frame MS] DOCUMENT`, runs through
// the bin as a command and through the library as a document, and the two
// outputs must be the same bytes; the same convention as the Rust CLI's
// crates/freeqrent-cli/tests/surface.rs ($SVG and $PNG for a logo file,
// $IMAGE for a picture). Picture lines run too: the bin decodes PNG.
//
// With FREEQRENT_CLI set to the Rust CLI's binary, the bin is also held to
// the CLI itself, on those lines and on flags the document can't show
// (samples, extracted palettes, a URL's own label).

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmdirSync, unlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

import * as fq from '../dist/node.mjs';
import { rgbaPng } from './fixtures/png.mjs';

const TEXT = 'HELLO freeQRent';
const BIN = fileURLToPath(new URL('../bin/freeqrent.js', import.meta.url));
const MARK =
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><circle cx="5" cy="5" r="4" fill="#c2410c"/></svg>';

// The surface test's picture: a warm white page, a navy block and an orange one.
function picture() {
  const data = new Uint8Array(24 * 24 * 4);
  for (let y = 0; y < 24; y++) {
    for (let x = 0; x < 24; x++) {
      const px = x < 10 ? (y < 8 ? [230, 90, 20, 255] : [20, 30, 80, 255]) : [250, 248, 244, 255];
      data.set(px, (y * 24 + x) * 4);
    }
  }
  return { width: 24, height: 24, data };
}

const dir = mkdtempSync(join(tmpdir(), 'freeqrent-js-bin-'));
const files = {
  svg: join(dir, 'mark.svg'),
  png: join(dir, 'picture.png'),
  css: join(dir, 'brand.css'),
  html: join(dir, 'page.html'),
  out: join(dir, 'out.svg'),
};
const png = rgbaPng(picture());
writeFileSync(files.svg, MARK);
writeFileSync(files.png, png);
writeFileSync(files.css, ':root { --brand: #1d3b72; --accent: #c2410c; --paper: #f8f5ee; }');
writeFileSync(files.html, '<meta name="theme-color" content="#0f3d3e"><body style="background:#fdf6e3;color:#073642">');

test.after(() => {
  for (const f of Object.values(files)) {
    try {
      unlinkSync(f);
    } catch {
      // out.svg is only written by one test
    }
  }
  rmdirSync(dir);
});

/** Splits a command line on spaces, keeping single-quoted words whole (as surface.rs does). */
function words(line) {
  const out = [];
  let word = '';
  let quoted = false;
  let any = false;
  for (const c of line) {
    if (c === "'") {
      quoted = !quoted;
      any = true;
    } else if (c === ' ' && !quoted) {
      if (any) out.push(word);
      word = '';
      any = false;
    } else {
      word += c;
      any = true;
    }
  }
  if (any) out.push(word);
  return out;
}

function flag(word) {
  if (word === '$SVG') return files.svg;
  if (word === '$PNG' || word === '$IMAGE') return files.png;
  return word;
}

function documentOf(json) {
  const p = picture();
  const image = JSON.stringify({ width: p.width, height: p.height, rgba: Buffer.from(p.data).toString('base64') });
  return json
    .replaceAll('$SVG', JSON.stringify(MARK))
    .replaceAll('$PNG', JSON.stringify(png.toString('base64')))
    .replaceAll('$IMAGE', image);
}

function equivalences() {
  const surface = readFileSync(new URL('../../SURFACE.md', import.meta.url), 'utf8');
  const lines = [];
  let inside = false;
  for (const raw of surface.split('\n')) {
    const l = raw.trim();
    if (l === '```equivalences') inside = true;
    else if (l === '```') inside = false;
    else if (inside && l !== '') {
      const at = l.indexOf(' => ');
      assert.ok(at > 0, `FLAGS => DOCUMENT: ${l}`);
      lines.push([l.slice(0, at).trim(), l.slice(at + 4).trim()]);
    }
  }
  return lines;
}

function bin(args, opts = {}) {
  return spawnSync(process.execPath, [BIN, ...args], { encoding: 'utf8', maxBuffer: 64 << 20, ...opts });
}

function viaDocument(doc) {
  const [head, rest] = [doc.slice(0, doc.indexOf(' ')), doc.slice(doc.indexOf(' ') + 1)];
  if (head === 'plan') return fq.raw.plan(TEXT, documentOf(rest));
  if (head === 'frame') {
    const sp = rest.indexOf(' ');
    return fq.renderFrame(TEXT, documentOf(rest.slice(sp + 1)), Number(rest.slice(0, sp)));
  }
  return fq.render(TEXT, documentOf(doc));
}

test('every equivalence line of bindings/SURFACE.md: the bin and the document agree', () => {
  const lines = equivalences();
  assert.ok(lines.length >= 60, `${lines.length} lines`);
  let pictures = 0;
  for (const [flags, doc] of lines) {
    if (flags.includes('$IMAGE')) pictures++;
    const out = bin([TEXT, ...words(flags).map(flag)]);
    assert.equal(out.status, 0, `${flags}: ${out.stderr}`);
    assert.ok(out.stdout === viaDocument(doc), `${flags} and ${doc} differ`);
  }
  assert.ok(pictures >= 5, 'the picture lines ran');
});

test('palette flags are two calls: the result goes into the palette', () => {
  const cases = [
    [['--palette-from-css', files.css], fq.paletteFromCss(readFileSync(files.css, 'utf8'), { allow_inverted: false })],
    [['--palette-from-html', files.html], fq.paletteFromHtml(readFileSync(files.html, 'utf8'), '')],
    [
      ['--palette-from-image', files.png, '--palette-clusters', '4', '--palette-contrast', '0.7'],
      fq.paletteFromImage(picture(), { clusters: 4, min_contrast: 0.7, allow_inverted: false }),
    ],
  ];
  for (const [flags, result] of cases) {
    const out = bin([TEXT, ...flags]);
    assert.equal(out.status, 0, out.stderr);
    assert.ok(out.stdout === fq.render(TEXT, { palette: result.palette }), flags.join(' '));
  }
  const css = readFileSync(files.css, 'utf8');
  const piped = bin([TEXT, '--palette-from-css', '-', '--fg', '#102030'], { input: css });
  assert.equal(piped.stdout, fq.render(TEXT, { palette: { ...fq.paletteFromCss(css).palette, fg: '#102030' } }));
  const shown = bin([TEXT, '--palette-from-css', files.css, '--show-palette']);
  assert.match(shown.stderr, /^palette: \d+ colors found\n/);
  assert.match(shown.stderr, /as flags: --bg '#[0-9a-f]{6}' --fg/);
});

test('output modes and exit codes', () => {
  const o = bin([TEXT, '--preset', 'ink', '-o', files.out]);
  assert.equal(o.status, 0, o.stderr);
  assert.equal(o.stdout, '');
  assert.equal(readFileSync(files.out, 'utf8'), fq.render(TEXT, { preset: 'ink' }));
  assert.equal(bin([TEXT, `--output=${files.out}`, '--shape=dots']).status, 0);
  assert.equal(readFileSync(files.out, 'utf8'), fq.render(TEXT, { modules: 'dots' }));

  const report = bin([TEXT, '--check']);
  assert.equal(report.status, 0);
  assert.deepEqual(JSON.parse(report.stdout), fq.check(TEXT));
  const refused = bin([TEXT, '--check', '--fg', '#eeeeee']);
  assert.equal(refused.status, 1);
  assert.equal(JSON.parse(refused.stdout).ok, false);

  const render = bin([TEXT, '--fg', '#eeeeee']);
  assert.equal(render.status, 1);
  assert.match(render.stderr, /^freeqrent: fg has contrast/);
  assert.equal(render.stdout, '');

  const warned = bin([TEXT, '--target-px', '40']);
  assert.equal(warned.status, 0);
  assert.match(warned.stderr, /^warning: /);

  for (const args of [
    [],
    [TEXT, 'two'],
    [TEXT, '--shape', 'hex'],
    [TEXT, '--frobnicate'],
    [TEXT, '--seed', 'x'],
    [TEXT, '--ink', 'mono'],
    [TEXT, '--logo-scale', '0.2', '--logo-frame', 'ring'],
    [TEXT, '--caption-size', '2'],
    [TEXT, '--intro', 'ripple', '--from-fg', '#000'],
    [TEXT, '--art', files.png, '--preset', 'ink'],
    [TEXT, '--logo', files.css],
    [TEXT, '--shape'],
    [TEXT, '--pretty=yes'],
    [TEXT, '--verify'],
    [TEXT, '--gallery', 'g.html'],
    [TEXT, '--scan-sheet', 's.html'],
    [TEXT, '--halftone', files.css],
  ]) {
    const out = bin(args);
    assert.equal(out.status, 2, `${args.join(' ')}: ${out.stdout}${out.stderr}`);
    assert.match(out.stderr, /^freeqrent: /);
  }
  assert.match(bin([TEXT, '--verify']).stderr, /Rust CLI/);
  const help = bin(['--help']);
  assert.equal(help.status, 0);
  assert.match(help.stdout, /^freeqrent \(npm\)/);
});

test('JPEG pictures are refused with a reason', () => {
  const jpeg = join(dir, 'photo.jpg');
  writeFileSync(jpeg, Buffer.from([0xff, 0xd8, 0xff, 0xe0, 0, 16]));
  try {
    for (const f of ['--halftone', '--art', '--palette-from-image', '--clarity-map', '--priority']) {
      const args = f === '--clarity-map' || f === '--priority' ? ['--halftone', files.png, f, jpeg] : [f, jpeg];
      const out = bin([TEXT, ...args]);
      assert.equal(out.status, 2, f);
      assert.match(out.stderr, /JPEG isn't decoded by the npm bin/, f);
    }
  } finally {
    unlinkSync(jpeg);
  }
});

test('samples, a URL label and a bigint seed', () => {
  const url = 'https://example.com/menu';
  assert.equal(bin([url]).stdout, fq.render(url, { label: url }));
  assert.equal(bin([url, '--label', 'Menu']).stdout, fq.render(url, { label: 'Menu' }));
  assert.equal(bin([TEXT, '--seed', '18446744073709551615']).stdout, fq.render(TEXT, '{"seed":18446744073709551615}'));
  assert.equal(bin([TEXT, '--logo', 'sample']).status, 0);
  assert.equal(bin([TEXT, '--halftone', 'sample']).status, 0);
  assert.equal(bin([TEXT, '--palette-from-css', 'sample-dark', '--allow-inverted']).status, 0);
  // -- ends the options.
  assert.equal(bin(['--', '--not-a-flag']).stdout, fq.render('--not-a-flag'));
});

// The Rust CLI itself, when it's at hand (CI builds it).
const cli = process.env.FREEQRENT_CLI;

test('the bin writes what the Rust CLI writes', { skip: cli ? false : 'FREEQRENT_CLI is not set' }, () => {
  const lines = equivalences().map(([flags]) => words(flags).map(flag));
  const extra = [
    ['--halftone', 'sample'],
    ['--art', 'sample', '--ink', 'color'],
    ['--art', 'sample', '--version', '12'],
    ['--palette-from-image', 'sample'],
    ['--palette-from-css', 'sample'],
    ['--palette-from-css', 'sample-dark', '--allow-inverted'],
    ['--palette-from-css', 'sample-dark', '--dark'],
    ['--preset', 'ink', '--palette-from-css', 'sample'],
    ['--palette-from-image', files.png, '--palette-clusters', '3'],
    ['--logo', files.png, '--palette-from-css', 'sample', '--ecc', 'H'],
    ['--logo', 'sample', '--logo-frame', 'beads:0.6'],
    ['--finder', 'rounded:3'],
    ['--alignment', 'rounded', '--version', '7'],
    ['--alignment', 'square:1', '--version', '7'],
    ['--stagger', 'random', '--intro', 'ripple'],
    ['--frame', 'circle', '--frame-ring', 'solid'],
    ['--frame-ring', 'solid'],
    ['--from-fg', '#c2410c', '--dark'],
    ['--ecc', 'q', '--seed', '9007199254740993'],
    ['--intro', 'ripple', '--plan'],
    ['--intro', 'iris', '--frame-at', '300'],
    // With a preset the extracted palette replaces the preset's, and with
    // an SVG logo the extracted frame keeps apart from its paints.
    ['--preset', 'candy', '--palette-from-html', files.html],
    ['--palette-from-html', files.html, '--logo', files.svg],
  ];
  for (const args of [...lines, ...extra]) {
    for (const text of [TEXT, 'https://example.com/x']) {
      const want = spawnSync(cli, [text, ...args], { encoding: 'utf8', maxBuffer: 64 << 20 });
      const got = bin([text, ...args]);
      assert.equal(got.status, want.status, `${args.join(' ')}: ${want.stderr} / ${got.stderr}`);
      assert.ok(got.stdout === want.stdout, `${text} ${args.join(' ')}: the bin and the CLI differ`);
    }
  }

});

test('the bin is executable through npm exec', { skip: process.platform === 'win32' }, () => {
  const out = execFileSync(BIN, [TEXT, '--preset', 'leaf'], { encoding: 'utf8' });
  assert.equal(out, fq.render(TEXT, { preset: 'leaf' }));
});
