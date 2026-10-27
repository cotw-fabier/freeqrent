#!/usr/bin/env python3
"""Writes ``python/freeqrent/_styles.py``, the typed Style classes, from
``schema/freeqrent.schema.json``.

    python tools/generate_styles.py           # write the module
    python tools/generate_styles.py --check   # exit 1 if it is stale

The rules:

* An object in the schema becomes a dataclass; a field left ``None`` is left
  out of the document, so the core's default applies.
* A ``oneOf`` whose objects are tagged by ``kind`` becomes one class per
  variant, named after its kind (``Dots``, ``Glow``). A kind that two unions
  share, or that isn't a usable class name, also takes its union's name
  (``RoundedModules``, ``RoundedRing``, ``NoneFrame``, ``ClassTrigger``).
  The union itself becomes a type alias of its variants and its bare names
  (``ModuleShape = Union[ModuleShapeName, SquareModules, ...]``).
* Every ``enum`` becomes a ``Literal`` alias (``Preset``, ``Token``, ``Idle``).
* Base64 fields take ``bytes`` (or base64 text); palettes are mappings from
  ``Token`` to CSS paint.

The output depends only on the schema, so it is checked in, and a test fails
when it is stale.
"""

from __future__ import annotations

import json
import keyword
import textwrap
import sys
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent.parent
SCHEMA = ROOT / "schema" / "freeqrent.schema.json"
OUT = HERE.parent / "python" / "freeqrent" / "_styles.py"

# The suffix a clashing kind takes, per union.
UNION_SUFFIX = {"ModuleShape": "Modules"}

# Names for things the schema leaves unnamed, by their path.
NAMES = {
    # enums
    "Style.idle[]": "Idle",
    "CircleFrame.ring": "FrameRing",
    "Caption.font": "CaptionFont",
    "Logo.size": "LogoSize",
    "Intro.logo": "LogoIntro",
    "Intro.logo_timing": "LogoTiming",
    "Bars.axis": "Axis",
    # inline unions
    "Logo.frame": "LogoFrame",
    "Logo.shape": "LogoShape",
    "Alignment.shape": "AlignmentShape",
    "Halftone.ink": "HalftoneInk",
    # inline objects
    "Finders.corners": "Corners",
    # the variants of unions without a kind
    "Intro#1": "Intro",
    "Intro#2": "PaletteShift",
    "Caption#1": "Caption",
}

# Union aliases whose natural name a class takes.
UNION_ALIAS = {"Intro": "AnyIntro"}

# Docstrings the schema doesn't give.
DOCS = {
    "PaletteOpts": "Options for ``Palette.from_image``, ``from_css`` and ``from_html``.",
    "Corners": "Rings and eyes for single finder corners.",
    "Intro": "An intro preset with its options.",
}

# Extra methods, as source, per class.
EXTRAS = {
    "Image": '''
    @classmethod
    def from_image(cls, image: Any) -> "Image":
        """The pixels of a Pillow-like image: anything with ``.size``,
        ``.mode``, ``.convert("RGBA")`` and ``.tobytes()``."""
        rgba, width, height = rgba_of(image)
        return cls(width=width, height=height, rgba=rgba)
''',
}

# Not usable as class names.
RESERVED = set(keyword.kwlist) | {"Any", "Doc", "Optional", "Union", "List", "Tuple"}


def pascal(name: str) -> str:
    return "".join(part[:1].upper() + part[1:] for part in name.replace("-", "_").split("_"))


def ident(key: str) -> str:
    name = key.replace("-", "_")
    if keyword.iskeyword(name):
        name += "_"
    return name


def doc_line(text: str) -> str:
    return text.replace("\\", "\\\\").replace('"""', '\\"\\"\\"')


class Generator:
    def __init__(self, schema: Dict[str, Any]) -> None:
        self.schema = schema
        self.defs: Dict[str, Any] = schema["$defs"]
        self.literals: Dict[str, Tuple[str, ...]] = {}  # alias -> values
        self.literal_by_values: Dict[Tuple[str, ...], str] = {}
        self.classes: Dict[str, str] = {}  # name -> source
        self.unions: Dict[str, str] = {}  # alias -> source
        self.done: Dict[str, str] = {}  # def name -> python type
        self.kind_owners: Dict[str, List[str]] = {}
        self.taken: set = set()

    # The first pass: which unions share which kinds.
    def scan(self, node: Any, ctx: str, seen: set) -> None:
        if isinstance(node, dict):
            if "$ref" in node:
                name = node["$ref"].split("/")[-1]
                if name not in seen:
                    seen.add(name)
                    self.scan(self.defs[name], name, seen)
                return
            if "oneOf" in node:
                union = NAMES.get(ctx, ctx)
                for v in node["oneOf"]:
                    kind = v.get("properties", {}).get("kind", {}).get("const")
                    if kind is not None:
                        self.kind_owners.setdefault(kind, []).append(union)
            for key, value in node.items():
                if key == "properties":
                    for k, v in value.items():
                        self.scan(v, f"{ctx}.{k}", seen)
                elif key in ("oneOf", "items", "additionalProperties"):
                    self.scan(value, ctx, seen)
        elif isinstance(node, list):
            for v in node:
                self.scan(v, ctx, seen)

    def claim(self, name: str) -> str:
        if name in self.taken:
            raise SystemExit(f"generate_styles: two things want the name {name}; add one to NAMES")
        self.taken.add(name)
        return name

    def variant_name(self, kind: str, union: str) -> str:
        base = pascal(kind)
        def_names = set(self.defs) | set(NAMES.values())
        if (
            len(self.kind_owners.get(kind, [])) > 1
            or base in RESERVED
            or base.lower() in {k.lower() for k in RESERVED}
            or base in def_names
        ):
            return base + UNION_SUFFIX.get(union, union)
        return base

    def literal(self, values: List[str], name: str) -> str:
        key = tuple(values)
        if key in self.literal_by_values:
            return self.literal_by_values[key]
        self.claim(name)
        self.literals[name] = key
        self.literal_by_values[key] = name
        return name

    def ty(self, node: Dict[str, Any], ctx: str) -> str:
        if "$ref" in node:
            return self.ref(node["$ref"].split("/")[-1])
        if "enum" in node:
            name = NAMES.get(ctx) or pascal(ctx.split(".")[-1].rstrip("[]"))
            return f"{self.literal(node['enum'], name)}"
        if "oneOf" in node:
            return self.one_of(node, NAMES.get(ctx, ctx), ctx)
        t = node.get("type")
        if t == "string":
            return "str"
        if t == "boolean":
            return "bool"
        if t == "integer":
            return "int"
        if t == "number":
            return "float"
        if t == "array":
            item = self.ty(node["items"], ctx + "[]")
            if node.get("minItems") == node.get("maxItems") == 2:
                return f"Tuple[{item}, {item}]"
            return f"Sequence[{item}]"
        if t == "object" and "properties" in node:
            return self.object(node, NAMES.get(ctx) or pascal(ctx.split(".")[-1]), None, None)
        raise SystemExit(f"generate_styles: no Python type for {ctx}: {node}")

    def ref(self, name: str) -> str:
        if name in self.done:
            return self.done[name]
        node = self.defs[name]
        if name == "Base64":
            t = "Union[bytes, str]"
        elif name == "Palette":
            token = self.literal(node["propertyNames"]["enum"], "Token")
            self.claim("Tokens")
            self.unions["Tokens"] = (
                f"#: Token paints: {token} to a CSS paint (a color, or a gradient where\n"
                f"#: the token takes one).\nTokens = Mapping[{token}, str]\n"
            )
            t = "Tokens"
        elif "oneOf" in node and "properties" not in node:
            # (Logo's oneOf only says which of svg and raster it needs.)
            t = self.one_of(node, name, name)
        else:
            t = self.object(node, name, None, None)
        self.done[name] = t
        return t

    def one_of(self, node: Dict[str, Any], union: str, ctx: str) -> str:
        variants = node["oneOf"]
        members: List[str] = []
        tagged = any("kind" in v.get("properties", {}) for v in variants)
        for i, v in enumerate(variants):
            kind = v.get("properties", {}).get("kind", {}).get("const")
            if "enum" in v:
                members.append(self.literal(v["enum"], union + "Name"))
            elif kind is not None:
                members.append(self.object(v, self.variant_name(kind, union), kind, union))
            elif v.get("type") == "object" and "properties" in v:
                cls = NAMES.get(f"{ctx}#{i}")
                if cls is None:
                    raise SystemExit(f"generate_styles: name the variant {ctx}#{i} in NAMES")
                members.append(self.object(v, cls, None, union))
            else:
                members.append(self.ty(v, f"{ctx}#{i}"))
        if not tagged and union not in UNION_ALIAS:
            # A plain union (a number or an image, text or a caption) stays inline.
            return f"Union[{', '.join(members)}]"
        alias = self.claim(UNION_ALIAS.get(union, union))
        desc = node.get("description")
        lines = []
        if desc:
            lines.extend("#: " + line for line in textwrap.wrap(desc, 84))
        lines.append(f"{alias} = Union[\n" + "".join(f"    {m},\n" for m in members) + "]\n")
        self.unions[alias] = "\n".join(lines)
        return alias

    def object(
        self, node: Dict[str, Any], name: str, kind: Optional[str], union: Optional[str]
    ) -> str:
        self.claim(name)
        self.classes[name] = ""  # reserve the slot, in first-seen order
        required = set(node.get("required", [])) - {"kind"}
        props = [(k, v) for k, v in node.get("properties", {}).items() if k != "kind"]
        fields_req: List[str] = []
        fields_opt: List[str] = []
        keys: Dict[str, str] = {}
        attrs: List[str] = []
        for key, prop in props:
            py = ident(key)
            if py != key:
                keys[py] = key
            t = self.ty(prop, f"{name}.{key}")
            if key in required:
                fields_req.append(f"    {py}: {t}")
            else:
                fields_opt.append(f"    {py}: Optional[{t}] = None")
            if "description" in prop:
                attrs.append(f"        {py}: {doc_line(prop['description'])}")
        doc: List[str] = []
        if kind is not None:
            doc.append(f'``{{"kind": "{kind}"}}``, one of ``{union}``.')
        elif union is not None:
            doc.append(f"One of ``{UNION_ALIAS.get(union, union)}``.")
        if node.get("description"):
            doc.append(doc_line(node["description"]))
        if name in DOCS:
            doc.append(DOCS[name])
        if not doc:
            doc.append(f"The ``{name}`` part of a Style document.")
        if attrs:
            doc.append("Attributes:\n" + "\n".join(attrs))
        text = "\n\n    ".join(doc)
        end = '\n    """' if "\n" in text else '"""'
        body = ['    """' + text + end, ""]
        if kind is not None:
            body.append(f'    kind: ClassVar[Literal["{kind}"]] = "{kind}"')
        if keys:
            body.append("    _keys: ClassVar[Mapping[str, str]] = {")
            body.extend(f'        "{k}": "{v}",' for k, v in keys.items())
            body.append("    }")
        if kind is not None or keys:
            body.append("")
        body.extend(fields_req + fields_opt)
        if name in EXTRAS:
            body.append(EXTRAS[name].rstrip())
        src = f"@dataclass\nclass {name}(Doc):\n" + "\n".join(body).rstrip() + "\n"
        self.classes[name] = src
        return name

    def run(self) -> str:
        self.scan({"$ref": "#/$defs/Style"}, "", set())
        self.scan({"$ref": "#/$defs/PaletteOpts"}, "", set())
        self.ref("Style")
        self.ref("PaletteOpts")
        version = self.schema.get("x-freeqrent-version", "")
        out: List[str] = [
            '"""The typed Style document (SPEC §10.1), one class per part.',
            "",
            "Generated by tools/generate_styles.py from schema/freeqrent.schema.json;",
            "don't edit by hand.",
            '"""',
            "",
            "# fmt: off",
            "from __future__ import annotations",
            "",
            "from dataclasses import dataclass",
            "from typing import Any, ClassVar, Literal, Mapping, Optional, Sequence, Tuple, Union",
            "",
            "from ._base import Doc",
            "from ._pixels import rgba_of",
            "",
            f'SCHEMA_VERSION = "{version}"',
            "",
        ]
        for name, values in self.literals.items():
            joined = ", ".join(json.dumps(v) for v in values)
            line = f"{name} = Literal[{joined}]"
            if len(line) > 88:
                line = f"{name} = Literal[\n" + "".join(f"    {json.dumps(v)},\n" for v in values) + "]"
            out.append(line)
        out.append("")
        for src in self.classes.values():
            out.append("")
            out.append(src)
        out.append("")
        for src in self.unions.values():
            out.append(src)
        names = sorted(set(self.literals) | set(self.classes) | set(self.unions))
        out.append("__all__ = [")
        out.extend(f'    "{n}",' for n in ["SCHEMA_VERSION", *names])
        out.append("]")
        return "\n".join(out).rstrip() + "\n"


def main() -> int:
    schema = json.loads(SCHEMA.read_text(encoding="utf-8"))
    text = Generator(schema).run()
    if "--check" in sys.argv[1:]:
        current = OUT.read_text(encoding="utf-8") if OUT.exists() else ""
        if current != text:
            print(f"{OUT} is stale: run python tools/generate_styles.py", file=sys.stderr)
            return 1
        return 0
    OUT.write_text(text, encoding="utf-8")
    print(f"wrote {OUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
