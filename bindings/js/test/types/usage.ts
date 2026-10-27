// Type-checked by test/types.test.mjs (tsc --noEmit, strict): the generated
// types accept real styles and refuse wrong ones.
import {
  check,
  FreeQrentError,
  init,
  paletteFromCss,
  paletteFromImage,
  plan,
  presets,
  raw,
  render,
  renderFrame,
  type Intro,
  type ModuleShape,
  type Style,
} from 'freeqrent';

await init();

const style: Style = {
  preset: 'ink',
  modules: { kind: 'bars', axis: 'horizontal', radius: 0.3 },
  finders: { ring: { kind: 'rounded', radius: 1 }, eye: 'circle', corners: { 'top-right': { ring: 'leaf' } } },
  alignment: { shape: { kind: 'own', ring: 'rounded', eye: 'circle' }, emphasis: 'outline' },
  palette: { fg: '#1d3b72', 'finder.eye': 'oklch(55% .2 30)' },
  frame: { kind: 'circle', filler_density: 0.4, ring: 'dashed' },
  effect: { kind: 'glow', radius: 0.3 },
  textures: ['grain', { kind: 'scanlines', opacity: 0.1 }],
  caption: { text: 'menu', font: 'mono' },
  logo: { raster: new Uint8Array(4), frame: { kind: 'orbit', moon: 2 }, shape: { kind: 'rounded', radius: 1 } },
  halftone: { image: { width: 1, height: 1, data: new Uint8ClampedArray(4) }, ink: { kind: 'color', colors: 4 }, bands: [0.3, 0.9] },
  intro: { preset: 'ripple', stagger: { kind: 'random', seed: 3n }, ease: { kind: 'custom', x1: 0.2, y1: 0, x2: 0, y2: 1 } },
  idle: ['finder-pulse', 'logo-turn'],
  seed: 18446744073709551615n,
  target_size: { kind: 'millimeters', mm: 25 },
  encoding: { ecc: 'H', tracking_grid: 'sparse' },
};

// Discriminated unions narrow on `kind`.
function describe(shape: ModuleShape): string {
  if (typeof shape === 'string') return shape;
  switch (shape.kind) {
    case 'dots':
      return `dots ${shape.size ?? 0.85}`;
    case 'bars':
      return `bars ${shape.axis ?? 'vertical'}`;
    default:
      return shape.kind;
  }
}
describe('liquid');

const shift: Intro = { palette_from: { fg: '#c2410c' }, duration_ms: 600, trigger: { kind: 'class', name: 'go' } };

const svg: string = render('https://example.com', style);
const frame: string = renderFrame(new Uint8Array([1, 2]), JSON.stringify(style), 400);
const report = check('x', { intro: shift });
const ok: boolean = report.ok && report.code.ecc === 'H';
const total: number = plan('x', { intro: 'sweep' }).totalMs;
const result = paletteFromCss(':root{--a:#123}', { allow_inverted: true });
const merged: Style = { palette: result.palette };
paletteFromImage({ width: 1, height: 1, data: new Uint8ClampedArray(4) }, '{"clusters":4}');
const names: string[] = presets();
const text: string = raw.plan('x', null);

try {
  render('x', '{}');
} catch (e) {
  if (e instanceof FreeQrentError) {
    const code: string = e.code;
    const detail: Record<string, unknown> = e.detail;
    console.log(code, detail, e.message);
  }
}

// @ts-expect-error: dots have a size, not a radius
const wrong: Style = { modules: { kind: 'dots', radius: 1 } };
// @ts-expect-error: not a preset
const unknownPreset: Style = { preset: 'neon' };
// @ts-expect-error: a logo is svg or raster, not both
const twoLogos: Style = { logo: { svg: '<svg/>', raster: 'AA==' } };
// @ts-expect-error: a palette shift takes no stagger
const shiftStagger: Intro = { palette_from: { fg: '#000' }, stagger: 'radial' };
// @ts-expect-error: tokens are a closed list
const badToken: Style = { palette: { foreground: '#000' } };
// @ts-expect-error: a custom ease needs its four numbers
const halfEase: Style = { intro: { ease: { kind: 'custom', x1: 0 } } };

export { svg, frame, ok, total, merged, names, text, wrong, unknownPreset, twoLogos, shiftStagger, badToken, halfEase };
