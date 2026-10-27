// The API (hand-written; the Style document's types above are generated).

/** What a code carries: text (segmented into the shortest modes), or bytes in byte mode. */
export type Payload = string | Uint8Array;

/** A style: a typed or plain Style object, its JSON text, or nothing for the default style. */
export type StyleInput = Style | string | null | undefined;

/** Palette options: an object, its JSON text, or nothing. */
export type PaletteOptsInput = PaletteOpts | string | null | undefined;

/** The error codes of SPEC §10.1, plus `internal` for a panic inside the library. */
export type ErrorCode =
  | 'invalid_style'
  | 'invalid_logo'
  | 'invalid_image'
  | 'invalid_argument'
  | 'data_too_long'
  | 'invalid_version_range'
  | 'invalid_character'
  | 'invalid_eci'
  | 'contrast_too_low'
  | 'inverted_colors'
  | 'quiet_zone_too_small'
  | 'option_out_of_range'
  | 'ecc_budget_exceeded'
  | 'logo_too_large'
  | 'logo_over_alignment'
  | 'turn_needs_round_frame'
  | 'replay_too_soon'
  | 'shadow_needs_background'
  | 'write'
  | 'no_colors'
  | 'render_error'
  | 'encode_error'
  | 'internal';

/** Every error the library throws. `code` is stable, `message` is English, `detail` holds the fields. */
export class FreeQrentError extends Error {
  constructor(code: string, message: string, detail?: Record<string, unknown>);
  readonly name: 'FreeQrentError';
  /** A stable snake_case name, e.g. `contrast_too_low`. */
  code: ErrorCode | (string & {});
  /** The error's fields, e.g. `{ token: "fg", delta_l: 0.2 }`; `{ path, reason }` for `invalid_style`. */
  detail: Record<string, unknown>;
}

/**
 * Loads the WebAssembly module. Call it once (`await init()`) before the first call in a
 * browser; in Node the module loads itself on first use, and this resolves at once. In a
 * browser the module is fetched from beside the script; pass a URL, a Response (or its
 * promise), bytes or a compiled WebAssembly.Module to load it from elsewhere.
 */
export function init(
  source?: string | URL | Request | Response | Promise<Response> | BufferSource | WebAssembly.Module,
): Promise<void>;

/** The SVG document for `payload` in `style`. Throws FreeQrentError when the style can't be drawn. */
export function render(payload: Payload, style?: StyleInput): string;

/** The intro stopped at `tMs` milliseconds, as a still SVG. A style without an intro gives its finished code. */
export function renderFrame(payload: Payload, style: StyleInput, tMs: number): string;

/** The scan report. A style `render` would refuse still has one, with `ok` false. */
export function check(payload: Payload, style?: StyleInput): Report;

/** The animation plan (SPEC §8.5), for players outside CSS. */
export function plan(payload: Payload, style?: StyleInput): Plan;

/** A palette from pixels: an ImageData, or `{width, height, data}` with 8-bit RGBA rows. */
export function paletteFromImage(
  image: { width: number; height: number; data: Uint8Array | Uint8ClampedArray } | RawImage,
  opts?: PaletteOptsInput,
): PaletteResult;

/** A palette from a style sheet's colors. */
export function paletteFromCss(css: string, opts?: PaletteOptsInput): PaletteResult;

/** A palette from a page's colors: theme-color, style sheets and style attributes. */
export function paletteFromHtml(html: string, opts?: PaletteOptsInput): PaletteResult;

/** The preset names. */
export function presets(): PresetName[];

/** The library version, the same in every package. */
export function version(): string;

/** A style as the Style document's JSON text: what every function sends to the library. */
export function toStyleJson(style: StyleInput): string;

/**
 * The same functions, returning the library's text byte for byte instead of parsed
 * objects: to hash, cache, or pass on (a plan to a native player, say).
 */
export const raw: {
  readonly render: (payload: Payload, style?: StyleInput) => string;
  readonly renderFrame: (payload: Payload, style: StyleInput, tMs: number) => string;
  readonly check: (payload: Payload, style?: StyleInput) => string;
  readonly plan: (payload: Payload, style?: StyleInput) => string;
  readonly paletteFromImage: (
    image: { width: number; height: number; data: Uint8Array | Uint8ClampedArray } | RawImage,
    opts?: PaletteOptsInput,
  ) => string;
  readonly paletteFromCss: (css: string, opts?: PaletteOptsInput) => string;
  readonly paletteFromHtml: (html: string, opts?: PaletteOptsInput) => string;
  readonly presets: () => string;
};

// The animation plan (SPEC §8.5), as `plan` returns it.

/** What a plan layer draws. */
export type LayerRole =
  | 'background'
  | 'shadow'
  | 'grain'
  | 'caption'
  | 'scanlines'
  | 'border'
  | 'ring'
  | 'filler'
  | 'data'
  | 'timing'
  | 'alignment'
  | 'finder-outer'
  | 'finder-eye'
  | 'logo'
  | 'shimmer'
  | 'halftone'
  | 'logo-frame'
  | 'logo-ink'
  | 'logo-glint'
  | 'logo-art';

/** A logo frame stroked along its path. */
export interface PlanOutline {
  width: number;
  length: number;
  dashes: number[];
  dashOffset: number;
}

/** One shape of the static code, in paint order. */
export interface PlanLayer {
  role: LayerRole;
  token: Token;
  /** A picture's fixed color, overriding the token. */
  color?: string;
  /** The path is in fifths of a module from here. */
  cells?: [number, number];
  /** Stroked this wide instead of filled. */
  stroke?: number;
  outline?: PlanOutline;
  /** At rest; 1 when left out. */
  opacity?: number;
  evenodd?: boolean;
  /** 1 when left out. */
  fillOpacity?: number;
  group?: number;
  corner?: 'top-left' | 'top-right' | 'bottom-left';
  /** The scale origin. */
  origin: [number, number];
  /** SVG path data, in modules. */
  path: string;
}

/** An animated property. */
export type PlanProperty =
  | 'opacity'
  | 'scale'
  | 'translateX'
  | 'translateY'
  | 'fill'
  | 'reveal'
  | 'rotate'
  | 'draw'
  | 'scaleY';

export interface Keyframe {
  /** 0 to 1 through the track. */
  offset: number;
  property: PlanProperty;
  /** A number, or a CSS color for `fill`. */
  value: number | string;
}

export interface Track {
  target: { layer: number } | 'reveal';
  delayMs: number;
  durationMs: number;
  /** Cubic-bezier control points x1, y1, x2, y2. */
  ease: [number, number, number, number];
  /** null repeats forever. */
  iterations: number | null;
  keys: Keyframe[];
}

export type PlanReveal =
  | { shape: 'wipe'; from: [number, number]; to: [number, number]; feather: number; layers: [number, number] }
  | { shape: 'iris'; center: [number, number]; radius: number; feather: number; layers: [number, number] };

export type PlanTrigger = 'load' | 'hover' | { class: string } | { every: number };

export type PlanEffect =
  | { kind: 'glow'; radius: number; strength: number }
  | { kind: 'emboss'; depth: number }
  | { kind: 'sketch'; roughness: number };

export interface PlanCaption {
  text: string;
  size: number;
  font: 'sans' | 'serif' | 'mono';
  bold: boolean;
  tracking: number;
  center: [number, number];
}

export type PlanTexture =
  | { kind: 'grain'; amount: number }
  | { kind: 'scanlines'; opacity: number }
  | { kind: 'ink'; fade: number };

export interface Plan {
  viewBox: [number, number];
  /** What the layers' tokens resolve through. */
  palette: Palette;
  layers: PlanLayer[];
  tracks: Track[];
  reveal?: PlanReveal;
  /** The end of the last finite track. */
  totalMs: number;
  trigger: PlanTrigger;
  reducedMotion: 'final-state';
  effect?: PlanEffect;
  caption?: PlanCaption;
  textures?: PlanTexture[];
}
