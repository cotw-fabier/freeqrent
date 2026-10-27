// Sizes and timings for the report: the .wasm raw and gzipped (SPEC §2's
// budget is 200 KB gzipped for the core), the first call (compile and
// instantiate), and renders of a version 10 code in Node.
//
//   node scripts/bench.mjs

import { readFileSync } from 'node:fs';
import { performance } from 'node:perf_hooks';
import { gzipSync } from 'node:zlib';

const wasm = readFileSync(new URL('../dist/freeqrent.wasm', import.meta.url));
const gz = gzipSync(wasm, { level: 9 }).length;
console.log(`dist/freeqrent.wasm: ${wasm.length} bytes, ${gz} gzipped (budget ${200 * 1024})`);

let t = performance.now();
const fq = await import('../dist/node.mjs');
fq.version();
console.log(`import and first call (compile + instantiate): ${(performance.now() - t).toFixed(1)} ms`);

const text = 'https://example.com/a-longer-path/for-a-version-ten-code?with=query&and=more';
const styles = {
  'default': {},
  'ink, dots': { preset: 'ink', modules: { kind: 'dots', size: 0.9 } },
  'liquid, ripple intro': { modules: 'liquid', intro: 'ripple' },
  'candy, logo, glow': {
    preset: 'candy',
    logo: { svg: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><circle cx="5" cy="5" r="4" fill="#c2410c"/></svg>' },
    effect: 'glow',
  },
};
for (const [name, style] of Object.entries(styles)) {
  const doc = { ...style, encoding: { version: 10 } };
  for (let i = 0; i < 20; i++) fq.render(text, doc);
  const n = 200;
  t = performance.now();
  for (let i = 0; i < n; i++) fq.render(text, doc);
  const ms = (performance.now() - t) / n;
  console.log(`v10 render, ${name}: ${ms.toFixed(3)} ms (${fq.check(text, doc).code.size} modules a side)`);
}
