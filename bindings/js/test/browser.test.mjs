// The browser build, loaded the way a page loads it: the module fetched
// over HTTP and compiled while it streams (WebAssembly.instantiateStreaming),
// from a local server. No browser is driven here; Node's fetch and
// WebAssembly stand in for one. The build also mustn't reach for Node.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createServer } from 'node:http';

import * as node from '../dist/node.mjs';

const dist = new URL('../dist/', import.meta.url);

test('the browser build has no Node imports', () => {
  const src = readFileSync(new URL('browser.mjs', dist), 'utf8');
  const hit = /^\s*import\b|\brequire\(|['"]node:/m.exec(src);
  assert.equal(hit, null, hit && hit[0]);
  assert.match(src, /new URL\('\.\/freeqrent\.wasm', import\.meta\.url\)/);
});

test('the browser build loads over HTTP with await init()', async () => {
  const wasm = readFileSync(new URL('freeqrent.wasm', dist));
  const requests = [];
  const server = createServer((req, res) => {
    requests.push(req.url);
    if (req.url === '/freeqrent.wasm') {
      res.writeHead(200, { 'content-type': 'application/wasm' });
      res.end(wasm);
    } else {
      res.writeHead(404);
      res.end();
    }
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  try {
    const browser = await import(new URL('browser.mjs', dist));
    assert.throws(() => browser.render('x'), /await init\(\)/);
    const url = `http://127.0.0.1:${server.address().port}/freeqrent.wasm`;
    await Promise.all([browser.init(url), browser.init(url)]);
    await browser.init();
    assert.deepEqual(requests, ['/freeqrent.wasm']);
    const style = { preset: 'sunset', intro: 'iris', logo: { svg: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 4 4"><rect width="4" height="4"/></svg>' } };
    assert.equal(browser.render('https://example.com', style), node.render('https://example.com', style));
    assert.deepEqual(browser.plan('https://example.com', style), node.plan('https://example.com', style));
    assert.throws(() => browser.render('x', { quiet_zone: 0 }), browser.FreeQrentError);
  } finally {
    server.close();
  }
});

test('init takes bytes or a compiled module too', async () => {
  // A second copy of the browser build, with its own instance.
  const url = new URL('browser.mjs', dist);
  url.search = '?bytes';
  const browser = await import(url);
  await browser.init(readFileSync(new URL('freeqrent.wasm', dist)));
  assert.equal(browser.render('bytes'), node.render('bytes'));

  url.search = '?module';
  const again = await import(url);
  await again.init(await WebAssembly.compile(readFileSync(new URL('freeqrent.wasm', dist))));
  assert.equal(again.render('module'), node.render('module'));
});
