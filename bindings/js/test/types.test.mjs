// The typed surface: src/style.d.ts is what scripts/gen-types.mjs makes of
// the schema today, the built types carry it, and TypeScript (strict)
// accepts real styles and refuses wrong ones (test/types/usage.ts).

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

import { generate, outputPath, schemaPath } from '../scripts/gen-types.mjs';

test('src/style.d.ts is up to date with the schema (npm run types)', () => {
  const schema = JSON.parse(readFileSync(schemaPath, 'utf8'));
  assert.equal(readFileSync(outputPath, 'utf8'), generate(schema), 'stale: run npm run types');
});

test('every schema name list is in the types', () => {
  const schema = readFileSync(schemaPath, 'utf8');
  const types = readFileSync(outputPath, 'utf8');
  for (const [, list] of schema.matchAll(/"enum": \[([^\]]*)\]/g)) {
    const union = JSON.parse(`[${list}]`).map((v) => JSON.stringify(v)).join(' | ');
    assert.ok(types.includes(union), union);
  }
  for (const [, kind] of schema.matchAll(/"const": "([^"]+)"/g)) {
    assert.ok(types.includes(`kind: "${kind}";`), kind);
  }
});

test('the built types are the generated ones plus the API', () => {
  const built = readFileSync(new URL('../dist/index.d.ts', import.meta.url), 'utf8');
  assert.ok(built.includes(readFileSync(outputPath, 'utf8')));
  assert.ok(built.includes(readFileSync(new URL('../src/api.d.ts', import.meta.url), 'utf8')));
  for (const name of ['index.d.mts', 'index.d.cts']) {
    assert.equal(readFileSync(new URL(`../dist/${name}`, import.meta.url), 'utf8'), built);
  }
});

test('TypeScript checks test/types/usage.ts (strict)', () => {
  const tsc = createRequire(import.meta.url).resolve('typescript/bin/tsc');
  const project = fileURLToPath(new URL('./types/tsconfig.json', import.meta.url));
  try {
    execFileSync(process.execPath, [tsc, '-p', project], { encoding: 'utf8' });
  } catch (e) {
    assert.fail(`tsc:\n${e.stdout}${e.stderr}`);
  }
});
