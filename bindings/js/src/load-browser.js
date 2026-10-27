// Loader fragment for browsers (and any runtime without Node's fs): the
// module is fetched from next to this file, compiled while it streams in,
// once, by `await init()`. Pass `init(source)` to load it from elsewhere:
// a URL or string, a Response or its promise, bytes, or a compiled
// WebAssembly.Module.

let loading = null;

function ready() {
  if (!wbg.loaded()) {
    throw new Error('freeqrent: call `await init()` once before the first call in a browser');
  }
}

/** Loads the WebAssembly module, once; later calls resolve at once. */
function init(source) {
  if (wbg.loaded()) return Promise.resolve();
  if (loading === null) {
    const module_or_path = source === undefined ? new URL('./freeqrent.wasm', import.meta.url) : source;
    loading = wbg.load({ module_or_path }).then(
      () => undefined,
      (e) => {
        loading = null;
        throw e;
      },
    );
  }
  return loading;
}
