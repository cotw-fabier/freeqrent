# freeqrent (npm)

Styled and animated SVG QR codes from freeQRent's Rust core, compiled to WebAssembly. One package for Node ≥ 18 and browsers, with ESM and CommonJS entry points, full TypeScript types generated from the style schema, a `freeqrent` command, and no runtime dependencies.

Output is deterministic: the same payload and style give the same bytes here as in Rust, Python, Dart and Go (the conformance suite in `conformance/cases.json` holds every package to it).

## Install

```sh
npm install freeqrent
```

## Quick start

```js
import { render } from 'freeqrent'; // or: const { render } = require('freeqrent');

const svg = render('https://example.com', {
  preset: 'ink',
  modules: { kind: 'dots', size: 0.9 },
  finders: { ring: 'squircle', eye: 'circle' },
  intro: 'ripple',
});
```

In a browser, load the module once first:

```js
import { init, render } from 'freeqrent';

await init();
document.body.innerHTML = render('https://example.com', { preset: 'candy' });
```

### Initialization

- **Node:** invisible. The `.wasm` file next to the entry point is read and compiled synchronously on the first call (about 2 ms). `await init()` works too and resolves at once, so code written for browsers runs unchanged.
- **Browsers (and Deno, workers, bundlers):** call `await init()` once. It fetches `freeqrent.wasm` from beside the script (`new URL('./freeqrent.wasm', import.meta.url)`) and compiles it while it streams in. Calling anything before that throws. To load the module from elsewhere, pass `init(source)`: a URL or string, a `Response` or its promise, the bytes, or a compiled `WebAssembly.Module`.

### Why this loading scheme

The package ships the module as a file, `dist/freeqrent.wasm`, and picks a loader through the `exports` conditions: `node` gets `dist/node.mjs` or `dist/node.cjs`, which read the file with `fs`; everything else gets `dist/browser.mjs`, which fetches it. Both work without a bundler, and bundlers that understand `new URL(…, import.meta.url)` (Vite, webpack 5, Parcel, Rollup with a plugin) copy the file for you.

Inlining the module as base64 was the other choice. It would make browser use synchronous, but it costs a third more over the wire (284 KB gzipped against 208 KB, since base64 compresses worse than the binary), adds a 640 KB string to parse before anything runs, and gives up streaming compilation. A browser has to wait for a fetch either way, so one `await init()` there is cheap, and Node pays nothing.

## API

Every function takes a **payload**, a string (text, segmented into the shortest modes) or bytes (`Uint8Array` or any typed-array view or `ArrayBuffer`, encoded in byte mode), and a **style**: a typed object, a plain object, the Style document's JSON text, or nothing for the default style.

| Function | Returns |
|---|---|
| `render(payload, style?)` | the SVG document |
| `renderFrame(payload, style, tMs)` | the intro stopped at `tMs` milliseconds, as a still SVG |
| `check(payload, style?)` | the scan report (`Report`), parsed; a style `render` would refuse still has one, with `ok` false |
| `plan(payload, style?)` | the animation plan (`Plan`, SPEC §8.5), parsed |
| `paletteFromImage({width, height, data}, opts?)` | a `PaletteResult`; an `ImageData` works as is |
| `paletteFromCss(css, opts?)`, `paletteFromHtml(html, opts?)` | a `PaletteResult` |
| `presets()` | the preset names |
| `version()` | the library version, the same in every package |
| `init(source?)` | a promise for the loaded module (see above) |
| `toStyleJson(style)` | the Style document's JSON text for a style |
| `raw.*` | the same functions returning the library's text byte for byte (to hash, cache, or hand a plan to a native player) |

A palette result's `palette` drops straight into a style:

```js
const { palette } = paletteFromCss(await (await fetch('/site.css')).text());
const svg = render('https://example.com', { preset: 'ink', palette });
```

Palette options are `{ clusters, seed, min_contrast, allow_inverted, logo_colors }` or `logo` (`{ svg }` or `{ image: {width, height, data} }`) in place of `logo_colors`, as an object or JSON text.

### Styles

The style is the Style document of SPEC §10.1; its keys keep the document's snake_case names (`quiet_zone`, `duration_ms`), so a style object and its JSON are the same thing, and the same in every language. The types in `dist/index.d.ts` are generated from `schema/freeqrent.schema.json`: variants are discriminated unions on `kind` (`ModuleShape`, `Frame`, `Effect`, `Texture`, `Ease`, `Stagger`, `Trigger`, `Ink`, `LogoFrame` …), a variant without data may also be its bare name (`'dots'` is `{ kind: 'dots' }`), and names are string-literal unions (`PresetName`, `IdleName`, `IntroPreset`, `Token` …). An intro is a preset, an `IntroOptions` or a `PaletteShift`, and a logo is `svg` or `raster`, never both; TypeScript refuses a mix.

The serializer adds a few JavaScript conveniences: bytes (`Uint8Array`) anywhere a document takes base64 are encoded for you (a raster logo, an image's `rgba`), `{width, height, data}` (an `ImageData`) is taken as an image, a `bigint` seed is written exactly (seeds are `u64`), and keys set to `undefined` are left out.

```ts
import { render, type Style } from 'freeqrent';

const style: Style = {
  logo: { raster: pngBytes, frame: { kind: 'orbit', moon: 2 } },
  halftone: { image: ctx.getImageData(0, 0, w, h), ink: { kind: 'color', colors: 4 } },
  seed: 18446744073709551615n,
};
```

### Errors

Everything the library refuses throws a `FreeQrentError` (a subclass of `Error`) with a stable `code`, an English `message`, and `detail`, the error's fields as an object:

```js
try {
  render('hi', { palette: { fg: '#eee' } });
} catch (e) {
  e.code;   // 'contrast_too_low'
  e.detail; // { token: 'fg', delta_l: 0.042 }
}
```

The codes are SPEC §10.1's: `invalid_style` (`detail.path` like `intro.trigger.ms`, `detail.reason`, and `offset` for text that isn't JSON), `invalid_logo`, `invalid_image`, `data_too_long`, `contrast_too_low`, `option_out_of_range` and the rest. Arguments JavaScript can get wrong (a payload that isn't a string or bytes, text with a lone surrogate, a negative `tMs`) are `invalid_argument`. A panic inside the library, which shouldn't happen, is `internal`, and the next call gets a fresh instance.

### Threads

Each worker thread that imports the package gets its own instance; the outputs are the same bytes as on the main thread.

## The `freeqrent` command

```sh
npx freeqrent "https://example.com" --preset ink --shape dots:0.9 --intro ripple -o code.svg
```

It takes the Rust CLI's flags (`bindings/SURFACE.md` lists them all) and writes the same bytes: the test suite runs every equivalence line of that page through it, and in CI holds it to the Rust CLI itself. `--check` prints the report as JSON, `--plan` the plan, `--frame-at MS` a still frame, `--show-palette` what `--palette-from-*` found; `-` reads a palette source from stdin. Exit codes are the CLI's: 0 ok, 1 refused, 2 bad arguments.

What differs:

- **Pictures are PNG only.** `--halftone`, `--art`, `--palette-from-image`, `--clarity-map` and `--priority` read PNG files (every color type and bit depth, palettes, transparency, interlacing) with a decoder in plain JavaScript over `node:zlib`, or `sample`. JPEG files are refused with a message, since the package carries no image decoder. Logos are passed to the library as they are, so JPEG, GIF and WebP logos work.
- **A JPEG logo with `--palette-from-*` is refused.** The extracted frame keeps apart from the logo's colors, and the bin reads them from an SVG's paints or a PNG's pixels only.
- **`--verify`, `--gallery`, `--gallery-picture` and `--scan-sheet`** are the Rust CLI's own tools (they need a rasterizer and decoders) and are refused.

## Build from source

You need Rust with the `wasm32-unknown-unknown` target, `wasm-bindgen-cli` at exactly the version in `crate/Cargo.lock`, and Node ≥ 18.

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked
cd bindings/js
npm ci           # binaryen (wasm-opt) and typescript, both dev-only
npm run build    # crate/ -> dist/
npm test
npm run bench    # sizes and timings
```

`npm run build` compiles `crate/` (wasm-bindgen exports over `freeqrent-doc`, size-tuned: opt-level "s", LTO, one codegen unit, panic=abort, strip), runs `wasm-bindgen --target web`, shrinks the module with `wasm-opt -Oz` from the `binaryen` devDependency (`--no-opt` skips it), and writes `dist/`: the module, the three entry points (the generated glue in its own scope, a loader from `src/load-*.js` and the API from `src/api.js`) and the types. It honors `CARGO_TARGET_DIR`. The bin's comparison with the Rust CLI runs when `FREEQRENT_CLI` points at its binary (`cargo build --release -p freeqrent-cli --no-default-features`).

## Regenerate the types

```sh
npm run types    # schema/freeqrent.schema.json -> src/style.d.ts
```

`src/style.d.ts` is checked in; `test/types.test.mjs` fails when it is stale, and type-checks `test/types/usage.ts` with `tsc --strict`, including styles it must refuse. The API's own types (`src/api.d.ts`) are written by hand, and the build joins the two into `dist/index.d.ts`.

## Size

`dist/freeqrent.wasm` is 478 KB, 208 KB gzipped (504 KB and 212 KB before `wasm-opt`). That is the whole binding feature set (the document parser, animation, palette extraction and pictures); SPEC §2's 200 KB gzipped budget is for the core with its default features, which `scripts/wasm-size.sh` measures at 176 KB.
