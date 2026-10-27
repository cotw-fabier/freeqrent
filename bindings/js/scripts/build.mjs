// Builds the npm package into dist/:
//
//   1. the Rust crate (crate/) for wasm32-unknown-unknown, release settings
//      from crate/Cargo.toml (opt-level "s", LTO, one codegen unit,
//      panic=abort, strip);
//   2. wasm-bindgen (the CLI at exactly the crate's version) with
//      `--target web`;
//   3. wasm-opt -Oz from the `binaryen` devDependency (skipped with
//      --no-opt, or when it isn't installed);
//   4. the entry points: dist/node.mjs, dist/node.cjs and dist/browser.mjs,
//      each the wasm-bindgen glue in its own scope, a loader
//      (src/load-node.js or src/load-browser.js) and the API (src/api.js),
//      with ESM or CJS exports; dist/freeqrent.wasm beside them; and the
//      types (src/style.d.ts, generated, plus src/api.d.ts).
//
// CARGO_TARGET_DIR is honored. Usage: node scripts/build.mjs [--no-opt]

import { execFileSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { gzipSync } from 'node:zlib';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const crate = join(root, 'crate');
const build = join(root, 'build');
const dist = join(root, 'dist');
const noOpt = process.argv.includes('--no-opt');

function run(cmd, args, opts = {}) {
  console.error(`> ${cmd} ${args.join(' ')}`);
  execFileSync(cmd, args, { stdio: 'inherit', ...opts });
}

function fail(message) {
  console.error(`build: ${message}`);
  process.exit(1);
}

// 1. The crate.
const targetDir = process.env.CARGO_TARGET_DIR || join(crate, 'target');
run('cargo', [
  'build',
  '--locked',
  '--release',
  '--target',
  'wasm32-unknown-unknown',
  '--manifest-path',
  join(crate, 'Cargo.toml'),
  '--target-dir',
  targetDir,
]);
const raw = join(targetDir, 'wasm32-unknown-unknown', 'release', 'freeqrent_wasm.wasm');

// 2. wasm-bindgen, at the crate's exact version.
const lock = readFileSync(join(crate, 'Cargo.lock'), 'utf8');
const want = /name = "wasm-bindgen"\nversion = "([^"]+)"/.exec(lock)?.[1];
let have;
try {
  have = execFileSync('wasm-bindgen', ['--version'], { encoding: 'utf8' }).trim().split(' ')[1];
} catch {
  fail(`wasm-bindgen isn't installed: cargo install wasm-bindgen-cli --version ${want}`);
}
if (have !== want) {
  fail(`wasm-bindgen ${have} is installed, the crate uses ${want}: cargo install wasm-bindgen-cli --version ${want}`);
}
mkdirSync(build, { recursive: true });
run('wasm-bindgen', ['--target', 'web', '--out-dir', build, '--out-name', 'freeqrent', raw]);
const bindgenWasm = join(build, 'freeqrent_bg.wasm');

// 3. wasm-opt.
mkdirSync(dist, { recursive: true });
const wasmOut = join(dist, 'freeqrent.wasm');
const wasmOpt = join(root, 'node_modules', 'binaryen', 'bin', 'wasm-opt');
if (!noOpt && existsSync(wasmOpt)) {
  run(process.execPath, [
    wasmOpt,
    '-Oz',
    '--converge',
    // The features rustc's wasm32 target uses and Node 18 has.
    '--enable-bulk-memory',
    '--enable-nontrapping-float-to-int',
    '--enable-sign-ext',
    '--enable-mutable-globals',
    '--strip-debug',
    '--strip-producers',
    bindgenWasm,
    '-o',
    wasmOut,
  ]);
} else {
  if (!noOpt) console.error('build: binaryen not installed (npm install); skipping wasm-opt');
  copyFileSync(bindgenWasm, wasmOut);
}

// 4. The entry points.
let glue = readFileSync(join(build, 'freeqrent.js'), 'utf8');
glue = glue.replace(/^\/\* @ts-self-types=.*\*\/\n/, '');
const exported = [];
glue = glue.replace(/^export function (\w+)/gm, (_, name) => {
  exported.push(name);
  return `function ${name}`;
});
const boundary = [
  'check',
  'palette_from_css',
  'palette_from_html',
  'palette_from_image',
  'plan',
  'presets',
  'render',
  'render_frame',
  'version',
];
if (exported.sort().join() !== boundary.join()) {
  fail(`wasm-bindgen exported ${exported.join(', ')}; expected ${boundary.join(', ')}`);
}
const tail = 'export { initSync, __wbg_init as default };';
if (glue.split(tail).length !== 2) fail('the glue no longer ends with its usual exports');
glue = glue.replace(tail, '');
if (/^\s*(export|import)\b/m.test(glue)) fail('the glue has an import or export left');
// What reset() rebuilds after a trap: the glue's object heap and instance.
const heapInit = /^let heap = (.*);\nheap\.push\((.*)\);$/m.exec(glue);
if (!heapInit || !/^let heap_next = heap\.length;$/m.test(glue) || !/^let wasmModule, wasmInstance, wasm;$/m.test(glue)) {
  fail('the glue keeps its state differently; update reset() in scripts/build.mjs');
}
const wbg = (flavor) => `const wbg = (() => {
${flavor === 'cjs' ? glue.replaceAll('import.meta.url', "require('node:url').pathToFileURL(__filename)") : glue}
  return {
    ${boundary.join(',\n    ')},
    initSync,
    load: __wbg_init,
    loaded: () => wasm !== undefined,
    reset() {
      const module = wasmModule;
      wasm = undefined;
      heap = ${heapInit[1]};
      heap.push(${heapInit[2]});
      heap_next = heap.length;
      if (module !== undefined) initSync({ module });
    },
  };
})();
`;

const api = readFileSync(join(root, 'src', 'api.js'), 'utf8');
const loadNode = readFileSync(join(root, 'src', 'load-node.js'), 'utf8');
const loadBrowser = readFileSync(join(root, 'src', 'load-browser.js'), 'utf8');
const names = [
  'FreeQrentError',
  'init',
  'render',
  'renderFrame',
  'check',
  'plan',
  'paletteFromImage',
  'paletteFromCss',
  'paletteFromHtml',
  'presets',
  'version',
  'toStyleJson',
  'raw',
];
const banner = `// freeqrent ${JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')).version}: generated by scripts/build.mjs; don't edit.\n`;
const esm = `export {\n  ${names.join(',\n  ')},\n};\n`;
const cjs = `module.exports = {\n  ${names.join(',\n  ')},\n};\n`;

writeFileSync(
  join(dist, 'node.mjs'),
  banner +
    "import { readFileSync } from 'node:fs';\n" +
    "const WASM_BYTES = () => readFileSync(new URL('./freeqrent.wasm', import.meta.url));\n" +
    wbg('esm') +
    loadNode +
    api +
    esm,
);
writeFileSync(
  join(dist, 'node.cjs'),
  banner +
    "'use strict';\n" +
    "const WASM_BYTES = () => require('node:fs').readFileSync(require('node:path').join(__dirname, 'freeqrent.wasm'));\n" +
    wbg('cjs') +
    loadNode +
    api +
    cjs,
);
writeFileSync(join(dist, 'browser.mjs'), banner + wbg('esm') + loadBrowser + api + esm);

// The types: one file for both module systems.
const types =
  banner.replace('//', '//') +
  readFileSync(join(root, 'src', 'style.d.ts'), 'utf8') +
  '\n' +
  readFileSync(join(root, 'src', 'api.d.ts'), 'utf8');
for (const name of ['index.d.ts', 'index.d.mts', 'index.d.cts']) writeFileSync(join(dist, name), types);

// The repository's license travels with the package.
copyFileSync(join(root, '..', '..', 'LICENSE'), join(dist, 'LICENSE'));

const bytes = readFileSync(wasmOut);
const before = readFileSync(bindgenWasm);
console.error(
  `build: dist/freeqrent.wasm ${bytes.length} bytes, ${gzipSync(bytes, { level: 9 }).length} gzipped` +
    ` (before wasm-opt ${before.length}, ${gzipSync(before, { level: 9 }).length} gzipped)`,
);
