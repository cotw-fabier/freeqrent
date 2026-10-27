// Loader fragment for Node (scripts/build.mjs fills in WASM_BYTES): the
// module sits next to this file and is read and compiled synchronously on
// the first call, so Node needs no init. `init()` exists so code written
// for browsers runs here unchanged.

function ready() {
  if (!wbg.loaded()) wbg.initSync({ module: WASM_BYTES() });
}

/** Loads the WebAssembly module. In Node this is optional. */
function init() {
  try {
    ready();
    return Promise.resolve();
  } catch (e) {
    return Promise.reject(e);
  }
}
