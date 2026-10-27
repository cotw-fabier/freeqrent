// The CommonJS entry point, and the package's exports map from both module
// systems (a package can require and import itself by name).
'use strict';

const { test } = require('node:test');
const assert = require('node:assert/strict');

const TEXT = 'https://example.com/cjs';
const STYLE = { preset: 'candy', modules: { kind: 'dots', size: 0.9 }, intro: 'ripple' };

test('require gives the CJS build, and it renders like the ESM one', async () => {
  const cjs = require('../dist/node.cjs');
  const esm = await import('../dist/node.mjs');
  assert.equal(typeof cjs.render, 'function');
  assert.equal(cjs.render(TEXT, STYLE), esm.render(TEXT, STYLE));
  assert.deepEqual(cjs.check(TEXT, STYLE), esm.check(TEXT, STYLE));
  assert.deepEqual(Object.keys(cjs).sort(), Object.keys(esm).sort());
  assert.throws(
    () => cjs.render(TEXT, { palette: { fg: '#eee' } }),
    (e) => e instanceof cjs.FreeQrentError && e.code === 'contrast_too_low' && e.detail.token === 'fg',
  );
});

test("the package's own name resolves to the Node builds", async () => {
  assert.equal(require.resolve('freeqrent'), require.resolve('../dist/node.cjs'));
  const byName = await import('freeqrent');
  const esm = await import('../dist/node.mjs');
  assert.equal(byName.render, esm.render);
  assert.equal(require('freeqrent').render, require('../dist/node.cjs').render);
  assert.equal(require.resolve('freeqrent/freeqrent.wasm'), require.resolve('../dist/freeqrent.wasm'));
});
