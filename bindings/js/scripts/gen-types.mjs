// Generates src/style.d.ts, the typed Style document and result types,
// from schema/freeqrent.schema.json (SPEC §10.3). Variants tagged by
// `kind` become discriminated unions of named interfaces; name lists
// become string-literal unions. The output is checked in, and
// test/types.test.mjs fails when it is stale.
//
//   node scripts/gen-types.mjs           # write src/style.d.ts
//   node scripts/gen-types.mjs --check   # exit 1 if it is stale

import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
export const schemaPath = join(root, '..', '..', 'schema', 'freeqrent.schema.json');
export const outputPath = join(root, 'src', 'style.d.ts');

// Names chosen by path are long; these read better. Keys are the names the
// generator would pick, values the names it writes.
const RENAMES = {
  Error: 'Issue',
  Image: 'RawImage',
  StylePreset: 'PresetName',
  StyleTiming: 'TimingStyle',
  StyleColorSpread: 'ColorSpread',
  StyleIdleItem: 'IdleName',
  PaletteKey: 'Token',
  AlignmentEmphasis: 'Emphasis',
  FindersCorners: 'FinderCorners',
  FrameCircleRing: 'FrameRing',
  CaptionObject: 'CaptionOptions',
  CaptionOptionsFont: 'CaptionFont',
  ModuleShapeBarsAxis: 'Axis',
  LogoOptionsSize: 'LogoSize',
  LogoOptionsFrame: 'LogoFrame',
  LogoOptionsFrameName: 'LogoFrameName',
  LogoOptionsShape: 'LogoShape',
  LogoOptionsShapeName: 'LogoShapeName',
  HalftoneInk: 'Ink',
  HalftoneInkName: 'InkName',
  HalftoneFit: 'Fit',
  IntroName: 'IntroPreset',
  Intro1: 'IntroOptions',
  Intro2: 'PaletteShift',
  IntroOptionsLogo: 'LogoIntro',
  IntroOptionsLogoTiming: 'LogoTiming',
  EncodingEcc: 'Ecc',
  EncodingTrackingGrid: 'TrackingGrid',
  EncodingEci: 'Eci',
  ReportSteeringObject: 'Steering',
  PaletteResultSwatchesItem: 'Swatch',
  PaletteResultChoicesItem: 'Choice',
  ChoiceOrigin: 'ChoiceOrigin',
};

const MAX_SAFE = 2 ** 53 - 1;

// What comes back from JSON.parse: its integers are plain numbers.
const OUTPUTS = new Set(['Error', 'Report', 'PaletteResult']);

function pascal(s) {
  return s
    .split(/[^A-Za-z0-9]+/)
    .filter(Boolean)
    .map((w) => w[0].toUpperCase() + w.slice(1))
    .join('');
}

const rename = (name) => RENAMES[name] ?? name;

function comment(text, indent = '') {
  if (!text) return '';
  return `${indent}/** ${text.replace(/\*\//g, '*\\/')} */\n`;
}

export function generate(schema) {
  const defs = schema.$defs;
  const out = []; // emitted declarations, in order
  const enumNames = new Map(); // JSON of an enum -> its type name
  const emitted = new Set();

  // References to Image name the union, which also takes an ImageData.
  const ref = (r) => (r === '#/$defs/Image' ? 'Image' : rename(r.replace('#/$defs/', '')));

  function literal(v) {
    return JSON.stringify(v);
  }

  let output = false;

  function integer(node) {
    return !output && node.maximum !== undefined && node.maximum > MAX_SAFE ? 'number | bigint' : 'number';
  }

  function scalar(t, node) {
    switch (t) {
      case 'string':
        return 'string';
      case 'number':
        return 'number';
      case 'integer':
        return integer(node);
      case 'boolean':
        return 'boolean';
      case 'null':
        return 'null';
      case 'object':
        return 'Record<string, unknown>';
      default:
        throw new Error(`unknown type ${t}`);
    }
  }

  // A named enum: one declaration per distinct list of names.
  function namedEnum(values, name, description) {
    const key = JSON.stringify(values);
    if (enumNames.has(key)) return enumNames.get(key);
    const n = rename(name);
    enumNames.set(key, n);
    declare(n, `${comment(description)}export type ${n} = ${values.map(literal).join(' | ')};\n`);
    return n;
  }

  function declare(name, text) {
    if (emitted.has(name)) throw new Error(`two declarations named ${name}`);
    emitted.add(name);
    out.push(text);
  }

  function isObject(node) {
    return node.type === 'object' && node.properties !== undefined;
  }

  // The members of an object type, one per line.
  function members(node, name, indent) {
    const required = new Set(node.required ?? []);
    const lines = [];
    for (const [key, prop] of Object.entries(node.properties)) {
      const t = type(prop, `${name}${pascal(key)}`);
      const k = /^[A-Za-z_$][\w$]*$/.test(key) ? key : JSON.stringify(key);
      lines.push(`${comment(prop.description, indent)}${indent}${k}${required.has(key) ? '' : '?'}: ${t};\n`);
    }
    return lines.join('');
  }

  function iface(node, name) {
    const n = rename(name);
    declare(n, `${comment(node.description)}export interface ${n} {\n${members(node, n, '  ')}}\n`);
    return n;
  }

  // An object with `oneOf` members that only require one of its keys: the
  // shared keys as an interface, and one variant per required key.
  function exclusive(node, name) {
    const n = rename(name);
    const only = node.oneOf.map((m) => m.required[0]);
    const base = { ...node, properties: { ...node.properties } };
    for (const k of only) delete base.properties[k];
    delete base.description;
    const opts = iface(base, `${n}Options`);
    const variants = only.map((k) => {
      const prop = node.properties[k];
      const others = only.filter((o) => o !== k).map((o) => ` ${o}?: never;`).join('');
      return `(${opts} & { ${k}: ${type(prop, `${n}${pascal(k)}`)};${others} })`;
    });
    declare(n, `${comment(node.description)}export type ${n} =\n  | ${variants.join('\n  | ')};\n`);
    return n;
  }

  function union(node, name) {
    const n = rename(name);
    const objects = node.oneOf.filter(isObject);
    const parts = node.oneOf.map((m, i) => {
      if (m.enum) return namedEnum(m.enum, `${n}Name`, m.description);
      if (isObject(m)) {
        const kind = m.properties.kind?.const;
        const variant =
          kind !== undefined
            ? `${n}${pascal(kind)}`
            : objects.length === 1
              ? `${n}Object`
              : `${n}${objects.indexOf(m) + 1}`;
        return iface(m, variant);
      }
      return type(m, `${n}${i + 1}`);
    });
    declare(n, `${comment(node.description)}export type ${n} = ${parts.join(' | ')};\n`);
    return n;
  }

  // The type of `node`, declaring what it needs under names built from
  // `name`.
  function type(node, name) {
    if (node.$ref) return ref(node.$ref);
    if (node.const !== undefined) return literal(node.const);
    if (node.enum) return namedEnum(node.enum, name, node.description);
    if (node.oneOf && node.type === 'object' && node.properties) return exclusive(node, name);
    if (node.oneOf) return union(node, name);
    if (Array.isArray(node.type)) return node.type.map((t) => scalar(t, node)).join(' | ');
    if (node.type === 'array') {
      const item = type(node.items, `${name}Item`);
      if (node.minItems !== undefined && node.minItems === node.maxItems) {
        return `[${Array(node.minItems).fill(item).join(', ')}]`;
      }
      const wrapped = /[ |&]/.test(item) ? `(${item})` : item;
      return `${wrapped}[]`;
    }
    if (node.type === 'object' && node.propertyNames?.enum) {
      const keys = namedEnum(node.propertyNames.enum, `${name}Key`);
      const value = type(node.additionalProperties, `${name}Value`);
      const n = rename(name);
      const doc = node.additionalProperties.description;
      declare(n, `${comment(node.description)}export type ${n} = {\n${comment(doc, '  ')}  [K in ${keys}]?: ${value};\n};\n`);
      return n;
    }
    if (isObject(node)) return iface(node, name);
    return scalar(node.type, node);
  }

  const header = `// The Style document and what comes back, as TypeScript types.
// Generated by scripts/gen-types.mjs from schema/freeqrent.schema.json
// (schema version ${schema['x-freeqrent-version']}); don't edit.
`;

  for (const [name, node] of Object.entries(defs)) {
    if (name === 'Base64') {
      declare(
        'Base64',
        `${comment(`${node.description} A Uint8Array (or Uint8ClampedArray) is encoded for you.`)}export type Base64 = string | Uint8Array | Uint8ClampedArray;\n`,
      );
      continue;
    }
    output = OUTPUTS.has(name);
    const t = type(node, name);
    if (t !== rename(name)) {
      declare(rename(name), `${comment(node.description)}export type ${rename(name)} = ${t};\n`);
    }
    if (name === 'Image') {
      declare(
        'ImagePixels',
        `${comment('Pixels as the browser holds them: an ImageData, or anything with its width, height and RGBA data. Sent as a RawImage.')}export interface ImagePixels {\n  width: number;\n  height: number;\n  data: Uint8Array | Uint8ClampedArray;\n}\n`,
      );
      declare('Image', `${comment('A picture: raw pixels, as the document has them or as an ImageData.')}export type Image = RawImage | ImagePixels;\n`);
    }
  }
  return `${header}\n${out.join('\n')}`;
}

function main() {
  const schema = JSON.parse(readFileSync(schemaPath, 'utf8'));
  const text = generate(schema);
  if (process.argv.includes('--check')) {
    if (readFileSync(outputPath, 'utf8') !== text) {
      console.error('src/style.d.ts is stale: run npm run types');
      process.exit(1);
    }
    return;
  }
  writeFileSync(outputPath, text);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) main();
